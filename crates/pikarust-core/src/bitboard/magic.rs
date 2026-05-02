use std::sync::LazyLock;

use crate::types::Square;

use super::Bitboard;
use super::attacks::{
    lame_leaper_attack_bishop, lame_leaper_attack_knight, lame_leaper_attack_knight_to,
    lame_leaper_path_bishop, lame_leaper_path_knight, lame_leaper_path_knight_to,
    sliding_attack_cannon, sliding_attack_rook,
};
use super::magic_numbers::{BISHOP_MAGICS, KNIGHT_MAGICS, KNIGHT_TO_MAGICS, ROOK_MAGICS};
use super::tables::{FILE_A_BB, FILE_I_BB, RANK_0_BB, RANK_9_BB, file_bb_of, rank_bb_of};

struct Magic {
    mask: Bitboard,
    multiplier: u128,
    shift: u32,
    offset: u32,
}

impl Magic {
    #[inline]
    fn index(&self, occupied: Bitboard) -> usize {
        let masked = (occupied & self.mask).raw();
        let product = masked.wrapping_mul(self.multiplier);
        (product >> self.shift) as usize + self.offset as usize
    }
}

struct MagicTables {
    rook_magics: [Magic; Square::NUM],
    cannon_magics: [Magic; Square::NUM],
    bishop_magics: [Magic; Square::NUM],
    knight_magics: [Magic; Square::NUM],
    knight_to_magics: [Magic; Square::NUM],
    rook_table: Vec<Bitboard>,
    cannon_table: Vec<Bitboard>,
    bishop_table: Vec<Bitboard>,
    knight_table: Vec<Bitboard>,
    knight_to_table: Vec<Bitboard>,
}

#[derive(Clone, Copy)]
enum PieceKind {
    Rook,
    Cannon,
    Bishop,
    Knight,
    KnightTo,
}

fn compute_mask(sq: Square, kind: PieceKind, rook_masks: &[Bitboard; Square::NUM]) -> Bitboard {
    let edges =
        (RANK_0_BB | RANK_9_BB) & !rank_bb_of(sq) | (FILE_A_BB | FILE_I_BB) & !file_bb_of(sq);

    let raw_mask = match kind {
        PieceKind::Rook => sliding_attack_rook(sq, Bitboard::EMPTY),
        PieceKind::Cannon => rook_masks[sq.index()],
        PieceKind::Bishop => lame_leaper_path_bishop(sq),
        PieceKind::Knight => lame_leaper_path_knight(sq),
        PieceKind::KnightTo => lame_leaper_path_knight_to(sq),
    };

    match kind {
        PieceKind::KnightTo => raw_mask,
        _ => raw_mask & !edges,
    }
}

fn compute_attack(sq: Square, occupied: Bitboard, kind: PieceKind) -> Bitboard {
    match kind {
        PieceKind::Rook => sliding_attack_rook(sq, occupied),
        PieceKind::Cannon => sliding_attack_cannon(sq, occupied),
        PieceKind::Bishop => lame_leaper_attack_bishop(sq, occupied),
        PieceKind::Knight => lame_leaper_attack_knight(sq, occupied),
        PieceKind::KnightTo => lame_leaper_attack_knight_to(sq, occupied),
    }
}

const fn get_magic_number(sq: Square, kind: PieceKind) -> u128 {
    match kind {
        PieceKind::Rook | PieceKind::Cannon => ROOK_MAGICS[sq.index()],
        PieceKind::Bishop => BISHOP_MAGICS[sq.index()],
        PieceKind::Knight => KNIGHT_MAGICS[sq.index()],
        PieceKind::KnightTo => KNIGHT_TO_MAGICS[sq.index()],
    }
}

#[allow(clippy::needless_range_loop)]
fn init_magics_for_kind(
    kind: PieceKind,
    rook_masks: &[Bitboard; Square::NUM],
) -> ([Magic; Square::NUM], Vec<Bitboard>) {
    let mut total_size: usize = 0;
    let mut sizes = [0u32; Square::NUM];
    let mut masks = [Bitboard::EMPTY; Square::NUM];

    for i in 0..Square::NUM {
        let sq = Square::from_raw_unchecked(i as u8);
        let mask = compute_mask(sq, kind, rook_masks);
        masks[i] = mask;
        let bits = mask.popcount();
        sizes[i] = 1u32 << bits;
        total_size += sizes[i] as usize;
    }

    let mut table = vec![Bitboard::EMPTY; total_size];

    let mut offset: u32 = 0;
    // SAFETY: array init — we fill all 90 entries in the loop below.
    let mut magics: [Magic; Square::NUM] = std::array::from_fn(|_| Magic {
        mask: Bitboard::EMPTY,
        multiplier: 0,
        shift: 128,
        offset: 0,
    });

    for i in 0..Square::NUM {
        let sq = Square::from_raw_unchecked(i as u8);
        let mask = masks[i];
        let magic_num = get_magic_number(sq, kind);
        let shift = 128 - mask.popcount();

        magics[i] = Magic {
            mask,
            multiplier: magic_num,
            shift,
            offset,
        };

        let mut b = Bitboard::EMPTY;
        loop {
            let attacks = compute_attack(sq, b, kind);
            let idx = magics[i].index(b);
            debug_assert!(
                table[idx].is_empty() || table[idx] == attacks,
                "magic collision at sq={i}, kind={}, idx={idx}",
                match kind {
                    PieceKind::Rook => "rook",
                    PieceKind::Cannon => "cannon",
                    PieceKind::Bishop => "bishop",
                    PieceKind::Knight => "knight",
                    PieceKind::KnightTo => "knight_to",
                }
            );
            table[idx] = attacks;

            b = Bitboard((b.raw().wrapping_sub(mask.raw())) & mask.raw());
            if b.is_empty() {
                break;
            }
        }

        offset += sizes[i];
    }

    (magics, table)
}

fn init_all() -> MagicTables {
    let mut rook_masks = [Bitboard::EMPTY; Square::NUM];
    for (i, mask) in rook_masks.iter_mut().enumerate() {
        let sq = Square::from_raw_unchecked(i as u8);
        let edges =
            (RANK_0_BB | RANK_9_BB) & !rank_bb_of(sq) | (FILE_A_BB | FILE_I_BB) & !file_bb_of(sq);
        *mask = sliding_attack_rook(sq, Bitboard::EMPTY) & !edges;
    }

    let (rook_magics, rook_table) = init_magics_for_kind(PieceKind::Rook, &rook_masks);
    let (cannon_magics, cannon_table) = init_magics_for_kind(PieceKind::Cannon, &rook_masks);
    let (bishop_magics, bishop_table) = init_magics_for_kind(PieceKind::Bishop, &rook_masks);
    let (knight_magics, knight_table) = init_magics_for_kind(PieceKind::Knight, &rook_masks);
    let (knight_to_magics, knight_to_table) =
        init_magics_for_kind(PieceKind::KnightTo, &rook_masks);

    MagicTables {
        rook_magics,
        cannon_magics,
        bishop_magics,
        knight_magics,
        knight_to_magics,
        rook_table,
        cannon_table,
        bishop_table,
        knight_table,
        knight_to_table,
    }
}

static TABLES: LazyLock<MagicTables> = LazyLock::new(init_all);

#[inline]
pub fn attacks_bb_rook(sq: Square, occupied: Bitboard) -> Bitboard {
    let t = &*TABLES;
    let m = &t.rook_magics[sq.index()];
    t.rook_table[m.index(occupied)]
}

#[inline]
pub fn attacks_bb_cannon(sq: Square, occupied: Bitboard) -> Bitboard {
    let t = &*TABLES;
    let m = &t.cannon_magics[sq.index()];
    t.cannon_table[m.index(occupied)]
}

#[inline]
pub fn attacks_bb_knight(sq: Square, occupied: Bitboard) -> Bitboard {
    let t = &*TABLES;
    let m = &t.knight_magics[sq.index()];
    t.knight_table[m.index(occupied)]
}

#[inline]
pub fn attacks_bb_bishop(sq: Square, occupied: Bitboard) -> Bitboard {
    let t = &*TABLES;
    let m = &t.bishop_magics[sq.index()];
    t.bishop_table[m.index(occupied)]
}

#[inline]
pub fn attacks_bb_knight_to(sq: Square, occupied: Bitboard) -> Bitboard {
    let t = &*TABLES;
    let m = &t.knight_to_magics[sq.index()];
    t.knight_to_table[m.index(occupied)]
}

pub fn ensure_initialized() {
    LazyLock::force(&TABLES);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_occupancy(seed: u64) -> Bitboard {
        let mut state = seed;
        let mut val: u128 = 0;
        for _ in 0..2 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            val = (val << 64) | u128::from(state);
        }
        Bitboard::new(val)
    }

    #[test]
    fn test_magic_rook_matches_direct() {
        for i in 0..90u8 {
            let sq = Square::from_raw_unchecked(i);
            for seed in 0..200u64 {
                let occ = random_occupancy(seed.wrapping_mul(7919).wrapping_add(u64::from(i)));
                let expected = sliding_attack_rook(sq, occ);
                let got = attacks_bb_rook(sq, occ);
                assert_eq!(
                    expected,
                    got,
                    "rook mismatch at sq={i}, occ={:#034X}",
                    occ.raw()
                );
            }
        }
    }

    #[test]
    fn test_magic_cannon_matches_direct() {
        for i in 0..90u8 {
            let sq = Square::from_raw_unchecked(i);
            for seed in 0..200u64 {
                let occ = random_occupancy(seed.wrapping_mul(6271).wrapping_add(u64::from(i)));
                let expected = sliding_attack_cannon(sq, occ);
                let got = attacks_bb_cannon(sq, occ);
                assert_eq!(
                    expected,
                    got,
                    "cannon mismatch at sq={i}, occ={:#034X}",
                    occ.raw()
                );
            }
        }
    }

    #[test]
    fn test_magic_knight_matches_direct() {
        for i in 0..90u8 {
            let sq = Square::from_raw_unchecked(i);
            for seed in 0..200u64 {
                let occ = random_occupancy(seed.wrapping_mul(5381).wrapping_add(u64::from(i)));
                let expected = lame_leaper_attack_knight(sq, occ);
                let got = attacks_bb_knight(sq, occ);
                assert_eq!(
                    expected,
                    got,
                    "knight mismatch at sq={i}, occ={:#034X}",
                    occ.raw()
                );
            }
        }
    }

    #[test]
    fn test_magic_bishop_matches_direct() {
        for i in 0..90u8 {
            let sq = Square::from_raw_unchecked(i);
            for seed in 0..200u64 {
                let occ = random_occupancy(seed.wrapping_mul(4253).wrapping_add(u64::from(i)));
                let expected = lame_leaper_attack_bishop(sq, occ);
                let got = attacks_bb_bishop(sq, occ);
                assert_eq!(
                    expected,
                    got,
                    "bishop mismatch at sq={i}, occ={:#034X}",
                    occ.raw()
                );
            }
        }
    }

    #[test]
    fn test_magic_knight_to_matches_direct() {
        for i in 0..90u8 {
            let sq = Square::from_raw_unchecked(i);
            for seed in 0..200u64 {
                let occ = random_occupancy(seed.wrapping_mul(3571).wrapping_add(u64::from(i)));
                let expected = lame_leaper_attack_knight_to(sq, occ);
                let got = attacks_bb_knight_to(sq, occ);
                assert_eq!(
                    expected,
                    got,
                    "knight_to mismatch at sq={i}, occ={:#034X}",
                    occ.raw()
                );
            }
        }
    }

    #[test]
    fn test_magic_rook_empty_board() {
        for i in 0..90u8 {
            let sq = Square::from_raw_unchecked(i);
            let expected = sliding_attack_rook(sq, Bitboard::EMPTY);
            let got = attacks_bb_rook(sq, Bitboard::EMPTY);
            assert_eq!(expected, got, "rook empty board mismatch at sq={i}");
        }
    }

    #[test]
    fn test_magic_cannon_empty_board() {
        for i in 0..90u8 {
            let sq = Square::from_raw_unchecked(i);
            let expected = sliding_attack_cannon(sq, Bitboard::EMPTY);
            let got = attacks_bb_cannon(sq, Bitboard::EMPTY);
            assert_eq!(expected, got, "cannon empty board mismatch at sq={i}");
        }
    }

    #[test]
    fn test_magic_rook_full_board() {
        let full = Bitboard::ALL_SQUARES;
        for i in 0..90u8 {
            let sq = Square::from_raw_unchecked(i);
            let expected = sliding_attack_rook(sq, full);
            let got = attacks_bb_rook(sq, full);
            assert_eq!(expected, got, "rook full board mismatch at sq={i}");
        }
    }

    #[test]
    fn test_magic_cannon_full_board() {
        let full = Bitboard::ALL_SQUARES;
        for i in 0..90u8 {
            let sq = Square::from_raw_unchecked(i);
            let expected = sliding_attack_cannon(sq, full);
            let got = attacks_bb_cannon(sq, full);
            assert_eq!(expected, got, "cannon full board mismatch at sq={i}");
        }
    }
}
