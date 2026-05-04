#![deny(unsafe_code)]

#[allow(
    clippy::missing_const_for_fn,
    clippy::manual_range_contains,
    clippy::use_self
)]
mod attacks;
#[allow(clippy::module_inception, clippy::use_self)]
mod bitboard;
mod magic;
#[allow(clippy::unreadable_literal)]
mod magic_numbers;
#[allow(clippy::missing_const_for_fn)]
mod tables;

pub use attacks::{
    PSEUDO_ATTACKS_SIZE, PseudoAttacksTable, lame_leaper_attack_bishop, lame_leaper_attack_knight,
    lame_leaper_attack_knight_to, lame_leaper_path_bishop, lame_leaper_path_dir_bishop,
    lame_leaper_path_dir_knight, lame_leaper_path_dir_knight_to, lame_leaper_path_knight,
    lame_leaper_path_knight_to, pawn_attacks_bb, pawn_attacks_to_bb, sliding_attack_cannon,
    sliding_attack_rook,
};
pub use bitboard::Bitboard;
pub use magic::{
    attacks_bb_bishop, attacks_bb_cannon, attacks_bb_knight, attacks_bb_knight_to, attacks_bb_rook,
    ensure_initialized,
};
pub use tables::{
    FILE_A_BB, FILE_B_BB, FILE_BB, FILE_C_BB, FILE_D_BB, FILE_E_BB, FILE_F_BB, FILE_G_BB,
    FILE_H_BB, FILE_I_BB, FILE_NB, HALF_BB, PALACE, PAWN_BB, RANK_0_BB, RANK_1_BB, RANK_2_BB,
    RANK_3_BB, RANK_4_BB, RANK_5_BB, RANK_6_BB, RANK_7_BB, RANK_8_BB, RANK_9_BB, RANK_BB,
    SQUARE_BB, SQUARE_DISTANCE, file_bb_of, file_distance, rank_bb_of, rank_distance,
    safe_destination, shift, square_bb, square_distance,
};

use std::sync::LazyLock;

use crate::types::Square;

static PSEUDO_ATTACKS: LazyLock<PseudoAttacksTable> = LazyLock::new(PseudoAttacksTable::new);

/// Returns a reference to the shared, lazily-initialized pseudo-attacks table.
#[inline]
pub fn pseudo_attacks() -> &'static PseudoAttacksTable {
    &PSEUDO_ATTACKS
}

static LEAPER_PASS_TABLE: LazyLock<[[Bitboard; Square::NUM]; Square::NUM]> = LazyLock::new(|| {
    let pseudo = pseudo_attacks();
    let mut table = [[Bitboard::EMPTY; Square::NUM]; Square::NUM];
    for (s1_idx, row) in table.iter_mut().enumerate() {
        let s1 = Square::from_raw_unchecked(s1_idx as u8);
        for (s2_idx, cell) in row.iter_mut().enumerate() {
            let s2 = Square::from_raw_unchecked(s2_idx as u8);
            if (pseudo.unconstrained_king(s1) & Bitboard::from(s2)).is_not_empty() {
                *cell |=
                    pseudo.get(crate::types::PieceType::Knight, s1) & pseudo.unconstrained_advisor(s2);
            }
            if (pseudo.unconstrained_advisor(s1) & Bitboard::from(s2)).is_not_empty() {
                *cell |=
                    pseudo.get(crate::types::PieceType::Bishop, s1) & pseudo.unconstrained_advisor(s2);
            }
        }
    }
    table
});

#[inline]
pub fn leaper_pass_bb(s1: Square, s2: Square) -> Bitboard {
    LEAPER_PASS_TABLE[s1.index()][s2.index()]
}

pub fn between_bb(s1: Square, s2: Square) -> Bitboard {
    let pseudo_rook_s1 = attacks_bb_rook(s1, Bitboard::EMPTY);

    let mut result = if (pseudo_rook_s1 & s2).is_not_empty() {
        attacks_bb_rook(s1, Bitboard::from(s2)) & attacks_bb_rook(s2, Bitboard::from(s1))
    } else {
        Bitboard::EMPTY
    };

    let pseudo_knight_s1 = attacks_bb_knight(s1, Bitboard::EMPTY);
    if (pseudo_knight_s1 & s2).is_not_empty() {
        let d_raw = i16::from(s2.raw()) - i16::from(s1.raw());
        result |= lame_leaper_path_dir_knight_to(crate::types::Direction(d_raw as i8), s1);
    }

    result | s2
}

pub fn line_bb(s1: Square, s2: Square) -> Bitboard {
    let pseudo_rook_s1 = attacks_bb_rook(s1, Bitboard::EMPTY);
    let pseudo_rook_s2 = attacks_bb_rook(s2, Bitboard::EMPTY);

    if (pseudo_rook_s1 & s2).is_not_empty() {
        (pseudo_rook_s1 & pseudo_rook_s2) | s1 | s2
    } else {
        Bitboard::EMPTY
    }
}

pub fn ray_pass_bb(s1: Square, s2: Square) -> Bitboard {
    let pseudo_rook_s1 = attacks_bb_rook(s1, Bitboard::EMPTY);
    if (pseudo_rook_s1 & s2).is_not_empty() {
        attacks_bb_cannon(s1, Bitboard::from(s2))
    } else {
        Bitboard::EMPTY
    }
}

#[inline]
pub fn aligned(s1: Square, s2: Square, s3: Square) -> bool {
    (line_bb(s1, s2) & s3).is_not_empty()
}
