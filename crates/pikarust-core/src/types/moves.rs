use std::fmt;

use crate::types::square::Square;

/// A move encoded as `(from << 7) | to` in a `u16`.
///
/// Bits 0-6: destination square (0..89).
/// Bits 7-13: origin square (0..89).
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Move(u16);

impl Move {
    pub const NONE: Self = Self(0);
    pub const NULL: Self = Self(129);

    #[inline]
    pub const fn make(from: Square, to: Square) -> Self {
        Self((from.raw() as u16) << 7 | to.raw() as u16)
    }

    #[inline]
    pub const fn from_sq(self) -> Square {
        Square::from_raw_unchecked(((self.0 >> 7) & 0x7F) as u8)
    }

    #[inline]
    pub const fn to_sq(self) -> Square {
        Square::from_raw_unchecked((self.0 & 0x7F) as u8)
    }

    #[inline]
    pub const fn is_ok(self) -> bool {
        self.0 != Self::NONE.0 && self.0 != Self::NULL.0
    }

    #[inline]
    pub const fn raw(self) -> u16 {
        self.0
    }

    #[inline]
    pub const fn from_raw(v: u16) -> Self {
        Self(v)
    }
}

impl fmt::Debug for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::NONE {
            f.write_str("Move::NONE")
        } else if *self == Self::NULL {
            f.write_str("Move::NULL")
        } else {
            write!(f, "Move({}{})", self.from_sq(), self.to_sq())
        }
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::NONE {
            f.write_str("0000")
        } else if *self == Self::NULL {
            f.write_str("null")
        } else {
            write!(f, "{}{}", self.from_sq(), self.to_sq())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_move_none_null() {
        assert_eq!(Move::NONE.raw(), 0);
        assert_eq!(Move::NULL.raw(), 129);
        assert!(!Move::NONE.is_ok());
        assert!(!Move::NULL.is_ok());
    }

    #[test]
    fn test_move_make_and_extract() {
        let m = Move::make(Square::SQ_E0, Square::SQ_E1);
        assert_eq!(m.from_sq(), Square::SQ_E0);
        assert_eq!(m.to_sq(), Square::SQ_E1);
        assert!(m.is_ok());
    }

    #[test]
    fn test_move_encoding() {
        let from = Square::SQ_A0;
        let to = Square::SQ_I9;
        let m = Move::make(from, to);
        assert_eq!(m.from_sq(), from);
        assert_eq!(m.to_sq(), to);
    }

    #[test]
    fn test_move_display() {
        let m = Move::make(Square::SQ_A0, Square::SQ_A1);
        assert_eq!(format!("{m}"), "a0a1");

        assert_eq!(format!("{}", Move::NONE), "0000");
        assert_eq!(format!("{}", Move::NULL), "null");
    }

    #[test]
    fn test_move_debug() {
        assert_eq!(format!("{:?}", Move::NONE), "Move::NONE");
        assert_eq!(format!("{:?}", Move::NULL), "Move::NULL");
        let m = Move::make(Square::SQ_E0, Square::SQ_E1);
        let dbg = format!("{m:?}");
        assert!(dbg.contains("e0e1"));
    }

    #[test]
    fn test_move_equality() {
        let m1 = Move::make(Square::SQ_A0, Square::SQ_B0);
        let m2 = Move::make(Square::SQ_A0, Square::SQ_B0);
        let m3 = Move::make(Square::SQ_A0, Square::SQ_C0);
        assert_eq!(m1, m2);
        assert_ne!(m1, m3);
    }

    #[test]
    fn test_move_all_squares() {
        for from_idx in 0..90u8 {
            for to_idx in 0..90u8 {
                let from = Square::from_raw_unchecked(from_idx);
                let to = Square::from_raw_unchecked(to_idx);
                let m = Move::make(from, to);
                assert_eq!(m.from_sq(), from);
                assert_eq!(m.to_sq(), to);
            }
        }
    }

    #[test]
    fn test_move_copy_clone() {
        let m = Move::make(Square::SQ_E0, Square::SQ_E1);
        let m2 = m;
        assert_eq!(m, m2);
    }
}
