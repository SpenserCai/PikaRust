use std::sync::LazyLock;

use crate::bitboard::{Bitboard, PseudoAttacksTable, attacks_bb_cannon, pawn_attacks_bb};
use crate::position::Position;
use crate::types::{Color, Piece, PieceType, Square};

use super::IndexList;
use super::half_ka_v2_hm::{ALL_PIECES, INDEX_MAP, KING_BUCKETS, VALID_BB, requires_mid_mirror};

pub const HASH_VALUE: u32 = 0x8f23_4cb8;
pub const DIMENSIONS: u32 = 45_547;
pub const MAX_ACTIVE_DIMENSIONS: usize = 64;

const VALID_PAIRS: [[bool; Piece::NUM]; Piece::NUM] = compute_valid_pairs();

const fn compute_valid_pairs() -> [[bool; Piece::NUM]; Piece::NUM] {
    #[allow(clippy::zero_prefixed_literal)]
    let raw: [[u8; Piece::NUM]; Piece::NUM] = [
        //    _  R  A  C  P  N  B  K  _  r  a  c  p  n  b  k
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // _  (0)
        [0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 0], // R  (1)
        [0, 1, 1, 1, 0, 1, 0, 0, 0, 1, 0, 1, 1, 1, 0, 0], // A  (2)
        [0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 0], // C  (3)
        [0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 1, 1, 1, 1, 1, 0], // P  (4)
        [0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 0], // N  (5)
        [0, 1, 0, 1, 1, 1, 1, 1, 0, 1, 0, 1, 1, 1, 0, 0], // B  (6)
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // K  (7)
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], // _  (8)
        [0, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1], // r  (9)
        [0, 1, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 1, 0, 1], // a  (10)
        [0, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1], // c  (11)
        [0, 0, 1, 1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 0], // p  (12)
        [0, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1], // n  (13)
        [0, 1, 0, 1, 1, 1, 0, 0, 0, 1, 0, 1, 1, 1, 1, 1], // b  (14)
        [0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 1, 1, 0, 1, 1, 0], // k  (15)
    ];
    let mut result = [[false; Piece::NUM]; Piece::NUM];
    let mut i = 0;
    while i < Piece::NUM {
        let mut j = 0;
        while j < Piece::NUM {
            result[i][j] = raw[i][j] != 0;
            j += 1;
        }
        i += 1;
    }
    result
}

struct ThreatOffsetTable {
    data: Box<[[[[u16; Piece::NUM]; Square::NUM]; Square::NUM]; Piece::NUM]>,
}

impl ThreatOffsetTable {
    fn new() -> Self {
        let pseudo_attacks = crate::bitboard::pseudo_attacks();
        let valid_bb = &*VALID_BB;

        #[allow(clippy::large_stack_arrays)]
        let mut data =
            vec![[[[DIMENSIONS as u16; Piece::NUM]; Square::NUM]; Square::NUM]; Piece::NUM]
                .into_boxed_slice();
        let data_ptr: *mut [[[u16; Piece::NUM]; Square::NUM]; Square::NUM] = data.as_mut_ptr();

        let mut cumulative_offset: u16 = 0;

        for &attacker in &ALL_PIECES {
            let pt = attacker.piece_type();
            let attacker_color = attacker.color();

            for from_idx in 0..Square::NUM {
                let from = Square::from_raw_unchecked(from_idx as u8);
                if !valid_bb[attacker.index()].contains(from) {
                    continue;
                }

                let attacks = if pt == PieceType::Pawn {
                    pawn_attacks_bb(attacker_color, from)
                } else if pt == PieceType::Cannon {
                    let king_attacks = pseudo_attacks.unconstrained_king(from);
                    attacks_bb_cannon(from, king_attacks)
                } else {
                    pseudo_attacks.get(pt, from)
                };

                for &attacked in &ALL_PIECES {
                    if !VALID_PAIRS[attacker.index()][attacked.index()] {
                        continue;
                    }

                    let targets = attacks & valid_bb[attacked.index()];
                    let mut targets_iter = targets;

                    while targets_iter.is_not_empty() {
                        let to = targets_iter.pop_lsb();

                        let enemy = attacker_color != attacked.color();
                        let same_file = from.file() == to.file();
                        let same_rank = from.rank() == to.rank();
                        let semi_excluded = pt == attacked.piece_type()
                            && (pt != PieceType::Pawn
                                || (enemy && same_file)
                                || (!enemy && same_rank))
                            && pt != PieceType::Knight;

                        if !semi_excluded || from.raw() > to.raw() {
                            // SAFETY: indices are within bounds
                            unsafe {
                                (*data_ptr.add(attacker.index()))[from_idx][to.index()]
                                    [attacked.index()] = cumulative_offset;
                            }
                            cumulative_offset += 1;
                        }
                    }
                }
            }
        }

        debug_assert!(
            u32::from(cumulative_offset) <= DIMENSIONS,
            "threat offset overflow: {cumulative_offset} > {DIMENSIONS}"
        );

        Self {
            // SAFETY: Box<[_; N]> from vec![...; N].into_boxed_slice() has the same layout
            // as Box<[[[[u16; P]; S]; S]; P]> because the element types and count match.
            data: unsafe { Box::from_raw(Box::into_raw(data).cast()) },
        }
    }

    #[inline]
    fn get(&self, attacker: Piece, from: Square, to: Square, victim: Piece) -> u16 {
        self.data[attacker.index()][from.index()][to.index()][victim.index()]
    }
}

// SAFETY: ThreatOffsetTable is read-only after initialization
unsafe impl Sync for ThreatOffsetTable {}
unsafe impl Send for ThreatOffsetTable {}

static THREAT_OFFSETS: LazyLock<ThreatOffsetTable> = LazyLock::new(ThreatOffsetTable::new);

fn make_index(
    perspective: Color,
    attacker: Piece,
    from: Square,
    to: Square,
    victim: Piece,
    mirror: bool,
) -> u32 {
    let mirror_idx = usize::from(mirror);
    let is_black_idx = usize::from(perspective == Color::Black);
    let mapped_from = INDEX_MAP[mirror_idx][is_black_idx][from.index()];
    let mapped_to = INDEX_MAP[mirror_idx][is_black_idx][to.index()];

    let (mapped_attacker, mapped_victim) = if perspective == Color::Black {
        (attacker.flip_color(), victim.flip_color())
    } else {
        (attacker, victim)
    };

    u32::from(THREAT_OFFSETS.get(
        mapped_attacker,
        Square::from_raw_unchecked(mapped_from),
        Square::from_raw_unchecked(mapped_to),
        mapped_victim,
    ))
}

pub fn append_active_indices(pos: &Position, perspective: Color, active: &mut IndexList) {
    let ksq = pos.king_square(perspective);
    let oksq = pos.king_square(!perspective);
    let mid_mirror = requires_mid_mirror(pos, perspective);
    let entry = &KING_BUCKETS[ksq.index()][oksq.index()][usize::from(mid_mirror)];
    let mirror = entry.mirror;
    let occupied = pos.all_pieces();

    let pseudo_attacks = crate::bitboard::pseudo_attacks();

    let mut bb = occupied;
    while bb.is_not_empty() {
        let from = bb.pop_lsb();
        let src_piece = pos.piece_on(from);
        if src_piece == Piece::NONE {
            continue;
        }
        let pt = src_piece.piece_type();
        let c = src_piece.color();

        let attacks = if pt == PieceType::Pawn {
            pawn_attacks_bb(c, from)
        } else {
            attacks_bb_with_occ(pseudo_attacks, pt, from, occupied)
        };

        let mut attack_bb = attacks & occupied;
        while attack_bb.is_not_empty() {
            let to = attack_bb.pop_lsb();
            let victim = pos.piece_on(to);
            let index = make_index(perspective, src_piece, from, to, victim, mirror);
            active.push_if_lt(index, DIMENSIONS);
        }
    }
}

#[inline]
fn attacks_bb_with_occ(
    pseudo: &PseudoAttacksTable,
    pt: PieceType,
    sq: Square,
    occupied: Bitboard,
) -> Bitboard {
    use crate::bitboard::{attacks_bb_bishop, attacks_bb_knight, attacks_bb_rook};
    match pt {
        PieceType::Rook => attacks_bb_rook(sq, occupied),
        PieceType::Cannon => attacks_bb_cannon(sq, occupied),
        PieceType::Knight => attacks_bb_knight(sq, occupied),
        PieceType::Bishop => attacks_bb_bishop(sq, occupied),
        PieceType::King => pseudo.get(PieceType::King, sq),
        PieceType::Advisor => pseudo.get(PieceType::Advisor, sq),
        PieceType::Pawn => unreachable!(),
    }
}

pub fn append_changed_indices(
    perspective: Color,
    mirror: bool,
    dirty: &crate::nnue::accumulator::DirtyThreats,
    removed: &mut IndexList,
    added: &mut IndexList,
) {
    for dt in dirty.as_slice() {
        let attacker = Piece::from_raw(dt.pc_raw());
        let victim = Piece::from_raw(dt.threatened_pc_raw());
        let from = Square::from_raw_unchecked(dt.pc_sq_raw());
        let to = Square::from_raw_unchecked(dt.threatened_sq_raw());
        let index = make_index(perspective, attacker, from, to, victim, mirror);
        if dt.is_add() {
            added.push_if_lt(index, DIMENSIONS);
        } else {
            removed.push_if_lt(index, DIMENSIONS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dimensions() {
        assert_eq!(DIMENSIONS, 45_547);
    }

    #[test]
    fn test_valid_pairs_symmetric_properties() {
        assert!(VALID_PAIRS[Piece::W_ROOK.index()][Piece::B_ROOK.index()]);
        assert!(VALID_PAIRS[Piece::W_ROOK.index()][Piece::W_KING.index()]);
        assert!(!VALID_PAIRS[Piece::W_KING.index()][Piece::W_ROOK.index()]);
        assert!(!VALID_PAIRS[Piece::NONE.index()][Piece::W_ROOK.index()]);
    }

    #[test]
    fn test_threat_offsets_initialized() {
        let _ = &*THREAT_OFFSETS;
    }

    #[test]
    fn test_threat_offset_invalid_returns_dimensions() {
        let offset = THREAT_OFFSETS.get(Piece::NONE, Square::SQ_A0, Square::SQ_A1, Piece::W_ROOK);
        assert_eq!(u32::from(offset), DIMENSIONS);
    }
}
