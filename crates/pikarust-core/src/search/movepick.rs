use crate::bitboard::{Bitboard, line_bb};
use crate::position::Position;
use crate::types::{Depth, Move, PIECE_VALUE, PieceType, Value};

use super::history::{
    ButterflyHistory, CapturePieceToHistory, LOW_PLY_HISTORY_SIZE, LowPlyHistory, PawnHistory,
    PieceToHistory,
};

const MAX_MOVES: usize = 128;

#[derive(Copy, Clone)]
struct ScoredMove {
    m: Move,
    score: i32,
}

#[derive(PartialEq, Eq)]
enum Stage {
    MainTT,
    CaptureInit,
    GoodCapture,
    QuietInit,
    GoodQuiet,
    BadCapture,
    BadQuiet,
    EvasionTT,
    EvasionInit,
    Evasion,
    ProbCutTT,
    ProbCutInit,
    ProbCut,
    QSearchTT,
    QCaptureInit,
    QCapture,
}

fn partial_insertion_sort(moves: &mut [ScoredMove], limit: i32) {
    let len = moves.len();
    if len <= 1 {
        return;
    }
    let mut sorted_end = 0usize;
    for p in 1..len {
        if moves[p].score >= limit {
            sorted_end += 1;
            let tmp = moves[p];
            moves[p] = moves[sorted_end];
            let mut q = sorted_end;
            while q > 0 && moves[q - 1].score < tmp.score {
                moves[q] = moves[q - 1];
                q -= 1;
            }
            moves[q] = tmp;
        }
    }
}

pub struct MovePicker {
    stage: Stage,
    tt_move: Move,
    depth: Depth,
    threshold: Value,
    ply: i32,
    cur: usize,
    end_moves: usize,
    end_bad_captures: usize,
    end_bad_quiets: usize,
    moves: [ScoredMove; MAX_MOVES],
    skip_quiets: bool,
}

/// History tables borrowed only while a move-generation stage is scored.
///
/// A picker never retains these references across recursive searches. Passing
/// current tables preserves the effects of history updates between stages.
#[derive(Default)]
pub struct MovePickerHistory<'a> {
    pub main: Option<&'a ButterflyHistory>,
    pub low_ply: Option<&'a LowPlyHistory>,
    pub capture: Option<&'a CapturePieceToHistory>,
    pub continuation: &'a [&'a PieceToHistory],
    pub pawn: Option<&'a PawnHistory>,
}

impl MovePicker {
    pub fn new_main(pos: &Position, tt_move: Move, depth: Depth, ply: i32) -> Self {
        let valid_tt = tt_move.is_ok() && pos.pseudo_legal(tt_move);
        let stage = if pos.checkers().is_not_empty() {
            if valid_tt {
                Stage::EvasionTT
            } else {
                Stage::EvasionInit
            }
        } else if depth > 0 {
            if valid_tt {
                Stage::MainTT
            } else {
                Stage::CaptureInit
            }
        } else if valid_tt {
            Stage::QSearchTT
        } else {
            Stage::QCaptureInit
        };
        Self {
            stage,
            tt_move: if valid_tt { tt_move } else { Move::NONE },
            depth,
            threshold: 0,
            ply,
            cur: 0,
            end_moves: 0,
            end_bad_captures: 0,
            end_bad_quiets: 0,
            moves: [ScoredMove {
                m: Move::NONE,
                score: 0,
            }; MAX_MOVES],
            skip_quiets: false,
        }
    }

    pub fn new_simple(pos: &Position, tt_move: Move, depth: Depth, ply: i32) -> Self {
        Self::new_main(pos, tt_move, depth, ply)
    }

    pub fn new_probcut(pos: &Position, tt_move: Move, threshold: Value) -> Self {
        let valid_tt = tt_move.is_ok() && pos.is_capture(tt_move) && pos.pseudo_legal(tt_move);
        let mut picker = Self::new_main(pos, Move::NONE, 0, 0);
        picker.tt_move = if valid_tt { tt_move } else { Move::NONE };
        picker.stage = if valid_tt {
            Stage::ProbCutTT
        } else {
            Stage::ProbCutInit
        };
        picker.threshold = threshold;
        picker
    }

    pub const fn skip_quiet_moves(&mut self) {
        self.skip_quiets = true;
    }

    fn score_captures(&mut self, pos: &Position, history: &MovePickerHistory<'_>) {
        use crate::position::{GenType, generate};
        let ml = generate(pos, GenType::Captures);
        for i in 0..ml.len() {
            let m = ml.get(i);
            let to = m.to_sq();
            let pc = pos.moved_piece(m);
            let captured = pos.piece_on(to);
            if captured == crate::types::Piece::NONE {
                continue;
            }
            let captured_type = captured.piece_type();
            let score = i32::from(
                history
                    .capture
                    .map_or(0, |table| table.get(pc, to, captured_type)),
            ) + 7 * PIECE_VALUE[captured.index()];
            self.moves[self.end_moves] = ScoredMove { m, score };
            self.end_moves += 1;
        }
    }

    fn score_quiets(&mut self, pos: &Position, history: &MovePickerHistory<'_>) {
        use crate::bitboard::square_bb;
        use crate::position::{GenType, generate};
        let ml = generate(pos, GenType::Quiets);
        let us = pos.side_to_move();
        let them = !us;

        let threat_by_advisor_bishop = pos.attacks_by(PieceType::Pawn, them);
        let threat_by_knight_cannon = threat_by_advisor_bishop
            | pos.attacks_by(PieceType::Advisor, them)
            | pos.attacks_by(PieceType::Bishop, them);
        let threat_by_rook = threat_by_knight_cannon
            | pos.attacks_by(PieceType::Knight, them)
            | pos.attacks_by(PieceType::Cannon, them);

        for i in 0..ml.len() {
            let m = ml.get(i);
            let from = m.from_sq();
            let to = m.to_sq();
            let pc = pos.moved_piece(m);
            let pt = pc.piece_type();

            let mut score = 2 * i32::from(history.main.map_or(0, |table| table.get(us, m)));
            score += 2 * i32::from(
                history
                    .pawn
                    .map_or(0, |table| table.entry(pos.pawn_key()).get(pc, to)),
            );
            for idx in [0, 1, 2, 3, 5] {
                if let Some(table) = history.continuation.get(idx) {
                    score += i32::from(table.get(pc, to));
                }
            }

            let check_sq = pos.check_squares(pt);
            let gives_check = if pt == PieceType::Cannon {
                let ksq = pos.king_square(!us);
                (check_sq & !line_bb(from, ksq) & to).is_not_empty()
            } else {
                (check_sq & to).is_not_empty()
            };

            if gives_check && pos.see_ge(m, -75) {
                score += 16384;
            }

            let threat = match pt {
                PieceType::Advisor | PieceType::Bishop => threat_by_advisor_bishop,
                PieceType::Knight | PieceType::Cannon => threat_by_knight_cannon,
                PieceType::Rook => threat_by_rook,
                _ => Bitboard::EMPTY,
            };
            let from_bb = square_bb(from);
            let to_bb = square_bb(to);
            let v = 20
                * (i32::from((threat & from_bb).is_not_empty())
                    - i32::from((threat & to_bb).is_not_empty()));
            score += PIECE_VALUE[pc] * v;

            if (self.ply as usize) < LOW_PLY_HISTORY_SIZE {
                score += 8 * i32::from(
                    history
                        .low_ply
                        .map_or(0, |table| table.get(self.ply as usize, m)),
                ) / (1 + self.ply);
            }

            self.moves[self.end_moves] = ScoredMove { m, score };
            self.end_moves += 1;
        }
    }

    fn score_evasions(&mut self, pos: &Position, history: &MovePickerHistory<'_>) {
        use crate::position::{GenType, generate};
        let ml = generate(pos, GenType::Evasions);
        let us = pos.side_to_move();

        for i in 0..ml.len() {
            let m = ml.get(i);
            let to = m.to_sq();
            let pc = pos.moved_piece(m);
            let captured = pos.piece_on(to);

            let score = if captured == crate::types::Piece::NONE {
                i32::from(history.main.map_or(0, |table| table.get(us, m)))
                    + i32::from(
                        history
                            .continuation
                            .first()
                            .map_or(0, |table| table.get(pc, to)),
                    )
            } else {
                PIECE_VALUE[captured.index()] + (1 << 28)
            };

            self.moves[self.end_moves] = ScoredMove { m, score };
            self.end_moves += 1;
        }
    }

    /// Pick a move without search history, using material and tactical ordering.
    pub fn next_move(&mut self, pos: &Position) -> Move {
        self.next_move_with_history(pos, &MovePickerHistory::default())
    }

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub fn next_move_with_history(
        &mut self,
        pos: &Position,
        history: &MovePickerHistory<'_>,
    ) -> Move {
        loop {
            match self.stage {
                Stage::MainTT | Stage::EvasionTT | Stage::ProbCutTT | Stage::QSearchTT => {
                    self.stage = match self.stage {
                        Stage::MainTT => Stage::CaptureInit,
                        Stage::EvasionTT => Stage::EvasionInit,
                        Stage::ProbCutTT => Stage::ProbCutInit,
                        _ => Stage::QCaptureInit,
                    };
                    return self.tt_move;
                }

                Stage::CaptureInit | Stage::ProbCutInit | Stage::QCaptureInit => {
                    let next = match self.stage {
                        Stage::CaptureInit => Stage::GoodCapture,
                        Stage::ProbCutInit => Stage::ProbCut,
                        _ => Stage::QCapture,
                    };
                    self.cur = 0;
                    self.end_bad_captures = 0;
                    self.end_moves = 0;
                    self.score_captures(pos, history);
                    partial_insertion_sort(&mut self.moves[..self.end_moves], i32::MIN);
                    self.stage = next;
                }

                Stage::GoodCapture => {
                    while self.cur < self.end_moves {
                        let sm = self.moves[self.cur];
                        self.cur += 1;
                        if sm.m == self.tt_move {
                            continue;
                        }
                        if pos.see_ge(sm.m, -sm.score / 18) {
                            return sm.m;
                        }
                        self.moves[self.end_bad_captures] = sm;
                        self.end_bad_captures += 1;
                    }
                    self.stage = Stage::QuietInit;
                }

                Stage::QuietInit => {
                    if !self.skip_quiets {
                        let save = self.end_bad_captures;
                        self.cur = save;
                        self.end_bad_quiets = save;
                        self.end_moves = save;
                        self.score_quiets(pos, history);
                        partial_insertion_sort(
                            &mut self.moves[self.cur..self.end_moves],
                            -3330 * self.depth,
                        );
                    }
                    self.stage = Stage::GoodQuiet;
                }

                Stage::GoodQuiet => {
                    if !self.skip_quiets {
                        while self.cur < self.end_moves {
                            let sm = self.moves[self.cur];
                            self.cur += 1;
                            if sm.m == self.tt_move {
                                continue;
                            }
                            if sm.score > -14000 {
                                return sm.m;
                            }
                            self.moves[self.end_bad_quiets] = sm;
                            self.end_bad_quiets += 1;
                        }
                    }
                    self.cur = 0;
                    self.end_moves = self.end_bad_captures;
                    self.stage = Stage::BadCapture;
                }

                Stage::BadCapture => {
                    while self.cur < self.end_moves {
                        let sm = self.moves[self.cur];
                        self.cur += 1;
                        if sm.m == self.tt_move {
                            continue;
                        }
                        return sm.m;
                    }
                    self.cur = self.end_bad_captures;
                    self.end_moves = self.end_bad_quiets;
                    self.stage = Stage::BadQuiet;
                }

                Stage::BadQuiet => {
                    if !self.skip_quiets {
                        while self.cur < self.end_moves {
                            let sm = self.moves[self.cur];
                            self.cur += 1;
                            if sm.m == self.tt_move {
                                continue;
                            }
                            return sm.m;
                        }
                    }
                    return Move::NONE;
                }

                Stage::EvasionInit => {
                    self.cur = 0;
                    self.end_moves = 0;
                    self.score_evasions(pos, history);
                    partial_insertion_sort(&mut self.moves[..self.end_moves], i32::MIN);
                    self.stage = Stage::Evasion;
                }

                Stage::Evasion | Stage::QCapture => {
                    while self.cur < self.end_moves {
                        let sm = self.moves[self.cur];
                        self.cur += 1;
                        if sm.m == self.tt_move {
                            continue;
                        }
                        return sm.m;
                    }
                    return Move::NONE;
                }

                Stage::ProbCut => {
                    while self.cur < self.end_moves {
                        let sm = self.moves[self.cur];
                        self.cur += 1;
                        if sm.m == self.tt_move {
                            continue;
                        }
                        if pos.see_ge(sm.m, self.threshold) {
                            return sm.m;
                        }
                    }
                    return Move::NONE;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Square;

    #[test]
    fn test_partial_insertion_sort_empty() {
        let mut moves: Vec<ScoredMove> = vec![];
        partial_insertion_sort(&mut moves, i32::MIN);
    }

    #[test]
    fn test_partial_insertion_sort_sorted() {
        let mut moves = [
            ScoredMove {
                m: Move::NONE,
                score: 100,
            },
            ScoredMove {
                m: Move::NONE,
                score: 50,
            },
            ScoredMove {
                m: Move::NONE,
                score: 10,
            },
        ];
        partial_insertion_sort(&mut moves, i32::MIN);
        assert!(moves[0].score >= moves[1].score);
        assert!(moves[1].score >= moves[2].score);
    }

    #[test]
    fn test_partial_insertion_sort_with_limit() {
        let mut moves = [
            ScoredMove {
                m: Move::NONE,
                score: 5,
            },
            ScoredMove {
                m: Move::NONE,
                score: 100,
            },
            ScoredMove {
                m: Move::NONE,
                score: 3,
            },
            ScoredMove {
                m: Move::NONE,
                score: 200,
            },
        ];
        partial_insertion_sort(&mut moves, 50);
        assert_eq!(moves[0].score, 200);
        assert_eq!(moves[1].score, 100);
    }

    #[test]
    fn test_movepicker_start_pos() {
        let pos = Position::start_pos().expect("start_pos should parse");

        let mut mp = MovePicker::new_main(&pos, Move::NONE, 5, 0);

        let mut count = 0;
        loop {
            let m = mp.next_move(&pos);
            if m == Move::NONE {
                break;
            }
            count += 1;
            assert!(count <= MAX_MOVES);
        }
        assert_eq!(
            count, 44,
            "picker must generate all legal start-position moves"
        );
    }

    #[test]
    fn test_movepicker_with_tt_move() {
        let pos = Position::start_pos().expect("start_pos should parse");

        let tt_move = Move::make(Square::SQ_B0, Square::SQ_C2);

        let mut mp = MovePicker::new_main(&pos, tt_move, 5, 0);

        let first = mp.next_move(&pos);
        assert_eq!(first, tt_move, "first move should be the TT move");

        let mut saw_tt_again = false;
        loop {
            let m = mp.next_move(&pos);
            if m == Move::NONE {
                break;
            }
            if m == tt_move {
                saw_tt_again = true;
            }
        }
        assert!(!saw_tt_again, "TT move should not appear again");
    }

    #[test]
    fn test_movepicker_qsearch() {
        let pos = Position::start_pos().expect("start_pos should parse");

        let mut mp = MovePicker::new_main(&pos, Move::NONE, 0, 0);

        let mut count = 0;
        loop {
            let m = mp.next_move(&pos);
            if m == Move::NONE {
                break;
            }
            count += 1;
        }
        // Start position has 2 cannon captures (each cannon jumps over a
        // friendly pawn to capture the opposing pawn on the same file).
        assert_eq!(count, 2, "cannon captures from start position in qsearch");
    }

    #[test]
    fn test_movepicker_skip_quiets() {
        let pos = Position::start_pos().expect("start_pos should parse");

        let mut mp = MovePicker::new_main(&pos, Move::NONE, 5, 0);
        mp.skip_quiet_moves();

        let mut count = 0;
        loop {
            let m = mp.next_move(&pos);
            if m == Move::NONE {
                break;
            }
            count += 1;
        }
        assert_eq!(
            count, 2,
            "only cannon captures from start position, quiets skipped"
        );
    }

    #[test]
    fn history_is_borrowed_only_for_each_selection() {
        let pos = Position::start_pos().expect("start position");
        let mut picker = MovePicker::new_main(&pos, Move::NONE, 5, 0);
        let first = {
            let capture = CapturePieceToHistory::new();
            picker.next_move_with_history(
                &pos,
                &MovePickerHistory {
                    capture: Some(&capture),
                    ..MovePickerHistory::default()
                },
            )
        };
        assert!(first.is_ok());
        // The history above no longer exists. Subsequent stages use only the
        // references provided for this call, never pointers retained by picker.
        let mut moves = vec![first];
        loop {
            let next = picker.next_move(&pos);
            if next == Move::NONE {
                break;
            }
            assert!(!moves.contains(&next));
            moves.push(next);
        }
        assert_eq!(moves.len(), 44);
    }

    #[test]
    fn test_movepicker_probcut() {
        let pos = Position::start_pos().expect("start_pos should parse");

        let mut mp = MovePicker::new_probcut(&pos, Move::NONE, 200);

        let mut count = 0;
        loop {
            let m = mp.next_move(&pos);
            if m == Move::NONE {
                break;
            }
            count += 1;
        }
        assert_eq!(count, 0, "no captures from start position");
    }
}
