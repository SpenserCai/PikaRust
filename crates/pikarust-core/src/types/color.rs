use std::fmt;
use std::ops::{Index, IndexMut, Not};

use thiserror::Error;

/// Error type for invalid color conversions.
#[derive(Debug, Error)]
#[error("invalid color value: {0} (expected 0 or 1)")]
pub struct ColorError(u8);

/// Side to move: White (red) or Black.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Color {
    White = 0,
    Black = 1,
}

impl Color {
    pub const NUM: usize = 2;
    pub const ALL: [Self; 2] = [Self::White, Self::Black];

    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }
}

impl Not for Color {
    type Output = Self;

    #[inline]
    fn not(self) -> Self {
        // SAFETY: `self as u8` is 0 or 1, XOR with 1 yields 1 or 0.
        unsafe { std::mem::transmute(self as u8 ^ 1) }
    }
}

impl TryFrom<u8> for Color {
    type Error = ColorError;

    #[inline]
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::White),
            1 => Ok(Self::Black),
            _ => Err(ColorError(v)),
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::White => f.write_str("White"),
            Self::Black => f.write_str("Black"),
        }
    }
}

impl<T> Index<Color> for [T; Color::NUM] {
    type Output = T;

    #[inline]
    fn index(&self, color: Color) -> &T {
        &self[color as usize]
    }
}

impl<T> IndexMut<Color> for [T; Color::NUM] {
    #[inline]
    fn index_mut(&mut self, color: Color) -> &mut T {
        &mut self[color as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_values() {
        assert_eq!(Color::White as u8, 0);
        assert_eq!(Color::Black as u8, 1);
    }

    #[test]
    fn test_color_not() {
        assert_eq!(!Color::White, Color::Black);
        assert_eq!(!Color::Black, Color::White);
    }

    #[test]
    fn test_color_try_from_valid() {
        assert_eq!(Color::try_from(0).unwrap(), Color::White);
        assert_eq!(Color::try_from(1).unwrap(), Color::Black);
    }

    #[test]
    fn test_color_try_from_invalid() {
        assert!(Color::try_from(2).is_err());
        assert!(Color::try_from(255).is_err());
    }

    #[test]
    fn test_color_display() {
        assert_eq!(format!("{}", Color::White), "White");
        assert_eq!(format!("{}", Color::Black), "Black");
    }

    #[test]
    fn test_color_index() {
        let arr = [10, 20];
        assert_eq!(arr[Color::White], 10);
        assert_eq!(arr[Color::Black], 20);
    }

    #[test]
    fn test_color_index_mut() {
        let mut arr = [0, 0];
        arr[Color::White] = 42;
        arr[Color::Black] = 99;
        assert_eq!(arr, [42, 99]);
    }

    #[test]
    fn test_color_all() {
        assert_eq!(Color::ALL, [Color::White, Color::Black]);
        assert_eq!(Color::ALL.len(), Color::NUM);
    }

    #[test]
    fn test_color_copy_clone() {
        let c = Color::White;
        let c2 = c;
        assert_eq!(c, c2);
    }

    #[test]
    fn test_color_error_display() {
        let err = Color::try_from(5).unwrap_err();
        assert_eq!(err.to_string(), "invalid color value: 5 (expected 0 or 1)");
    }
}
