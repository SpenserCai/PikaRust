//! Reproduction test for Bug B: `update_piece_threats` produces incomplete
//! DirtyThreats during `do_move_with_threats`, causing incremental threat
//! accumulator updates to differ from full refresh.

use std::collections::BTreeSet;

use pikarust_core::nnue::DirtyThreats;
use pikarust_core::nnue::features::IndexList;
use pikarust_core::nnue::features::full_threats;
use pikarust_core::nnue::features::half_ka_v2_hm;
use pikarust_core::position::{GenType, Position, generate};
use pikarust_core::types::Color;

const FENS: &[&str] = &[
    "2bckab2/4a4/5n3/C4N2p/5r3/PR2P1B2/9/2n1B4/4A4/3AK1C2 w - - 1 1",
    "C3kab2/4a4/1R1nb3n/8p/6p2/1p2c3r/P5P2/4B3N/3CA4/2BAK4 w - - 1 1",
    // Bench position 8 (where bug first manifested)
    "2bckab2/4a4/5n3/CR3N2p/5r3/P3P1B2/9/2n1B4/4A4/3AK1C2 w",
    // Bench position 26 (another known failing position)
    "rnbakab2/2r6/1c4nc1/p3p1C1p/2p3p2/2P6/P3P1P1P/1CN3N2/8R/R1BAKAB2 b",
    // Startpos
    "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w",
];

fn active_indices(pos: &Position, perspective: Color) -> BTreeSet<u32> {
    let mut list = IndexList::new();
    full_threats::append_active_indices(pos, perspective, &mut list);
    list.as_slice().iter().copied().collect()
}

fn do_move_with_threats(pos: &mut Position, m: pikarust_core::types::Move) -> DirtyThreats {
    let gives_check = pos.gives_check(m);
    let mut dts = DirtyThreats::new();
    pos.do_move_with_threats(m, gives_check, &mut dts);
    dts
}

/// For each FEN, for every legal move, verify DirtyThreats net effect matches
/// the set-difference of active indices before/after the move.
/// Also check for duplicate same-direction entries that would cause double-counting.
#[test]
fn test_dirty_threats_completeness_all_legal_moves() {
    let mut failures = Vec::new();

    for &fen in FENS {
        let mut pos = Position::from_fen(fen).expect("parse fen");
        let ml = generate(&pos, GenType::Legal);

        for i in 0..ml.len() {
            let m = ml.get(i);

            let before_w = active_indices(&pos, Color::White);
            let before_b = active_indices(&pos, Color::Black);

            let dts = do_move_with_threats(&mut pos, m);

            let after_w = active_indices(&pos, Color::White);
            let after_b = active_indices(&pos, Color::Black);

            for (c, perspective, before, after) in [
                (0, Color::White, &before_w, &after_w),
                (1, Color::Black, &before_b, &after_b),
            ] {
                if dts.requires_refresh[c] {
                    continue;
                }

                let (_, mirror, _) = half_ka_v2_hm::make_feature_bucket(perspective, &pos);
                let mut rem_list = IndexList::new();
                let mut add_list = IndexList::new();
                full_threats::append_changed_indices(
                    perspective,
                    mirror,
                    &dts,
                    &mut rem_list,
                    &mut add_list,
                );

                // Check for duplicate same-direction entries
                let add_vec: Vec<u32> = add_list.as_slice().to_vec();
                let rem_vec: Vec<u32> = rem_list.as_slice().to_vec();

                use std::collections::BTreeMap;
                let mut add_counts: BTreeMap<u32, u32> = BTreeMap::new();
                for &idx in &add_vec {
                    *add_counts.entry(idx).or_insert(0) += 1;
                }
                let mut rem_counts: BTreeMap<u32, u32> = BTreeMap::new();
                for &idx in &rem_vec {
                    *rem_counts.entry(idx).or_insert(0) += 1;
                }

                // Check: any index added more times than removed (or vice versa)
                // beyond what the expected diff requires
                let mut net: BTreeMap<u32, i32> = BTreeMap::new();
                for (&idx, &cnt) in &add_counts {
                    *net.entry(idx).or_insert(0) += cnt as i32;
                }
                for (&idx, &cnt) in &rem_counts {
                    *net.entry(idx).or_insert(0) -= cnt as i32;
                }
                net.retain(|_, v| *v != 0);

                let mut expected_net: BTreeMap<u32, i32> = BTreeMap::new();
                for &idx in after.difference(before) {
                    *expected_net.entry(idx).or_insert(0) += 1;
                }
                for &idx in before.difference(after) {
                    *expected_net.entry(idx).or_insert(0) -= 1;
                }

                if net != expected_net {
                    let msg = format!(
                        "FEN: {fen}\n  move: {m} perspective={c}\n  \
                         net_diff={:?}\n  expected_net={:?}",
                        net, expected_net,
                    );
                    eprintln!("NET MISMATCH: {msg}");
                    failures.push(msg);
                }

                // Check for duplicate adds/removes that would cause double-counting
                for (&idx, &cnt) in &add_counts {
                    if cnt > 1 {
                        let rem_cnt = rem_counts.get(&idx).copied().unwrap_or(0);
                        if cnt != rem_cnt {
                            eprintln!(
                                "DUPLICATE ADD: fen={fen} move={m} c={c} idx={idx} add_cnt={cnt} rem_cnt={rem_cnt}"
                            );
                        }
                    }
                }
                for (&idx, &cnt) in &rem_counts {
                    if cnt > 1 {
                        let add_cnt = add_counts.get(&idx).copied().unwrap_or(0);
                        if cnt != add_cnt {
                            eprintln!(
                                "DUPLICATE REM: fen={fen} move={m} c={c} idx={idx} rem_cnt={cnt} add_cnt={add_cnt}"
                            );
                        }
                    }
                }
            }

            pos.undo_move(m);
        }
    }

    assert!(
        failures.is_empty(),
        "{} dirty-threat net mismatches found:\n{}",
        failures.len(),
        failures.join("\n---\n")
    );
}

/// Simulate the search pattern: refresh at root, then for each legal move,
/// do_move_with_threats → incremental → undo_move. Verify that after undo,
/// the root accumulator still matches a fresh refresh.
#[test]
fn test_search_flow_single_move() {
    use pikarust_core::nnue::simd::Dispatch;
    use pikarust_core::nnue::{self, Accumulator};

    let model = nnue::NnueModel::load(std::path::Path::new("../../models/pikafish.nnue"))
        .expect("load model");
    let simd = Dispatch::new();

    for &fen in FENS {
        let mut pos = Position::from_fen(fen).expect("parse fen");

        // Step 1: Refresh at root
        let mut root_acc = Accumulator::new();
        nnue::feature_transformer::refresh_threat_accumulator(&model, &pos, &mut root_acc, &simd);

        // Step 2: For each legal move, do incremental and compare with refresh
        let ml = generate(&pos, GenType::Legal);
        let mut failures = Vec::new();

        for i in 0..ml.len() {
            let m = ml.get(i);
            let gc = pos.gives_check(m);

            // do_move_with_threats
            let mut dts = DirtyThreats::new();
            pos.do_move_with_threats(m, gc, &mut dts);

            // Incremental update from root_acc
            let mut inc_acc = Accumulator::new();
            nnue::feature_transformer::update_threat_accumulator_incremental(
                &model,
                &pos,
                &root_acc,
                &mut inc_acc,
                &dts,
                &simd,
            );

            // Full refresh for comparison
            let mut ref_acc = Accumulator::new();
            nnue::feature_transformer::refresh_threat_accumulator(
                &model,
                &pos,
                &mut ref_acc,
                &simd,
            );

            for c in 0..2 {
                if inc_acc.accumulation[c] != ref_acc.accumulation[c] {
                    failures.push(format!(
                        "fen={fen} move={m} c={c} rr=[{},{}] count={}",
                        dts.requires_refresh[0], dts.requires_refresh[1], dts.count,
                    ));
                    break;
                }
            }

            // Undo the move
            pos.undo_move(m);

            // Verify root_acc still matches a fresh refresh of the root position
            let mut fresh_root = Accumulator::new();
            nnue::feature_transformer::refresh_threat_accumulator(
                &model,
                &pos,
                &mut fresh_root,
                &simd,
            );
            for c in 0..2 {
                if root_acc.accumulation[c] != fresh_root.accumulation[c] {
                    failures.push(format!(
                        "fen={fen} ROOT ACC CHANGED after move={m} undo c={c}"
                    ));
                    break;
                }
            }
        }

        assert!(
            failures.is_empty(),
            "{} mismatches:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
/// 3-move sequence from the first FEN to catch accumulated errors.
#[test]
fn test_dirty_threats_3move_sequence() {
    let fen = FENS[0];
    let mut pos = Position::from_fen(fen).expect("parse fen");
    let mut failures = Vec::new();

    // Pick first 3 legal moves at each ply (bounded)
    let ml1 = generate(&pos, GenType::Legal);
    let limit1 = ml1.len().min(5);

    for i in 0..limit1 {
        let m1 = ml1.get(i);
        let before1_w = active_indices(&pos, Color::White);
        let before1_b = active_indices(&pos, Color::Black);
        let dts1 = do_move_with_threats(&mut pos, m1);
        check_dirty(
            &pos,
            &dts1,
            &before1_w,
            &before1_b,
            m1,
            fen,
            1,
            &mut failures,
        );

        let ml2 = generate(&pos, GenType::Legal);
        let limit2 = ml2.len().min(5);
        for j in 0..limit2 {
            let m2 = ml2.get(j);
            let before2_w = active_indices(&pos, Color::White);
            let before2_b = active_indices(&pos, Color::Black);
            let dts2 = do_move_with_threats(&mut pos, m2);
            check_dirty(
                &pos,
                &dts2,
                &before2_w,
                &before2_b,
                m2,
                fen,
                2,
                &mut failures,
            );

            let ml3 = generate(&pos, GenType::Legal);
            let limit3 = ml3.len().min(5);
            for k in 0..limit3 {
                let m3 = ml3.get(k);
                let before3_w = active_indices(&pos, Color::White);
                let before3_b = active_indices(&pos, Color::Black);
                let dts3 = do_move_with_threats(&mut pos, m3);
                check_dirty(
                    &pos,
                    &dts3,
                    &before3_w,
                    &before3_b,
                    m3,
                    fen,
                    3,
                    &mut failures,
                );
                pos.undo_move(m3);
            }

            pos.undo_move(m2);
        }

        pos.undo_move(m1);
    }

    assert!(
        failures.is_empty(),
        "{} dirty-threat mismatches in 3-move sequence:\n{}",
        failures.len(),
        failures.join("\n---\n")
    );
}

fn check_dirty(
    pos: &Position,
    dts: &DirtyThreats,
    before_w: &BTreeSet<u32>,
    before_b: &BTreeSet<u32>,
    m: pikarust_core::types::Move,
    fen: &str,
    ply: usize,
    failures: &mut Vec<String>,
) {
    let after_w = active_indices(pos, Color::White);
    let after_b = active_indices(pos, Color::Black);

    for (c, perspective, before, after) in [
        (0, Color::White, before_w, &after_w),
        (1, Color::Black, before_b, &after_b),
    ] {
        if dts.requires_refresh[c] {
            continue;
        }

        let (_, mirror, _) = half_ka_v2_hm::make_feature_bucket(perspective, pos);
        let mut rem_list = IndexList::new();
        let mut add_list = IndexList::new();
        full_threats::append_changed_indices(
            perspective,
            mirror,
            dts,
            &mut rem_list,
            &mut add_list,
        );
        let dirty_removed: BTreeSet<u32> = rem_list.as_slice().iter().copied().collect();
        let dirty_added: BTreeSet<u32> = add_list.as_slice().iter().copied().collect();

        let expected_added: BTreeSet<u32> = after.difference(before).copied().collect();
        let expected_removed: BTreeSet<u32> = before.difference(after).copied().collect();

        let missing_added: BTreeSet<u32> =
            expected_added.difference(&dirty_added).copied().collect();
        let extra_added: BTreeSet<u32> = dirty_added.difference(&expected_added).copied().collect();
        let missing_removed: BTreeSet<u32> = expected_removed
            .difference(&dirty_removed)
            .copied()
            .collect();
        let extra_removed: BTreeSet<u32> = dirty_removed
            .difference(&expected_removed)
            .copied()
            .collect();

        if !missing_added.is_empty()
            || !extra_added.is_empty()
            || !missing_removed.is_empty()
            || !extra_removed.is_empty()
        {
            let self_cancel: BTreeSet<u32> =
                extra_added.intersection(&extra_removed).copied().collect();
            let real_extra_added: BTreeSet<u32> =
                extra_added.difference(&self_cancel).copied().collect();
            let real_extra_removed: BTreeSet<u32> =
                extra_removed.difference(&self_cancel).copied().collect();

            if !missing_added.is_empty()
                || !real_extra_added.is_empty()
                || !missing_removed.is_empty()
                || !real_extra_removed.is_empty()
            {
                let msg = format!(
                    "FEN: {fen} ply={ply}\n  move: {m} perspective={c}\n  \
                     missing_added({})={:?}\n  extra_added({})={:?}\n  \
                     missing_removed({})={:?}\n  extra_removed({})={:?}",
                    missing_added.len(),
                    missing_added,
                    real_extra_added.len(),
                    real_extra_added,
                    missing_removed.len(),
                    missing_removed,
                    real_extra_removed.len(),
                    real_extra_removed,
                );
                eprintln!("MISMATCH ply {ply}: {msg}");
                failures.push(msg);
            }
        }
    }
}
