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
