// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2004-2026 The Stockfish developers (see notices/upstream/Pikafish-AUTHORS)
// Copyright (c) 2026 SpenserCai and PikaRust contributors
// Rust adaptation and modifications, 2026; see NOTICE.md for upstream sources.
// Search/evaluation aligned with Pikafish b562d6ae, 2026-09-28.
// Distributed without warranty; see LICENSE and notices/upstream/Pikafish-COPYRIGHT.

use crate::position::Position;
use crate::types::{
    Color, PIECE_VALUE, Piece, PieceType, VALUE_MATE_IN_MAX_PLY, VALUE_MATED_IN_MAX_PLY, Value,
};

pub fn evaluate(
    pos: &Position,
    nnue_psqt: Value,
    nnue_positional: Value,
    optimism: Value,
) -> Value {
    let nnue = nnue_psqt + nnue_positional;
    let us = pos.side_to_move();
    let mut simple = pos.major_material(us) - pos.major_material(!us);
    let mut material = pos.total_major_material();
    for pt in [PieceType::Pawn, PieceType::Advisor, PieceType::Bishop] {
        let value = PIECE_VALUE[Piece::make(Color::White, pt)];
        let ours = i32::from(pos.count_type(us, pt));
        let theirs = i32::from(pos.count_type(!us, pt));
        simple += value * (ours - theirs);
        material += value * (ours + theirs);
    }
    let simple_norm = simple * 1024 / (simple.abs() + 1024);
    let nnue_norm = nnue * 1024 / (nnue.abs() + 1024);
    let alignment = simple_norm * nnue_norm / 512;
    let base_eval = nnue + nnue * alignment / 65_536 + optimism * alignment / 16_384;
    let mut v = (i64::from(base_eval) * i64::from(80_030 + material) / 80_030) as Value;

    let rule60 = pos.rule60_count();
    v -= v * rule60 / 244;

    v.clamp(VALUE_MATED_IN_MAX_PLY + 1, VALUE_MATE_IN_MAX_PLY - 1)
}

pub fn evaluate_simple(pos: &Position, optimism: Value) -> Value {
    let us = pos.side_to_move();
    let them = !us;
    let material = pos.major_material(us) - pos.major_material(them);
    let v = material + optimism / 16;
    v.clamp(VALUE_MATED_IN_MAX_PLY + 1, VALUE_MATE_IN_MAX_PLY - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_clamping() {
        let pos = Position::start_pos().expect("start_pos should parse");
        let v = evaluate(&pos, 0, 0, 0);
        assert!(v > VALUE_MATED_IN_MAX_PLY);
        assert!(v < VALUE_MATE_IN_MAX_PLY);
    }

    #[test]
    fn test_evaluate_simple_start_pos() {
        let pos = Position::start_pos().expect("start_pos should parse");
        let v = evaluate_simple(&pos, 0);
        assert_eq!(v, 0, "start position should be equal material");
    }

    #[test]
    fn test_evaluate_with_optimism() {
        let pos = Position::start_pos().expect("start_pos should parse");
        let v_pos = evaluate(&pos, 100, 50, 200);
        let v_neg = evaluate(&pos, 100, 50, -200);
        // Equal material makes alignment zero, so optimism contributes zero.
        assert_eq!(v_pos, 176);
        assert_eq!(v_neg, 176);
    }

    #[test]
    fn test_evaluate_rule60_dampening() {
        let pos = Position::start_pos().expect("start_pos should parse");
        let v = evaluate(&pos, 500, 300, 100);
        // With rule60 = 0 at start, no dampening
        assert!(v != 0 || (500 + 300 == 0));
        // The value should be reasonable
        assert!(v.abs() < 10000);
    }

    // -------------------------------------------------------------------
    // NNUE evaluation smoke tests
    // -------------------------------------------------------------------

    #[test]
    fn test_evaluate_startpos_near_zero() {
        // Start position is symmetric, so NNUE-like evaluation should be near 0.
        // Using evaluate with balanced NNUE values (psqt=0, positional=0) should give ~0.
        let pos = Position::start_pos().expect("start_pos should parse");
        let v = evaluate(&pos, 0, 0, 0);
        assert_eq!(
            v, 0,
            "symmetric position with zero NNUE should evaluate to 0"
        );
    }

    #[test]
    fn test_evaluate_positive_nnue_gives_positive_score() {
        let pos = Position::start_pos().expect("start_pos should parse");
        // Positive NNUE values should produce a positive evaluation
        let v = evaluate(&pos, 200, 100, 0);
        assert!(v > 0, "positive NNUE should give positive eval, got {v}");
    }

    #[test]
    fn test_evaluate_negative_nnue_gives_negative_score() {
        let pos = Position::start_pos().expect("start_pos should parse");
        // Negative NNUE values should produce a negative evaluation
        let v = evaluate(&pos, -200, -100, 0);
        assert!(v < 0, "negative NNUE should give negative eval, got {v}");
    }

    #[test]
    fn test_evaluate_simple_material_advantage() {
        // Position where white has a rook advantage
        let fen = "4k4/9/9/9/9/9/9/9/9/4K3R w - - 0 1";
        let pos = Position::from_fen(fen).expect("should parse");
        let v = evaluate_simple(&pos, 0);
        assert!(
            v > 0,
            "white with extra rook should have positive eval, got {v}"
        );
    }

    #[test]
    fn test_evaluate_simple_material_disadvantage() {
        // Position where white is down a rook (black has extra rook)
        let fen = "4k3r/9/9/9/9/9/9/9/9/4K4 w - - 0 1";
        let pos = Position::from_fen(fen).expect("should parse");
        let v = evaluate_simple(&pos, 0);
        assert!(
            v < 0,
            "white down a rook should have negative eval, got {v}"
        );
    }

    #[test]
    fn test_evaluate_never_exceeds_mate_bounds() {
        // Even with extreme NNUE values, evaluate should clamp
        let pos = Position::start_pos().expect("start_pos should parse");
        let v = evaluate(&pos, 30000, 30000, 30000);
        assert!(v < VALUE_MATE_IN_MAX_PLY, "should be clamped below mate");
        assert!(v > VALUE_MATED_IN_MAX_PLY, "should be clamped above -mate");

        let v2 = evaluate(&pos, -30000, -30000, -30000);
        assert!(v2 < VALUE_MATE_IN_MAX_PLY);
        assert!(v2 > VALUE_MATED_IN_MAX_PLY);
    }

    #[test]
    fn test_evaluate_uses_sum_of_network_outputs() {
        let pos = Position::start_pos().expect("start_pos should parse");
        // The current upstream scaling takes one raw NNUE value; the split is
        // retained only to keep the legacy model evaluation API usable.
        assert_eq!(evaluate(&pos, 500, 500, 0), 1178);
        assert_eq!(evaluate(&pos, 1000, 0, 0), 1178);
    }

    #[test]
    fn test_evaluate_optimism_effect() {
        let pos = Position::from_fen("3k5/9/9/9/9/9/9/9/9/4K3R w - - 0 1").unwrap();
        // Independently calculated from b562d6ae evaluate.cpp with RookValue=1305.
        assert_eq!(evaluate(&pos, 300, 200, 0), 510);
        assert_eq!(evaluate(&pos, 300, 200, 500), 521);
        assert_eq!(evaluate(&pos, 300, 200, -500), 499);
        assert_eq!(evaluate(&pos, -300, -200, 500), -517);
    }
}
