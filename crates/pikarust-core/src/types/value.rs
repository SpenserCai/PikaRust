use std::fmt;
use std::ops::{Index, IndexMut};

use thiserror::Error;

/// Evaluation score.
pub type Value = i32;

/// Search depth.
pub type Depth = i32;

/// Zobrist hash key.
pub type Key = u64;

// ---------------------------------------------------------------------------
// Value constants
// ---------------------------------------------------------------------------

pub const VALUE_ZERO: Value = 0;
pub const VALUE_DRAW: Value = 0;
pub const VALUE_NONE: Value = 32002;
pub const VALUE_INFINITE: Value = 32001;
pub const VALUE_MATE: Value = 32000;
pub const MAX_PLY: i32 = 246;
pub const VALUE_MATE_IN_MAX_PLY: Value = VALUE_MATE - MAX_PLY;
pub const VALUE_MATED_IN_MAX_PLY: Value = -VALUE_MATE_IN_MAX_PLY;
pub const MAX_MOVES: usize = 128;

// ---------------------------------------------------------------------------
// Depth constants
// ---------------------------------------------------------------------------

pub const DEPTH_QS: Depth = 0;
pub const DEPTH_UNSEARCHED: Depth = -2;
pub const DEPTH_NONE: Depth = -3;

// ---------------------------------------------------------------------------
// Value helper functions
// ---------------------------------------------------------------------------

#[inline]
pub const fn mate_in(ply: i32) -> Value {
    VALUE_MATE - ply
}

#[inline]
pub const fn mated_in(ply: i32) -> Value {
    -VALUE_MATE + ply
}

#[inline]
pub const fn is_valid(v: Value) -> bool {
    v != VALUE_NONE
}

#[inline]
pub const fn is_win(v: Value) -> bool {
    v >= VALUE_MATE_IN_MAX_PLY
}

#[inline]
pub const fn is_loss(v: Value) -> bool {
    v <= VALUE_MATED_IN_MAX_PLY
}

#[inline]
pub const fn is_decisive(v: Value) -> bool {
    is_win(v) || is_loss(v)
}

// ---------------------------------------------------------------------------
// Bound
// ---------------------------------------------------------------------------

/// Error type for invalid bound conversions.
#[derive(Debug, Error)]
#[error("invalid bound value: {0} (expected 0..=3)")]
pub struct BoundError(u8);

/// Transposition table entry bound type.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Bound {
    None = 0,
    Upper = 1,
    Lower = 2,
    Exact = 3,
}

impl Bound {
    pub const NUM: usize = 4;

    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }
}

impl TryFrom<u8> for Bound {
    type Error = BoundError;

    #[inline]
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::None),
            1 => Ok(Self::Upper),
            2 => Ok(Self::Lower),
            3 => Ok(Self::Exact),
            _ => Err(BoundError(v)),
        }
    }
}

impl fmt::Display for Bound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => f.write_str("None"),
            Self::Upper => f.write_str("Upper"),
            Self::Lower => f.write_str("Lower"),
            Self::Exact => f.write_str("Exact"),
        }
    }
}

impl<T> Index<Bound> for [T; Bound::NUM] {
    type Output = T;

    #[inline]
    fn index(&self, b: Bound) -> &T {
        &self[b as usize]
    }
}

impl<T> IndexMut<Bound> for [T; Bound::NUM] {
    #[inline]
    fn index_mut(&mut self, b: Bound) -> &mut T {
        &mut self[b as usize]
    }
}

// ---------------------------------------------------------------------------
// Zobrist key helper
// ---------------------------------------------------------------------------

/// Congruential pseudo-random key derivation (matches Pikafish `make_key`).
#[inline]
pub const fn make_key(seed: u64) -> Key {
    seed.wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_constants() {
        assert_eq!(VALUE_ZERO, 0);
        assert_eq!(VALUE_DRAW, 0);
        assert_eq!(VALUE_NONE, 32002);
        assert_eq!(VALUE_INFINITE, 32001);
        assert_eq!(VALUE_MATE, 32000);
    }

    #[test]
    fn test_mate_helpers() {
        assert_eq!(mate_in(0), VALUE_MATE);
        assert_eq!(mate_in(1), VALUE_MATE - 1);
        assert_eq!(mated_in(0), -VALUE_MATE);
        assert_eq!(mated_in(1), -VALUE_MATE + 1);
    }

    #[test]
    fn test_mate_in_max_ply() {
        assert_eq!(VALUE_MATE_IN_MAX_PLY, VALUE_MATE - MAX_PLY);
        assert_eq!(VALUE_MATED_IN_MAX_PLY, -(VALUE_MATE - MAX_PLY));
    }

    #[test]
    fn test_is_valid() {
        assert!(is_valid(VALUE_ZERO));
        assert!(is_valid(VALUE_MATE));
        assert!(is_valid(VALUE_INFINITE));
        assert!(!is_valid(VALUE_NONE));
    }

    #[test]
    fn test_is_win_loss() {
        assert!(is_win(VALUE_MATE));
        assert!(is_win(VALUE_MATE_IN_MAX_PLY));
        assert!(!is_win(VALUE_MATE_IN_MAX_PLY - 1));

        assert!(is_loss(-VALUE_MATE));
        assert!(is_loss(VALUE_MATED_IN_MAX_PLY));
        assert!(!is_loss(VALUE_MATED_IN_MAX_PLY + 1));
    }

    #[test]
    fn test_is_decisive() {
        assert!(is_decisive(VALUE_MATE));
        assert!(is_decisive(-VALUE_MATE));
        assert!(!is_decisive(VALUE_ZERO));
        assert!(!is_decisive(100));
    }

    #[test]
    fn test_depth_constants() {
        assert_eq!(DEPTH_QS, 0);
        assert_eq!(DEPTH_UNSEARCHED, -2);
        assert_eq!(DEPTH_NONE, -3);
        let (none, unsearched, qs) = (DEPTH_NONE, DEPTH_UNSEARCHED, DEPTH_QS);
        assert!(none < unsearched);
        assert!(unsearched < qs);
    }

    #[test]
    fn test_bound_values() {
        assert_eq!(Bound::None as u8, 0);
        assert_eq!(Bound::Upper as u8, 1);
        assert_eq!(Bound::Lower as u8, 2);
        assert_eq!(Bound::Exact as u8, 3);
        assert_eq!(Bound::Exact as u8, Bound::Upper as u8 | Bound::Lower as u8);
    }

    #[test]
    fn test_bound_try_from() {
        assert_eq!(Bound::try_from(0).unwrap(), Bound::None);
        assert_eq!(Bound::try_from(3).unwrap(), Bound::Exact);
        assert!(Bound::try_from(4).is_err());
    }

    #[test]
    fn test_bound_display() {
        assert_eq!(format!("{}", Bound::None), "None");
        assert_eq!(format!("{}", Bound::Exact), "Exact");
    }

    #[test]
    fn test_bound_index() {
        let arr = [10, 20, 30, 40];
        assert_eq!(arr[Bound::None], 10);
        assert_eq!(arr[Bound::Exact], 40);
    }

    #[test]
    fn test_make_key() {
        let k1 = make_key(0);
        let k2 = make_key(1);
        assert_ne!(k1, k2);
        assert_eq!(make_key(0), make_key(0));
    }

    #[test]
    fn test_max_moves() {
        assert_eq!(MAX_MOVES, 128);
    }

    #[test]
    fn test_max_ply() {
        assert_eq!(MAX_PLY, 246);
    }
}
