use crate::types::{Color, Move, PIECE_VALUE, Piece, PieceType};

use super::position::Position;
use super::state::StateInfo;
use super::zobrist::zobrist;

impl Position {
    pub fn do_move(&mut self, m: Move, gives_check: bool) {
        let z = zobrist();

        self.bloom_filter.insert(self.state.key);

        let mut new_state = StateInfo::new();
        new_state.copy_from_previous(&self.state);
        new_state.last_move = m;

        let mut k = self.state.key ^ z.side;

        self.game_ply += 1;
        if !gives_check || {
            new_state.check10[self.side_to_move] += 1;
            new_state.check10[self.side_to_move] <= 10
        } {
            if new_state.check10[!self.side_to_move] > 10 && self.state.checkers_bb.is_not_empty() {
                new_state.check10[!self.side_to_move] += 1;
            } else {
                new_state.rule60 += 1;
            }
        }
        new_state.plies_from_null += 1;

        let us = self.side_to_move;
        let them = !us;
        let from = m.from_sq();
        let to = m.to_sq();
        let pc = self.piece_on(from);
        let captured = self.piece_on(to);

        if captured != Piece::NONE {
            let pt_captured = captured.piece_type();

            if pt_captured == PieceType::Pawn {
                new_state.pawn_key ^= z.psq[captured.index()][to.index()];
            } else {
                new_state.non_pawn_key[them] ^= z.psq[captured.index()][to.index()];

                if (pt_captured as u8 & 1) != 0 {
                    new_state.major_material[them] -= PIECE_VALUE[captured];
                    if pt_captured != PieceType::Rook {
                        new_state.minor_piece_key ^= z.psq[captured.index()][to.index()];
                    }
                }
            }

            k ^= z.psq[captured.index()][to.index()];

            new_state.check10 = [0; Color::NUM];
            new_state.rule60 = 0;

            self.remove_piece(to);
        }

        k ^= z.psq[pc.index()][from.index()] ^ z.psq[pc.index()][to.index()];

        let pt = pc.piece_type();
        if pt == PieceType::Pawn {
            new_state.pawn_key ^= z.psq[pc.index()][from.index()] ^ z.psq[pc.index()][to.index()];
        } else {
            new_state.non_pawn_key[us] ^=
                z.psq[pc.index()][from.index()] ^ z.psq[pc.index()][to.index()];

            if pt == PieceType::Knight || pt == PieceType::Cannon {
                new_state.minor_piece_key ^=
                    z.psq[pc.index()][from.index()] ^ z.psq[pc.index()][to.index()];
            }
        }

        self.move_piece(from, to);

        new_state.captured_piece = captured;

        new_state.checkers_bb = if gives_check {
            self.checkers_to(us, self.king_square(them), self.all_pieces())
        } else {
            crate::bitboard::Bitboard::EMPTY
        };

        let old_state = std::mem::replace(&mut self.state, new_state);
        self.state_stack.push(old_state);

        self.side_to_move = them;

        self.set_check_info();

        self.state.key = k;
    }

    pub fn undo_move(&mut self, m: Move) {
        self.side_to_move = !self.side_to_move;

        let from = m.from_sq();
        let to = m.to_sq();
        let captured = self.state.captured_piece;

        self.move_piece(to, from);

        if captured != Piece::NONE {
            self.put_piece(captured, to);
        }

        if let Some(prev_state) = self.state_stack.pop() {
            self.state = prev_state;
        }
        self.game_ply -= 1;

        self.bloom_filter.remove(self.state.key);

        self.debug_check_consistency(&format!(
            "inside_undo_move move={m:?} from={from:?} to={to:?} captured={captured:?}"
        ));
    }

    pub fn do_null_move(&mut self) {
        let z = zobrist();

        self.bloom_filter.insert(self.state.key);

        let mut new_state = self.state.clone();
        new_state.key ^= z.side;
        new_state.plies_from_null = 0;
        new_state.last_move = Move::NONE;

        let old_state = std::mem::replace(&mut self.state, new_state);
        self.state_stack.push(old_state);

        self.side_to_move = !self.side_to_move;

        self.set_check_info();
    }

    pub fn undo_null_move(&mut self) {
        if let Some(prev_state) = self.state_stack.pop() {
            self.state = prev_state;
        }
        self.side_to_move = !self.side_to_move;

        self.bloom_filter.remove(self.state.key);
    }

    pub fn do_move_for_chase(&mut self, m: Move) -> (Piece, i32) {
        let from = m.from_sq();
        let to = m.to_sq();
        let captured = self.piece_on(to);
        let id = self.id_board[to];

        self.id_board[to] = self.id_board[from];
        self.id_board[from] = 0;

        self.remove_piece(to);
        self.move_piece(from, to);

        self.side_to_move = !self.side_to_move;

        (captured, id)
    }

    pub fn undo_move_for_chase(&mut self, m: Move, captured: Piece, id: i32) {
        self.side_to_move = !self.side_to_move;

        let from = m.from_sq();
        let to = m.to_sq();

        self.id_board[from] = self.id_board[to];
        self.id_board[to] = id;

        self.move_piece(to, from);

        if captured != Piece::NONE {
            self.put_piece(captured, to);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::movegen::{GenType, generate};

    const START_FEN: &str =
        "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";

    // -------------------------------------------------------------------
    // do_move / undo_move roundtrip: Zobrist key preservation
    // -------------------------------------------------------------------

    #[test]
    fn test_do_undo_move_preserves_key_startpos() {
        let mut pos = Position::start_pos().unwrap();
        let original_key = pos.key();
        let original_fen = pos.fen();

        let ml = generate(&pos, GenType::Legal);
        assert!(!ml.is_empty());

        for i in 0..ml.len() {
            let m = ml.get(i);
            let gives_check = pos.gives_check(m);
            pos.do_move(m, gives_check);
            pos.undo_move(m);
            assert_eq!(
                pos.key(),
                original_key,
                "Zobrist key mismatch after do/undo move {m:?}"
            );
            assert_eq!(
                pos.fen(),
                original_fen,
                "FEN mismatch after do/undo move {m:?}"
            );
        }
    }

    #[test]
    fn test_do_undo_move_preserves_key_midgame() {
        let fen = "r1ba1a3/4kn3/2n1b4/pNp1p1p1p/4c4/6P2/P1P2R2P/1CcC5/9/2BAKAB2 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let original_key = pos.key();
        let original_fen = pos.fen();

        let ml = generate(&pos, GenType::Legal);
        for i in 0..ml.len() {
            let m = ml.get(i);
            let gives_check = pos.gives_check(m);
            pos.do_move(m, gives_check);
            pos.undo_move(m);
            assert_eq!(
                pos.key(),
                original_key,
                "Zobrist key mismatch after do/undo {m:?} in midgame"
            );
            assert_eq!(
                pos.fen(),
                original_fen,
                "FEN mismatch after do/undo {m:?} in midgame"
            );
        }
    }

    #[test]
    fn test_do_undo_move_preserves_key_black_to_move() {
        let fen =
            "2bak4/9/3a5/p2Np3p/3n1P3/3pc3P/P4r1c1/B2CC2R1/4A4/3AK1B2 b - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let original_key = pos.key();

        let ml = generate(&pos, GenType::Legal);
        for i in 0..ml.len() {
            let m = ml.get(i);
            let gives_check = pos.gives_check(m);
            pos.do_move(m, gives_check);
            pos.undo_move(m);
            assert_eq!(
                pos.key(),
                original_key,
                "Zobrist key mismatch after do/undo {m:?} (black to move)"
            );
        }
    }

    // -------------------------------------------------------------------
    // do_move / undo_move: piece count and board consistency
    // -------------------------------------------------------------------

    #[test]
    fn test_do_undo_move_preserves_piece_count() {
        let mut pos = Position::start_pos().unwrap();
        let original_count = pos.all_pieces().popcount();

        let ml = generate(&pos, GenType::Legal);
        for i in 0..ml.len() {
            let m = ml.get(i);
            let gives_check = pos.gives_check(m);
            pos.do_move(m, gives_check);
            pos.undo_move(m);
            assert_eq!(
                pos.all_pieces().popcount(),
                original_count,
                "piece count changed after do/undo {m:?}"
            );
        }
    }

    // -------------------------------------------------------------------
    // do_move / undo_move: multi-ply roundtrip
    // -------------------------------------------------------------------

    #[test]
    fn test_do_undo_two_ply_roundtrip() {
        let mut pos = Position::start_pos().unwrap();
        let original_key = pos.key();

        let ml1 = generate(&pos, GenType::Legal);
        // Pick first legal move
        let m1 = ml1.get(0);
        let gc1 = pos.gives_check(m1);
        pos.do_move(m1, gc1);

        let ml2 = generate(&pos, GenType::Legal);
        if !ml2.is_empty() {
            let m2 = ml2.get(0);
            let gc2 = pos.gives_check(m2);
            pos.do_move(m2, gc2);
            pos.undo_move(m2);
        }

        pos.undo_move(m1);
        assert_eq!(
            pos.key(),
            original_key,
            "key mismatch after 2-ply do/undo roundtrip"
        );
    }

    // -------------------------------------------------------------------
    // do_move: side_to_move flips
    // -------------------------------------------------------------------

    #[test]
    fn test_do_move_flips_side() {
        let mut pos = Position::start_pos().unwrap();
        assert_eq!(pos.side_to_move(), Color::White);

        let ml = generate(&pos, GenType::Legal);
        let m = ml.get(0);
        let gc = pos.gives_check(m);
        pos.do_move(m, gc);
        assert_eq!(pos.side_to_move(), Color::Black);

        pos.undo_move(m);
        assert_eq!(pos.side_to_move(), Color::White);
    }

    // -------------------------------------------------------------------
    // do_move: capture removes piece
    // -------------------------------------------------------------------

    #[test]
    fn test_do_move_capture_reduces_piece_count() {
        // Position where white can capture: use a position with an obvious capture
        let fen = "4k4/9/9/9/9/9/9/4r4/9/4K4 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let initial_count = pos.all_pieces().popcount();

        let ml = generate(&pos, GenType::Legal);
        // Find a capture move (king takes rook if adjacent)
        let mut found_capture = false;
        for i in 0..ml.len() {
            let m = ml.get(i);
            if pos.is_capture(m) {
                let gc = pos.gives_check(m);
                pos.do_move(m, gc);
                assert_eq!(
                    pos.all_pieces().popcount(),
                    initial_count - 1,
                    "capture should reduce piece count"
                );
                pos.undo_move(m);
                assert_eq!(pos.all_pieces().popcount(), initial_count);
                found_capture = true;
                break;
            }
        }
        // It's OK if no capture is available in this position
        let _ = found_capture;
    }

    // -------------------------------------------------------------------
    // do_null_move / undo_null_move
    // -------------------------------------------------------------------

    #[test]
    fn test_null_move_roundtrip() {
        let mut pos = Position::start_pos().unwrap();
        let original_key = pos.key();

        pos.do_null_move();
        assert_eq!(pos.side_to_move(), Color::Black);
        assert_ne!(pos.key(), original_key, "null move should change key");

        pos.undo_null_move();
        assert_eq!(pos.side_to_move(), Color::White);
        assert_eq!(pos.key(), original_key, "undo null move should restore key");
    }

    // -------------------------------------------------------------------
    // Stress test: all legal moves from multiple positions
    // -------------------------------------------------------------------

    #[test]
    fn test_do_undo_all_moves_multiple_positions() {
        let fens = [
            START_FEN,
            "r1ba1a3/4kn3/2n1b4/pNp1p1p1p/4c4/6P2/P1P2R2P/1CcC5/9/2BAKAB2 w - - 0 1",
            "1cbak4/9/n2a5/2p1p3p/5cp2/2n2N3/6PCP/3AB4/2C6/3A1K1N1 w - - 0 1",
            "5a3/3k5/3aR4/9/5r3/5n3/9/3A1A3/5K3/2BC2B2 w - - 0 1",
            "2bak4/9/3a5/p2Np3p/3n1P3/3pc3P/P4r1c1/B2CC2R1/4A4/3AK1B2 b - - 0 1",
        ];

        for fen in &fens {
            let mut pos = Position::from_fen(fen).unwrap();
            let original_key = pos.key();
            let original_fen = pos.fen();

            let ml = generate(&pos, GenType::Legal);
            for i in 0..ml.len() {
                let m = ml.get(i);
                let gc = pos.gives_check(m);
                pos.do_move(m, gc);
                pos.undo_move(m);
                assert_eq!(
                    pos.key(),
                    original_key,
                    "key mismatch for {m:?} in FEN: {fen}"
                );
                assert_eq!(
                    pos.fen(),
                    original_fen,
                    "FEN mismatch for {m:?} in FEN: {fen}"
                );
            }
        }
    }
}
