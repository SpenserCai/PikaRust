use std::sync::LazyLock;

use crate::bitboard::{
    Bitboard, FILE_A_BB, FILE_C_BB, FILE_D_BB, FILE_E_BB, FILE_F_BB, FILE_G_BB, FILE_I_BB, HALF_BB,
    PALACE, PAWN_BB, RANK_0_BB, RANK_1_BB, RANK_2_BB, RANK_4_BB, RANK_5_BB, RANK_7_BB, RANK_8_BB,
    RANK_9_BB,
};
use crate::position::Position;
use crate::types::{Color, File, Piece, PieceType, Rank, Square};

use super::IndexList;

pub const HASH_VALUE: u32 = 0x7f23_4cb8;
pub const PS_NB: u32 = 689;
pub const ATTACK_BUCKET_NB: u32 = 4;
pub const KING_BUCKET_NB: u32 = 6;
pub const DIMENSIONS: u32 = KING_BUCKET_NB * ATTACK_BUCKET_NB * PS_NB;
pub const MAX_ACTIVE_DIMENSIONS: usize = 32;

pub const ALL_PIECES: [Piece; 14] = [
    Piece::W_ROOK,
    Piece::W_ADVISOR,
    Piece::W_CANNON,
    Piece::W_PAWN,
    Piece::W_KNIGHT,
    Piece::W_BISHOP,
    Piece::W_KING,
    Piece::B_ROOK,
    Piece::B_ADVISOR,
    Piece::B_CANNON,
    Piece::B_PAWN,
    Piece::B_KNIGHT,
    Piece::B_BISHOP,
    Piece::B_KING,
];

pub static VALID_BB: LazyLock<[Bitboard; Piece::NUM]> = LazyLock::new(compute_valid_bb);

fn compute_valid_bb() -> [Bitboard; Piece::NUM] {
    let full = HALF_BB[Color::White] | HALF_BB[Color::Black];
    let w_advisor = (RANK_0_BB | RANK_2_BB) & (FILE_D_BB | FILE_F_BB) | (RANK_1_BB & FILE_E_BB);
    let b_advisor = (RANK_7_BB | RANK_9_BB) & (FILE_D_BB | FILE_F_BB) | (RANK_8_BB & FILE_E_BB);
    let w_bishop = (RANK_0_BB | RANK_4_BB) & (FILE_C_BB | FILE_G_BB)
        | (RANK_2_BB & (FILE_A_BB | FILE_E_BB | FILE_I_BB));
    let b_bishop = (RANK_5_BB | RANK_9_BB) & (FILE_C_BB | FILE_G_BB)
        | (RANK_7_BB & (FILE_A_BB | FILE_E_BB | FILE_I_BB));
    let w_king = HALF_BB[Color::White] & PALACE & !FILE_F_BB;

    let mut bb = [Bitboard::EMPTY; Piece::NUM];
    bb[Piece::W_ROOK.index()] = full;
    bb[Piece::W_ADVISOR.index()] = w_advisor;
    bb[Piece::W_CANNON.index()] = full;
    bb[Piece::W_PAWN.index()] = PAWN_BB[Color::White];
    bb[Piece::W_KNIGHT.index()] = full;
    bb[Piece::W_BISHOP.index()] = w_bishop;
    bb[Piece::W_KING.index()] = w_king;
    bb[Piece::B_ROOK.index()] = full;
    bb[Piece::B_ADVISOR.index()] = b_advisor;
    bb[Piece::B_CANNON.index()] = full;
    bb[Piece::B_PAWN.index()] = PAWN_BB[Color::Black];
    bb[Piece::B_KNIGHT.index()] = full;
    bb[Piece::B_BISHOP.index()] = b_bishop;
    bb[Piece::B_KING.index()] = HALF_BB[Color::Black] & PALACE;
    bb
}

pub static PSQ_OFFSETS: LazyLock<[[u16; Square::NUM]; Piece::NUM]> =
    LazyLock::new(compute_psq_offsets);

fn compute_psq_offsets() -> [[u16; Square::NUM]; Piece::NUM] {
    let valid_bb = &*VALID_BB;
    let mut offsets = [[0u16; Square::NUM]; Piece::NUM];
    let mut cumulative: u16 = 0;
    for &pc in &ALL_PIECES {
        for sq_idx in 0..Square::NUM {
            let sq = Square::from_raw_unchecked(sq_idx as u8);
            if valid_bb[pc.index()].contains(sq) {
                offsets[pc.index()][sq_idx] = cumulative;
                cumulative += 1;
            }
        }
    }
    debug_assert!(cumulative == PS_NB as u16);
    offsets
}

const KING_BUCKET_RAW: [u8; Square::NUM] = {
    const M: u8 = 1 << 3;
    let mut table = [0u8; Square::NUM];
    // Rank 0: D0=3, E0=4, F0=5
    table[3] = 0;
    table[4] = 1;
    table[5] = M;
    // Rank 1: D1=12, E1=13, F1=14
    table[12] = 2;
    table[13] = 3;
    table[14] = M | 2;
    // Rank 2: D2=21, E2=22, F2=23
    table[21] = 4;
    table[22] = 5;
    table[23] = M | 4;
    // Rank 7: D7=66, E7=67, F7=68
    table[66] = 4;
    table[67] = 5;
    table[68] = M | 4;
    // Rank 8: D8=75, E8=76, F8=77
    table[75] = 2;
    table[76] = 3;
    table[77] = M | 2;
    // Rank 9: D9=84, E9=85, F9=86
    table[84] = 0;
    table[85] = 1;
    table[86] = M;
    table
};

pub struct KingBucketEntry {
    pub king_bucket: u8,
    pub mirror: bool,
}

pub static KING_BUCKETS: LazyLock<[[[KingBucketEntry; 2]; Square::NUM]; Square::NUM]> =
    LazyLock::new(compute_king_buckets);

fn compute_king_buckets() -> [[[KingBucketEntry; 2]; Square::NUM]; Square::NUM] {
    let default_entry = || KingBucketEntry {
        king_bucket: 0,
        mirror: false,
    };
    let mut table: Vec<[[KingBucketEntry; 2]; Square::NUM]> = Vec::with_capacity(Square::NUM);
    for _ in 0..Square::NUM {
        table.push(std::array::from_fn(|_| [default_entry(), default_entry()]));
    }

    for ksq in 0..Square::NUM {
        #[allow(clippy::needless_range_loop)]
        for oksq in 0..Square::NUM {
            for midm in 0..2usize {
                let king_bucket_raw = KING_BUCKET_RAW[ksq];
                let king_bucket = king_bucket_raw & 0x7;
                let oking_bucket = KING_BUCKET_RAW[oksq] & 0x7;
                let mirror = (king_bucket_raw >> 3) != 0
                    || ((king_bucket & 1) != 0
                        && ((KING_BUCKET_RAW[oksq] >> 3) != 0
                            || ((oking_bucket & 1) != 0 && midm != 0)));
                table[ksq][oksq][midm] = KingBucketEntry {
                    king_bucket,
                    mirror,
                };
            }
        }
    }

    let mut result: [[[KingBucketEntry; 2]; Square::NUM]; Square::NUM] =
        std::array::from_fn(|_| std::array::from_fn(|_| [default_entry(), default_entry()]));
    for (i, row) in table.into_iter().enumerate() {
        result[i] = row;
    }
    result
}

pub static INDEX_MAP: LazyLock<[[[u8; Square::NUM]; 2]; 2]> = LazyLock::new(compute_index_map);

fn compute_index_map() -> [[[u8; Square::NUM]; 2]; 2] {
    let mut v = [[[0u8; Square::NUM]; 2]; 2];
    for m in 0..2u8 {
        for r in 0..2u8 {
            for s in 0..Square::NUM {
                let sq = Square::from_raw_unchecked(s as u8);
                let mut ss = sq;
                if m != 0 {
                    ss = ss.flip_file();
                }
                if r != 0 {
                    ss = ss.flip_rank();
                }
                v[m as usize][r as usize][s] = ss.raw();
            }
        }
    }
    v
}

pub const BALANCE_ENCODING: u64 = 0xa4a9_2a74_e989_d3a7;

pub static MID_MIRROR_ENCODING: LazyLock<[[u64; Square::NUM]; Piece::NUM]> =
    LazyLock::new(compute_mid_mirror_encoding);

fn compute_mid_mirror_encoding() -> [[u64; Square::NUM]; Piece::NUM] {
    const SHIFTS: [[u8; 2]; 8] = [
        [0, 0],   // pt=0 (unused)
        [44, 0],  // ROOK=1
        [60, 36], // ADVISOR=2
        [47, 7],  // CANNON=3
        [53, 21], // PAWN=4
        [50, 14], // KNIGHT=5
        [57, 29], // BISHOP=6
        [0, 0],   // KING=7
    ];

    let mut encodings = [[0u64; Square::NUM]; Piece::NUM];

    for &c in &Color::ALL {
        for pt_val in PieceType::Rook as u8..=PieceType::King as u8 {
            let pt = PieceType::ALL[(pt_val - 1) as usize];
            for r_val in Rank::R0 as u8..Rank::NUM as u8 {
                for f_val in File::A as u8..File::NUM as u8 {
                    let mut encoding: u64 = 0;
                    if f_val != File::E as u8 && pt != PieceType::King {
                        let r_ = if c == Color::White {
                            r_val
                        } else {
                            Rank::R9 as u8 - r_val
                        };
                        let f_ = if f_val < File::E as u8 {
                            f_val
                        } else {
                            File::I as u8 - f_val
                        };
                        let [s1, s2] = SHIFTS[pt_val as usize];
                        let sq_val = u64::from(File::D as u8 - f_) * 10 + u64::from(r_);
                        encoding = (1u64 << s1) | (sq_val << s2);
                        if f_val >= File::E as u8 {
                            encoding = (-(encoding as i64)) as u64;
                        }
                    } else if f_val != File::E as u8 && pt == PieceType::King {
                        encoding = 1u64 << 63;
                    }
                    let pc = Piece::make(c, pt);
                    let sq = Square::make(
                        // SAFETY: f_val < 9
                        unsafe { std::mem::transmute(f_val) },
                        // SAFETY: r_val < 10
                        unsafe { std::mem::transmute(r_val) },
                    );
                    encodings[pc.index()][sq.index()] = encoding;
                }
            }
        }
    }
    encodings
}

pub fn requires_mid_mirror(pos: &Position, c: Color) -> bool {
    let my_enc = pos.mid_encoding(c);
    let opp_enc = pos.mid_encoding(!c);
    ((1u64 << 63) & my_enc & opp_enc) != 0
        && (my_enc < BALANCE_ENCODING || (my_enc == BALANCE_ENCODING && opp_enc < BALANCE_ENCODING))
}

pub fn make_attack_bucket(pos: &Position, c: Color) -> u32 {
    let rook_count = pos.count_type(c, PieceType::Rook);
    let knight_count = pos.count_type(c, PieceType::Knight);
    let cannon_count = pos.count_type(c, PieceType::Cannon);
    u32::from(rook_count > 0) * 2 + u32::from(knight_count + cannon_count > 0)
}

pub fn make_feature_bucket(perspective: Color, pos: &Position) -> (u32, bool, u32) {
    let ksq = pos.king_square(perspective);
    let oksq = pos.king_square(!perspective);
    let mid_mirror = requires_mid_mirror(pos, perspective);
    let entry = &KING_BUCKETS[ksq.index()][oksq.index()][usize::from(mid_mirror)];
    let king_bucket = u32::from(entry.king_bucket);
    let mirror = entry.mirror;
    let attack_bucket = make_attack_bucket(pos, perspective);
    let bucket = king_bucket * ATTACK_BUCKET_NB + attack_bucket;
    (bucket, mirror, attack_bucket)
}

pub fn make_layer_stack_bucket(pos: &Position) -> u32 {
    static LAYER_STACK_BUCKETS: LazyLock<[[[[u8; 5]; 5]; 3]; 3]> =
        LazyLock::new(compute_layer_stack_buckets);

    fn compute_layer_stack_buckets() -> [[[[u8; 5]; 5]; 3]; 3] {
        let mut v = [[[[0u8; 5]; 5]; 3]; 3];
        #[allow(clippy::needless_range_loop)]
        for us_rook in 0..3usize {
            for opp_rook in 0..3usize {
                for us_kc in 0..5usize {
                    for opp_kc in 0..5usize {
                        v[us_rook][opp_rook][us_kc][opp_kc] = if us_rook == opp_rook {
                            (us_rook * 4
                                + usize::from(us_kc + opp_kc >= 4) * 2
                                + usize::from(us_kc == opp_kc)) as u8
                        } else if us_rook == 2 && opp_rook == 1 {
                            12
                        } else if us_rook == 1 && opp_rook == 2 {
                            13
                        } else if us_rook > 0 && opp_rook == 0 {
                            14
                        } else {
                            15
                        };
                    }
                }
            }
        }
        v
    }

    let us = pos.side_to_move();
    let us_rook = pos.count_type(us, PieceType::Rook) as usize;
    let opp_rook = pos.count_type(!us, PieceType::Rook) as usize;
    let us_kc =
        (pos.count_type(us, PieceType::Knight) + pos.count_type(us, PieceType::Cannon)) as usize;
    let opp_kc =
        (pos.count_type(!us, PieceType::Knight) + pos.count_type(!us, PieceType::Cannon)) as usize;
    u32::from(LAYER_STACK_BUCKETS[us_rook][opp_rook][us_kc][opp_kc])
}

#[inline]
fn index_map(mirror: bool, is_black: bool, sq: Square) -> u8 {
    INDEX_MAP[usize::from(mirror)][usize::from(is_black)][sq.index()]
}

pub fn make_index(perspective: Color, sq: Square, pc: Piece, bucket: u32, mirror: bool) -> u32 {
    let mapped_sq = index_map(mirror, perspective == Color::Black, sq);
    let mapped_pc = if perspective == Color::Black {
        pc.flip_color()
    } else {
        pc
    };
    let offset = PSQ_OFFSETS[mapped_pc.index()][mapped_sq as usize];
    u32::from(offset) + PS_NB * bucket
}

pub fn append_active_indices(pos: &Position, perspective: Color, active: &mut IndexList) {
    let (bucket, mirror, _) = make_feature_bucket(perspective, pos);
    let occupied = pos.all_pieces();

    let mut bb = occupied;
    while bb.is_not_empty() {
        let sq = bb.pop_lsb();
        let pc = pos.piece_on(sq);
        if pc == Piece::NONE {
            continue;
        }
        active.push(make_index(perspective, sq, pc, bucket, mirror));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn append_changed_indices(
    perspective: Color,
    bucket: u32,
    mirror: bool,
    from: Square,
    to: Square,
    pc: Piece,
    remove_sq: Square,
    remove_pc: Piece,
    removed: &mut IndexList,
    added: &mut IndexList,
) {
    removed.push(make_index(perspective, from, pc, bucket, mirror));

    if to != Square::NONE {
        added.push(make_index(perspective, to, pc, bucket, mirror));
    }

    if remove_sq != Square::NONE {
        removed.push(make_index(
            perspective,
            remove_sq,
            remove_pc,
            bucket,
            mirror,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_bb_piece_counts() {
        let vbb = &*VALID_BB;
        assert_eq!(vbb[Piece::W_ROOK.index()].popcount(), 90);
        assert_eq!(vbb[Piece::W_CANNON.index()].popcount(), 90);
        assert_eq!(vbb[Piece::W_KNIGHT.index()].popcount(), 90);
        assert_eq!(vbb[Piece::W_ADVISOR.index()].popcount(), 5);
        assert_eq!(vbb[Piece::B_ADVISOR.index()].popcount(), 5);
        assert_eq!(vbb[Piece::W_BISHOP.index()].popcount(), 7);
        assert_eq!(vbb[Piece::B_BISHOP.index()].popcount(), 7);
        assert_eq!(vbb[Piece::W_KING.index()].popcount(), 6);
        assert_eq!(vbb[Piece::B_KING.index()].popcount(), 9);
    }

    #[test]
    fn test_psq_offsets_total() {
        let offsets = &*PSQ_OFFSETS;
        let mut max_offset: u16 = 0;
        let vbb = &*VALID_BB;
        for &pc in &ALL_PIECES {
            for sq_idx in 0..Square::NUM {
                let sq = Square::from_raw_unchecked(sq_idx as u8);
                if vbb[pc.index()].contains(sq) {
                    let off = offsets[pc.index()][sq_idx];
                    if off > max_offset {
                        max_offset = off;
                    }
                }
            }
        }
        assert_eq!(max_offset + 1, PS_NB as u16);
    }

    #[test]
    fn test_index_map_identity() {
        for s in 0..Square::NUM {
            let sq = Square::from_raw_unchecked(s as u8);
            assert_eq!(index_map(false, false, sq), sq.raw());
        }
    }

    #[test]
    fn test_index_map_mirror() {
        let sq = Square::SQ_A0;
        let mapped = index_map(true, false, sq);
        assert_eq!(mapped, Square::SQ_I0.raw());
    }

    #[test]
    fn test_index_map_rotate() {
        let sq = Square::SQ_A0;
        let mapped = index_map(false, true, sq);
        assert_eq!(mapped, Square::SQ_A9.raw());
    }

    #[test]
    fn test_king_bucket_d0() {
        let entry = &KING_BUCKETS[Square::SQ_D0.index()][Square::SQ_D9.index()][0];
        assert_eq!(entry.king_bucket, 0);
        assert!(!entry.mirror);
    }

    #[test]
    fn test_king_bucket_f0_mirrors() {
        let entry = &KING_BUCKETS[Square::SQ_F0.index()][Square::SQ_D9.index()][0];
        assert_eq!(entry.king_bucket, 0);
        assert!(entry.mirror);
    }

    #[test]
    fn test_mid_mirror_encoding_center_file() {
        let enc = &*MID_MIRROR_ENCODING;
        let sq_e4 = Square::SQ_E4;
        for &pc in &ALL_PIECES {
            if pc.piece_type() != PieceType::King {
                assert_eq!(enc[pc.index()][sq_e4.index()], 0);
            }
        }
    }

    #[test]
    fn test_balance_encoding() {
        assert_eq!(BALANCE_ENCODING, 0xa4a9_2a74_e989_d3a7);
    }

    #[test]
    fn test_dimensions() {
        assert_eq!(DIMENSIONS, 6 * 4 * 689);
        assert_eq!(DIMENSIONS, 16536);
    }
}
