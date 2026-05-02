use std::fmt;

use crate::bitboard::Bitboard;
use crate::types::{Color, Move, Piece, PieceType, Square};

use super::state::{BloomFilter, StateInfo};

pub struct Position {
    pub(crate) board: [Piece; Square::NUM],
    pub(crate) by_type_bb: [Bitboard; PieceType::PIECE_TYPE_NB],
    pub(crate) by_color_bb: [Bitboard; Color::NUM],
    pub(crate) piece_count: [u8; Piece::NUM],
    pub(crate) side_to_move: Color,
    pub(crate) game_ply: u16,
    pub(crate) state: StateInfo,
    pub(crate) state_stack: Vec<StateInfo>,
    pub(crate) bloom_filter: BloomFilter,
    pub(crate) id_board: [i32; Square::NUM],
    pub(crate) mid_encoding_val: [u64; Color::NUM],
}

impl Position {
    pub fn new() -> Self {
        Self {
            board: [Piece::NONE; Square::NUM],
            by_type_bb: [Bitboard::EMPTY; PieceType::PIECE_TYPE_NB],
            by_color_bb: [Bitboard::EMPTY; Color::NUM],
            piece_count: [0; Piece::NUM],
            side_to_move: Color::White,
            game_ply: 0,
            state: StateInfo::new(),
            state_stack: Vec::with_capacity(256),
            bloom_filter: BloomFilter::new(),
            id_board: [0; Square::NUM],
            mid_encoding_val: [0; Color::NUM],
        }
    }

    #[inline]
    pub fn piece_on(&self, sq: Square) -> Piece {
        self.board[sq]
    }

    #[inline]
    pub fn is_empty(&self, sq: Square) -> bool {
        self.board[sq] == Piece::NONE
    }

    #[inline]
    pub fn moved_piece(&self, m: Move) -> Piece {
        self.board[m.from_sq()]
    }

    #[inline]
    pub const fn all_pieces(&self) -> Bitboard {
        self.by_type_bb[0]
    }

    #[inline]
    pub const fn pieces_by_type(&self, pt: PieceType) -> Bitboard {
        self.by_type_bb[pt.index()]
    }

    pub fn pieces_by_types(&self, pts: &[PieceType]) -> Bitboard {
        let mut bb = Bitboard::EMPTY;
        for &pt in pts {
            bb |= self.by_type_bb[pt.index()];
        }
        bb
    }

    #[inline]
    pub fn pieces_by_color(&self, c: Color) -> Bitboard {
        self.by_color_bb[c]
    }

    #[inline]
    pub fn pieces(&self, c: Color, pt: PieceType) -> Bitboard {
        self.by_color_bb[c] & self.by_type_bb[pt.index()]
    }

    pub fn pieces_multi(&self, c: Color, pts: &[PieceType]) -> Bitboard {
        self.by_color_bb[c] & self.pieces_by_types(pts)
    }

    #[inline]
    pub fn king_square(&self, c: Color) -> Square {
        let king_bb = self.pieces(c, PieceType::King);
        debug_assert!(king_bb.is_not_empty());
        king_bb.lsb()
    }

    #[inline]
    pub fn count(&self, pc: Piece) -> u8 {
        self.piece_count[pc]
    }

    #[inline]
    pub fn count_type(&self, c: Color, pt: PieceType) -> u8 {
        self.piece_count[Piece::make(c, pt)]
    }

    #[inline]
    pub const fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    #[inline]
    pub const fn game_ply(&self) -> u16 {
        self.game_ply
    }

    #[inline]
    pub const fn state(&self) -> &StateInfo {
        &self.state
    }

    #[inline]
    pub const fn checkers(&self) -> Bitboard {
        self.state.checkers_bb
    }

    #[inline]
    pub fn blockers_for_king(&self, c: Color) -> Bitboard {
        self.state.blockers_for_king[c]
    }

    #[inline]
    pub fn pinners(&self, c: Color) -> Bitboard {
        self.state.pinners[c]
    }

    #[inline]
    pub const fn check_squares(&self, pt: PieceType) -> Bitboard {
        self.state.check_squares[pt.index()]
    }

    #[inline]
    pub const fn key(&self) -> u64 {
        self.state.key
    }

    #[inline]
    pub const fn pawn_key(&self) -> u64 {
        self.state.pawn_key
    }

    #[inline]
    pub const fn minor_piece_key(&self) -> u64 {
        self.state.minor_piece_key
    }

    #[inline]
    pub fn non_pawn_key(&self, c: Color) -> u64 {
        self.state.non_pawn_key[c]
    }

    #[inline]
    pub fn major_material(&self, c: Color) -> i32 {
        self.state.major_material[c]
    }

    #[inline]
    pub fn total_major_material(&self) -> i32 {
        self.state.major_material[Color::White] + self.state.major_material[Color::Black]
    }

    #[inline]
    pub const fn rule60_count(&self) -> i32 {
        self.state.rule60
    }

    #[inline]
    pub fn is_capture(&self, m: Move) -> bool {
        !self.is_empty(m.to_sq())
    }

    #[inline]
    pub const fn captured_piece(&self) -> Piece {
        self.state.captured_piece
    }

    #[inline]
    pub fn mid_encoding(&self, c: Color) -> u64 {
        self.mid_encoding_val[c]
    }

    pub fn put_piece(&mut self, pc: Piece, sq: Square) {
        self.board[sq] = pc;
        let sq_bb = Bitboard::from(sq);
        self.by_type_bb[0] |= sq_bb;
        self.by_type_bb[pc.piece_type().index()] |= sq_bb;
        self.by_color_bb[pc.color()] |= sq_bb;
        self.piece_count[pc] += 1;
        self.piece_count[Piece::make(pc.color(), PieceType::Rook).index()] += 0;
    }

    pub fn remove_piece(&mut self, sq: Square) {
        let pc = self.board[sq];
        let sq_bb = Bitboard::from(sq);
        self.by_type_bb[0] ^= sq_bb;
        self.by_type_bb[pc.piece_type().index()] ^= sq_bb;
        self.by_color_bb[pc.color()] ^= sq_bb;
        self.board[sq] = Piece::NONE;
        self.piece_count[pc] -= 1;
    }

    pub fn move_piece(&mut self, from: Square, to: Square) {
        let pc = self.board[from];
        let from_to = Bitboard::from(from) ^ Bitboard::from(to);
        self.by_type_bb[0] ^= from_to;
        self.by_type_bb[pc.piece_type().index()] ^= from_to;
        self.by_color_bb[pc.color()] ^= from_to;
        self.board[from] = Piece::NONE;
        self.board[to] = pc;
    }
}

impl Clone for Position {
    fn clone(&self) -> Self {
        Self {
            board: self.board,
            by_type_bb: self.by_type_bb,
            by_color_bb: self.by_color_bb,
            piece_count: self.piece_count,
            side_to_move: self.side_to_move,
            game_ply: self.game_ply,
            state: self.state.clone(),
            state_stack: self.state_stack.clone(),
            bloom_filter: BloomFilter::new(),
            id_board: self.id_board,
            mid_encoding_val: self.mid_encoding_val,
        }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, " +---+---+---+---+---+---+---+---+---+")?;
        for r in (0..10u8).rev() {
            for file in 0..9u8 {
                let sq = Square::from_raw_unchecked(r * 9 + file);
                let pc = self.piece_on(sq);
                write!(f, " | {pc}")?;
            }
            writeln!(f, " | {r}")?;
            writeln!(f, " +---+---+---+---+---+---+---+---+---+")?;
        }
        writeln!(f, "   a   b   c   d   e   f   g   h   i")?;
        Ok(())
    }
}
