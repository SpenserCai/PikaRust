use crate::types::{Color, PieceType, Value};

const WDL_A: [f64; 4] = [
    220.598_913_65,
    -810.357_304_30,
    928.681_851_98,
    79.839_554_23,
];
const WDL_B: [f64; 4] = [
    61.992_874_16,
    -233.726_741_82,
    325.855_083_22,
    -68.727_208_54,
];

fn win_rate_params(piece_counts: &[u8; 16]) -> (f64, f64) {
    let count_both = |pt: PieceType| -> i32 {
        i32::from(piece_counts[crate::types::Piece::make(Color::White, pt).index()])
            + i32::from(piece_counts[crate::types::Piece::make(Color::Black, pt).index()])
    };

    let material = 10 * count_both(PieceType::Rook)
        + 5 * count_both(PieceType::Knight)
        + 5 * count_both(PieceType::Cannon)
        + 3 * count_both(PieceType::Bishop)
        + 2 * count_both(PieceType::Advisor)
        + count_both(PieceType::Pawn);

    let m = f64::from(material.clamp(17, 110)) / 65.0;

    let a = WDL_A[0]
        .mul_add(m, WDL_A[1])
        .mul_add(m, WDL_A[2])
        .mul_add(m, WDL_A[3]);
    let b = WDL_B[0]
        .mul_add(m, WDL_B[1])
        .mul_add(m, WDL_B[2])
        .mul_add(m, WDL_B[3]);

    (a, b)
}

fn win_rate_model(v: Value, piece_counts: &[u8; 16]) -> i32 {
    let (a, b) = win_rate_params(piece_counts);
    (0.5 + 1000.0 / (1.0 + ((a - f64::from(v)) / b).exp())) as i32
}

pub fn to_cp(v: Value, piece_counts: &[u8; 16]) -> i32 {
    let (a, _) = win_rate_params(piece_counts);
    (100.0 * f64::from(v) / a).round() as i32
}

pub fn wdl(v: Value, piece_counts: &[u8; 16]) -> (i32, i32, i32) {
    let w = win_rate_model(v, piece_counts);
    let l = win_rate_model(-v, piece_counts);
    let d = 1000 - w - l;
    (w, d, l)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Piece;

    fn startpos_counts() -> [u8; 16] {
        let mut counts = [0u8; 16];
        counts[Piece::W_ROOK.index()] = 2;
        counts[Piece::W_KNIGHT.index()] = 2;
        counts[Piece::W_BISHOP.index()] = 2;
        counts[Piece::W_ADVISOR.index()] = 2;
        counts[Piece::W_CANNON.index()] = 2;
        counts[Piece::W_PAWN.index()] = 5;
        counts[Piece::B_ROOK.index()] = 2;
        counts[Piece::B_KNIGHT.index()] = 2;
        counts[Piece::B_BISHOP.index()] = 2;
        counts[Piece::B_ADVISOR.index()] = 2;
        counts[Piece::B_CANNON.index()] = 2;
        counts[Piece::B_PAWN.index()] = 5;
        counts
    }

    #[test]
    fn test_win_rate_params_startpos() {
        let (a, b) = win_rate_params(&startpos_counts());
        assert!(a > 0.0, "a should be positive");
        assert!(b > 0.0, "b should be positive");
    }

    #[test]
    fn test_wdl_zero_score_symmetric() {
        let counts = startpos_counts();
        let (w, d, l) = wdl(0, &counts);
        assert_eq!(w, l, "W and L should be equal at score 0");
        assert_eq!(w + d + l, 1000, "W+D+L should sum to 1000");
    }

    #[test]
    fn test_wdl_positive_score() {
        let counts = startpos_counts();
        let (w, d, l) = wdl(200, &counts);
        assert!(w > l, "positive score should have W > L");
        assert_eq!(w + d + l, 1000);
    }

    #[test]
    fn test_wdl_negative_score() {
        let counts = startpos_counts();
        let (w, d, l) = wdl(-200, &counts);
        assert!(l > w, "negative score should have L > W");
        assert_eq!(w + d + l, 1000);
    }

    #[test]
    fn test_to_cp_zero() {
        let counts = startpos_counts();
        assert_eq!(to_cp(0, &counts), 0);
    }

    #[test]
    fn test_to_cp_positive() {
        let counts = startpos_counts();
        let cp = to_cp(100, &counts);
        assert!(cp > 0, "positive value should give positive cp");
    }

    #[test]
    fn test_to_cp_symmetry() {
        let counts = startpos_counts();
        let cp_pos = to_cp(150, &counts);
        let cp_neg = to_cp(-150, &counts);
        assert_eq!(cp_pos, -cp_neg, "to_cp should be antisymmetric");
    }

    #[test]
    fn test_wdl_sum_always_1000() {
        let counts = startpos_counts();
        for v in [-500, -200, -100, -50, 0, 50, 100, 200, 500] {
            let (w, d, l) = wdl(v, &counts);
            assert_eq!(w + d + l, 1000, "W+D+L must be 1000 for v={v}");
        }
    }

    #[test]
    fn test_material_clamping() {
        let mut counts = [0u8; 16];
        counts[Piece::W_PAWN.index()] = 1;
        let (a, b) = win_rate_params(&counts);
        assert!(a > 0.0 && b > 0.0, "should handle low material");

        let full = startpos_counts();
        let (a2, b2) = win_rate_params(&full);
        assert!(a2 > 0.0 && b2 > 0.0, "should handle full material");
    }
}
