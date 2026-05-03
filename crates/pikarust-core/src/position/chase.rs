use crate::bitboard::{
    Bitboard, HALF_BB, attacks_bb_bishop, attacks_bb_cannon, attacks_bb_knight, attacks_bb_rook,
    between_bb, pawn_attacks_bb, square_bb,
};
use crate::types::{Color, Move, Piece, PieceType, Square, VALUE_DRAW, Value, mate_in, mated_in};

use super::position::Position;
use super::state::StateInfo;

struct PositionSnapshot {
    board: [Piece; Square::NUM],
    by_type_bb: [Bitboard; PieceType::PIECE_TYPE_NB],
    by_color_bb: [Bitboard; Color::NUM],
    piece_count: [u8; Piece::NUM],
    side_to_move: Color,
    game_ply: u16,
    state: StateInfo,
}

impl Position {
    fn save_snapshot(&self) -> PositionSnapshot {
        PositionSnapshot {
            board: self.board,
            by_type_bb: self.by_type_bb,
            by_color_bb: self.by_color_bb,
            piece_count: self.piece_count,
            side_to_move: self.side_to_move,
            game_ply: self.game_ply,
            state: self.state.clone(),
        }
    }

    #[allow(clippy::missing_const_for_fn)]
    fn restore_snapshot(&mut self, snap: PositionSnapshot) {
        self.board = snap.board;
        self.by_type_bb = snap.by_type_bb;
        self.by_color_bb = snap.by_color_bb;
        self.piece_count = snap.piece_count;
        self.side_to_move = snap.side_to_move;
        self.game_ply = snap.game_ply;
        self.state = snap.state;
    }

    fn attacks_bb_by_type(&self, pt: PieceType, sq: Square) -> Bitboard {
        let occupied = self.all_pieces();
        match pt {
            PieceType::Rook => attacks_bb_rook(sq, occupied),
            PieceType::Cannon => attacks_bb_cannon(sq, occupied),
            PieceType::Knight => attacks_bb_knight(sq, occupied),
            PieceType::Bishop => attacks_bb_bishop(sq, occupied),
            PieceType::Advisor => self.pseudo_attacks_advisor(sq),
            PieceType::King => self.pseudo_attacks_king(sq),
            PieceType::Pawn => pawn_attacks_bb(self.side_to_move, sq),
        }
    }

    fn chase_legal(&self, m: Move) -> bool {
        let us = self.side_to_move;
        let from = m.from_sq();
        let to = m.to_sq();
        let occupied = (self.all_pieces() ^ from) | to;

        if self.piece_on(from).piece_type() == PieceType::King {
            return self.checkers_to(!us, to, occupied).is_empty();
        }

        (self.checkers_to(!us, self.king_square(us), occupied) & !square_bb(to)).is_empty()
    }

    pub(crate) fn chased(&mut self, c: Color) -> u16 {
        let mut chase: u16 = 0;

        let original_stm = self.side_to_move;
        self.side_to_move = c;

        let mut attackers = self.pieces_by_color(self.side_to_move)
            ^ self.pieces_multi(self.side_to_move, &[PieceType::King, PieceType::Pawn]);

        while attackers.is_not_empty() {
            let from = attackers.pop_lsb();
            let attacker_type = self.piece_on(from).piece_type();
            let mut attacks = self.attacks_bb_by_type(attacker_type, from);

            if (self.blockers_for_king(self.side_to_move) & from).is_not_empty() {
                attacks &= self.pinners(!self.side_to_move) & !self.pieces_by_type(PieceType::King);
            } else {
                attacks &= (self.pieces_by_color(!self.side_to_move)
                    ^ self.pieces_multi(!self.side_to_move, &[PieceType::King, PieceType::Pawn]))
                    | (self.pieces(!self.side_to_move, PieceType::Pawn)
                        & HALF_BB[self.side_to_move]);
            }

            while attacks.is_not_empty() {
                let to = attacks.pop_lsb();
                let m = Move::make(from, to);

                if !self.chase_legal(m) {
                    continue;
                }

                let target_type = self.piece_on(to).piece_type();

                if (attacker_type == PieceType::Knight || attacker_type == PieceType::Cannon)
                    && target_type == PieceType::Rook
                {
                    chase |= 1 << self.id_board[to];
                    continue;
                }
                if (attacker_type == PieceType::Advisor || attacker_type == PieceType::Bishop)
                    && (target_type as u8 & 1) != 0
                {
                    chase |= 1 << self.id_board[to];
                    continue;
                }

                let mut true_chase = true;
                let (captured, id) = self.do_move_for_chase(m);
                self.debug_check_consistency("after_do_move_for_chase");
                let mut recaptures =
                    self.attackers_to_simple(to) & self.pieces_by_color(self.side_to_move);
                while recaptures.is_not_empty() {
                    let s = recaptures.pop_lsb();
                    if self.chase_legal(Move::make(s, to)) {
                        true_chase = false;
                        break;
                    }
                }
                self.undo_move_for_chase(m, captured, id);
                self.debug_check_consistency("after_undo_move_for_chase");

                if true_chase {
                    if attacker_type == target_type {
                        self.side_to_move = !self.side_to_move;
                        let blocked = attacker_type == PieceType::Knight
                            && ((between_bb(from, to) ^ to) & self.all_pieces()).is_not_empty();
                        if blocked || !self.chase_legal(Move::make(to, from)) {
                            chase |= 1 << self.id_board[to];
                        }
                        self.side_to_move = !self.side_to_move;
                    } else {
                        chase |= 1 << self.id_board[to];
                    }
                }
            }
        }

        self.side_to_move = original_stm;
        chase
    }

    pub(crate) fn detect_chases(&mut self, d: i32, ply: i32) -> Value {
        let snap = self.save_snapshot();

        let mut white_id = 0i32;
        let mut black_id = 0i32;
        for s_idx in 0..Square::NUM {
            let s = Square::from_raw_unchecked(s_idx as u8);
            if self.board[s] != Piece::NONE {
                if self.board[s].color() == Color::White {
                    self.id_board[s] = white_id;
                    white_id += 1;
                } else {
                    self.id_board[s] = black_id;
                    black_id += 1;
                }
            }
        }

        let us = self.side_to_move;
        let them = !us;

        let mut chase = [0xFFFFu16; Color::NUM];

        let mut stack_cursor = self.state_stack.len();
        let result = self.detect_chases_inner(d, ply, us, them, &mut chase, &mut stack_cursor);

        self.restore_snapshot(snap);

        result
    }

    fn detect_chases_inner(
        &mut self,
        d: i32,
        ply: i32,
        us: Color,
        them: Color,
        chase: &mut [u16; Color::NUM],
        stack_cursor: &mut usize,
    ) -> Value {
        for _i in 0..d {
            if self.state.checkers_bb.is_not_empty() {
                return VALUE_DRAW;
            }

            let stm_opponent = !self.side_to_move;

            if chase[stm_opponent] == 0 {
                if chase[self.side_to_move] == 0 {
                    break;
                }
                self.undo_move_light(stack_cursor);
            } else {
                let after = self.chased(stm_opponent);
                self.undo_move_light(stack_cursor);
                let before = self.chased(self.side_to_move);
                chase[self.side_to_move] &= after & !before;
            }
        }

        let chase_us = chase[us] != 0;
        let chase_them = chase[them] != 0;

        if chase_us ^ chase_them {
            if chase_us {
                mated_in(ply)
            } else {
                mate_in(ply)
            }
        } else {
            VALUE_DRAW
        }
    }

    fn undo_move_light(&mut self, stack_cursor: &mut usize) {
        let m = self.state.last_move;
        let captured = self.state.captured_piece;

        self.side_to_move = !self.side_to_move;

        let from = m.from_sq();
        let to = m.to_sq();

        self.move_piece(to, from);

        if captured != Piece::NONE {
            self.put_piece(captured, to);
        }

        if *stack_cursor > 0 {
            *stack_cursor -= 1;
            self.state = self.state_stack[*stack_cursor].clone();
        }
        self.game_ply = self.game_ply.saturating_sub(1);
    }
}
