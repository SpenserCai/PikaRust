//! Diagnostic test that simulates the exact search integration pattern for
//! threat incremental accumulator updates.
//!
//! Background: bench position 7 at depth 13 produces wrong node counts when
//! threat incremental update is wired into search. Unit tests pass because they
//! test `update_threat_accumulator_incremental` directly (prev → current), but
//! the search uses an `AccumulatorStack` with push/pop, null-move, and pruning
//! patterns that may expose bugs in the stack-based incremental path.

use pikarust_core::nnue::feature_transformer::{
    refresh_threat_accumulator, update_threat_accumulator_incremental,
};
use pikarust_core::nnue::features::half_ka_v2_hm;
use pikarust_core::nnue::{Accumulator, AccumulatorStack, DiffType, DirtyThreats, Network, NnueModel};
use pikarust_core::position::{GenType, generate};
use pikarust_core::position::Position;
use pikarust_core::types::Color;

const BENCH7_FEN: &str =
    "2b1ka2r/3na2c1/4b3n/8R/8C/4C1P2/P1P1P3P/4B1N2/1r2A4/2BAK4 w - - 0 1";

fn load_network() -> Option<Network> {
    let path = std::path::Path::new("../../models/pikafish.nnue");
    if !path.exists() {
        return None;
    }
    Some(Network::new(NnueModel::load(path).expect("model load")))
}

/// Compute mirror flags for both perspectives.
fn mirrors(pos: &Position) -> [bool; 2] {
    [
        half_ka_v2_hm::make_feature_bucket(Color::White, pos).1,
        half_ka_v2_hm::make_feature_bucket(Color::Black, pos).1,
    ]
}

/// Do a move, collecting DirtyThreats and setting requires_refresh from mirror changes.
fn do_move_collect_threats(pos: &mut Position, m: pikarust_core::types::Move) -> DirtyThreats {
    let mirror_before = mirrors(pos);
    let gives_check = pos.gives_check(m);
    let mut dts = DirtyThreats::new();
    pos.do_move_with_threats(m, gives_check, &mut dts);
    let mirror_after = mirrors(pos);
    dts.requires_refresh[0] = mirror_before[0] != mirror_after[0];
    dts.requires_refresh[1] = mirror_before[1] != mirror_after[1];
    dts
}

/// Compare two accumulators, printing diagnostics on mismatch. Returns true if equal.
fn assert_acc_eq(label: &str, inc: &Accumulator, full: &Accumulator) {
    for c in 0..2 {
        if inc.accumulation[c] != full.accumulation[c] {
            let mut diffs = Vec::new();
            for (i, (&a, &b)) in inc.accumulation[c]
                .iter()
                .zip(full.accumulation[c].iter())
                .enumerate()
            {
                if a != b {
                    diffs.push((i, a, b));
                }
            }
            panic!(
                "{label}: accumulation mismatch perspective={c}, {} diffs (first 5: {:?})",
                diffs.len(),
                &diffs[..diffs.len().min(5)]
            );
        }
        if inc.psqt_accumulation[c] != full.psqt_accumulation[c] {
            panic!(
                "{label}: psqt mismatch perspective={c}, inc={:?} full={:?}",
                inc.psqt_accumulation[c], full.psqt_accumulation[c]
            );
        }
    }
}

// ---------------------------------------------------------------
// Test 1: 1-ply using AccumulatorStack (search pattern)
// ---------------------------------------------------------------
#[test]
fn test_1ply_stack_based_incremental() {
    let Some(net) = load_network() else { return };
    let model = net.model();
    let simd = net.simd();

    let mut pos = Position::from_fen(BENCH7_FEN).expect("parse fen");
    let mut stack = AccumulatorStack::new(128);

    // Initialize root threat accumulator
    refresh_threat_accumulator(model, &pos, &mut stack.current_threat_mut().acc, simd);

    let ml = generate(&pos, GenType::Legal);
    for i in 0..ml.len() {
        let m = ml.get(i);
        let dts = do_move_collect_threats(&mut pos, m);

        stack.push();
        stack.set_threat_diff(dts.clone());

        // Incremental via stack
        if let Some((prev, current)) = stack.prev_and_current_threat_mut() {
            if let DiffType::DirtyThreats(ref dt) = current.diff {
                let dt_copy = dt.clone();
                update_threat_accumulator_incremental(
                    model, &pos, &prev.acc, &mut current.acc, &dt_copy, simd,
                );
            }
        }

        // Full refresh for comparison
        let mut ref_acc = Accumulator::new();
        refresh_threat_accumulator(model, &pos, &mut ref_acc, simd);

        assert_acc_eq(
            &format!("1ply move {i} ({m:?})"),
            &stack.current_threat().acc,
            &ref_acc,
        );

        stack.pop();
        pos.undo_move(m);
    }
}

// ---------------------------------------------------------------
// Test 2: 2-ply using AccumulatorStack (limited to first 5 moves)
// ---------------------------------------------------------------
#[test]
fn test_2ply_stack_based_incremental() {
    let Some(net) = load_network() else { return };
    let model = net.model();
    let simd = net.simd();

    let mut pos = Position::from_fen(BENCH7_FEN).expect("parse fen");
    let mut stack = AccumulatorStack::new(128);

    refresh_threat_accumulator(model, &pos, &mut stack.current_threat_mut().acc, simd);

    let ml1 = generate(&pos, GenType::Legal);
    let limit1 = ml1.len().min(5);

    for i in 0..limit1 {
        let m1 = ml1.get(i);
        let dts1 = do_move_collect_threats(&mut pos, m1);

        stack.push();
        stack.set_threat_diff(dts1);

        // Compute threat acc for ply 1 (incremental from root)
        if let Some((prev, current)) = stack.prev_and_current_threat_mut() {
            if let DiffType::DirtyThreats(ref dt) = current.diff {
                let dt_copy = dt.clone();
                update_threat_accumulator_incremental(
                    model, &pos, &prev.acc, &mut current.acc, &dt_copy, simd,
                );
            }
        }

        let ml2 = generate(&pos, GenType::Legal);
        let limit2 = ml2.len().min(5);

        for j in 0..limit2 {
            let m2 = ml2.get(j);
            let dts2 = do_move_collect_threats(&mut pos, m2);

            stack.push();
            stack.set_threat_diff(dts2);

            // Compute threat acc for ply 2 (incremental from ply 1)
            if let Some((prev, current)) = stack.prev_and_current_threat_mut() {
                if let DiffType::DirtyThreats(ref dt) = current.diff {
                    let dt_copy = dt.clone();
                    update_threat_accumulator_incremental(
                        model, &pos, &prev.acc, &mut current.acc, &dt_copy, simd,
                    );
                }
            }

            let mut ref_acc = Accumulator::new();
            refresh_threat_accumulator(model, &pos, &mut ref_acc, simd);

            assert_acc_eq(
                &format!("2ply m1={m1:?}(i={i}) m2={m2:?}(j={j})"),
                &stack.current_threat().acc,
                &ref_acc,
            );

            stack.pop();
            pos.undo_move(m2);
        }

        stack.pop();
        pos.undo_move(m1);
    }
}

// ---------------------------------------------------------------
// Test 3: Null-move scenario
//   push m1, compute threat, do_null_move (NO push), then push m2
//   The null move doesn't push the stack, so m2's prev is m1's level.
// ---------------------------------------------------------------
#[test]
fn test_null_move_scenario() {
    let Some(net) = load_network() else { return };
    let model = net.model();
    let simd = net.simd();

    let mut pos = Position::from_fen(BENCH7_FEN).expect("parse fen");
    let mut stack = AccumulatorStack::new(128);

    refresh_threat_accumulator(model, &pos, &mut stack.current_threat_mut().acc, simd);

    let ml1 = generate(&pos, GenType::Legal);
    let limit1 = ml1.len().min(3);

    for i in 0..limit1 {
        let m1 = ml1.get(i);
        let dts1 = do_move_collect_threats(&mut pos, m1);

        stack.push();
        stack.set_threat_diff(dts1);

        // Compute threat acc for m1
        if let Some((prev, current)) = stack.prev_and_current_threat_mut() {
            if let DiffType::DirtyThreats(ref dt) = current.diff {
                let dt_copy = dt.clone();
                update_threat_accumulator_incremental(
                    model, &pos, &prev.acc, &mut current.acc, &dt_copy, simd,
                );
            }
        }

        // Null move — no stack push (matches search pattern)
        pos.do_null_move();

        let ml2 = generate(&pos, GenType::Legal);
        let limit2 = ml2.len().min(5);

        for j in 0..limit2 {
            let m2 = ml2.get(j);
            let dts2 = do_move_collect_threats(&mut pos, m2);

            // Push for m2 — prev is still m1's stack level
            stack.push();
            stack.set_threat_diff(dts2);

            if let Some((prev, current)) = stack.prev_and_current_threat_mut() {
                if let DiffType::DirtyThreats(ref dt) = current.diff {
                    let dt_copy = dt.clone();
                    update_threat_accumulator_incremental(
                        model, &pos, &prev.acc, &mut current.acc, &dt_copy, simd,
                    );
                }
            }

            let mut ref_acc = Accumulator::new();
            refresh_threat_accumulator(model, &pos, &mut ref_acc, simd);

            assert_acc_eq(
                &format!("null-move m1={m1:?}(i={i}) m2={m2:?}(j={j})"),
                &stack.current_threat().acc,
                &ref_acc,
            );

            stack.pop();
            pos.undo_move(m2);
        }

        pos.undo_null_move();
        stack.pop();
        pos.undo_move(m1);
    }
}

// ---------------------------------------------------------------
// Test 4: Pruning scenario — m2 is pushed but never computed,
//   then m3 tries incremental from m2's (uncomputed) accumulator.
// ---------------------------------------------------------------
#[test]
fn test_pruning_skip_scenario() {
    let Some(net) = load_network() else { return };
    let model = net.model();
    let simd = net.simd();

    let mut pos = Position::from_fen(BENCH7_FEN).expect("parse fen");
    let mut stack = AccumulatorStack::new(128);

    refresh_threat_accumulator(model, &pos, &mut stack.current_threat_mut().acc, simd);

    let ml1 = generate(&pos, GenType::Legal);
    if ml1.len() < 1 {
        return;
    }

    // m1: push, compute
    let m1 = ml1.get(0);
    let dts1 = do_move_collect_threats(&mut pos, m1);
    stack.push();
    stack.set_threat_diff(dts1);

    if let Some((prev, current)) = stack.prev_and_current_threat_mut() {
        if let DiffType::DirtyThreats(ref dt) = current.diff {
            let dt_copy = dt.clone();
            update_threat_accumulator_incremental(
                model, &pos, &prev.acc, &mut current.acc, &dt_copy, simd,
            );
        }
    }

    let ml2 = generate(&pos, GenType::Legal);
    if ml2.len() < 1 {
        pos.undo_move(m1);
        stack.pop();
        return;
    }

    // m2: push, set_threat_diff, but DON'T compute (simulating pruning)
    let m2 = ml2.get(0);
    let dts2 = do_move_collect_threats(&mut pos, m2);
    stack.push();
    stack.set_threat_diff(dts2);
    // Intentionally skip computing threat acc for m2

    // Verify m2's acc is NOT computed
    assert!(
        !stack.current_threat().acc.computed[0] && !stack.current_threat().acc.computed[1],
        "m2 threat acc should not be computed (pruning simulation)"
    );

    let ml3 = generate(&pos, GenType::Legal);
    if ml3.len() < 1 {
        pos.undo_move(m2);
        stack.pop();
        pos.undo_move(m1);
        stack.pop();
        return;
    }

    // m3: push, set_threat_diff, try incremental
    let m3 = ml3.get(0);
    let dts3 = do_move_collect_threats(&mut pos, m3);
    stack.push();
    stack.set_threat_diff(dts3);

    // The prev (m2) has computed=[false,false].
    // update_threat_accumulator_incremental should detect this and fall back to refresh.
    if let Some((prev, current)) = stack.prev_and_current_threat_mut() {
        // Diagnostic: check what the prev state looks like
        let prev_computed = prev.acc.computed;
        eprintln!(
            "pruning test: prev.computed={:?}, prev has diff={:?}",
            prev_computed,
            matches!(prev.diff, DiffType::DirtyThreats(_))
        );

        if let DiffType::DirtyThreats(ref dt) = current.diff {
            let dt_copy = dt.clone();
            update_threat_accumulator_incremental(
                model, &pos, &prev.acc, &mut current.acc, &dt_copy, simd,
            );
        }
    }

    let mut ref_acc = Accumulator::new();
    refresh_threat_accumulator(model, &pos, &mut ref_acc, simd);

    assert_acc_eq(
        &format!("pruning m1={m1:?} m2={m2:?}(skipped) m3={m3:?}"),
        &stack.current_threat().acc,
        &ref_acc,
    );

    // Cleanup
    stack.pop();
    pos.undo_move(m3);
    stack.pop();
    pos.undo_move(m2);
    stack.pop();
    pos.undo_move(m1);
}

// ---------------------------------------------------------------
// Test 5: evaluate_threat_side path (the actual search uses this
//   for lazy evaluation with forward/backward walks)
// ---------------------------------------------------------------
#[test]
fn test_evaluate_threat_side_2ply() {
    use pikarust_core::nnue::feature_transformer::evaluate_threat_side;

    let Some(net) = load_network() else { return };
    let model = net.model();
    let simd = net.simd();

    let mut pos = Position::from_fen(BENCH7_FEN).expect("parse fen");
    let mut stack = AccumulatorStack::new(128);

    refresh_threat_accumulator(model, &pos, &mut stack.current_threat_mut().acc, simd);

    let ml1 = generate(&pos, GenType::Legal);
    let limit1 = ml1.len().min(5);

    for i in 0..limit1 {
        let m1 = ml1.get(i);
        let dts1 = do_move_collect_threats(&mut pos, m1);

        stack.push();
        stack.set_threat_diff(dts1);

        // DON'T compute m1's threat acc yet — let evaluate_threat_side handle it lazily

        let ml2 = generate(&pos, GenType::Legal);
        let limit2 = ml2.len().min(5);

        for j in 0..limit2 {
            let m2 = ml2.get(j);
            let dts2 = do_move_collect_threats(&mut pos, m2);

            stack.push();
            stack.set_threat_diff(dts2);

            // Use evaluate_threat_side to lazily compute the whole chain
            let stack_size = stack.size() + 1; // size() is 0-indexed top, need slice len
            for &perspective in &Color::ALL {
                evaluate_threat_side(
                    model,
                    &pos,
                    perspective,
                    stack.threat_stack_mut(),
                    stack_size,
                    simd,
                );
            }

            let mut ref_acc = Accumulator::new();
            refresh_threat_accumulator(model, &pos, &mut ref_acc, simd);

            assert_acc_eq(
                &format!("eval_threat_side 2ply m1={m1:?}(i={i}) m2={m2:?}(j={j})"),
                &stack.current_threat().acc,
                &ref_acc,
            );

            stack.pop();
            pos.undo_move(m2);
        }

        stack.pop();
        pos.undo_move(m1);
    }
}

// ---------------------------------------------------------------
// Test 6: evaluate_threat_side with pruning gap (m2 skipped)
// ---------------------------------------------------------------
#[test]
fn test_evaluate_threat_side_pruning_gap() {
    use pikarust_core::nnue::feature_transformer::evaluate_threat_side;

    let Some(net) = load_network() else { return };
    let model = net.model();
    let simd = net.simd();

    let mut pos = Position::from_fen(BENCH7_FEN).expect("parse fen");
    let mut stack = AccumulatorStack::new(128);

    refresh_threat_accumulator(model, &pos, &mut stack.current_threat_mut().acc, simd);

    let ml1 = generate(&pos, GenType::Legal);
    if ml1.len() < 1 {
        return;
    }

    let m1 = ml1.get(0);
    let dts1 = do_move_collect_threats(&mut pos, m1);
    stack.push();
    stack.set_threat_diff(dts1);

    // Compute m1 via evaluate_threat_side
    {
        let stack_size = stack.size() + 1;
        for &perspective in &Color::ALL {
            evaluate_threat_side(
                model, &pos, perspective, stack.threat_stack_mut(), stack_size, simd,
            );
        }
    }

    let ml2 = generate(&pos, GenType::Legal);
    if ml2.len() < 1 {
        stack.pop();
        pos.undo_move(m1);
        return;
    }

    // m2: push + diff but DON'T evaluate (pruning)
    let m2 = ml2.get(0);
    let dts2 = do_move_collect_threats(&mut pos, m2);
    stack.push();
    stack.set_threat_diff(dts2);

    let ml3 = generate(&pos, GenType::Legal);
    if ml3.len() < 1 {
        stack.pop();
        pos.undo_move(m2);
        stack.pop();
        pos.undo_move(m1);
        return;
    }

    // m3: push + diff, then evaluate via evaluate_threat_side
    let m3 = ml3.get(0);
    let dts3 = do_move_collect_threats(&mut pos, m3);
    stack.push();
    stack.set_threat_diff(dts3);

    {
        let stack_size = stack.size() + 1;
        for &perspective in &Color::ALL {
            evaluate_threat_side(
                model, &pos, perspective, stack.threat_stack_mut(), stack_size, simd,
            );
        }
    }

    let mut ref_acc = Accumulator::new();
    refresh_threat_accumulator(model, &pos, &mut ref_acc, simd);

    assert_acc_eq(
        &format!("eval_threat_side pruning gap m1={m1:?} m2={m2:?}(skip) m3={m3:?}"),
        &stack.current_threat().acc,
        &ref_acc,
    );

    stack.pop();
    pos.undo_move(m3);
    stack.pop();
    pos.undo_move(m2);
    stack.pop();
    pos.undo_move(m1);
}

// ---------------------------------------------------------------
// Test 7: Null-move with evaluate_threat_side
// ---------------------------------------------------------------
#[test]
fn test_evaluate_threat_side_null_move() {
    use pikarust_core::nnue::feature_transformer::evaluate_threat_side;

    let Some(net) = load_network() else { return };
    let model = net.model();
    let simd = net.simd();

    let mut pos = Position::from_fen(BENCH7_FEN).expect("parse fen");
    let mut stack = AccumulatorStack::new(128);

    refresh_threat_accumulator(model, &pos, &mut stack.current_threat_mut().acc, simd);

    let ml1 = generate(&pos, GenType::Legal);
    if ml1.len() < 1 {
        return;
    }

    let m1 = ml1.get(0);
    let dts1 = do_move_collect_threats(&mut pos, m1);
    stack.push();
    stack.set_threat_diff(dts1);

    // Compute m1
    {
        let stack_size = stack.size() + 1;
        for &perspective in &Color::ALL {
            evaluate_threat_side(
                model, &pos, perspective, stack.threat_stack_mut(), stack_size, simd,
            );
        }
    }

    // Null move — no stack push
    pos.do_null_move();

    let ml2 = generate(&pos, GenType::Legal);
    let limit2 = ml2.len().min(5);

    for j in 0..limit2 {
        let m2 = ml2.get(j);
        let dts2 = do_move_collect_threats(&mut pos, m2);

        stack.push();
        stack.set_threat_diff(dts2);

        // evaluate_threat_side for m2 after null move
        {
            let stack_size = stack.size() + 1;
            for &perspective in &Color::ALL {
                evaluate_threat_side(
                    model, &pos, perspective, stack.threat_stack_mut(), stack_size, simd,
                );
            }
        }

        let mut ref_acc = Accumulator::new();
        refresh_threat_accumulator(model, &pos, &mut ref_acc, simd);

        assert_acc_eq(
            &format!("eval_threat_side null-move m1={m1:?} m2={m2:?}(j={j})"),
            &stack.current_threat().acc,
            &ref_acc,
        );

        stack.pop();
        pos.undo_move(m2);
    }

    pos.undo_null_move();
    stack.pop();
    pos.undo_move(m1);
}
