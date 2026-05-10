use std::fmt;

use crate::bitboard::Bitboard;
use crate::types::{Color, Move, Piece, PieceType, Square, make_key};

use crate::nnue::features::half_ka_v2_hm::MID_MIRROR_ENCODING;

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

    pub fn attacks_by(&self, pt: PieceType, c: Color) -> Bitboard {
        use crate::bitboard::{
            attacks_bb_bishop, attacks_bb_cannon, attacks_bb_knight, attacks_bb_rook,
            pawn_attacks_bb,
        };

        let mut threats = Bitboard::EMPTY;
        let mut attackers = self.pieces(c, pt);
        let occ = self.all_pieces();
        while attackers.is_not_empty() {
            let sq = attackers.pop_lsb();
            threats |= match pt {
                PieceType::Pawn => pawn_attacks_bb(c, sq),
                PieceType::Rook => attacks_bb_rook(sq, occ),
                PieceType::Cannon => attacks_bb_cannon(sq, occ),
                PieceType::Knight => attacks_bb_knight(sq, occ),
                PieceType::Bishop => attacks_bb_bishop(sq, occ),
                PieceType::Advisor => self.pseudo_attacks_advisor(sq),
                PieceType::King => self.pseudo_attacks_king(sq),
            };
        }
        threats
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
    pub const fn piece_count_array(&self) -> &[u8; Piece::NUM] {
        &self.piece_count
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
        self.adjust_key60(self.state.key)
    }

    #[inline]
    const fn adjust_key60(&self, k: u64) -> u64 {
        let rule60_part = if self.state.rule60 < 14 {
            k
        } else {
            k ^ make_key(((self.state.rule60 - 14) / 8) as u64)
        };
        let bloom_part = if self.bloom_filter.count(self.state.key) > 0 {
            make_key(14)
        } else {
            0
        };
        rule60_part ^ bloom_part
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
        self.mid_encoding_val[pc.color().index()] = self.mid_encoding_val[pc.color().index()]
            .wrapping_add(MID_MIRROR_ENCODING[pc.index()][sq.index()]);
    }

    pub fn remove_piece(&mut self, sq: Square) {
        let pc = self.board[sq];
        let sq_bb = Bitboard::from(sq);
        self.by_type_bb[0] ^= sq_bb;
        self.by_type_bb[pc.piece_type().index()] ^= sq_bb;
        self.by_color_bb[pc.color()] ^= sq_bb;
        self.board[sq] = Piece::NONE;
        self.piece_count[pc] -= 1;
        self.mid_encoding_val[pc.color().index()] = self.mid_encoding_val[pc.color().index()]
            .wrapping_sub(MID_MIRROR_ENCODING[pc.index()][sq.index()]);
    }

    pub fn move_piece(&mut self, from: Square, to: Square) {
        let pc = self.board[from];
        let from_to = Bitboard::from(from) ^ Bitboard::from(to);
        self.by_type_bb[0] ^= from_to;
        self.by_type_bb[pc.piece_type().index()] ^= from_to;
        self.by_color_bb[pc.color()] ^= from_to;
        self.board[from] = Piece::NONE;
        self.board[to] = pc;
        let c = pc.color().index();
        self.mid_encoding_val[c] = self.mid_encoding_val[c]
            .wrapping_sub(MID_MIRROR_ENCODING[pc.index()][from.index()])
            .wrapping_add(MID_MIRROR_ENCODING[pc.index()][to.index()]);
    }

    /// Atomically replace the piece at `sq` with `new_pc`.
    /// Used in capture path to match Pikafish's `swap_piece` semantics.
    pub fn swap_piece(&mut self, sq: Square, new_pc: Piece) {
        let old = self.board[sq];
        self.remove_piece(sq);
        self.put_piece(new_pc, sq);
        let _ = old; // old is consumed by remove_piece
    }

    /// Compute threat diffs when a piece `pc` is placed (`put_piece=true`) or
    /// removed (`put_piece=false`) at square `s`. When `COMPUTE_RAY` is true,
    /// discovered threats through sliding/leaping lines are also computed.
    ///
    /// Uses const generic for compile-time specialization, matching Pikafish's
    /// `template<bool ComputeRay>` pattern.
    ///
    /// Ported from Pikafish `position.cpp:736-886`.
    #[inline]
    pub fn update_piece_threats<const COMPUTE_RAY: bool>(
        &self,
        pc: Piece,
        put_piece: bool,
        s: Square,
        dts: &mut crate::nnue::DirtyThreats,
    ) {
        use crate::bitboard::{
            attacks_bb_bishop, attacks_bb_cannon, attacks_bb_knight,
            attacks_bb_knight_to, attacks_bb_rook, leaper_pass_bb, pawn_attacks_bb,
            pawn_attacks_to_bb, pseudo_attacks, ray_pass_bb,
        };
        use crate::nnue::DirtyThreat;

        let pseudo = pseudo_attacks();
        let occupied = self.all_pieces();
        let r_attacks = attacks_bb_rook(s, occupied);
        let c_attacks = attacks_bb_cannon(s, occupied);

        // Outgoing threats
        let threatened = match pc.piece_type() {
            PieceType::Pawn => pawn_attacks_bb(pc.color(), s),
            PieceType::Rook => r_attacks,
            PieceType::Cannon => c_attacks,
            PieceType::Knight => attacks_bb_knight(s, occupied),
            PieceType::Bishop => attacks_bb_bishop(s, occupied),
            PieceType::King => pseudo.get(PieceType::King, s),
            PieceType::Advisor => pseudo.get(PieceType::Advisor, s),
        } & occupied;

        let mut bb = threatened;
        while bb.is_not_empty() {
            let tsq = bb.pop_lsb();
            let tpc = self.piece_on(tsq);
            debug_assert!(tpc != Piece::NONE);
            dts.push(DirtyThreat::new(put_piece, pc, tpc, s, tsq));
        }

        // Incoming threats
        let mut incoming = (pawn_attacks_to_bb(Color::White, s) & self.pieces(Color::White, PieceType::Pawn))
            | (pawn_attacks_to_bb(Color::Black, s) & self.pieces(Color::Black, PieceType::Pawn))
            | (attacks_bb_knight_to(s, occupied) & self.pieces_by_type(PieceType::Knight))
            | (attacks_bb_bishop(s, occupied) & self.pieces_by_type(PieceType::Bishop))
            | (pseudo.get(PieceType::Advisor, s) & self.pieces_by_type(PieceType::Advisor))
            | (pseudo.get(PieceType::King, s) & self.pieces_by_type(PieceType::King));

        if COMPUTE_RAY {
            // Rooks threatening pieces on the other side of s
            let mut sliders = r_attacks & self.pieces_by_type(PieceType::Rook);
            while sliders.is_not_empty() {
                let slider_sq = sliders.pop_lsb();
                let slider = self.piece_on(slider_sq);
                let discovered = ray_pass_bb(slider_sq, s) & r_attacks & occupied;
                if discovered.is_not_empty() {
                    let tsq = discovered.lsb();
                    let tpc = self.piece_on(tsq);
                    dts.push(DirtyThreat::new(!put_piece, slider, tpc, slider_sq, tsq));
                }
                dts.push(DirtyThreat::new(put_piece, slider, pc, slider_sq, s));
            }

            // Cannons threatening pieces on the other side of s (cannon attack line)
            let mut sliders = c_attacks & self.pieces_by_type(PieceType::Cannon);
            while sliders.is_not_empty() {
                let slider_sq = sliders.pop_lsb();
                let slider = self.piece_on(slider_sq);
                // Jumping over the first piece before s
                let discovered = ray_pass_bb(slider_sq, s) & r_attacks & occupied;
                if discovered.is_not_empty() {
                    let tsq = discovered.lsb();
                    let tpc = self.piece_on(tsq);
                    dts.push(DirtyThreat::new(!put_piece, slider, tpc, slider_sq, tsq));
                }
                dts.push(DirtyThreat::new(put_piece, slider, pc, slider_sq, s));
            }

            // Cannons on rook attack line through s
            let mut sliders = r_attacks & self.pieces_by_type(PieceType::Cannon);
            while sliders.is_not_empty() {
                let slider_sq = sliders.pop_lsb();
                let slider = self.piece_on(slider_sq);
                // Jumping over s
                let discovered = ray_pass_bb(slider_sq, s) & r_attacks & occupied;
                if discovered.is_not_empty() {
                    let tsq = discovered.lsb();
                    let tpc = self.piece_on(tsq);
                    dts.push(DirtyThreat::new(put_piece, slider, tpc, slider_sq, tsq));
                }
                // Jumping over the first piece after s
                let discovered2 = ray_pass_bb(slider_sq, s) & c_attacks & occupied;
                if discovered2.is_not_empty() {
                    let tsq = discovered2.lsb();
                    let tpc = self.piece_on(tsq);
                    dts.push(DirtyThreat::new(!put_piece, slider, tpc, slider_sq, tsq));
                }
            }

            // Knights with s as blocking square, Bishops with s as eye
            let mut leapers = (pseudo.unconstrained_king(s) & self.pieces_by_type(PieceType::Knight))
                | (pseudo.unconstrained_advisor(s) & self.pieces_by_type(PieceType::Bishop));
            while leapers.is_not_empty() {
                let leaper_sq = leapers.pop_lsb();
                let leaper = self.piece_on(leaper_sq);
                let mut discovered = leaper_pass_bb(leaper_sq, s) & occupied;
                while discovered.is_not_empty() {
                    let tsq = discovered.pop_lsb();
                    let tpc = self.piece_on(tsq);
                    dts.push(DirtyThreat::new(!put_piece, leaper, tpc, leaper_sq, tsq));
                }
            }
        } else {
            // When not computing ray, add rook/cannon sliders as incoming threats
            incoming |= (r_attacks & self.pieces_by_type(PieceType::Rook))
                | (c_attacks & self.pieces_by_type(PieceType::Cannon));
        }

        // Process all incoming threats
        while incoming.is_not_empty() {
            let src_sq = incoming.pop_lsb();
            let src_pc = self.piece_on(src_sq);
            debug_assert!(src_pc != Piece::NONE);
            dts.push(DirtyThreat::new(put_piece, src_pc, pc, src_sq, s));
        }
    }

    #[cfg(debug_assertions)]
    pub fn debug_check_consistency(&self, context: &str) {
        for sq_idx in 0..Square::NUM {
            let sq = Square::from_raw_unchecked(sq_idx as u8);
            let pc = self.board[sq];
            let in_all = (self.by_type_bb[0] & Bitboard::from(sq)).is_not_empty();
            if pc == Piece::NONE {
                debug_assert!(
                    !in_all,
                    "CONSISTENCY[{context}]: sq={sq:?} is NONE in board but set in all_pieces bitboard"
                );
            } else {
                debug_assert!(
                    in_all,
                    "CONSISTENCY[{context}]: sq={sq:?} has {pc:?} in board but NOT set in all_pieces bitboard"
                );
                let in_color = (self.by_color_bb[pc.color()] & Bitboard::from(sq)).is_not_empty();
                debug_assert!(
                    in_color,
                    "CONSISTENCY[{context}]: sq={sq:?} has {pc:?} in board but NOT set in color bitboard"
                );
                let in_type =
                    (self.by_type_bb[pc.piece_type().index()] & Bitboard::from(sq)).is_not_empty();
                debug_assert!(
                    in_type,
                    "CONSISTENCY[{context}]: sq={sq:?} has {pc:?} in board but NOT set in type bitboard"
                );
            }
        }
    }

    #[cfg(not(debug_assertions))]
    #[inline(always)]
    pub fn debug_check_consistency(&self, _context: &str) {}
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
