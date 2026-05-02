use crate::bitboard::line_bb;
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
    // Stored references to history tables (as raw pointers to avoid lifetime issues)
    main_history: *const ButterflyHistory,
    low_ply_history: *const LowPlyHistory,
    capture_history: *const CapturePieceToHistory,
    cont_hist_ptrs: [*const PieceToHistory; 6],
    cont_hist_len: usize,
    pawn_history: *const PawnHistory,
    has_pawn_history: bool,
}

// SAFETY: MovePicker only lives within a single search call where all referenced
// data outlives it. The raw pointers are never sent across threads.
#[allow(unsafe_code)]
unsafe impl Send for MovePicker {}

impl MovePicker {
    #[allow(clippy::too_many_arguments)]
    pub fn new_main(
        pos: &Position,
        tt_move: Move,
        depth: Depth,
        main_history: &ButterflyHistory,
        low_ply_history: &LowPlyHistory,
        capture_history: &CapturePieceToHistory,
        cont_hist: &[&PieceToHistory],
        pawn_history: &PawnHistory,
        ply: i32,
    ) -> Self {
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

        let mut cont_hist_ptrs = [std::ptr::null(); 6];
        let cont_hist_len = cont_hist.len().min(6);
        for (i, &ch) in cont_hist.iter().take(6).enumerate() {
            cont_hist_ptrs[i] = &raw const *ch;
        }

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
            main_history: &raw const *main_history,
            low_ply_history: &raw const *low_ply_history,
            capture_history: &raw const *capture_history,
            cont_hist_ptrs,
            cont_hist_len,
            pawn_history: &raw const *pawn_history,
            has_pawn_history: true,
        }
    }

    pub fn new_simple(pos: &Position, tt_move: Move, depth: Depth, ply: i32) -> Self {
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
            main_history: std::ptr::null(),
            low_ply_history: std::ptr::null(),
            capture_history: std::ptr::null(),
            cont_hist_ptrs: [std::ptr::null(); 6],
            cont_hist_len: 0,
            pawn_history: std::ptr::null(),
            has_pawn_history: false,
        }
    }

    pub fn new_probcut(
        pos: &Position,
        tt_move: Move,
        threshold: Value,
        capture_history: &CapturePieceToHistory,
    ) -> Self {
        let valid_tt = tt_move.is_ok() && pos.is_capture(tt_move) && pos.pseudo_legal(tt_move);
        let stage = if valid_tt {
            Stage::ProbCutTT
        } else {
            Stage::ProbCutInit
        };

        Self {
            stage,
            tt_move: if valid_tt { tt_move } else { Move::NONE },
            depth: 0,
            threshold,
            ply: 0,
            cur: 0,
            end_moves: 0,
            end_bad_captures: 0,
            end_bad_quiets: 0,
            moves: [ScoredMove {
                m: Move::NONE,
                score: 0,
            }; MAX_MOVES],
            skip_quiets: false,
            main_history: std::ptr::null(),
            low_ply_history: std::ptr::null(),
            capture_history: &raw const *capture_history,
            cont_hist_ptrs: [std::ptr::null(); 6],
            cont_hist_len: 0,
            pawn_history: std::ptr::null(),
            has_pawn_history: false,
        }
    }

    pub const fn skip_quiet_moves(&mut self) {
        self.skip_quiets = true;
    }

    #[allow(unsafe_code)]
    fn score_captures(&mut self, pos: &Position) {
        use crate::position::{GenType, generate};
        let ml = generate(pos, GenType::Captures);
        // SAFETY: capture_history pointer is valid for the lifetime of the search call.
        let capture_hist = unsafe { &*self.capture_history };
        for i in 0..ml.len() {
            let m = ml.get(i);
            let to = m.to_sq();
            let pc = pos.moved_piece(m);
            let captured = pos.piece_on(to);
            if captured == crate::types::Piece::NONE {
                continue;
            }
            let captured_type = captured.piece_type();
            let score = i32::from(capture_hist.get(pc, to, captured_type))
                + 7 * PIECE_VALUE[captured.index()];
            self.moves[self.end_moves] = ScoredMove { m, score };
            self.end_moves += 1;
        }
    }

    #[allow(unsafe_code)]
    fn score_quiets(&mut self, pos: &Position) {
        use crate::position::{GenType, generate};
        let ml = generate(pos, GenType::Quiets);
        let us = pos.side_to_move();

        // SAFETY: All pointers are valid for the lifetime of the search call.
        let main_hist = unsafe { &*self.main_history };
        let low_ply_hist = unsafe { &*self.low_ply_history };

        for i in 0..ml.len() {
            let m = ml.get(i);
            let to = m.to_sq();
            let pc = pos.moved_piece(m);
            let pt = pc.piece_type();

            let mut score = 2 * i32::from(main_hist.get(us, m));

            if self.has_pawn_history {
                // SAFETY: pawn_history pointer is valid.
                let ph = unsafe { &*self.pawn_history };
                score += 2 * i32::from(ph.entry(pos.pawn_key()).get(pc, to));
            }

            for idx in [0, 1, 2, 3, 5] {
                if idx < self.cont_hist_len && !self.cont_hist_ptrs[idx].is_null() {
                    // SAFETY: cont_hist pointer is valid.
                    let ch = unsafe { &*self.cont_hist_ptrs[idx] };
                    score += i32::from(ch.get(pc, to));
                }
            }

            let check_sq = pos.check_squares(pt);
            let gives_check = if pt == PieceType::Cannon {
                let from = m.from_sq();
                let ksq = pos.king_square(!us);
                (check_sq & !line_bb(from, ksq) & to).is_not_empty()
            } else {
                (check_sq & to).is_not_empty()
            };

            if gives_check && pos.see_ge(m, -75) {
                score += 16384;
            }

            if (self.ply as usize) < LOW_PLY_HISTORY_SIZE {
                score += 8 * i32::from(low_ply_hist.get(self.ply as usize, m)) / (1 + self.ply);
            }

            self.moves[self.end_moves] = ScoredMove { m, score };
            self.end_moves += 1;
        }
    }

    #[allow(unsafe_code)]
    fn score_evasions(&mut self, pos: &Position) {
        use crate::position::{GenType, generate};
        let ml = generate(pos, GenType::Evasions);
        let us = pos.side_to_move();

        for i in 0..ml.len() {
            let m = ml.get(i);
            let to = m.to_sq();
            let pc = pos.moved_piece(m);
            let captured = pos.piece_on(to);

            let score = if captured == crate::types::Piece::NONE {
                let mut v = 0i32;
                if !self.main_history.is_null() {
                    // SAFETY: main_history pointer is valid.
                    let mh = unsafe { &*self.main_history };
                    v += i32::from(mh.get(us, m));
                }
                if self.cont_hist_len > 0 && !self.cont_hist_ptrs[0].is_null() {
                    // SAFETY: cont_hist pointer is valid.
                    let ch = unsafe { &*self.cont_hist_ptrs[0] };
                    v += i32::from(ch.get(pc, to));
                }
                v
            } else {
                PIECE_VALUE[captured.index()] + (1 << 28)
            };

            self.moves[self.end_moves] = ScoredMove { m, score };
            self.end_moves += 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn next_move_with_history(
        &mut self,
        pos: &Position,
        main_history: &ButterflyHistory,
        low_ply_history: &LowPlyHistory,
        capture_history: &CapturePieceToHistory,
        cont_hist: &[Option<&PieceToHistory>],
        pawn_history: Option<&PawnHistory>,
    ) -> Move {
        self.main_history = &raw const *main_history;
        self.low_ply_history = &raw const *low_ply_history;
        self.capture_history = &raw const *capture_history;
        self.cont_hist_len = cont_hist.len().min(6);
        for (i, opt) in cont_hist.iter().take(6).enumerate() {
            self.cont_hist_ptrs[i] = opt.map_or(std::ptr::null(), |ch| &raw const *ch);
        }
        if let Some(ph) = pawn_history {
            self.pawn_history = &raw const *ph;
            self.has_pawn_history = true;
        }
        self.next_move(pos)
    }

    #[allow(clippy::too_many_lines)]
    #[allow(clippy::cognitive_complexity)]
    pub fn next_move(&mut self, pos: &Position) -> Move {
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
                    self.score_captures(pos);
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
                        self.score_quiets(pos);
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
                    self.score_evasions(pos);
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
        let bh = ButterflyHistory::new();
        let lph = LowPlyHistory::new();
        let cph = CapturePieceToHistory::new();
        let ph = PawnHistory::new();
        let cont_hist_sentinel = PieceToHistory::new();
        let cont_hist: [&PieceToHistory; 1] = [&cont_hist_sentinel];

        let mut mp = MovePicker::new_main(&pos, Move::NONE, 5, &bh, &lph, &cph, &cont_hist, &ph, 0);

        let mut count = 0;
        loop {
            let m = mp.next_move(&pos);
            if m == Move::NONE {
                break;
            }
            count += 1;
            assert!(count <= MAX_MOVES);
        }
        assert!(count > 0, "should generate moves from start position");
    }

    #[test]
    fn test_movepicker_with_tt_move() {
        let pos = Position::start_pos().expect("start_pos should parse");
        let bh = ButterflyHistory::new();
        let lph = LowPlyHistory::new();
        let cph = CapturePieceToHistory::new();
        let ph = PawnHistory::new();
        let cont_hist_sentinel = PieceToHistory::new();
        let cont_hist: [&PieceToHistory; 1] = [&cont_hist_sentinel];

        let tt_move = Move::make(Square::SQ_B0, Square::SQ_C2);

        let mut mp = MovePicker::new_main(&pos, tt_move, 5, &bh, &lph, &cph, &cont_hist, &ph, 0);

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
        let bh = ButterflyHistory::new();
        let lph = LowPlyHistory::new();
        let cph = CapturePieceToHistory::new();
        let ph = PawnHistory::new();
        let cont_hist_sentinel = PieceToHistory::new();
        let cont_hist: [&PieceToHistory; 1] = [&cont_hist_sentinel];

        let mut mp = MovePicker::new_main(&pos, Move::NONE, 0, &bh, &lph, &cph, &cont_hist, &ph, 0);

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
        let bh = ButterflyHistory::new();
        let lph = LowPlyHistory::new();
        let cph = CapturePieceToHistory::new();
        let ph = PawnHistory::new();
        let cont_hist_sentinel = PieceToHistory::new();
        let cont_hist: [&PieceToHistory; 1] = [&cont_hist_sentinel];

        let mut mp = MovePicker::new_main(&pos, Move::NONE, 5, &bh, &lph, &cph, &cont_hist, &ph, 0);
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
    fn test_movepicker_probcut() {
        let pos = Position::start_pos().expect("start_pos should parse");
        let cph = CapturePieceToHistory::new();

        let mut mp = MovePicker::new_probcut(&pos, Move::NONE, 200, &cph);

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
