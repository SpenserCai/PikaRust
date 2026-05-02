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
}
