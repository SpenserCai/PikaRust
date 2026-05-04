//! Targeted reproduction of the threat mismatch from bench position 8 diagnostic:
//!
//! ```text
//! THREAT MISMATCH perspective=1 node=10119 path=incremental stack_size=1
//!   prev_computed=[true,true] prev_diff=None cur_diff=DirtyThreats
//!   fen=2bckab2/4a4/5n3/C4N2p/5r3/PR2P1B2/9/2n1B4/4A4/3AK1C2 w
//! ```
//!
//! Root: `2bckab2/4a4/5n3/CR3N2p/5r3/P3P1B2/9/2n1B4/4A4/3AK1C2 w`
//! After Rook b6→b4: board becomes `2bckab2/4a4/5n3/C4N2p/5r3/PR2P1B2/...`

use pikarust_core::nnue::feature_transformer::{
    evaluate_threat_side, refresh_threat_accumulator, update_threat_accumulator_incremental,
};
use pikarust_core::nnue::features::half_ka_v2_hm;
use pikarust_core::nnue::{
    Accumulator, AccumulatorStack, DiffType, DirtyThreats, Network, NnueModel,
};
use pikarust_core::position::{GenType, Position, generate};
use pikarust_core::types::Color;

const ROOT_FEN: &str = "2bckab2/4a4/5n3/CR3N2p/5r3/P3P1B2/9/2n1B4/4A4/3AK1C2 w";

/// Board portion of the mismatch FEN (without side/counters).
const MISMATCH_BOARD: &str = "2bckab2/4a4/5n3/C4N2p/5r3/PR2P1B2/9/2n1B4/4A4/3AK1C2";

fn load_network() -> Option<Network> {
    for p in &[
        "../../models/pikafish.nnue",
        "../models/pikafish.nnue",
        "models/pikafish.nnue",
    ] {
        let path = std::path::Path::new(p);
        if path.exists() {
            return NnueModel::load(path).ok().map(Network::new);
        }
    }
    None
}

fn fen_board(fen: &str) -> &str {
    fen.split_whitespace().next().unwrap_or("")
}

fn mirrors(pos: &Position) -> [bool; 2] {
    [
        half_ka_v2_hm::make_feature_bucket(Color::White, pos).1,
        half_ka_v2_hm::make_feature_bucket(Color::Black, pos).1,
    ]
}

fn do_move_collect_threats(
    pos: &mut Position,
    m: pikarust_core::types::Move,
) -> DirtyThreats {
    let mirror_before = mirrors(pos);
    let gives_check = pos.gives_check(m);
    let mut dts = DirtyThreats::new();
    pos.do_move_with_threats(m, gives_check, &mut dts);
    let mirror_after = mirrors(pos);
    dts.requires_refresh[0] = mirror_before[0] != mirror_after[0];
    dts.requires_refresh[1] = mirror_before[1] != mirror_after[1];
    dts
}

/// Test 1: For every legal move from root, compare incremental vs refresh.
/// Reproduces the diagnostic scenario (stack_size=1, prev=root refresh).
#[test]
fn test_all_moves_from_root_incremental_vs_refresh() {
    let Some(net) = load_network() else {
        eprintln!("NNUE model not found, skipping");
        return;
    };
    let model = net.model();
    let simd = net.simd();

    let mut pos = Position::from_fen(ROOT_FEN).expect("parse root fen");
    let mut root_acc = Accumulator::new();
    refresh_threat_accumulator(model, &pos, &mut root_acc, simd);

    let ml = generate(&pos, GenType::Legal);
    let mut mismatch_count = 0;

    for i in 0..ml.len() {
        let m = ml.get(i);
        let mirror_before = mirrors(&pos);
        let dts = do_move_collect_threats(&mut pos, m);
        let mirror_after = mirrors(&pos);

        let mut inc_acc = Accumulator::new();
        update_threat_accumulator_incremental(model, &pos, &root_acc, &mut inc_acc, &dts, simd);

        let mut ref_acc = Accumulator::new();
        refresh_threat_accumulator(model, &pos, &mut ref_acc, simd);

        for c in 0..2 {
            if inc_acc.accumulation[c] != ref_acc.accumulation[c] {
                mismatch_count += 1;
                let fen_after = pos.fen();
                let is_target = fen_board(&fen_after) == MISMATCH_BOARD;
                eprintln!("=== MISMATCH perspective={c} move={m:?} target={is_target} ===");
                eprintln!("  FEN after: {fen_after}");
                eprintln!(
                    "  mirror_before=[{}, {}] mirror_after=[{}, {}]",
                    mirror_before[0], mirror_before[1], mirror_after[0], mirror_after[1]
                );
                eprintln!(
                    "  requires_refresh=[{}, {}]",
                    dts.requires_refresh[0], dts.requires_refresh[1]
                );
                eprintln!(
                    "  path: {}",
                    if dts.requires_refresh[c] || !root_acc.computed[c] {
                        "refresh"
                    } else {
                        "incremental"
                    }
                );
                eprintln!("  dirty count={}", dts.count);
                for dt in dts.as_slice().iter().take(5) {
                    eprintln!(
                        "    {}(pc={},victim={},from={},to={})",
                        if dt.is_add() { "ADD" } else { "REM" },
                        dt.pc_raw(),
                        dt.threatened_pc_raw(),
                        dt.pc_sq_raw(),
                        dt.threatened_sq_raw()
                    );
                }
                let diffs: Vec<_> = inc_acc.accumulation[c]
                    .iter()
                    .zip(ref_acc.accumulation[c].iter())
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .take(10)
                    .map(|(i, (&a, &b))| (i, a, b))
                    .collect();
                eprintln!("  inc[{c}] first 10: {:?}", &inc_acc.accumulation[c][..10]);
                eprintln!("  ref[{c}] first 10: {:?}", &ref_acc.accumulation[c][..10]);
                eprintln!("  diffs (idx, inc, ref): {diffs:?}");
            }
        }

        pos.undo_move(m);
    }

    assert_eq!(
        mismatch_count, 0,
        "{mismatch_count} threat mismatches across {} legal moves",
        ml.len()
    );
}

/// Test 2: Verify the mismatch board is reachable from root.
#[test]
fn test_specific_mismatch_fen_reachable() {
    let mut pos = Position::from_fen(ROOT_FEN).expect("parse root fen");
    let ml = generate(&pos, GenType::Legal);

    let mut found = false;
    for i in 0..ml.len() {
        let m = ml.get(i);
        let gc = pos.gives_check(m);
        pos.do_move(m, gc);
        if fen_board(&pos.fen()) == MISMATCH_BOARD {
            eprintln!("Mismatch board reached by move: {m:?} → {}", pos.fen());
            found = true;
            pos.undo_move(m);
            break;
        }
        pos.undo_move(m);
    }

    assert!(
        found,
        "Could not find a legal move from root that reaches the mismatch board"
    );
}

/// Test 3: do a DIFFERENT move, undo it, then do the mismatch move.
/// Checks whether do_move/undo_move corrupts position state affecting refresh.
#[test]
fn test_do_undo_different_move_then_mismatch_move() {
    let Some(net) = load_network() else {
        eprintln!("NNUE model not found, skipping");
        return;
    };
    let model = net.model();
    let simd = net.simd();

    let mut pos = Position::from_fen(ROOT_FEN).expect("parse root fen");
    let ml = generate(&pos, GenType::Legal);

    // Find the move that leads to the mismatch board
    let mut mismatch_move = None;
    for i in 0..ml.len() {
        let m = ml.get(i);
        let gc = pos.gives_check(m);
        pos.do_move(m, gc);
        if fen_board(&pos.fen()) == MISMATCH_BOARD {
            mismatch_move = Some(m);
        }
        pos.undo_move(m);
    }

    let mismatch_move = mismatch_move.expect("mismatch move must exist");
    eprintln!("Mismatch move: {mismatch_move:?}");

    // Compute clean baseline
    let mut root_acc_clean = Accumulator::new();
    refresh_threat_accumulator(model, &pos, &mut root_acc_clean, simd);

    let _dts_clean = do_move_collect_threats(&mut pos, mismatch_move);
    let mut ref_clean = Accumulator::new();
    refresh_threat_accumulator(model, &pos, &mut ref_clean, simd);
    pos.undo_move(mismatch_move);

    // For each OTHER move: do it, undo it, then redo mismatch move and compare
    let mut corruption_count = 0;
    for i in 0..ml.len() {
        let other = ml.get(i);
        if other == mismatch_move {
            continue;
        }

        let gc = pos.gives_check(other);
        pos.do_move(other, gc);
        pos.undo_move(other);

        // Recompute root accumulator
        let mut root_acc_after = Accumulator::new();
        refresh_threat_accumulator(model, &pos, &mut root_acc_after, simd);

        for c in 0..2 {
            if root_acc_after.accumulation[c] != root_acc_clean.accumulation[c] {
                eprintln!("ROOT ACC CORRUPTED after do/undo {other:?}, perspective={c}");
                corruption_count += 1;
            }
        }

        // Do mismatch move and compare
        let dts = do_move_collect_threats(&mut pos, mismatch_move);
        let mut inc_after = Accumulator::new();
        update_threat_accumulator_incremental(
            model, &pos, &root_acc_after, &mut inc_after, &dts, simd,
        );
        let mut ref_after = Accumulator::new();
        refresh_threat_accumulator(model, &pos, &mut ref_after, simd);

        for c in 0..2 {
            if inc_after.accumulation[c] != ref_after.accumulation[c] {
                eprintln!(
                    "INC/REF MISMATCH after do/undo {other:?} then {mismatch_move:?}, perspective={c}"
                );
                corruption_count += 1;
            }
            if ref_after.accumulation[c] != ref_clean.accumulation[c] {
                eprintln!(
                    "REFRESH CHANGED after do/undo {other:?}, perspective={c} — position corrupted!"
                );
                corruption_count += 1;
            }
        }

        pos.undo_move(mismatch_move);
    }

    assert_eq!(corruption_count, 0, "{corruption_count} corruptions detected");
}

/// Test 4: do_move_with_threats + undo preserves root accumulator.
#[test]
fn test_do_move_with_threats_undo_preserves_root_acc() {
    let Some(net) = load_network() else {
        eprintln!("NNUE model not found, skipping");
        return;
    };
    let model = net.model();
    let simd = net.simd();

    let mut pos = Position::from_fen(ROOT_FEN).expect("parse root fen");
    let mut root_acc_before = Accumulator::new();
    refresh_threat_accumulator(model, &pos, &mut root_acc_before, simd);

    let ml = generate(&pos, GenType::Legal);
    for i in 0..ml.len() {
        let m = ml.get(i);
        let _dts = do_move_collect_threats(&mut pos, m);
        pos.undo_move(m);

        let mut root_acc_after = Accumulator::new();
        refresh_threat_accumulator(model, &pos, &mut root_acc_after, simd);

        for c in 0..2 {
            assert_eq!(
                root_acc_after.accumulation[c], root_acc_before.accumulation[c],
                "Root acc corrupted after do_move_with_threats/undo of {m:?}, perspective={c}"
            );
        }
    }
}

/// Test 5: Stack-based incremental using evaluate_threat_side (the actual search path).
/// The diagnostic shows stack_size=1 with prev_diff=None, which means the search uses
/// evaluate_threat_side to walk the stack. This tests that path.
#[test]
fn test_evaluate_threat_side_from_root() {
    let Some(net) = load_network() else {
        eprintln!("NNUE model not found, skipping");
        return;
    };
    let model = net.model();
    let simd = net.simd();

    let mut pos = Position::from_fen(ROOT_FEN).expect("parse root fen");
    let mut stack = AccumulatorStack::new(128);

    refresh_threat_accumulator(model, &pos, &mut stack.current_threat_mut().acc, simd);

    let ml = generate(&pos, GenType::Legal);
    let mut mismatch_count = 0;

    for i in 0..ml.len() {
        let m = ml.get(i);
        let dts = do_move_collect_threats(&mut pos, m);

        stack.push();
        stack.set_threat_diff(dts);

        // Use evaluate_threat_side (the actual search path)
        let stack_size = stack.size() + 1;
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

        let is_target = fen_board(&pos.fen()) == MISMATCH_BOARD;
        for c in 0..2 {
            if stack.current_threat().acc.accumulation[c] != ref_acc.accumulation[c] {
                mismatch_count += 1;
                eprintln!(
                    "STACK MISMATCH perspective={c} move={m:?} target={is_target} fen={}",
                    pos.fen()
                );
            }
        }

        stack.pop();
        pos.undo_move(m);
    }

    assert_eq!(
        mismatch_count, 0,
        "{mismatch_count} stack-based mismatches across {} legal moves",
        ml.len()
    );
}

/// Test 6: 2-ply from root — tests whether the mismatch appears at depth 2
/// when the first move is NOT the mismatch move.
#[test]
fn test_2ply_from_root_all_moves() {
    let Some(net) = load_network() else {
        eprintln!("NNUE model not found, skipping");
        return;
    };
    let model = net.model();
    let simd = net.simd();

    let mut pos = Position::from_fen(ROOT_FEN).expect("parse root fen");
    let mut stack = AccumulatorStack::new(128);

    refresh_threat_accumulator(model, &pos, &mut stack.current_threat_mut().acc, simd);

    let ml1 = generate(&pos, GenType::Legal);
    let mut mismatch_count = 0;

    for i in 0..ml1.len() {
        let m1 = ml1.get(i);
        let dts1 = do_move_collect_threats(&mut pos, m1);

        stack.push();
        stack.set_threat_diff(dts1);

        // Compute ply-1 via incremental
        if let Some((prev, current)) = stack.prev_and_current_threat_mut() {
            if let DiffType::DirtyThreats(ref dt) = current.diff {
                let dt_copy = dt.clone();
                update_threat_accumulator_incremental(
                    model, &pos, &prev.acc, &mut current.acc, &dt_copy, simd,
                );
            }
        }

        // Verify ply-1
        let mut ref1 = Accumulator::new();
        refresh_threat_accumulator(model, &pos, &mut ref1, simd);
        for c in 0..2 {
            if stack.current_threat().acc.accumulation[c] != ref1.accumulation[c] {
                mismatch_count += 1;
                eprintln!(
                    "PLY1 MISMATCH perspective={c} m1={m1:?} fen={}",
                    pos.fen()
                );
            }
        }

        // All ply-2 moves
        let ml2 = generate(&pos, GenType::Legal);
        for j in 0..ml2.len() {
            let m2 = ml2.get(j);
            let dts2 = do_move_collect_threats(&mut pos, m2);

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

            let mut ref2 = Accumulator::new();
            refresh_threat_accumulator(model, &pos, &mut ref2, simd);

            for c in 0..2 {
                if stack.current_threat().acc.accumulation[c] != ref2.accumulation[c] {
                    mismatch_count += 1;
                    let is_target = fen_board(&pos.fen()) == MISMATCH_BOARD;
                    eprintln!(
                        "PLY2 MISMATCH perspective={c} m1={m1:?} m2={m2:?} target={is_target} fen={}",
                        pos.fen()
                    );
                }
            }

            stack.pop();
            pos.undo_move(m2);
        }

        stack.pop();
        pos.undo_move(m1);
    }

    assert_eq!(
        mismatch_count, 0,
        "{mismatch_count} mismatches in 2-ply test"
    );
}
