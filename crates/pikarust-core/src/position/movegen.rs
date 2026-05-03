use crate::bitboard::{
    Bitboard, HALF_BB, attacks_bb_bishop, attacks_bb_cannon, attacks_bb_knight, attacks_bb_rook,
    between_bb, line_bb, pawn_attacks_bb,
};
use crate::types::{Color, MAX_MOVES, Move, PieceType};

use super::position::Position;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GenType {
    Captures,
    Quiets,
    Evasions,
    PseudoLegal,
    Legal,
}

pub struct MoveList {
    moves: [Move; MAX_MOVES],
    len: usize,
}

impl MoveList {
    pub const fn new() -> Self {
        Self {
            moves: [Move::NONE; MAX_MOVES],
            len: 0,
        }
    }

    #[inline]
    pub fn push(&mut self, m: Move) {
        debug_assert!(self.len < MAX_MOVES);
        self.moves[self.len] = m;
        self.len += 1;
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub const fn get(&self, idx: usize) -> Move {
        self.moves[idx]
    }

    pub fn contains(&self, m: Move) -> bool {
        self.moves[..self.len].contains(&m)
    }

    pub fn as_slice(&self) -> &[Move] {
        &self.moves[..self.len]
    }

    pub const fn remove_at(&mut self, idx: usize) {
        self.len -= 1;
        self.moves[idx] = self.moves[self.len];
    }
}

impl Default for MoveList {
    fn default() -> Self {
        Self::new()
    }
}

fn generate_piece_moves(
    pos: &Position,
    moves: &mut MoveList,
    us: Color,
    pt: PieceType,
    target: Bitboard,
    gen_type: GenType,
) {
    let occupied = pos.all_pieces();
    let mut bb = pos.pieces(us, pt);

    while bb.is_not_empty() {
        let from = bb.pop_lsb();
        let mut attacks;

        match pt {
            PieceType::Cannon => {
                attacks = Bitboard::EMPTY;
                if gen_type != GenType::Quiets {
                    attacks |= attacks_bb_cannon(from, occupied) & pos.pieces_by_color(!us);
                }
                if gen_type != GenType::Captures {
                    attacks |= attacks_bb_rook(from, occupied) & !occupied;
                }
                if gen_type == GenType::Evasions {
                    attacks &= target;
                }
            }
            PieceType::Pawn => {
                attacks = pawn_attacks_bb(us, from) & target;
            }
            PieceType::Rook => {
                attacks = attacks_bb_rook(from, occupied) & target;
            }
            PieceType::Knight => {
                attacks = attacks_bb_knight(from, occupied) & target;
            }
            PieceType::Bishop => {
                attacks = attacks_bb_bishop(from, occupied) & HALF_BB[us] & target;
            }
            PieceType::Advisor => {
                attacks = pos.pseudo_attacks_advisor(from) & target;
            }
            PieceType::King => {
                attacks = pos.pseudo_attacks_king(from) & target;
            }
        }

        while attacks.is_not_empty() {
            let to = attacks.pop_lsb();
            moves.push(Move::make(from, to));
        }
    }
}

fn generate_non_king_moves(
    pos: &Position,
    moves: &mut MoveList,
    us: Color,
    target: Bitboard,
    gen_type: GenType,
) {
    for pt in [
        PieceType::Pawn,
        PieceType::Bishop,
        PieceType::Advisor,
        PieceType::Knight,
        PieceType::Cannon,
        PieceType::Rook,
    ] {
        generate_piece_moves(pos, moves, us, pt, target, gen_type);
    }
}

fn generate_all(pos: &Position, moves: &mut MoveList, us: Color, gen_type: GenType) {
    let target = match gen_type {
        GenType::PseudoLegal => !pos.pieces_by_color(us),
        GenType::Captures => pos.pieces_by_color(!us),
        GenType::Quiets => !pos.all_pieces(),
        _ => Bitboard::EMPTY,
    };

    generate_non_king_moves(pos, moves, us, target, gen_type);

    if gen_type != GenType::Evasions {
        let ksq = pos.king_square(us);
        let mut b = pos.pseudo_attacks_king(ksq) & target;
        while b.is_not_empty() {
            let to = b.pop_lsb();
            moves.push(Move::make(ksq, to));
        }
    }
}

fn generate_evasions(pos: &Position, moves: &mut MoveList) {
    let us = pos.side_to_move();
    let ksq = pos.king_square(us);
    let checkers = pos.checkers();

    if checkers.more_than_one() {
        return generate_all(pos, moves, us, GenType::PseudoLegal);
    }

    let checksq = checkers.lsb();
    let pt = pos.piece_on(checksq).piece_type();

    let target = (between_bb(ksq, checksq)) & !pos.pieces_by_color(us);
    generate_non_king_moves(pos, moves, us, target, GenType::Evasions);

    let mut b = pos.pseudo_attacks_king(ksq) & !pos.pieces_by_color(us);
    if pt == PieceType::Rook || pt == PieceType::Cannon {
        b &= !line_bb(checksq, ksq) | pos.pieces_by_color(!us);
    }
    while b.is_not_empty() {
        let to = b.pop_lsb();
        moves.push(Move::make(ksq, to));
    }

    if pt == PieceType::Cannon {
        let hurdle = between_bb(ksq, checksq) & pos.pieces_by_color(us);
        if hurdle.is_not_empty() {
            let hurdle_sq = hurdle.lsb();
            let hurdle_pt = pos.piece_on(hurdle_sq).piece_type();
            let occupied = pos.all_pieces();
            let not_line = !line_bb(checksq, hurdle_sq);

            let hurdle_moves = match hurdle_pt {
                PieceType::Pawn => {
                    pawn_attacks_bb(us, hurdle_sq) & not_line & !pos.pieces_by_color(us)
                }
                PieceType::Cannon => {
                    (attacks_bb_rook(hurdle_sq, occupied) & not_line & !occupied)
                        | (attacks_bb_cannon(hurdle_sq, occupied) & pos.pieces_by_color(!us))
                }
                _ => {
                    let att = match hurdle_pt {
                        PieceType::Rook => attacks_bb_rook(hurdle_sq, occupied),
                        PieceType::Knight => attacks_bb_knight(hurdle_sq, occupied),
                        PieceType::Bishop => {
                            attacks_bb_bishop(hurdle_sq, occupied) & HALF_BB[us]
                        }
                        PieceType::Advisor => pos.pseudo_attacks_advisor(hurdle_sq),
                        _ => Bitboard::EMPTY,
                    };
                    att & not_line & !pos.pieces_by_color(us)
                }
            };

            let mut hm = hurdle_moves;
            while hm.is_not_empty() {
                let to = hm.pop_lsb();
                moves.push(Move::make(hurdle_sq, to));
            }
        }
    }
}

pub fn generate(pos: &Position, gen_type: GenType) -> MoveList {
    let mut moves = MoveList::new();

    match gen_type {
        GenType::Legal => {
            if pos.checkers().is_not_empty() {
                generate_evasions(pos, &mut moves);
            } else {
                generate_all(pos, &mut moves, pos.side_to_move(), GenType::PseudoLegal);
            }
            let mut i = 0;
            while i < moves.len() {
                if pos.is_legal(moves.get(i)) {
                    i += 1;
                } else {
                    moves.remove_at(i);
                }
            }
        }
        GenType::Evasions => {
            generate_evasions(pos, &mut moves);
        }
        _ => {
            generate_all(pos, &mut moves, pos.side_to_move(), gen_type);
        }
    }

    moves
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::Position;

    #[test]
    fn test_start_pos_legal_moves() {
        let pos = Position::start_pos().expect("start_pos should parse");
        let moves = generate(&pos, GenType::Legal);
        assert!(!moves.is_empty(), "start position should have legal moves");
        assert_eq!(moves.len(), 44, "start position should have 44 legal moves");
    }

    #[test]
    fn test_start_pos_pseudo_legal_moves() {
        let pos = Position::start_pos().expect("start_pos should parse");
        let moves = generate(&pos, GenType::PseudoLegal);
        assert!(moves.len() >= 44);
    }

    #[test]
    fn test_kings_only_position() {
        let pos = Position::from_fen("4k4/9/9/9/9/9/9/9/9/4K4 w - - 0 1").expect("should parse");
        let moves = generate(&pos, GenType::Legal);
        assert!(!moves.is_empty());
    }
}
