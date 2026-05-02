use crate::bitboard::{
    Bitboard, HALF_BB, aligned, between_bb, lame_leaper_attack_bishop, lame_leaper_attack_knight,
    lame_leaper_attack_knight_to, pawn_attacks_bb, pawn_attacks_to_bb, ray_pass_bb,
    sliding_attack_cannon, sliding_attack_rook, square_bb,
};
use crate::types::{
    ADVISOR_VALUE, BISHOP_VALUE, CANNON_VALUE, Color, KNIGHT_VALUE, PAWN_VALUE, PIECE_VALUE,
    PieceType, ROOK_VALUE, Square, Value,
};

use super::position::Position;

impl Position {
    pub fn attackers_to(&self, sq: Square, occupied: Bitboard) -> Bitboard {
        (pawn_attacks_to_bb(Color::White, sq) & self.pieces(Color::White, PieceType::Pawn))
            | (pawn_attacks_to_bb(Color::Black, sq) & self.pieces(Color::Black, PieceType::Pawn))
            | (lame_leaper_attack_knight_to(sq, occupied) & self.pieces_by_type(PieceType::Knight))
            | (sliding_attack_rook(sq, occupied) & self.pieces_by_type(PieceType::Rook))
            | (sliding_attack_cannon(sq, occupied) & self.pieces_by_type(PieceType::Cannon))
            | (lame_leaper_attack_bishop(sq, occupied) & self.pieces_by_type(PieceType::Bishop))
            | (self.pseudo_attacks_advisor(sq) & self.pieces_by_type(PieceType::Advisor))
            | (self.pseudo_attacks_king(sq) & self.pieces_by_type(PieceType::King))
    }

    pub fn attackers_to_simple(&self, sq: Square) -> Bitboard {
        self.attackers_to(sq, self.all_pieces())
    }

    pub fn checkers_to(&self, c: Color, sq: Square, occupied: Bitboard) -> Bitboard {
        ((pawn_attacks_to_bb(c, sq) & self.pieces_by_type(PieceType::Pawn))
            | (lame_leaper_attack_knight_to(sq, occupied) & self.pieces_by_type(PieceType::Knight))
            | (sliding_attack_rook(sq, occupied)
                & (self.pieces_by_type(PieceType::King) | self.pieces_by_type(PieceType::Rook)))
            | (sliding_attack_cannon(sq, occupied) & self.pieces_by_type(PieceType::Cannon)))
            & self.pieces_by_color(c)
    }

    pub fn update_blockers(&mut self, c: Color) {
        let ksq = self.king_square(c);
        self.state.blockers_for_king[c] = Bitboard::EMPTY;
        self.state.pinners[!c] = Bitboard::EMPTY;

        let snipers = ((sliding_attack_rook(ksq, Bitboard::EMPTY)
            & (self.pieces_by_type(PieceType::Rook)
                | self.pieces_by_type(PieceType::Cannon)
                | self.pieces_by_type(PieceType::King)))
            | (lame_leaper_attack_knight(ksq, Bitboard::EMPTY)
                & self.pieces_by_type(PieceType::Knight)))
            & self.pieces_by_color(!c);

        let occupancy = self.all_pieces() ^ (snipers & !self.pieces_by_type(PieceType::Cannon));

        let mut snipers_iter = snipers;
        while snipers_iter.is_not_empty() {
            let sniper_sq = snipers_iter.pop_lsb();
            let is_cannon = self.piece_on(sniper_sq).piece_type() == PieceType::Cannon;
            let b = between_bb(ksq, sniper_sq)
                & if is_cannon {
                    self.all_pieces() ^ sniper_sq
                } else {
                    occupancy
                };

            let count = b.popcount();
            if b.is_not_empty() && ((!is_cannon && count == 1) || (is_cannon && count == 2)) {
                self.state.blockers_for_king[c] |= b;
                if (b & self.pieces_by_color(c)).is_not_empty() {
                    self.state.pinners[!c] |= sniper_sq;
                }
            }
        }
    }

    pub(crate) fn set_check_info(&mut self) {
        self.update_blockers(Color::White);
        self.update_blockers(Color::Black);

        let ksq = self.king_square(!self.side_to_move);

        self.state.need_full_check = self.state.checkers_bb.is_not_empty()
            || (sliding_attack_rook(self.king_square(self.side_to_move), Bitboard::EMPTY)
                & self.pieces(!self.side_to_move, PieceType::Cannon))
            .is_not_empty();

        self.state.check_squares[PieceType::Pawn.index()] =
            pawn_attacks_to_bb(self.side_to_move, ksq);
        self.state.check_squares[PieceType::Knight.index()] =
            lame_leaper_attack_knight_to(ksq, self.all_pieces());
        self.state.check_squares[PieceType::Cannon.index()] =
            sliding_attack_cannon(ksq, self.all_pieces());
        self.state.check_squares[PieceType::Rook.index()] =
            sliding_attack_rook(ksq, self.all_pieces());
        self.state.check_squares[PieceType::King.index()] = Bitboard::EMPTY;
        self.state.check_squares[PieceType::Advisor.index()] = Bitboard::EMPTY;
        self.state.check_squares[PieceType::Bishop.index()] = Bitboard::EMPTY;

        let hollow_cannons = self.state.check_squares[PieceType::Rook.index()]
            & self.pieces(self.side_to_move, PieceType::Cannon);
        if hollow_cannons.is_not_empty() {
            let mut hollow_cannon_discover = Bitboard::EMPTY;
            let mut hc = hollow_cannons;
            while hc.is_not_empty() {
                let hc_sq = hc.pop_lsb();
                hollow_cannon_discover |= between_bb(hc_sq, ksq);
            }
            for pt_idx in PieceType::Rook.index()..PieceType::King.index() {
                self.state.check_squares[pt_idx] |= hollow_cannon_discover;
            }
        }
    }

    pub fn is_legal(&self, m: crate::types::Move) -> bool {
        let us = self.side_to_move;
        let from = m.from_sq();
        let to = m.to_sq();
        let occupied = (self.all_pieces() ^ from) | to;

        let pc = self.piece_on(from);
        if pc == crate::types::Piece::NONE {
            debug_assert!(
                false,
                "is_legal: piece_on(from={from:?}) is NONE, to={to:?}, move_raw={}, side={us:?}, \
                 fen={}",
                m.raw(),
                self.fen()
            );
            return false;
        }

        if pc.piece_type() == PieceType::King {
            return self.checkers_to(!us, to, occupied).is_empty();
        }

        if !self.state.need_full_check
            && (!(self.blockers_for_king(us) & from).is_not_empty()
                || ((self.piece_on(from).piece_type() != PieceType::Cannon || !self.is_capture(m))
                    && aligned(from, to, self.king_square(us))))
        {
            return true;
        }

        (self.checkers_to(!us, self.king_square(us), occupied) & !square_bb(to)).is_empty()
    }

    pub fn pseudo_legal(&self, m: crate::types::Move) -> bool {
        let us = self.side_to_move;
        let from = m.from_sq();
        let to = m.to_sq();
        let pc = self.moved_piece(m);

        if pc == crate::types::Piece::NONE || pc.color() != us {
            return false;
        }

        if (self.pieces_by_color(us) & to).is_not_empty() {
            return false;
        }

        let pt = pc.piece_type();
        let occupied = self.all_pieces();

        match pt {
            PieceType::Pawn => (pawn_attacks_bb(us, from) & to).is_not_empty(),
            PieceType::Cannon => {
                if self.is_capture(m) {
                    (sliding_attack_cannon(from, occupied) & to).is_not_empty()
                } else {
                    (sliding_attack_rook(from, occupied) & to).is_not_empty()
                }
            }
            PieceType::Rook => (sliding_attack_rook(from, occupied) & to).is_not_empty(),
            PieceType::Knight => (lame_leaper_attack_knight(from, occupied) & to).is_not_empty(),
            PieceType::Bishop => {
                (lame_leaper_attack_bishop(from, occupied) & HALF_BB[us] & to).is_not_empty()
            }
            PieceType::Advisor => (self.pseudo_attacks_advisor(from) & to).is_not_empty(),
            PieceType::King => (self.pseudo_attacks_king(from) & to).is_not_empty(),
        }
    }

    pub fn gives_check(&self, m: crate::types::Move) -> bool {
        let from = m.from_sq();
        let to = m.to_sq();
        let ksq = self.king_square(!self.side_to_move);
        let pt = self.moved_piece(m).piece_type();

        if pt == PieceType::Cannon
            && (self.check_squares(PieceType::Rook) & from).is_not_empty()
            && aligned(from, to, ksq)
        {
            if self.is_capture(m) && (ray_pass_bb(ksq, from) & to).is_not_empty() {
                return true;
            }
        } else if (self.check_squares(pt) & to).is_not_empty() {
            return true;
        }

        if (self.blockers_for_king(!self.side_to_move) & from).is_not_empty()
            && (!aligned(from, to, ksq) || self.is_capture(m))
        {
            return true;
        }

        false
    }

    #[allow(clippy::too_many_lines)]
    pub fn see_ge(&self, m: crate::types::Move, threshold: Value) -> bool {
        let from = m.from_sq();
        let to = m.to_sq();

        let mut swap = PIECE_VALUE[self.piece_on(to)] - threshold;
        if swap < 0 {
            return false;
        }

        swap = PIECE_VALUE[self.piece_on(from)] - swap;
        if swap <= 0 {
            return true;
        }

        let mut occupied = self.all_pieces() ^ from ^ to;
        let mut stm = self.side_to_move;
        let attackers = self.attackers_to(to, occupied);

        let king_attacks = (attackers & self.pieces_by_type(PieceType::King)).is_not_empty();
        let mut non_cannons = attackers & !self.pieces_by_type(PieceType::Cannon);
        if king_attacks {
            non_cannons |= sliding_attack_rook(to, occupied) & self.pieces_by_type(PieceType::King);
        }
        let mut cannons = attackers & self.pieces_by_type(PieceType::Cannon);
        let mut all_attackers = non_cannons | cannons;
        let mut res = 1i32;

        loop {
            stm = !stm;
            all_attackers &= occupied;

            let stm_attackers = all_attackers & self.pieces_by_color(stm);
            if stm_attackers.is_empty() {
                break;
            }

            if (self.pinners(!stm) & occupied).is_not_empty() {
                let filtered = stm_attackers & !self.blockers_for_king(stm);
                if filtered.is_empty() {
                    break;
                }
            }

            res ^= 1;

            let bb;
            if (stm_attackers & self.pieces_by_type(PieceType::Pawn)).is_not_empty() {
                swap = PAWN_VALUE - swap;
                if swap < res {
                    break;
                }
                bb = stm_attackers & self.pieces_by_type(PieceType::Pawn);
                occupied ^= Bitboard::new(bb.raw() & bb.raw().wrapping_neg());
                non_cannons |= sliding_attack_rook(to, occupied)
                    & if king_attacks {
                        self.pieces_by_type(PieceType::King) | self.pieces_by_type(PieceType::Rook)
                    } else {
                        self.pieces_by_type(PieceType::Rook)
                    };
                cannons =
                    sliding_attack_cannon(to, occupied) & self.pieces_by_type(PieceType::Cannon);
                all_attackers = non_cannons | cannons;
            } else if (stm_attackers & self.pieces_by_type(PieceType::Bishop)).is_not_empty() {
                swap = BISHOP_VALUE - swap;
                if swap < res {
                    break;
                }
                bb = stm_attackers & self.pieces_by_type(PieceType::Bishop);
                occupied ^= Bitboard::new(bb.raw() & bb.raw().wrapping_neg());
            } else if (stm_attackers & self.pieces_by_type(PieceType::Advisor)).is_not_empty() {
                swap = ADVISOR_VALUE - swap;
                if swap < res {
                    break;
                }
                bb = stm_attackers & self.pieces_by_type(PieceType::Advisor);
                occupied ^= Bitboard::new(bb.raw() & bb.raw().wrapping_neg());
                non_cannons |= lame_leaper_attack_knight_to(to, occupied)
                    & self.pieces_by_type(PieceType::Knight);
                all_attackers = non_cannons | cannons;
            } else if (stm_attackers & self.pieces_by_type(PieceType::Cannon)).is_not_empty() {
                swap = CANNON_VALUE - swap;
                if swap < res {
                    break;
                }
                bb = stm_attackers & self.pieces_by_type(PieceType::Cannon);
                occupied ^= Bitboard::new(bb.raw() & bb.raw().wrapping_neg());
                cannons =
                    sliding_attack_cannon(to, occupied) & self.pieces_by_type(PieceType::Cannon);
                all_attackers = non_cannons | cannons;
            } else if (stm_attackers & self.pieces_by_type(PieceType::Knight)).is_not_empty() {
                swap = KNIGHT_VALUE - swap;
                if swap < res {
                    break;
                }
                bb = stm_attackers & self.pieces_by_type(PieceType::Knight);
                occupied ^= Bitboard::new(bb.raw() & bb.raw().wrapping_neg());
            } else if (stm_attackers & self.pieces_by_type(PieceType::Rook)).is_not_empty() {
                swap = ROOK_VALUE - swap;
                bb = stm_attackers & self.pieces_by_type(PieceType::Rook);
                occupied ^= Bitboard::new(bb.raw() & bb.raw().wrapping_neg());
                non_cannons |= sliding_attack_rook(to, occupied)
                    & if king_attacks {
                        self.pieces_by_type(PieceType::King) | self.pieces_by_type(PieceType::Rook)
                    } else {
                        self.pieces_by_type(PieceType::Rook)
                    };
                cannons =
                    sliding_attack_cannon(to, occupied) & self.pieces_by_type(PieceType::Cannon);
                all_attackers = non_cannons | cannons;
            } else {
                return if (all_attackers & !self.pieces_by_color(stm)).is_not_empty() {
                    res ^ 1 != 0
                } else {
                    res != 0
                };
            }
        }

        res != 0
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn pseudo_attacks_king(&self, sq: Square) -> Bitboard {
        use crate::bitboard::{PALACE, safe_destination};
        use crate::types::Direction;

        let mut attacks = Bitboard::EMPTY;
        for step in [
            Direction::NORTH.raw(),
            Direction::SOUTH.raw(),
            Direction::EAST.raw(),
            Direction::WEST.raw(),
        ] {
            attacks |= safe_destination(sq, step) & PALACE;
        }
        attacks
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn pseudo_attacks_advisor(&self, sq: Square) -> Bitboard {
        use crate::bitboard::{PALACE, safe_destination};
        use crate::types::Direction;

        let mut attacks = Bitboard::EMPTY;
        for step in [
            Direction::NORTH_EAST.raw(),
            Direction::NORTH_WEST.raw(),
            Direction::SOUTH_EAST.raw(),
            Direction::SOUTH_WEST.raw(),
        ] {
            attacks |= safe_destination(sq, step) & PALACE;
        }
        attacks
    }
}
