use std::fmt;
use std::ops::{Index, IndexMut};

use thiserror::Error;

use crate::types::color::Color;

/// Error type for invalid piece type conversions.
#[derive(Debug, Error)]
#[error("invalid piece type value: {0} (expected 1..=7)")]
pub struct PieceTypeError(u8);

/// The seven piece types in Chinese chess.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PieceType {
    Rook = 1,
    Advisor = 2,
    Cannon = 3,
    Pawn = 4,
    Knight = 5,
    Bishop = 6,
    King = 7,
}

impl PieceType {
    /// Number of valid piece types (1..=7).
    pub const NUM: usize = 7;
    /// Array size including index 0 (used for `ALL_PIECES` bitboard).
    pub const PIECE_TYPE_NB: usize = 8;
    pub const ALL: [Self; 7] = [
        Self::Rook,
        Self::Advisor,
        Self::Cannon,
        Self::Pawn,
        Self::Knight,
        Self::Bishop,
        Self::King,
    ];

    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }
}

impl TryFrom<u8> for PieceType {
    type Error = PieceTypeError;

    #[inline]
    #[allow(unsafe_code)]
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        if (1..=7).contains(&v) {
            // SAFETY: v is in 1..=7, matching all enum variants.
            Ok(unsafe { std::mem::transmute(v) })
        } else {
            Err(PieceTypeError(v))
        }
    }
}

impl fmt::Display for PieceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Rook => "Rook",
            Self::Advisor => "Advisor",
            Self::Cannon => "Cannon",
            Self::Pawn => "Pawn",
            Self::Knight => "Knight",
            Self::Bishop => "Bishop",
            Self::King => "King",
        };
        f.write_str(s)
    }
}

/// Piece value constants aligned with Pikafish.
pub const ROOK_VALUE: i32 = 1305;
pub const ADVISOR_VALUE: i32 = 219;
pub const CANNON_VALUE: i32 = 773;
pub const PAWN_VALUE: i32 = 144;
pub const KNIGHT_VALUE: i32 = 720;
pub const BISHOP_VALUE: i32 = 187;

/// A chess piece: color + piece type encoded as `(color << 3) | piece_type`.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Piece(u8);

impl Piece {
    pub const NONE: Self = Self(0);
    /// Total number of piece values (0..15 inclusive, matching Pikafish `PIECE_NB`=16).
    pub const NUM: usize = 16;

    pub const W_ROOK: Self = Self::make(Color::White, PieceType::Rook);
    pub const W_ADVISOR: Self = Self::make(Color::White, PieceType::Advisor);
    pub const W_CANNON: Self = Self::make(Color::White, PieceType::Cannon);
    pub const W_PAWN: Self = Self::make(Color::White, PieceType::Pawn);
    pub const W_KNIGHT: Self = Self::make(Color::White, PieceType::Knight);
    pub const W_BISHOP: Self = Self::make(Color::White, PieceType::Bishop);
    pub const W_KING: Self = Self::make(Color::White, PieceType::King);

    pub const B_ROOK: Self = Self::make(Color::Black, PieceType::Rook);
    pub const B_ADVISOR: Self = Self::make(Color::Black, PieceType::Advisor);
    pub const B_CANNON: Self = Self::make(Color::Black, PieceType::Cannon);
    pub const B_PAWN: Self = Self::make(Color::Black, PieceType::Pawn);
    pub const B_KNIGHT: Self = Self::make(Color::Black, PieceType::Knight);
    pub const B_BISHOP: Self = Self::make(Color::Black, PieceType::Bishop);
    pub const B_KING: Self = Self::make(Color::Black, PieceType::King);

    #[inline]
    pub const fn make(color: Color, pt: PieceType) -> Self {
        Self((color as u8) << 3 | pt as u8)
    }

    #[inline]
    #[allow(unsafe_code)]
    pub const fn color(self) -> Color {
        debug_assert!(self.0 != 0, "called color() on Piece::NONE");
        // SAFETY: bit 3 is 0 or 1 for valid pieces.
        unsafe { std::mem::transmute(self.0 >> 3) }
    }

    #[inline]
    #[allow(unsafe_code)]
    pub const fn piece_type(self) -> PieceType {
        debug_assert!(self.0 != 0, "called piece_type() on Piece::NONE");
        // SAFETY: low 3 bits of a valid piece are 1..=7.
        unsafe { std::mem::transmute(self.0 & 7) }
    }

    /// Flip the color of this piece (XOR bit 3).
    #[inline]
    #[must_use]
    pub const fn flip_color(self) -> Self {
        Self(self.0 ^ 8)
    }

    #[inline]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    #[inline]
    pub const fn raw(self) -> u8 {
        self.0
    }

    #[inline]
    pub const fn from_raw(v: u8) -> Self {
        Self(v)
    }
}

impl fmt::Debug for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::NONE {
            f.write_str("Piece::NONE")
        } else {
            write!(f, "Piece({}, {})", self.color(), self.piece_type())
        }
    }
}

impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::NONE {
            return f.write_str(".");
        }
        let ch = match self.piece_type() {
            PieceType::Rook => 'R',
            PieceType::Advisor => 'A',
            PieceType::Cannon => 'C',
            PieceType::Pawn => 'P',
            PieceType::Knight => 'N',
            PieceType::Bishop => 'B',
            PieceType::King => 'K',
        };
        if self.color() == Color::Black {
            write!(f, "{}", ch.to_ascii_lowercase())
        } else {
            write!(f, "{ch}")
        }
    }
}

impl<T> Index<Piece> for [T; Piece::NUM] {
    type Output = T;

    #[inline]
    fn index(&self, pc: Piece) -> &T {
        &self[pc.0 as usize]
    }
}

impl<T> IndexMut<Piece> for [T; Piece::NUM] {
    #[inline]
    fn index_mut(&mut self, pc: Piece) -> &mut T {
        &mut self[pc.0 as usize]
    }
}

/// Piece values indexed by `Piece::index()`.
pub const PIECE_VALUE: [i32; Piece::NUM] = [
    0,             // NO_PIECE
    ROOK_VALUE,    // W_ROOK
    ADVISOR_VALUE, // W_ADVISOR
    CANNON_VALUE,  // W_CANNON
    PAWN_VALUE,    // W_PAWN
    KNIGHT_VALUE,  // W_KNIGHT
    BISHOP_VALUE,  // W_BISHOP
    0,             // W_KING
    0,             // (gap)
    ROOK_VALUE,    // B_ROOK
    ADVISOR_VALUE, // B_ADVISOR
    CANNON_VALUE,  // B_CANNON
    PAWN_VALUE,    // B_PAWN
    KNIGHT_VALUE,  // B_KNIGHT
    BISHOP_VALUE,  // B_BISHOP
    0,             // B_KING
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_piece_type_values() {
        assert_eq!(PieceType::Rook as u8, 1);
        assert_eq!(PieceType::Advisor as u8, 2);
        assert_eq!(PieceType::Cannon as u8, 3);
        assert_eq!(PieceType::Pawn as u8, 4);
        assert_eq!(PieceType::Knight as u8, 5);
        assert_eq!(PieceType::Bishop as u8, 6);
        assert_eq!(PieceType::King as u8, 7);
    }

    #[test]
    fn test_piece_type_try_from_valid() {
        for v in 1..=7u8 {
            assert!(PieceType::try_from(v).is_ok());
        }
    }

    #[test]
    fn test_piece_type_try_from_invalid() {
        assert!(PieceType::try_from(0).is_err());
        assert!(PieceType::try_from(8).is_err());
        assert!(PieceType::try_from(255).is_err());
    }

    #[test]
    fn test_piece_type_display() {
        assert_eq!(format!("{}", PieceType::Rook), "Rook");
        assert_eq!(format!("{}", PieceType::King), "King");
    }

    #[test]
    fn test_piece_make_and_extract() {
        let pc = Piece::make(Color::White, PieceType::Rook);
        assert_eq!(pc.color(), Color::White);
        assert_eq!(pc.piece_type(), PieceType::Rook);
        assert_eq!(pc.raw(), 1);

        let pc = Piece::make(Color::Black, PieceType::Knight);
        assert_eq!(pc.color(), Color::Black);
        assert_eq!(pc.piece_type(), PieceType::Knight);
        assert_eq!(pc.raw(), 13);
    }

    #[test]
    fn test_piece_encoding() {
        assert_eq!(Piece::NONE.raw(), 0);
        assert_eq!(Piece::W_ROOK.raw(), 1);
        assert_eq!(Piece::W_KING.raw(), 7);
        assert_eq!(Piece::B_ROOK.raw(), 9);
        assert_eq!(Piece::B_KING.raw(), 15);
    }

    #[test]
    fn test_piece_flip_color() {
        assert_eq!(Piece::W_ROOK.flip_color(), Piece::B_ROOK);
        assert_eq!(Piece::B_KNIGHT.flip_color(), Piece::W_KNIGHT);
    }

    #[test]
    fn test_piece_display() {
        assert_eq!(format!("{}", Piece::NONE), ".");
        assert_eq!(format!("{}", Piece::W_ROOK), "R");
        assert_eq!(format!("{}", Piece::B_ROOK), "r");
        assert_eq!(format!("{}", Piece::W_KING), "K");
        assert_eq!(format!("{}", Piece::B_KING), "k");
        assert_eq!(format!("{}", Piece::W_CANNON), "C");
        assert_eq!(format!("{}", Piece::B_CANNON), "c");
    }

    #[test]
    fn test_piece_index() {
        let mut arr = [0i32; Piece::NUM];
        arr[Piece::W_ROOK] = 100;
        arr[Piece::B_KING] = 200;
        assert_eq!(arr[Piece::W_ROOK], 100);
        assert_eq!(arr[Piece::B_KING], 200);
    }

    #[test]
    fn test_piece_value_table() {
        assert_eq!(PIECE_VALUE[Piece::NONE], 0);
        assert_eq!(PIECE_VALUE[Piece::W_ROOK], ROOK_VALUE);
        assert_eq!(PIECE_VALUE[Piece::B_ROOK], ROOK_VALUE);
        assert_eq!(PIECE_VALUE[Piece::W_PAWN], PAWN_VALUE);
        assert_eq!(PIECE_VALUE[Piece::B_PAWN], PAWN_VALUE);
    }

    #[test]
    fn test_piece_type_all() {
        assert_eq!(PieceType::ALL.len(), PieceType::NUM);
        for (i, pt) in PieceType::ALL.iter().enumerate() {
            assert_eq!(pt.index(), i + 1);
        }
    }

    #[test]
    fn test_piece_named_constants() {
        assert_eq!(
            Piece::W_ADVISOR,
            Piece::make(Color::White, PieceType::Advisor)
        );
        assert_eq!(
            Piece::B_BISHOP,
            Piece::make(Color::Black, PieceType::Bishop)
        );
    }

    #[test]
    fn test_piece_debug() {
        assert_eq!(format!("{:?}", Piece::NONE), "Piece::NONE");
        let dbg = format!("{:?}", Piece::W_ROOK);
        assert!(dbg.contains("White"));
        assert!(dbg.contains("Rook"));
    }
}
