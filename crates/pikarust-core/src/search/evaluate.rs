use crate::position::Position;
use crate::types::{VALUE_MATE_IN_MAX_PLY, VALUE_MATED_IN_MAX_PLY, Value};

pub fn evaluate(
    pos: &Position,
    nnue_psqt: Value,
    nnue_positional: Value,
    optimism: Value,
) -> Value {
    let nnue = nnue_psqt + nnue_positional;
    let nnue_complexity = (nnue_psqt - nnue_positional).abs();

    let adjusted_optimism = optimism + optimism * nnue_complexity / 465;
    let adjusted_nnue = nnue - nnue * nnue_complexity / 11743;

    let material = pos.total_major_material();

    let mut v =
        (adjusted_nnue * (17380 + material) + adjusted_optimism * (3061 + material)) / 20582;

    let rule60 = pos.rule60_count();
    v -= v * rule60 / 253;

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
        assert!(v_pos > v_neg);
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
    fn test_evaluate_complexity_effect() {
        let pos = Position::start_pos().expect("start_pos should parse");
        // When psqt and positional are far apart, complexity is high
        // This should reduce the absolute value of the evaluation
        let v_low_complexity = evaluate(&pos, 500, 500, 0);
        let v_high_complexity = evaluate(&pos, 1000, 0, 0);
        // Both have same sum (1000) but different complexity
        // High complexity should dampen the score more
        assert!(
            v_low_complexity.abs() >= v_high_complexity.abs(),
            "high complexity should dampen: low={v_low_complexity}, high={v_high_complexity}"
        );
    }

    #[test]
    fn test_evaluate_optimism_effect() {
        let pos = Position::start_pos().expect("start_pos should parse");
        // Optimism should shift the evaluation
        let v_no_opt = evaluate(&pos, 300, 200, 0);
        let v_pos_opt = evaluate(&pos, 300, 200, 500);
        let v_neg_opt = evaluate(&pos, 300, 200, -500);
        assert!(
            v_pos_opt > v_no_opt,
            "positive optimism should increase eval"
        );
        assert!(
            v_neg_opt < v_no_opt,
            "negative optimism should decrease eval"
        );
    }
}
