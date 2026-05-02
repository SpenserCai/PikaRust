use crate::types::{Color, Direction, File, Rank, Square};

use super::Bitboard;

pub const FILE_NB: u8 = 9;

pub const RANK_0_BB: Bitboard = Bitboard(0x1FF);
pub const RANK_1_BB: Bitboard = Bitboard(0x1FF << FILE_NB);
pub const RANK_2_BB: Bitboard = Bitboard(0x1FF << (FILE_NB * 2));
pub const RANK_3_BB: Bitboard = Bitboard(0x1FF << (FILE_NB * 3));
pub const RANK_4_BB: Bitboard = Bitboard(0x1FF << (FILE_NB * 4));
pub const RANK_5_BB: Bitboard = Bitboard(0x1FF << (FILE_NB * 5));
pub const RANK_6_BB: Bitboard = Bitboard(0x1FF << (FILE_NB * 6));
pub const RANK_7_BB: Bitboard = Bitboard(0x1FF << (FILE_NB * 7));
pub const RANK_8_BB: Bitboard = Bitboard(0x1FF << (FILE_NB * 8));
pub const RANK_9_BB: Bitboard = Bitboard(0x1FF << (FILE_NB * 9));

pub const FILE_A_BB: Bitboard = Bitboard((0x0002_0100_u128 << 64) | 0x8040_2010_0804_0201_u128);
pub const FILE_B_BB: Bitboard = Bitboard(FILE_A_BB.0 << 1);
pub const FILE_C_BB: Bitboard = Bitboard(FILE_A_BB.0 << 2);
pub const FILE_D_BB: Bitboard = Bitboard(FILE_A_BB.0 << 3);
pub const FILE_E_BB: Bitboard = Bitboard(FILE_A_BB.0 << 4);
pub const FILE_F_BB: Bitboard = Bitboard(FILE_A_BB.0 << 5);
pub const FILE_G_BB: Bitboard = Bitboard(FILE_A_BB.0 << 6);
pub const FILE_H_BB: Bitboard = Bitboard(FILE_A_BB.0 << 7);
pub const FILE_I_BB: Bitboard = Bitboard(FILE_A_BB.0 << 8);

pub const PALACE: Bitboard = Bitboard((0x0070_381C_u128 << 64) | 0x00E0_7038_u128);

pub const HALF_BB: [Bitboard; Color::NUM] = [
    Bitboard(RANK_0_BB.0 | RANK_1_BB.0 | RANK_2_BB.0 | RANK_3_BB.0 | RANK_4_BB.0),
    Bitboard(RANK_5_BB.0 | RANK_6_BB.0 | RANK_7_BB.0 | RANK_8_BB.0 | RANK_9_BB.0),
];

const PAWN_FILE_BB: Bitboard =
    Bitboard(FILE_A_BB.0 | FILE_C_BB.0 | FILE_E_BB.0 | FILE_G_BB.0 | FILE_I_BB.0);

pub const PAWN_BB: [Bitboard; Color::NUM] = [
    Bitboard(HALF_BB[1].0 | ((RANK_3_BB.0 | RANK_4_BB.0) & PAWN_FILE_BB.0)),
    Bitboard(HALF_BB[0].0 | ((RANK_6_BB.0 | RANK_5_BB.0) & PAWN_FILE_BB.0)),
];

pub const RANK_BB: [Bitboard; Rank::NUM] = [
    RANK_0_BB, RANK_1_BB, RANK_2_BB, RANK_3_BB, RANK_4_BB, RANK_5_BB, RANK_6_BB, RANK_7_BB,
    RANK_8_BB, RANK_9_BB,
];

pub const FILE_BB: [Bitboard; File::NUM] = [
    FILE_A_BB, FILE_B_BB, FILE_C_BB, FILE_D_BB, FILE_E_BB, FILE_F_BB, FILE_G_BB, FILE_H_BB,
    FILE_I_BB,
];

pub const SQUARE_BB: [Bitboard; Square::NUM] = {
    let mut table = [Bitboard(0); Square::NUM];
    let mut i = 0u8;
    while i < 90 {
        table[i as usize] = Bitboard(1u128 << i);
        i += 1;
    }
    table
};

#[inline]
pub const fn square_bb(sq: Square) -> Bitboard {
    SQUARE_BB[sq.index()]
}

#[inline]
pub const fn rank_bb_of(sq: Square) -> Bitboard {
    RANK_BB[sq.rank() as usize]
}

#[inline]
pub const fn file_bb_of(sq: Square) -> Bitboard {
    FILE_BB[sq.file() as usize]
}

pub const fn shift(d: Direction, b: Bitboard) -> Bitboard {
    let n = Direction::NORTH.raw();
    let e = Direction::EAST.raw();
    let ne = Direction::NORTH_EAST.raw();
    let nw = Direction::NORTH_WEST.raw();
    let d_raw = d.raw();

    if d_raw == n {
        Bitboard((b.0 & !RANK_9_BB.0) << n as u8)
    } else if d_raw == -n {
        Bitboard(b.0 >> n as u8)
    } else if d_raw == 2 * n {
        Bitboard((b.0 & !RANK_9_BB.0 & !RANK_8_BB.0) << (2 * n) as u8)
    } else if d_raw == -(2 * n) {
        Bitboard(b.0 >> (2 * n) as u8)
    } else if d_raw == e {
        Bitboard((b.0 & !FILE_I_BB.0) << e as u8)
    } else if d_raw == -e {
        Bitboard((b.0 & !FILE_A_BB.0) >> e as u8)
    } else if d_raw == ne {
        Bitboard((b.0 & !FILE_I_BB.0) << ne as u8)
    } else if d_raw == nw {
        Bitboard((b.0 & !FILE_A_BB.0) << nw as u8)
    } else if d_raw == -nw {
        Bitboard((b.0 & !FILE_I_BB.0) >> nw as u8)
    } else if d_raw == -ne {
        Bitboard((b.0 & !FILE_A_BB.0) >> ne as u8)
    } else {
        Bitboard::EMPTY
    }
}

pub const SQUARE_DISTANCE: [[u8; Square::NUM]; Square::NUM] = {
    let mut table = [[0u8; Square::NUM]; Square::NUM];
    let mut s1 = 0u8;
    while s1 < 90 {
        let mut s2 = 0u8;
        while s2 < 90 {
            let f1 = s1 % 9;
            let r1 = s1 / 9;
            let f2 = s2 % 9;
            let r2 = s2 / 9;
            let df = f1.abs_diff(f2);
            let dr = r1.abs_diff(r2);
            table[s1 as usize][s2 as usize] = if df > dr { df } else { dr };
            s2 += 1;
        }
        s1 += 1;
    }
    table
};

#[inline]
pub const fn square_distance(s1: Square, s2: Square) -> u8 {
    SQUARE_DISTANCE[s1.index()][s2.index()]
}

#[inline]
pub const fn file_distance(s1: Square, s2: Square) -> u8 {
    let f1 = s1.raw() % 9;
    let f2 = s2.raw() % 9;
    f1.abs_diff(f2)
}

#[inline]
pub const fn rank_distance(s1: Square, s2: Square) -> u8 {
    let r1 = s1.raw() / 9;
    let r2 = s2.raw() / 9;
    r1.abs_diff(r2)
}

pub fn safe_destination(s: Square, step: i8) -> Bitboard {
    let to_raw = i16::from(s.raw()) + i16::from(step);
    if !(0..90).contains(&to_raw) {
        return Bitboard::EMPTY;
    }
    let to = Square::from_raw_unchecked(to_raw as u8);
    if square_distance(s, to) <= 2 {
        square_bb(to)
    } else {
        Bitboard::EMPTY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rank_bb_constants() {
        assert_eq!(RANK_0_BB.popcount(), 9);
        assert_eq!(RANK_9_BB.popcount(), 9);
        for r in &RANK_BB {
            assert_eq!(r.popcount(), 9);
        }
    }

    #[test]
    fn test_file_bb_constants() {
        assert_eq!(FILE_A_BB.popcount(), 10);
        assert_eq!(FILE_I_BB.popcount(), 10);
        for f in &FILE_BB {
            assert_eq!(f.popcount(), 10);
        }
    }

    #[test]
    fn test_palace() {
        assert_eq!(PALACE.popcount(), 18);
    }

    #[test]
    fn test_half_bb() {
        assert_eq!(HALF_BB[Color::White].popcount(), 45);
        assert_eq!(HALF_BB[Color::Black].popcount(), 45);
        assert!((HALF_BB[Color::White] & HALF_BB[Color::Black]).is_empty());
    }

    #[test]
    fn test_square_bb_table() {
        for i in 0..90u8 {
            let sq = Square::from_raw_unchecked(i);
            assert_eq!(SQUARE_BB[sq].popcount(), 1);
            assert!(SQUARE_BB[sq].contains(sq));
        }
    }

    #[test]
    fn test_shift_north() {
        let bb = Bitboard::from(Square::SQ_E4);
        let shifted = shift(Direction::NORTH, bb);
        assert!(shifted.contains(Square::SQ_E5));
        assert_eq!(shifted.popcount(), 1);
    }

    #[test]
    fn test_shift_south() {
        let bb = Bitboard::from(Square::SQ_E4);
        let shifted = shift(Direction::SOUTH, bb);
        assert!(shifted.contains(Square::SQ_E3));
    }

    #[test]
    fn test_shift_east() {
        let bb = Bitboard::from(Square::SQ_E4);
        let shifted = shift(Direction::EAST, bb);
        assert!(shifted.contains(Square::SQ_F4));
    }

    #[test]
    fn test_shift_west() {
        let bb = Bitboard::from(Square::SQ_E4);
        let shifted = shift(Direction::WEST, bb);
        assert!(shifted.contains(Square::SQ_D4));
    }

    #[test]
    fn test_shift_edge_no_wrap() {
        let bb = Bitboard::from(Square::SQ_I4);
        let shifted = shift(Direction::EAST, bb);
        assert!(shifted.is_empty());

        let bb = Bitboard::from(Square::SQ_A4);
        let shifted = shift(Direction::WEST, bb);
        assert!(shifted.is_empty());

        let bb = RANK_9_BB;
        let shifted = shift(Direction::NORTH, bb);
        assert!(shifted.is_empty());
    }

    #[test]
    fn test_square_distance_same() {
        assert_eq!(square_distance(Square::SQ_E4, Square::SQ_E4), 0);
    }

    #[test]
    fn test_square_distance_adjacent() {
        assert_eq!(square_distance(Square::SQ_E4, Square::SQ_E5), 1);
        assert_eq!(square_distance(Square::SQ_E4, Square::SQ_F4), 1);
        assert_eq!(square_distance(Square::SQ_E4, Square::SQ_F5), 1);
    }

    #[test]
    fn test_square_distance_corners() {
        assert_eq!(square_distance(Square::SQ_A0, Square::SQ_I9), 9);
    }

    #[test]
    fn test_pawn_bb() {
        assert!(PAWN_BB[Color::White].popcount() > 45);
        assert!(PAWN_BB[Color::Black].popcount() > 45);
    }

    #[test]
    fn test_safe_destination_valid() {
        let bb = safe_destination(Square::SQ_E4, Direction::NORTH.raw());
        assert!(bb.contains(Square::SQ_E5));
    }

    #[test]
    fn test_safe_destination_off_board() {
        let bb = safe_destination(Square::SQ_A0, Direction::SOUTH.raw());
        assert!(bb.is_empty());
    }
}
