use std::fmt;
use std::ops::{Add, Index, IndexMut, Neg, Sub};

use thiserror::Error;

// ---------------------------------------------------------------------------
// File
// ---------------------------------------------------------------------------

/// Board column (A=0 through I=8).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum File {
    A = 0,
    B = 1,
    C = 2,
    D = 3,
    E = 4,
    F = 5,
    G = 6,
    H = 7,
    I = 8,
}

impl File {
    pub const NUM: usize = 9;
    pub const ALL: [Self; 9] = [
        Self::A,
        Self::B,
        Self::C,
        Self::D,
        Self::E,
        Self::F,
        Self::G,
        Self::H,
        Self::I,
    ];

    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }
}

impl TryFrom<u8> for File {
    type Error = SquareError;

    #[inline]
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        if v < 9 {
            // SAFETY: v is in 0..=8, matching all enum variants.
            Ok(unsafe { std::mem::transmute(v) })
        } else {
            Err(SquareError::InvalidFile(v))
        }
    }
}

impl fmt::Display for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ch = b'a' + *self as u8;
        f.write_str(std::str::from_utf8(&[ch]).unwrap_or("?"))
    }
}

// ---------------------------------------------------------------------------
// Rank
// ---------------------------------------------------------------------------

/// Board row (R0=0 through R9=9).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Rank {
    R0 = 0,
    R1 = 1,
    R2 = 2,
    R3 = 3,
    R4 = 4,
    R5 = 5,
    R6 = 6,
    R7 = 7,
    R8 = 8,
    R9 = 9,
}

impl Rank {
    pub const NUM: usize = 10;
    pub const ALL: [Self; 10] = [
        Self::R0,
        Self::R1,
        Self::R2,
        Self::R3,
        Self::R4,
        Self::R5,
        Self::R6,
        Self::R7,
        Self::R8,
        Self::R9,
    ];

    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }
}

impl TryFrom<u8> for Rank {
    type Error = SquareError;

    #[inline]
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        if v < 10 {
            // SAFETY: v is in 0..=9, matching all enum variants.
            Ok(unsafe { std::mem::transmute(v) })
        } else {
            Err(SquareError::InvalidRank(v))
        }
    }
}

impl fmt::Display for Rank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", *self as u8)
    }
}

// ---------------------------------------------------------------------------
// Direction
// ---------------------------------------------------------------------------

/// Movement direction on the 9x10 board.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Direction(pub i8);

impl Direction {
    pub const NORTH: Self = Self(9);
    pub const SOUTH: Self = Self(-9);
    pub const EAST: Self = Self(1);
    pub const WEST: Self = Self(-1);
    pub const NORTH_EAST: Self = Self(10);
    pub const NORTH_WEST: Self = Self(8);
    pub const SOUTH_EAST: Self = Self(-8);
    pub const SOUTH_WEST: Self = Self(-10);

    #[inline]
    pub const fn raw(self) -> i8 {
        self.0
    }
}

impl Neg for Direction {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Add for Direction {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

// ---------------------------------------------------------------------------
// Square
// ---------------------------------------------------------------------------

/// Error type for invalid square/file/rank conversions.
#[derive(Debug, Error)]
pub enum SquareError {
    #[error("invalid square index: {0} (expected 0..89)")]
    InvalidIndex(u8),
    #[error("invalid file value: {0} (expected 0..8)")]
    InvalidFile(u8),
    #[error("invalid rank value: {0} (expected 0..9)")]
    InvalidRank(u8),
    #[error("invalid square string: {0}")]
    InvalidString(String),
}

/// A square on the 9x10 Chinese chess board (0..89).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Square(u8);

impl Square {
    pub const NUM: usize = 90;
    pub const NONE: Self = Self(90);

    #[inline]
    pub const fn make(file: File, rank: Rank) -> Self {
        Self(rank as u8 * 9 + file as u8)
    }

    #[inline]
    pub const fn file(self) -> File {
        // SAFETY: self.0 % 9 is always in 0..=8.
        unsafe { std::mem::transmute(self.0 % 9) }
    }

    #[inline]
    pub const fn rank(self) -> Rank {
        // SAFETY: for valid squares (0..89), self.0 / 9 is in 0..=9.
        unsafe { std::mem::transmute(self.0 / 9) }
    }

    /// Vertical flip (swap rank 0 <-> rank 9).
    #[inline]
    #[must_use]
    pub const fn flip_rank(self) -> Self {
        Self::make(self.file(), unsafe { std::mem::transmute(9 - self.0 / 9) })
    }

    /// Horizontal flip (swap file A <-> file I).
    #[inline]
    #[must_use]
    pub const fn flip_file(self) -> Self {
        Self::make(unsafe { std::mem::transmute(8 - self.0 % 9) }, self.rank())
    }

    #[inline]
    pub const fn is_valid(self) -> bool {
        self.0 < 90
    }

    #[inline]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    #[inline]
    pub const fn raw(self) -> u8 {
        self.0
    }

    /// Construct a `Square` from a raw u8 without bounds checking.
    /// Caller must ensure `v < 90`.
    #[inline]
    pub const fn from_raw_unchecked(v: u8) -> Self {
        debug_assert!(v < 90, "Square::from_raw_unchecked out of range");
        Self(v)
    }
}

impl TryFrom<u8> for Square {
    type Error = SquareError;

    #[inline]
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        if v < 90 {
            Ok(Self(v))
        } else {
            Err(SquareError::InvalidIndex(v))
        }
    }
}

impl Add<Direction> for Square {
    type Output = Self;

    #[inline]
    fn add(self, d: Direction) -> Self {
        Self((i16::from(self.0) + i16::from(d.0)) as u8)
    }
}

impl Sub<Direction> for Square {
    type Output = Self;

    #[inline]
    fn sub(self, d: Direction) -> Self {
        Self((i16::from(self.0) - i16::from(d.0)) as u8)
    }
}

impl fmt::Debug for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::NONE {
            f.write_str("Square::NONE")
        } else if self.is_valid() {
            write!(f, "Square({}{})", self.file(), self.rank())
        } else {
            write!(f, "Square(invalid:{})", self.0)
        }
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_valid() {
            write!(f, "{}{}", self.file(), self.rank())
        } else {
            f.write_str("-")
        }
    }
}

impl<T> Index<Square> for [T; Square::NUM] {
    type Output = T;

    #[inline]
    fn index(&self, sq: Square) -> &T {
        &self[sq.0 as usize]
    }
}

impl<T> IndexMut<Square> for [T; Square::NUM] {
    #[inline]
    fn index_mut(&mut self, sq: Square) -> &mut T {
        &mut self[sq.0 as usize]
    }
}

// ---------------------------------------------------------------------------
// Named squares
// ---------------------------------------------------------------------------

macro_rules! define_squares {
    ($($name:ident = $val:expr),+ $(,)?) => {
        impl Square {
            $(pub const $name: Self = Self($val);)+
        }
    };
}

define_squares! {
    SQ_A0 = 0,  SQ_B0 = 1,  SQ_C0 = 2,  SQ_D0 = 3,  SQ_E0 = 4,
    SQ_F0 = 5,  SQ_G0 = 6,  SQ_H0 = 7,  SQ_I0 = 8,
    SQ_A1 = 9,  SQ_B1 = 10, SQ_C1 = 11, SQ_D1 = 12, SQ_E1 = 13,
    SQ_F1 = 14, SQ_G1 = 15, SQ_H1 = 16, SQ_I1 = 17,
    SQ_A2 = 18, SQ_B2 = 19, SQ_C2 = 20, SQ_D2 = 21, SQ_E2 = 22,
    SQ_F2 = 23, SQ_G2 = 24, SQ_H2 = 25, SQ_I2 = 26,
    SQ_A3 = 27, SQ_B3 = 28, SQ_C3 = 29, SQ_D3 = 30, SQ_E3 = 31,
    SQ_F3 = 32, SQ_G3 = 33, SQ_H3 = 34, SQ_I3 = 35,
    SQ_A4 = 36, SQ_B4 = 37, SQ_C4 = 38, SQ_D4 = 39, SQ_E4 = 40,
    SQ_F4 = 41, SQ_G4 = 42, SQ_H4 = 43, SQ_I4 = 44,
    SQ_A5 = 45, SQ_B5 = 46, SQ_C5 = 47, SQ_D5 = 48, SQ_E5 = 49,
    SQ_F5 = 50, SQ_G5 = 51, SQ_H5 = 52, SQ_I5 = 53,
    SQ_A6 = 54, SQ_B6 = 55, SQ_C6 = 56, SQ_D6 = 57, SQ_E6 = 58,
    SQ_F6 = 59, SQ_G6 = 60, SQ_H6 = 61, SQ_I6 = 62,
    SQ_A7 = 63, SQ_B7 = 64, SQ_C7 = 65, SQ_D7 = 66, SQ_E7 = 67,
    SQ_F7 = 68, SQ_G7 = 69, SQ_H7 = 70, SQ_I7 = 71,
    SQ_A8 = 72, SQ_B8 = 73, SQ_C8 = 74, SQ_D8 = 75, SQ_E8 = 76,
    SQ_F8 = 77, SQ_G8 = 78, SQ_H8 = 79, SQ_I8 = 80,
    SQ_A9 = 81, SQ_B9 = 82, SQ_C9 = 83, SQ_D9 = 84, SQ_E9 = 85,
    SQ_F9 = 86, SQ_G9 = 87, SQ_H9 = 88, SQ_I9 = 89,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_values() {
        assert_eq!(File::A as u8, 0);
        assert_eq!(File::I as u8, 8);
        assert_eq!(File::ALL.len(), File::NUM);
    }

    #[test]
    fn test_rank_values() {
        assert_eq!(Rank::R0 as u8, 0);
        assert_eq!(Rank::R9 as u8, 9);
        assert_eq!(Rank::ALL.len(), Rank::NUM);
    }

    #[test]
    fn test_file_try_from() {
        assert_eq!(File::try_from(0).unwrap(), File::A);
        assert_eq!(File::try_from(8).unwrap(), File::I);
        assert!(File::try_from(9).is_err());
    }

    #[test]
    fn test_rank_try_from() {
        assert_eq!(Rank::try_from(0).unwrap(), Rank::R0);
        assert_eq!(Rank::try_from(9).unwrap(), Rank::R9);
        assert!(Rank::try_from(10).is_err());
    }

    #[test]
    fn test_square_make() {
        assert_eq!(Square::make(File::A, Rank::R0), Square::SQ_A0);
        assert_eq!(Square::make(File::I, Rank::R9), Square::SQ_I9);
        assert_eq!(Square::make(File::E, Rank::R4), Square::SQ_E4);
    }

    #[test]
    fn test_square_file_rank() {
        let sq = Square::SQ_E4;
        assert_eq!(sq.file(), File::E);
        assert_eq!(sq.rank(), Rank::R4);

        let sq = Square::SQ_A0;
        assert_eq!(sq.file(), File::A);
        assert_eq!(sq.rank(), Rank::R0);

        let sq = Square::SQ_I9;
        assert_eq!(sq.file(), File::I);
        assert_eq!(sq.rank(), Rank::R9);
    }

    #[test]
    fn test_square_roundtrip() {
        for i in 0..90u8 {
            let sq = Square(i);
            assert_eq!(Square::make(sq.file(), sq.rank()), sq);
        }
    }

    #[test]
    fn test_square_flip_rank() {
        assert_eq!(Square::SQ_A0.flip_rank(), Square::SQ_A9);
        assert_eq!(Square::SQ_E4.flip_rank(), Square::SQ_E5);
        assert_eq!(Square::SQ_I9.flip_rank(), Square::SQ_I0);
    }

    #[test]
    fn test_square_flip_file() {
        assert_eq!(Square::SQ_A0.flip_file(), Square::SQ_I0);
        assert_eq!(Square::SQ_E4.flip_file(), Square::SQ_E4);
        assert_eq!(Square::SQ_I9.flip_file(), Square::SQ_A9);
    }

    #[test]
    fn test_square_is_valid() {
        assert!(Square::SQ_A0.is_valid());
        assert!(Square::SQ_I9.is_valid());
        assert!(!Square::NONE.is_valid());
    }

    #[test]
    fn test_square_add_direction() {
        assert_eq!(Square::SQ_E4 + Direction::NORTH, Square::SQ_E5);
        assert_eq!(Square::SQ_E4 + Direction::EAST, Square::SQ_F4);
        assert_eq!(Square::SQ_E4 + Direction::SOUTH, Square::SQ_E3);
        assert_eq!(Square::SQ_E4 + Direction::WEST, Square::SQ_D4);
    }

    #[test]
    fn test_square_sub_direction() {
        assert_eq!(Square::SQ_E4 - Direction::NORTH, Square::SQ_E3);
        assert_eq!(Square::SQ_E4 - Direction::SOUTH, Square::SQ_E5);
    }

    #[test]
    fn test_direction_neg() {
        assert_eq!(-Direction::NORTH, Direction::SOUTH);
        assert_eq!(-Direction::EAST, Direction::WEST);
    }

    #[test]
    fn test_direction_add() {
        assert_eq!(Direction::NORTH + Direction::EAST, Direction::NORTH_EAST);
        assert_eq!(Direction::SOUTH + Direction::WEST, Direction::SOUTH_WEST);
    }

    #[test]
    fn test_square_display() {
        assert_eq!(format!("{}", Square::SQ_A0), "a0");
        assert_eq!(format!("{}", Square::SQ_I9), "i9");
        assert_eq!(format!("{}", Square::SQ_E4), "e4");
        assert_eq!(format!("{}", Square::NONE), "-");
    }

    #[test]
    fn test_square_debug() {
        assert_eq!(format!("{:?}", Square::NONE), "Square::NONE");
        assert!(format!("{:?}", Square::SQ_A0).contains("a0"));
    }

    #[test]
    fn test_square_try_from() {
        assert_eq!(Square::try_from(0).unwrap(), Square::SQ_A0);
        assert_eq!(Square::try_from(89).unwrap(), Square::SQ_I9);
        assert!(Square::try_from(90).is_err());
        assert!(Square::try_from(255).is_err());
    }

    #[test]
    fn test_square_index() {
        let mut arr = [0u8; Square::NUM];
        arr[Square::SQ_A0] = 1;
        arr[Square::SQ_I9] = 2;
        assert_eq!(arr[Square::SQ_A0], 1);
        assert_eq!(arr[Square::SQ_I9], 2);
    }

    #[test]
    fn test_named_squares_count() {
        assert_eq!(Square::SQ_A0.raw(), 0);
        assert_eq!(Square::SQ_I9.raw(), 89);
        assert_eq!(Square::SQ_E0.raw(), 4);
        assert_eq!(Square::SQ_A1.raw(), 9);
    }

    #[test]
    fn test_file_display() {
        assert_eq!(format!("{}", File::A), "a");
        assert_eq!(format!("{}", File::I), "i");
    }

    #[test]
    fn test_rank_display() {
        assert_eq!(format!("{}", Rank::R0), "0");
        assert_eq!(format!("{}", Rank::R9), "9");
    }

    #[test]
    fn test_square_ordering() {
        assert!(Square::SQ_A0 < Square::SQ_B0);
        assert!(Square::SQ_A0 < Square::SQ_A1);
        assert!(Square::SQ_I9 > Square::SQ_A0);
    }

    #[test]
    fn test_direction_diagonals() {
        assert_eq!(Direction::NORTH_EAST.raw(), 10);
        assert_eq!(Direction::NORTH_WEST.raw(), 8);
        assert_eq!(Direction::SOUTH_EAST.raw(), -8);
        assert_eq!(Direction::SOUTH_WEST.raw(), -10);
    }
}
