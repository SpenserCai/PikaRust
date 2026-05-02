use crate::types::{Color, Direction, PieceType, Square};

use super::Bitboard;
use super::tables::{
    FILE_A_BB, FILE_I_BB, HALF_BB, PALACE, RANK_9_BB, file_distance, rank_distance,
    safe_destination, square_bb, square_distance,
};

pub const PSEUDO_ATTACKS_SIZE: usize = PieceType::PIECE_TYPE_NB + 3;

pub fn sliding_attack_rook(sq: Square, occupied: Bitboard) -> Bitboard {
    let mut attack = Bitboard::EMPTY;
    let directions = [
        Direction::NORTH,
        Direction::SOUTH,
        Direction::EAST,
        Direction::WEST,
    ];

    for &d in &directions {
        let mut s = sq;
        loop {
            let to_raw = i16::from(s.raw()) + i16::from(d.raw());
            if !(0..90).contains(&to_raw) {
                break;
            }
            let next = Square::from_raw_unchecked(to_raw as u8);
            if square_distance(s, next) != 1 {
                break;
            }
            attack |= next;
            if occupied.contains(next) {
                break;
            }
            s = next;
        }
    }
    attack
}

pub fn sliding_attack_cannon(sq: Square, occupied: Bitboard) -> Bitboard {
    let mut attack = Bitboard::EMPTY;
    let directions = [
        Direction::NORTH,
        Direction::SOUTH,
        Direction::EAST,
        Direction::WEST,
    ];

    for &d in &directions {
        let mut s = sq;
        let mut hurdle = false;
        loop {
            let to_raw = i16::from(s.raw()) + i16::from(d.raw());
            if !(0..90).contains(&to_raw) {
                break;
            }
            let next = Square::from_raw_unchecked(to_raw as u8);
            if square_distance(s, next) != 1 {
                break;
            }
            if hurdle {
                attack |= next;
            }
            if occupied.contains(next) {
                if hurdle {
                    break;
                }
                hurdle = true;
            }
            s = next;
        }
    }
    attack
}

const fn get_bishop_directions() -> [Direction; 4] {
    [
        Direction(2 * Direction::NORTH_EAST.raw()),
        Direction(2 * Direction::SOUTH_EAST.raw()),
        Direction(2 * Direction::SOUTH_WEST.raw()),
        Direction(2 * Direction::NORTH_WEST.raw()),
    ]
}

const fn get_knight_directions() -> [Direction; 8] {
    let n = Direction::NORTH.raw();
    let s = Direction::SOUTH.raw();
    let e = Direction::EAST.raw();
    let w = Direction::WEST.raw();
    [
        Direction(2 * s + w),
        Direction(2 * s + e),
        Direction(s + 2 * w),
        Direction(s + 2 * e),
        Direction(n + 2 * w),
        Direction(n + 2 * e),
        Direction(2 * n + w),
        Direction(2 * n + e),
    ]
}

const fn abs_i8(x: i8) -> i8 {
    x.abs()
}

pub fn lame_leaper_path_dir_knight(d: Direction, s: Square) -> Bitboard {
    let to_raw = i16::from(s.raw()) + i16::from(d.raw());
    if !(0..90).contains(&to_raw) {
        return Bitboard::EMPTY;
    }
    let to = Square::from_raw_unchecked(to_raw as u8);
    if square_distance(s, to) > 3 {
        return Bitboard::EMPTY;
    }

    let dr: Direction = if d.raw() > 0 {
        Direction::NORTH
    } else {
        Direction::SOUTH
    };
    let d_mod = d.raw() % Direction::NORTH.raw();
    let d_mod_abs = if abs_i8(d_mod) < Direction::NORTH.raw() / 2 {
        d_mod
    } else {
        -d_mod
    };
    let df: Direction = if d_mod_abs < 0 {
        Direction::WEST
    } else {
        Direction::EAST
    };

    let diff = file_distance(s, to) as i8 - rank_distance(s, to) as i8;
    let blocking_raw = match diff.cmp(&0) {
        std::cmp::Ordering::Greater => i16::from(s.raw()) + i16::from(df.raw()),
        std::cmp::Ordering::Less => i16::from(s.raw()) + i16::from(dr.raw()),
        std::cmp::Ordering::Equal => i16::from(s.raw()) + i16::from(df.raw()) + i16::from(dr.raw()),
    };

    if !(0..90).contains(&blocking_raw) {
        return Bitboard::EMPTY;
    }
    Bitboard::from(Square::from_raw_unchecked(blocking_raw as u8))
}

pub fn lame_leaper_path_dir_knight_to(d: Direction, s: Square) -> Bitboard {
    let to_raw = i16::from(s.raw()) + i16::from(d.raw());
    if !(0..90).contains(&to_raw) {
        return Bitboard::EMPTY;
    }
    let to = Square::from_raw_unchecked(to_raw as u8);
    if square_distance(s, to) > 3 {
        return Bitboard::EMPTY;
    }

    let swapped_s = to;
    let neg_d = Direction(-d.raw());

    let dr: Direction = if neg_d.raw() > 0 {
        Direction::NORTH
    } else {
        Direction::SOUTH
    };
    let d_mod = neg_d.raw() % Direction::NORTH.raw();
    let d_mod_abs = if abs_i8(d_mod) < Direction::NORTH.raw() / 2 {
        d_mod
    } else {
        -d_mod
    };
    let df: Direction = if d_mod_abs < 0 {
        Direction::WEST
    } else {
        Direction::EAST
    };

    let swapped_to = s;
    let diff =
        file_distance(swapped_s, swapped_to) as i8 - rank_distance(swapped_s, swapped_to) as i8;
    let blocking_raw = match diff.cmp(&0) {
        std::cmp::Ordering::Greater => i16::from(swapped_s.raw()) + i16::from(df.raw()),
        std::cmp::Ordering::Less => i16::from(swapped_s.raw()) + i16::from(dr.raw()),
        std::cmp::Ordering::Equal => {
            i16::from(swapped_s.raw()) + i16::from(df.raw()) + i16::from(dr.raw())
        }
    };

    if !(0..90).contains(&blocking_raw) {
        return Bitboard::EMPTY;
    }
    Bitboard::from(Square::from_raw_unchecked(blocking_raw as u8))
}

pub fn lame_leaper_path_dir_bishop(d: Direction, s: Square) -> Bitboard {
    let to_raw = i16::from(s.raw()) + i16::from(d.raw());
    if !(0..90).contains(&to_raw) {
        return Bitboard::EMPTY;
    }
    let to = Square::from_raw_unchecked(to_raw as u8);
    if square_distance(s, to) > 3 {
        return Bitboard::EMPTY;
    }

    let dr: Direction = if d.raw() > 0 {
        Direction::NORTH
    } else {
        Direction::SOUTH
    };
    let d_mod = d.raw() % Direction::NORTH.raw();
    let d_mod_abs = if abs_i8(d_mod) < Direction::NORTH.raw() / 2 {
        d_mod
    } else {
        -d_mod
    };
    let df: Direction = if d_mod_abs < 0 {
        Direction::WEST
    } else {
        Direction::EAST
    };

    let diff = file_distance(s, to) as i8 - rank_distance(s, to) as i8;
    let blocking_raw = match diff.cmp(&0) {
        std::cmp::Ordering::Greater => i16::from(s.raw()) + i16::from(df.raw()),
        std::cmp::Ordering::Less => i16::from(s.raw()) + i16::from(dr.raw()),
        std::cmp::Ordering::Equal => i16::from(s.raw()) + i16::from(df.raw()) + i16::from(dr.raw()),
    };

    if !(0..90).contains(&blocking_raw) {
        return Bitboard::EMPTY;
    }
    Bitboard::from(Square::from_raw_unchecked(blocking_raw as u8))
}

pub fn lame_leaper_path_knight(s: Square) -> Bitboard {
    let mut b = Bitboard::EMPTY;
    for &d in &get_knight_directions() {
        b |= lame_leaper_path_dir_knight(d, s);
    }
    b
}

pub fn lame_leaper_path_knight_to(s: Square) -> Bitboard {
    let mut b = Bitboard::EMPTY;
    for &d in &get_knight_directions() {
        b |= lame_leaper_path_dir_knight_to(d, s);
    }
    b
}

pub fn lame_leaper_path_bishop(s: Square) -> Bitboard {
    let mut b = Bitboard::EMPTY;
    for &d in &get_bishop_directions() {
        b |= lame_leaper_path_dir_bishop(d, s);
    }
    b &= HALF_BB[usize::from(s.rank() as u8 > 4)];
    b
}

pub fn lame_leaper_attack_knight(s: Square, occupied: Bitboard) -> Bitboard {
    let mut b = Bitboard::EMPTY;
    for &d in &get_knight_directions() {
        let to_raw = i16::from(s.raw()) + i16::from(d.raw());
        if !(0..90).contains(&to_raw) {
            continue;
        }
        let to = Square::from_raw_unchecked(to_raw as u8);
        if square_distance(s, to) >= 3 {
            continue;
        }
        if (lame_leaper_path_dir_knight(d, s) & occupied).is_empty() {
            b |= to;
        }
    }
    b
}

pub fn lame_leaper_attack_knight_to(s: Square, occupied: Bitboard) -> Bitboard {
    let mut b = Bitboard::EMPTY;
    for &d in &get_knight_directions() {
        let to_raw = i16::from(s.raw()) + i16::from(d.raw());
        if !(0..90).contains(&to_raw) {
            continue;
        }
        let to = Square::from_raw_unchecked(to_raw as u8);
        if square_distance(s, to) >= 3 {
            continue;
        }
        if (lame_leaper_path_dir_knight_to(d, s) & occupied).is_empty() {
            b |= to;
        }
    }
    b
}

pub fn lame_leaper_attack_bishop(s: Square, occupied: Bitboard) -> Bitboard {
    let mut b = Bitboard::EMPTY;
    for &d in &get_bishop_directions() {
        let to_raw = i16::from(s.raw()) + i16::from(d.raw());
        if !(0..90).contains(&to_raw) {
            continue;
        }
        let to = Square::from_raw_unchecked(to_raw as u8);
        if square_distance(s, to) >= 3 {
            continue;
        }
        if (lame_leaper_path_dir_bishop(d, s) & occupied).is_empty() {
            b |= to;
        }
    }
    b &= HALF_BB[usize::from(s.rank() as u8 > 4)];
    b
}

pub fn pawn_attacks_bb(c: Color, sq: Square) -> Bitboard {
    let b = square_bb(sq);
    match c {
        Color::White => {
            let mut attack = Bitboard((b.0 & !RANK_9_BB.0) << Direction::NORTH.raw() as u8);
            if sq.rank() as u8 > 4 {
                attack |= Bitboard((b.0 & !FILE_A_BB.0) >> Direction::EAST.raw() as u8);
                attack |= Bitboard((b.0 & !FILE_I_BB.0) << Direction::EAST.raw() as u8);
            }
            attack
        }
        Color::Black => {
            let mut attack = Bitboard(b.0 >> Direction::NORTH.raw() as u8);
            if (sq.rank() as u8) < 5 {
                attack |= Bitboard((b.0 & !FILE_A_BB.0) >> Direction::EAST.raw() as u8);
                attack |= Bitboard((b.0 & !FILE_I_BB.0) << Direction::EAST.raw() as u8);
            }
            attack
        }
    }
}

pub fn pawn_attacks_to_bb(c: Color, sq: Square) -> Bitboard {
    let b = square_bb(sq);
    match c {
        Color::White => {
            let mut attack = Bitboard(b.0 >> Direction::NORTH.raw() as u8);
            if sq.rank() as u8 > 4 {
                attack |= Bitboard((b.0 & !FILE_A_BB.0) >> Direction::EAST.raw() as u8);
                attack |= Bitboard((b.0 & !FILE_I_BB.0) << Direction::EAST.raw() as u8);
            }
            attack
        }
        Color::Black => {
            let mut attack = Bitboard((b.0 & !RANK_9_BB.0) << Direction::NORTH.raw() as u8);
            if (sq.rank() as u8) < 5 {
                attack |= Bitboard((b.0 & !FILE_A_BB.0) >> Direction::EAST.raw() as u8);
                attack |= Bitboard((b.0 & !FILE_I_BB.0) << Direction::EAST.raw() as u8);
            }
            attack
        }
    }
}

pub struct PseudoAttacksTable {
    pub table: [[Bitboard; Square::NUM]; PSEUDO_ATTACKS_SIZE],
}

impl PseudoAttacksTable {
    pub fn new() -> Self {
        let mut table = [[Bitboard::EMPTY; Square::NUM]; PSEUDO_ATTACKS_SIZE];

        for i in 0..90u8 {
            let s = Square::from_raw_unchecked(i);

            table[0][s.index()] = pawn_attacks_bb(Color::White, s);
            table[PieceType::Pawn as usize][s.index()] = pawn_attacks_bb(Color::Black, s);

            table[8][s.index()] = pawn_attacks_to_bb(Color::White, s);
            table[9][s.index()] = pawn_attacks_to_bb(Color::Black, s);

            table[PieceType::Rook as usize][s.index()] = sliding_attack_rook(s, Bitboard::EMPTY);
            table[PieceType::Bishop as usize][s.index()] =
                lame_leaper_attack_bishop(s, Bitboard::EMPTY);
            table[PieceType::Knight as usize][s.index()] =
                lame_leaper_attack_knight(s, Bitboard::EMPTY);

            for step in [
                Direction::NORTH.raw(),
                Direction::SOUTH.raw(),
                Direction::WEST.raw(),
                Direction::EAST.raw(),
            ] {
                if PALACE.contains(s) {
                    table[PieceType::King as usize][s.index()] |=
                        safe_destination(s, step) & PALACE;
                }
                table[PieceType::King as usize + 3][s.index()] |= safe_destination(s, step);
            }

            for step in [
                Direction::NORTH_WEST.raw(),
                Direction::NORTH_EAST.raw(),
                Direction::SOUTH_WEST.raw(),
                Direction::SOUTH_EAST.raw(),
            ] {
                if PALACE.contains(s) {
                    table[PieceType::Advisor as usize][s.index()] |=
                        safe_destination(s, step) & PALACE;
                }
                table[PieceType::Advisor as usize + 1][s.index()] |= safe_destination(s, step);
            }
        }

        Self { table }
    }

    #[inline]
    pub const fn get(&self, pt: PieceType, sq: Square) -> Bitboard {
        self.table[pt as usize][sq.index()]
    }

    #[inline]
    pub const fn pawn_attacks(&self, c: Color, sq: Square) -> Bitboard {
        match c {
            Color::White => self.table[0][sq.index()],
            Color::Black => self.table[PieceType::Pawn as usize][sq.index()],
        }
    }

    #[inline]
    pub const fn pawn_attacks_to(&self, c: Color, sq: Square) -> Bitboard {
        match c {
            Color::White => self.table[8][sq.index()],
            Color::Black => self.table[9][sq.index()],
        }
    }

    #[inline]
    pub const fn unconstrained_king(&self, sq: Square) -> Bitboard {
        self.table[PieceType::King as usize + 3][sq.index()]
    }

    #[inline]
    pub const fn unconstrained_advisor(&self, sq: Square) -> Bitboard {
        self.table[PieceType::Advisor as usize + 1][sq.index()]
    }
}

impl Default for PseudoAttacksTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rook_attack_empty_board() {
        let attacks = sliding_attack_rook(Square::SQ_E4, Bitboard::EMPTY);
        assert!(attacks.contains(Square::SQ_E0));
        assert!(attacks.contains(Square::SQ_E9));
        assert!(attacks.contains(Square::SQ_A4));
        assert!(attacks.contains(Square::SQ_I4));
        assert!(!attacks.contains(Square::SQ_E4));
    }

    #[test]
    fn test_rook_attack_blocked() {
        let occ = Bitboard::from(Square::SQ_E6);
        let attacks = sliding_attack_rook(Square::SQ_E4, occ);
        assert!(attacks.contains(Square::SQ_E5));
        assert!(attacks.contains(Square::SQ_E6));
        assert!(!attacks.contains(Square::SQ_E7));
    }

    #[test]
    fn test_cannon_attack_empty_board() {
        let attacks = sliding_attack_cannon(Square::SQ_E4, Bitboard::EMPTY);
        assert!(attacks.is_empty());
    }

    #[test]
    fn test_cannon_attack_with_hurdle() {
        let occ = Bitboard::from(Square::SQ_E6) | Bitboard::from(Square::SQ_E8);
        let attacks = sliding_attack_cannon(Square::SQ_E4, occ);
        assert!(!attacks.contains(Square::SQ_E5));
        assert!(!attacks.contains(Square::SQ_E6));
        assert!(attacks.contains(Square::SQ_E7));
        assert!(attacks.contains(Square::SQ_E8));
        assert!(!attacks.contains(Square::SQ_E9));
    }

    #[test]
    fn test_knight_attack_empty_board() {
        let attacks = lame_leaper_attack_knight(Square::SQ_E4, Bitboard::EMPTY);
        assert!(attacks.popcount() == 8);
    }

    #[test]
    fn test_knight_attack_blocked() {
        let occ = Bitboard::from(Square::SQ_E5);
        let attacks = lame_leaper_attack_knight(Square::SQ_E4, occ);
        assert!(!attacks.contains(Square::SQ_D6));
        assert!(!attacks.contains(Square::SQ_F6));
        assert!(attacks.popcount() == 6);
    }

    #[test]
    fn test_bishop_attack_empty_board() {
        let attacks = lame_leaper_attack_bishop(Square::SQ_C0, Bitboard::EMPTY);
        assert!(attacks.contains(Square::SQ_A2));
        assert!(attacks.contains(Square::SQ_E2));
    }

    #[test]
    fn test_bishop_restricted_to_half() {
        let attacks = lame_leaper_attack_bishop(Square::SQ_C4, Bitboard::EMPTY);
        for sq in attacks {
            assert!(sq.rank() as u8 <= 4);
        }
    }

    #[test]
    fn test_pawn_attacks_white_before_river() {
        let attacks = pawn_attacks_bb(Color::White, Square::SQ_E3);
        assert!(attacks.contains(Square::SQ_E4));
        assert!(!attacks.contains(Square::SQ_D3));
        assert!(!attacks.contains(Square::SQ_F3));
        assert_eq!(attacks.popcount(), 1);
    }

    #[test]
    fn test_pawn_attacks_white_after_river() {
        let attacks = pawn_attacks_bb(Color::White, Square::SQ_E5);
        assert!(attacks.contains(Square::SQ_E6));
        assert!(attacks.contains(Square::SQ_D5));
        assert!(attacks.contains(Square::SQ_F5));
        assert_eq!(attacks.popcount(), 3);
    }

    #[test]
    fn test_pawn_attacks_black_before_river() {
        let attacks = pawn_attacks_bb(Color::Black, Square::SQ_E6);
        assert!(attacks.contains(Square::SQ_E5));
        assert!(!attacks.contains(Square::SQ_D6));
        assert_eq!(attacks.popcount(), 1);
    }

    #[test]
    fn test_pawn_attacks_black_after_river() {
        let attacks = pawn_attacks_bb(Color::Black, Square::SQ_E4);
        assert!(attacks.contains(Square::SQ_E3));
        assert!(attacks.contains(Square::SQ_D4));
        assert!(attacks.contains(Square::SQ_F4));
        assert_eq!(attacks.popcount(), 3);
    }

    #[test]
    fn test_pseudo_attacks_king_in_palace() {
        let pa = PseudoAttacksTable::new();
        let king_attacks = pa.get(PieceType::King, Square::SQ_E0);
        assert!(king_attacks.contains(Square::SQ_D0));
        assert!(king_attacks.contains(Square::SQ_F0));
        assert!(king_attacks.contains(Square::SQ_E1));
        for sq in king_attacks {
            assert!(PALACE.contains(sq));
        }
    }

    #[test]
    fn test_pseudo_attacks_advisor_in_palace() {
        let pa = PseudoAttacksTable::new();
        let adv_attacks = pa.get(PieceType::Advisor, Square::SQ_E1);
        for sq in adv_attacks {
            assert!(PALACE.contains(sq));
        }
    }

    #[test]
    fn test_pseudo_attacks_rook() {
        let pa = PseudoAttacksTable::new();
        let rook_attacks = pa.get(PieceType::Rook, Square::SQ_E4);
        assert_eq!(rook_attacks.popcount(), 17);
    }
}
