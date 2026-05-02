use crate::bitboard::Bitboard;
use crate::types::{Color, Key, Move, Piece, PieceType, Value};

pub const BLOOM_FILTER_SIZE: usize = 1 << 14;

pub struct BloomFilter {
    table: [u8; BLOOM_FILTER_SIZE],
}

impl BloomFilter {
    pub const fn new() -> Self {
        Self {
            table: [0; BLOOM_FILTER_SIZE],
        }
    }

    #[inline]
    pub const fn insert(&mut self, key: Key) {
        self.table[(key as usize) & (BLOOM_FILTER_SIZE - 1)] =
            self.table[(key as usize) & (BLOOM_FILTER_SIZE - 1)].wrapping_add(1);
    }

    #[inline]
    pub const fn remove(&mut self, key: Key) {
        self.table[(key as usize) & (BLOOM_FILTER_SIZE - 1)] =
            self.table[(key as usize) & (BLOOM_FILTER_SIZE - 1)].wrapping_sub(1);
    }

    #[inline]
    pub const fn count(&self, key: Key) -> u8 {
        self.table[(key as usize) & (BLOOM_FILTER_SIZE - 1)]
    }
}

impl Default for BloomFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct StateInfo {
    pub pawn_key: Key,
    pub minor_piece_key: Key,
    pub non_pawn_key: [Key; Color::NUM],
    pub major_material: [Value; Color::NUM],
    pub check10: [i16; Color::NUM],
    pub rule60: i32,
    pub plies_from_null: i32,

    pub key: Key,
    pub checkers_bb: Bitboard,
    pub blockers_for_king: [Bitboard; Color::NUM],
    pub pinners: [Bitboard; Color::NUM],
    pub check_squares: [Bitboard; PieceType::PIECE_TYPE_NB],
    pub need_full_check: bool,
    pub captured_piece: Piece,
    pub last_move: Move,
}

impl StateInfo {
    pub const fn new() -> Self {
        Self {
            pawn_key: 0,
            minor_piece_key: 0,
            non_pawn_key: [0; Color::NUM],
            major_material: [0; Color::NUM],
            check10: [0; Color::NUM],
            rule60: 0,
            plies_from_null: 0,
            key: 0,
            checkers_bb: Bitboard::EMPTY,
            blockers_for_king: [Bitboard::EMPTY; Color::NUM],
            pinners: [Bitboard::EMPTY; Color::NUM],
            check_squares: [Bitboard::EMPTY; PieceType::PIECE_TYPE_NB],
            need_full_check: false,
            captured_piece: Piece::NONE,
            last_move: Move::NONE,
        }
    }

    pub const fn copy_from_previous(&mut self, prev: &Self) {
        self.pawn_key = prev.pawn_key;
        self.minor_piece_key = prev.minor_piece_key;
        self.non_pawn_key = prev.non_pawn_key;
        self.major_material = prev.major_material;
        self.check10 = prev.check10;
        self.rule60 = prev.rule60;
        self.plies_from_null = prev.plies_from_null;
    }
}

impl Default for StateInfo {
    fn default() -> Self {
        Self::new()
    }
}
