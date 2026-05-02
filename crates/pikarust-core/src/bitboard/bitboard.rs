use std::fmt;
use std::ops::{
    BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not, Shl, ShlAssign, Shr,
    ShrAssign,
};

use crate::types::Square;

const BOARD_MASK: u128 = (1u128 << 90) - 1;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Default)]
pub struct Bitboard(pub u128);

impl Bitboard {
    pub const EMPTY: Self = Self(0);
    pub const ALL_SQUARES: Self = Self(BOARD_MASK);

    #[inline]
    pub const fn new(v: u128) -> Self {
        Self(v & BOARD_MASK)
    }

    #[inline]
    pub const fn raw(self) -> u128 {
        self.0
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn is_not_empty(self) -> bool {
        self.0 != 0
    }

    #[inline]
    pub const fn more_than_one(self) -> bool {
        self.0 & (self.0.wrapping_sub(1)) != 0
    }

    #[inline]
    pub const fn popcount(self) -> u32 {
        self.0.count_ones()
    }

    #[inline]
    pub fn lsb(self) -> Square {
        debug_assert!(self.0 != 0);
        Square::from_raw_unchecked(self.0.trailing_zeros() as u8)
    }

    #[inline]
    pub fn pop_lsb(&mut self) -> Square {
        debug_assert!(self.0 != 0);
        let sq = self.lsb();
        self.0 &= self.0 - 1;
        sq
    }

    #[inline]
    pub const fn contains(self, sq: Square) -> bool {
        self.0 & (1u128 << sq.raw()) != 0
    }
}

impl From<Square> for Bitboard {
    #[inline]
    fn from(sq: Square) -> Self {
        Self(1u128 << sq.raw())
    }
}

impl BitAnd for Bitboard {
    type Output = Self;
    #[inline]
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl BitOr for Bitboard {
    type Output = Self;
    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitXor for Bitboard {
    type Output = Self;
    #[inline]
    fn bitxor(self, rhs: Self) -> Self {
        Self(self.0 ^ rhs.0)
    }
}

impl Not for Bitboard {
    type Output = Self;
    #[inline]
    fn not(self) -> Self {
        Self(!self.0 & BOARD_MASK)
    }
}

impl Shl<u8> for Bitboard {
    type Output = Self;
    #[inline]
    fn shl(self, rhs: u8) -> Self {
        Self((self.0 << rhs) & BOARD_MASK)
    }
}

impl Shr<u8> for Bitboard {
    type Output = Self;
    #[inline]
    fn shr(self, rhs: u8) -> Self {
        Self(self.0 >> rhs)
    }
}

impl BitAndAssign for Bitboard {
    #[inline]
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl BitOrAssign for Bitboard {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitXorAssign for Bitboard {
    #[inline]
    fn bitxor_assign(&mut self, rhs: Self) {
        self.0 ^= rhs.0;
    }
}

impl ShlAssign<u8> for Bitboard {
    #[inline]
    fn shl_assign(&mut self, rhs: u8) {
        self.0 = (self.0 << rhs) & BOARD_MASK;
    }
}

impl ShrAssign<u8> for Bitboard {
    #[inline]
    fn shr_assign(&mut self, rhs: u8) {
        self.0 >>= rhs;
    }
}

impl BitAnd<Square> for Bitboard {
    type Output = Self;
    #[inline]
    fn bitand(self, sq: Square) -> Self {
        self & Self::from(sq)
    }
}

impl BitOr<Square> for Bitboard {
    type Output = Self;
    #[inline]
    fn bitor(self, sq: Square) -> Self {
        self | Self::from(sq)
    }
}

impl BitXor<Square> for Bitboard {
    type Output = Self;
    #[inline]
    fn bitxor(self, sq: Square) -> Self {
        self ^ Self::from(sq)
    }
}

impl BitOrAssign<Square> for Bitboard {
    #[inline]
    fn bitor_assign(&mut self, sq: Square) {
        *self |= Self::from(sq);
    }
}

impl BitXorAssign<Square> for Bitboard {
    #[inline]
    fn bitxor_assign(&mut self, sq: Square) {
        *self ^= Self::from(sq);
    }
}

impl BitAnd<Bitboard> for Square {
    type Output = Bitboard;
    #[inline]
    fn bitand(self, b: Bitboard) -> Bitboard {
        b & self
    }
}

impl BitOr<Bitboard> for Square {
    type Output = Bitboard;
    #[inline]
    fn bitor(self, b: Bitboard) -> Bitboard {
        b | self
    }
}

impl BitXor<Bitboard> for Square {
    type Output = Bitboard;
    #[inline]
    fn bitxor(self, b: Bitboard) -> Bitboard {
        b ^ self
    }
}

impl BitOr<Square> for Square {
    type Output = Bitboard;
    #[inline]
    fn bitor(self, rhs: Self) -> Bitboard {
        Bitboard::from(self) | Bitboard::from(rhs)
    }
}

#[allow(clippy::copy_iterator)]
impl Iterator for Bitboard {
    type Item = Square;

    #[inline]
    fn next(&mut self) -> Option<Square> {
        if self.0 == 0 {
            None
        } else {
            Some(self.pop_lsb())
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let c = self.popcount() as usize;
        (c, Some(c))
    }
}

impl fmt::Debug for Bitboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bitboard(0x{:023X})", self.0)
    }
}

impl fmt::Display for Bitboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "+---+---+---+---+---+---+---+---+---+")?;
        for r in (0..10u8).rev() {
            for file in 0..9u8 {
                let sq_idx = r * 9 + file;
                if self.0 & (1u128 << sq_idx) != 0 {
                    write!(f, "| X ")?;
                } else {
                    write!(f, "|   ")?;
                }
            }
            writeln!(f, "| {r}")?;
            writeln!(f, "+---+---+---+---+---+---+---+---+---+")?;
        }
        writeln!(f, "  a   b   c   d   e   f   g   h   i")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitboard_empty_and_all() {
        assert!(Bitboard::EMPTY.is_empty());
        assert!(!Bitboard::ALL_SQUARES.is_empty());
        assert_eq!(Bitboard::ALL_SQUARES.popcount(), 90);
    }

    #[test]
    fn test_bitboard_from_square() {
        let bb = Bitboard::from(Square::SQ_A0);
        assert_eq!(bb.0, 1);
        assert!(bb.contains(Square::SQ_A0));
        assert!(!bb.contains(Square::SQ_B0));
    }

    #[test]
    fn test_bitboard_operators() {
        let a = Bitboard::from(Square::SQ_A0);
        let b = Bitboard::from(Square::SQ_B0);
        assert_eq!((a | b).popcount(), 2);
        assert!((a & b).is_empty());
        assert_eq!((a ^ b).popcount(), 2);
        assert_eq!((!Bitboard::EMPTY).popcount(), 90);
    }

    #[test]
    fn test_bitboard_square_operators() {
        let bb = Bitboard::EMPTY | Square::SQ_E4;
        assert!(bb.contains(Square::SQ_E4));

        let bb2 = bb ^ Square::SQ_E4;
        assert!(bb2.is_empty());

        let bb3 = Square::SQ_A0 | Square::SQ_I9;
        assert_eq!(bb3.popcount(), 2);
    }

    #[test]
    fn test_bitboard_lsb_pop() {
        let mut bb = Bitboard::from(Square::SQ_A0) | Bitboard::from(Square::SQ_I9);
        assert_eq!(bb.lsb(), Square::SQ_A0);
        let s = bb.pop_lsb();
        assert_eq!(s, Square::SQ_A0);
        assert_eq!(bb.lsb(), Square::SQ_I9);
    }

    #[test]
    fn test_bitboard_more_than_one() {
        assert!(!Bitboard::EMPTY.more_than_one());
        assert!(!Bitboard::from(Square::SQ_A0).more_than_one());
        assert!((Bitboard::from(Square::SQ_A0) | Bitboard::from(Square::SQ_B0)).more_than_one());
    }

    #[test]
    fn test_bitboard_shift() {
        let bb = Bitboard::from(Square::SQ_A0);
        let shifted = bb << 1;
        assert!(shifted.contains(Square::SQ_B0));
    }

    #[test]
    fn test_bitboard_iterator() {
        let bb = Bitboard::from(Square::SQ_A0)
            | Bitboard::from(Square::SQ_E4)
            | Bitboard::from(Square::SQ_I9);
        let squares: Vec<Square> = bb.into_iter().collect();
        assert_eq!(squares.len(), 3);
        assert_eq!(squares[0], Square::SQ_A0);
        assert_eq!(squares[1], Square::SQ_E4);
        assert_eq!(squares[2], Square::SQ_I9);
    }

    #[test]
    fn test_bitboard_not_wraps() {
        let not_all = !Bitboard::ALL_SQUARES;
        assert!(not_all.is_empty());
    }

    #[test]
    fn test_bitboard_mask_high_bits() {
        let bb = Bitboard::new(u128::MAX);
        assert_eq!(bb.popcount(), 90);
    }

    #[test]
    fn test_bitboard_assign_ops() {
        let mut bb = Bitboard::EMPTY;
        bb |= Square::SQ_A0;
        assert!(bb.contains(Square::SQ_A0));
        bb ^= Square::SQ_A0;
        assert!(bb.is_empty());
        bb |= Bitboard::from(Square::SQ_E4);
        bb &= Bitboard::from(Square::SQ_E4);
        assert!(bb.contains(Square::SQ_E4));
    }

    #[test]
    fn test_bitboard_display() {
        let bb = Bitboard::from(Square::SQ_E4);
        let s = format!("{bb}");
        assert!(s.contains('X'));
    }
}
