use crate::position::Position;
use crate::types::{Color, Piece, Square};

use super::accumulator::{Accumulator, DirtyPiece};
use super::features::IndexList;
use super::features::full_threats;
use super::features::half_ka_v2_hm;
use super::model::{NnueModel, PSQT_BUCKETS, TRANSFORMED_DIMS};
use super::simd::Dispatch;

pub fn refresh_psq_accumulator(
    model: &NnueModel,
    pos: &Position,
    acc: &mut Accumulator,
    simd: &Dispatch,
) {
    for &perspective in &Color::ALL {
        let mut active = IndexList::new();
        half_ka_v2_hm::append_active_indices(pos, perspective, &mut active);

        let c = perspective as usize;
        acc.accumulation[c] = *model.ft.biases;
        acc.psqt_accumulation[c] = [0i32; PSQT_BUCKETS];

        for &idx in active.as_slice() {
            let offset = idx as usize * TRANSFORMED_DIMS;
            let psqt_offset = idx as usize * PSQT_BUCKETS;

            if offset + TRANSFORMED_DIMS <= model.ft.weights.len() {
                simd.vec_add_i16_widening(
                    &mut acc.accumulation[c],
                    &model.ft.weights[offset..offset + TRANSFORMED_DIMS],
                );
            }

            if psqt_offset + PSQT_BUCKETS <= model.ft.psqt_weights.len() {
                for j in 0..PSQT_BUCKETS {
                    acc.psqt_accumulation[c][j] += model.ft.psqt_weights[psqt_offset + j];
                }
            }
        }

        acc.computed[c] = true;
    }
}

pub fn update_psq_accumulator_incremental(
    model: &NnueModel,
    pos: &Position,
    prev: &Accumulator,
    acc: &mut Accumulator,
    dirty: &DirtyPiece,
    simd: &Dispatch,
) {
    for &perspective in &Color::ALL {
        let c = perspective as usize;

        if dirty.requires_refresh[c] || !prev.computed[c] {
            let mut active = IndexList::new();
            half_ka_v2_hm::append_active_indices(pos, perspective, &mut active);

            acc.accumulation[c] = *model.ft.biases;
            acc.psqt_accumulation[c] = [0i32; PSQT_BUCKETS];

            for &idx in active.as_slice() {
                let offset = idx as usize * TRANSFORMED_DIMS;
                let psqt_offset = idx as usize * PSQT_BUCKETS;

                if offset + TRANSFORMED_DIMS <= model.ft.weights.len() {
                    simd.vec_add_i16_widening(
                        &mut acc.accumulation[c],
                        &model.ft.weights[offset..offset + TRANSFORMED_DIMS],
                    );
                }
                if psqt_offset + PSQT_BUCKETS <= model.ft.psqt_weights.len() {
                    for j in 0..PSQT_BUCKETS {
                        acc.psqt_accumulation[c][j] += model.ft.psqt_weights[psqt_offset + j];
                    }
                }
            }
        } else {
            acc.accumulation[c] = prev.accumulation[c];
            acc.psqt_accumulation[c] = prev.psqt_accumulation[c];

            let (bucket, mirror, _) = half_ka_v2_hm::make_feature_bucket(perspective, pos);
            let mut removed = IndexList::new();
            let mut added = IndexList::new();

            let captured_sq = if dirty.dirty_num > 1 {
                dirty.from[1]
            } else {
                Square::NONE
            };
            let captured_pc = if dirty.dirty_num > 1 {
                dirty.pc[1]
            } else {
                Piece::NONE
            };

            half_ka_v2_hm::append_changed_indices(
                perspective,
                bucket,
                mirror,
                dirty.from[0],
                dirty.to[0],
                dirty.pc[0],
                captured_sq,
                captured_pc,
                &mut removed,
                &mut added,
            );

            for &idx in removed.as_slice() {
                let offset = idx as usize * TRANSFORMED_DIMS;
                let psqt_offset = idx as usize * PSQT_BUCKETS;

                if offset + TRANSFORMED_DIMS <= model.ft.weights.len() {
                    simd.vec_sub_i16_widening(
                        &mut acc.accumulation[c],
                        &model.ft.weights[offset..offset + TRANSFORMED_DIMS],
                    );
                }
                if psqt_offset + PSQT_BUCKETS <= model.ft.psqt_weights.len() {
                    for j in 0..PSQT_BUCKETS {
                        acc.psqt_accumulation[c][j] -= model.ft.psqt_weights[psqt_offset + j];
                    }
                }
            }

            for &idx in added.as_slice() {
                let offset = idx as usize * TRANSFORMED_DIMS;
                let psqt_offset = idx as usize * PSQT_BUCKETS;

                if offset + TRANSFORMED_DIMS <= model.ft.weights.len() {
                    simd.vec_add_i16_widening(
                        &mut acc.accumulation[c],
                        &model.ft.weights[offset..offset + TRANSFORMED_DIMS],
                    );
                }
                if psqt_offset + PSQT_BUCKETS <= model.ft.psqt_weights.len() {
                    for j in 0..PSQT_BUCKETS {
                        acc.psqt_accumulation[c][j] += model.ft.psqt_weights[psqt_offset + j];
                    }
                }
            }
        }

        acc.computed[c] = true;
    }
}

pub fn update_threat_accumulator_incremental(
    model: &NnueModel,
    pos: &Position,
    prev: &Accumulator,
    acc: &mut Accumulator,
    dirty: &super::accumulator::DirtyThreats,
    simd: &Dispatch,
) {
    for &perspective in &Color::ALL {
        let c = perspective as usize;

        if dirty.requires_refresh[c] || !prev.computed[c] {
            // Fall back to full refresh for this perspective
            let mut active = IndexList::new();
            full_threats::append_active_indices(pos, perspective, &mut active);

            acc.accumulation[c] = [0i16; TRANSFORMED_DIMS];
            acc.psqt_accumulation[c] = [0i32; PSQT_BUCKETS];

            for &idx in active.as_slice() {
                let offset = idx as usize * TRANSFORMED_DIMS;
                let psqt_offset = idx as usize * PSQT_BUCKETS;
                if offset + TRANSFORMED_DIMS <= model.ft.threat_weights.len() {
                    simd.vec_add_i16_widening(
                        &mut acc.accumulation[c],
                        &model.ft.threat_weights[offset..offset + TRANSFORMED_DIMS],
                    );
                }
                if psqt_offset + PSQT_BUCKETS <= model.ft.threat_psqt_weights.len() {
                    for j in 0..PSQT_BUCKETS {
                        acc.psqt_accumulation[c][j] +=
                            model.ft.threat_psqt_weights[psqt_offset + j];
                    }
                }
            }
        } else {
            acc.accumulation[c] = prev.accumulation[c];
            acc.psqt_accumulation[c] = prev.psqt_accumulation[c];

            let (_, mirror, _) = half_ka_v2_hm::make_feature_bucket(perspective, pos);
            let mut removed = IndexList::new();
            let mut added = IndexList::new();
            full_threats::append_changed_indices(
                perspective,
                mirror,
                dirty,
                &mut removed,
                &mut added,
            );

            for &idx in removed.as_slice() {
                let offset = idx as usize * TRANSFORMED_DIMS;
                let psqt_offset = idx as usize * PSQT_BUCKETS;
                if offset + TRANSFORMED_DIMS <= model.ft.threat_weights.len() {
                    simd.vec_sub_i16_widening(
                        &mut acc.accumulation[c],
                        &model.ft.threat_weights[offset..offset + TRANSFORMED_DIMS],
                    );
                }
                if psqt_offset + PSQT_BUCKETS <= model.ft.threat_psqt_weights.len() {
                    for j in 0..PSQT_BUCKETS {
                        acc.psqt_accumulation[c][j] -=
                            model.ft.threat_psqt_weights[psqt_offset + j];
                    }
                }
            }

            for &idx in added.as_slice() {
                let offset = idx as usize * TRANSFORMED_DIMS;
                let psqt_offset = idx as usize * PSQT_BUCKETS;
                if offset + TRANSFORMED_DIMS <= model.ft.threat_weights.len() {
                    simd.vec_add_i16_widening(
                        &mut acc.accumulation[c],
                        &model.ft.threat_weights[offset..offset + TRANSFORMED_DIMS],
                    );
                }
                if psqt_offset + PSQT_BUCKETS <= model.ft.threat_psqt_weights.len() {
                    for j in 0..PSQT_BUCKETS {
                        acc.psqt_accumulation[c][j] +=
                            model.ft.threat_psqt_weights[psqt_offset + j];
                    }
                }
            }
        }

        acc.computed[c] = true;
    }
}

/// Apply a threat diff for one perspective.
///
/// When `FORWARD` is true, uses normal (removed, added) order from `diff`.
/// When false, swaps them (backward update).
/// Copies the base accumulator from `from_acc` into `to_acc` before applying.
#[inline]
pub fn apply_threat_diff<const FORWARD: bool>(
    model: &NnueModel,
    perspective: Color,
    mirror: bool,
    from_acc: &Accumulator,
    to_acc: &mut Accumulator,
    dirty: &super::accumulator::DirtyThreats,
    simd: &Dispatch,
) {
    let c = perspective as usize;
    to_acc.accumulation[c] = from_acc.accumulation[c];
    to_acc.psqt_accumulation[c] = from_acc.psqt_accumulation[c];

    let mut list_a = IndexList::new();
    let mut list_b = IndexList::new();
    if FORWARD {
        full_threats::append_changed_indices(perspective, mirror, dirty, &mut list_a, &mut list_b);
    } else {
        // Backward: swap added/removed
        full_threats::append_changed_indices(perspective, mirror, dirty, &mut list_b, &mut list_a);
    }
    // list_a = removed, list_b = added (regardless of direction)

    for &idx in list_a.as_slice() {
        let offset = idx as usize * TRANSFORMED_DIMS;
        let psqt_offset = idx as usize * PSQT_BUCKETS;
        if offset + TRANSFORMED_DIMS <= model.ft.threat_weights.len() {
            simd.vec_sub_i16_widening(
                &mut to_acc.accumulation[c],
                &model.ft.threat_weights[offset..offset + TRANSFORMED_DIMS],
            );
        }
        if psqt_offset + PSQT_BUCKETS <= model.ft.threat_psqt_weights.len() {
            for j in 0..PSQT_BUCKETS {
                to_acc.psqt_accumulation[c][j] -= model.ft.threat_psqt_weights[psqt_offset + j];
            }
        }
    }

    for &idx in list_b.as_slice() {
        let offset = idx as usize * TRANSFORMED_DIMS;
        let psqt_offset = idx as usize * PSQT_BUCKETS;
        if offset + TRANSFORMED_DIMS <= model.ft.threat_weights.len() {
            simd.vec_add_i16_widening(
                &mut to_acc.accumulation[c],
                &model.ft.threat_weights[offset..offset + TRANSFORMED_DIMS],
            );
        }
        if psqt_offset + PSQT_BUCKETS <= model.ft.threat_psqt_weights.len() {
            for j in 0..PSQT_BUCKETS {
                to_acc.psqt_accumulation[c][j] += model.ft.threat_psqt_weights[psqt_offset + j];
            }
        }
    }

    to_acc.computed[c] = true;
}

pub fn refresh_threat_accumulator(
    model: &NnueModel,
    pos: &Position,
    acc: &mut Accumulator,
    simd: &Dispatch,
) {
    for &perspective in &Color::ALL {
        let mut active = IndexList::new();
        full_threats::append_active_indices(pos, perspective, &mut active);

        let c = perspective as usize;
        acc.accumulation[c] = [0i16; TRANSFORMED_DIMS];
        acc.psqt_accumulation[c] = [0i32; PSQT_BUCKETS];

        for &idx in active.as_slice() {
            let offset = idx as usize * TRANSFORMED_DIMS;
            let psqt_offset = idx as usize * PSQT_BUCKETS;

            if offset + TRANSFORMED_DIMS <= model.ft.threat_weights.len() {
                simd.vec_add_i16_widening(
                    &mut acc.accumulation[c],
                    &model.ft.threat_weights[offset..offset + TRANSFORMED_DIMS],
                );
            }

            if psqt_offset + PSQT_BUCKETS <= model.ft.threat_psqt_weights.len() {
                for j in 0..PSQT_BUCKETS {
                    acc.psqt_accumulation[c][j] += model.ft.threat_psqt_weights[psqt_offset + j];
                }
            }
        }

        acc.computed[c] = true;
    }
}

/// Evaluate one perspective of the threat accumulator.
///
/// Uses the Pikafish `evaluate_side` pattern: `find_last_usable` →
/// `forward_update` OR (refresh top + `backward_update`).
pub fn evaluate_threat_side(
    model: &NnueModel,
    pos: &Position,
    perspective: Color,
    stack: &mut [super::accumulator::AccumulatorState],
    stack_size: usize,
    simd: &Dispatch,
) {
    use super::accumulator::DiffType;

    let c = perspective as usize;

    // find_last_usable: walk backward from top
    let mut last_usable = 0;
    for idx in (1..stack_size).rev() {
        if stack[idx].acc.computed[c] {
            last_usable = idx;
            break;
        }
        if let DiffType::DirtyThreats(ref dt) = stack[idx].diff {
            if dt.requires_refresh[c] {
                last_usable = idx;
                break;
            }
        }
    }

    let (_, mirror, _) = half_ka_v2_hm::make_feature_bucket(perspective, pos);

    if stack[last_usable].acc.computed[c] {
        // Case A: forward update from last_usable to top
        for next in (last_usable + 1)..stack_size {
            if let DiffType::DirtyThreats(ref dt) = stack[next].diff {
                let dirty = dt.clone();
                let (head, tail) = stack.split_at_mut(next);
                apply_threat_diff::<true>(
                    model,
                    perspective,
                    mirror,
                    &head[next - 1].acc,
                    &mut tail[0].acc,
                    &dirty,
                    simd,
                );
            }
        }
    } else {
        // Case B: refresh top, then backward update to last_usable
        refresh_threat_accumulator_one(
            model,
            pos,
            perspective,
            &mut stack[stack_size - 1].acc,
            simd,
        );

        for next in (last_usable..(stack_size - 1)).rev() {
            if let DiffType::DirtyThreats(ref dt) = stack[next + 1].diff {
                let dirty = dt.clone();
                let (head, tail) = stack.split_at_mut(next + 1);
                apply_threat_diff::<false>(
                    model,
                    perspective,
                    mirror,
                    &tail[0].acc,
                    &mut head[next].acc,
                    &dirty,
                    simd,
                );
            }
        }
    }
}

/// Refresh threat accumulator for a single perspective.
fn refresh_threat_accumulator_one(
    model: &NnueModel,
    pos: &Position,
    perspective: Color,
    acc: &mut super::accumulator::Accumulator,
    simd: &Dispatch,
) {
    let c = perspective as usize;
    let mut active = IndexList::new();
    full_threats::append_active_indices(pos, perspective, &mut active);

    acc.accumulation[c] = [0i16; TRANSFORMED_DIMS];
    acc.psqt_accumulation[c] = [0i32; PSQT_BUCKETS];

    for &idx in active.as_slice() {
        let offset = idx as usize * TRANSFORMED_DIMS;
        let psqt_offset = idx as usize * PSQT_BUCKETS;
        if offset + TRANSFORMED_DIMS <= model.ft.threat_weights.len() {
            simd.vec_add_i16_widening(
                &mut acc.accumulation[c],
                &model.ft.threat_weights[offset..offset + TRANSFORMED_DIMS],
            );
        }
        if psqt_offset + PSQT_BUCKETS <= model.ft.threat_psqt_weights.len() {
            for j in 0..PSQT_BUCKETS {
                acc.psqt_accumulation[c][j] += model.ft.threat_psqt_weights[psqt_offset + j];
            }
        }
    }
    acc.computed[c] = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nnue::features::half_ka_v2_hm;
    use crate::position::{GenType, generate};
    use crate::types::{Color, PieceType};

    fn test_simd() -> Dispatch {
        Dispatch::new()
    }

    #[test]
    fn test_refresh_psq_accumulator_start_pos() {
        let model_path = std::path::Path::new("../../models/pikafish.nnue");
        if !model_path.exists() {
            return;
        }
        let model = NnueModel::load(model_path).expect("model load");
        let pos = Position::start_pos().expect("start_pos");
        let simd = test_simd();
        let mut acc = Accumulator::new();
        refresh_psq_accumulator(&model, &pos, &mut acc, &simd);
        assert!(acc.computed[0]);
        assert!(acc.computed[1]);
        let has_nonzero = acc.accumulation[0].iter().any(|&v| v != 0);
        assert!(has_nonzero, "PSQ accumulator should have non-zero values");
    }

    #[test]
    fn test_refresh_threat_accumulator_start_pos() {
        let model_path = std::path::Path::new("../../models/pikafish.nnue");
        if !model_path.exists() {
            return;
        }
        let model = NnueModel::load(model_path).expect("model load");
        let pos = Position::start_pos().expect("start_pos");
        let simd = test_simd();
        let mut acc = Accumulator::new();
        refresh_threat_accumulator(&model, &pos, &mut acc, &simd);
        assert!(acc.computed[0]);
        assert!(acc.computed[1]);
        let has_nonzero = acc.accumulation[0].iter().any(|&v| v != 0);
        assert!(
            has_nonzero,
            "Threat accumulator should have non-zero values"
        );
    }

    #[test]
    fn test_incremental_psq_matches_refresh_all_moves() {
        let model_path = std::path::Path::new("../../models/pikafish.nnue");
        if !model_path.exists() {
            return;
        }
        let model = NnueModel::load(model_path).expect("model load");
        let mut pos = Position::start_pos().expect("start_pos");

        let simd = test_simd();
        let mut prev_acc = Accumulator::new();
        refresh_psq_accumulator(&model, &pos, &mut prev_acc, &simd);

        let ml = generate(&pos, GenType::Legal);
        for i in 0..ml.len() {
            let m = ml.get(i);
            let from = m.from_sq();
            let to = m.to_sq();
            let pc = pos.piece_on(from);
            let captured = pos.piece_on(to);

            let us = pos.side_to_move();
            let mut dirty = DirtyPiece::new();
            dirty.pc[0] = pc;
            dirty.from[0] = from;
            dirty.to[0] = to;
            if captured == Piece::NONE {
                dirty.dirty_num = 1;
            } else {
                dirty.dirty_num = 2;
                dirty.pc[1] = captured;
                dirty.from[1] = to;
                dirty.to[1] = Square::NONE;
            }
            dirty.requires_refresh[us as usize] = pc.piece_type() == PieceType::King;
            dirty.requires_refresh[(!us) as usize] = pc.piece_type() == PieceType::King;

            let gives_check = pos.gives_check(m);
            pos.do_move(m, gives_check);

            let mut inc_acc = Accumulator::new();
            update_psq_accumulator_incremental(
                &model,
                &pos,
                &prev_acc,
                &mut inc_acc,
                &dirty,
                &simd,
            );

            let mut ref_acc = Accumulator::new();
            refresh_psq_accumulator(&model, &pos, &mut ref_acc, &simd);

            for c in 0..2 {
                assert_eq!(
                    inc_acc.accumulation[c], ref_acc.accumulation[c],
                    "PSQ accumulation mismatch for color {c} after move {m:?}"
                );
                assert_eq!(
                    inc_acc.psqt_accumulation[c], ref_acc.psqt_accumulation[c],
                    "PSQT accumulation mismatch for color {c} after move {m:?}"
                );
            }

            pos.undo_move(m);
        }
    }

    #[test]
    fn test_incremental_psq_matches_refresh_captures() {
        let model_path = std::path::Path::new("../../models/pikafish.nnue");
        if !model_path.exists() {
            return;
        }
        let model = NnueModel::load(model_path).expect("model load");

        let fens = [
            "r1ba1a3/4kn3/2n1b4/pNp1p1p1p/4c4/6P2/P1P2R2P/1CcC5/9/2BAKAB2 w - - 0 1",
            "2bak4/9/3a5/p2Np3p/3n1P3/3pc3P/P4r1c1/B2CC2R1/4A4/3AK1B2 b - - 0 1",
            "5a3/3k5/3aR4/9/5r3/5n3/9/3A1A3/5K3/2BC2B2 w - - 0 1",
        ];

        for fen in &fens {
            let mut pos = Position::from_fen(fen).expect("parse fen");
            let simd = test_simd();
            let mut prev_acc = Accumulator::new();
            refresh_psq_accumulator(&model, &pos, &mut prev_acc, &simd);

            let ml = generate(&pos, GenType::Legal);
            for i in 0..ml.len() {
                let m = ml.get(i);
                let from = m.from_sq();
                let to = m.to_sq();
                let pc = pos.piece_on(from);
                let captured = pos.piece_on(to);

                let us = pos.side_to_move();
                let mut dirty = DirtyPiece::new();
                dirty.pc[0] = pc;
                dirty.from[0] = from;
                dirty.to[0] = to;
                if captured == Piece::NONE {
                    dirty.dirty_num = 1;
                } else {
                    dirty.dirty_num = 2;
                    dirty.pc[1] = captured;
                    dirty.from[1] = to;
                    dirty.to[1] = Square::NONE;
                }
                dirty.requires_refresh[us as usize] = pc.piece_type() == PieceType::King;
                dirty.requires_refresh[(!us) as usize] = pc.piece_type() == PieceType::King;

                if captured != Piece::NONE {
                    let cpt = captured.piece_type();
                    if cpt == PieceType::Rook
                        || cpt == PieceType::Knight
                        || cpt == PieceType::Cannon
                    {
                        let them = !us;
                        let before_bucket = half_ka_v2_hm::make_attack_bucket(&pos, them);
                        let new_bucket = {
                            let rook_count = pos.count_type(them, PieceType::Rook)
                                - u8::from(cpt == PieceType::Rook);
                            let kc_count = pos.count_type(them, PieceType::Knight)
                                + pos.count_type(them, PieceType::Cannon)
                                - u8::from(cpt == PieceType::Knight || cpt == PieceType::Cannon);
                            u32::from(rook_count > 0) * 2 + u32::from(kc_count > 0)
                        };
                        if new_bucket != before_bucket {
                            dirty.requires_refresh[them as usize] = true;
                        }
                    }
                }

                let gives_check = pos.gives_check(m);
                pos.do_move(m, gives_check);

                let mut inc_acc = Accumulator::new();
                update_psq_accumulator_incremental(
                    &model,
                    &pos,
                    &prev_acc,
                    &mut inc_acc,
                    &dirty,
                    &simd,
                );

                let mut ref_acc = Accumulator::new();
                refresh_psq_accumulator(&model, &pos, &mut ref_acc, &simd);

                for c in 0..2 {
                    assert_eq!(
                        inc_acc.accumulation[c], ref_acc.accumulation[c],
                        "PSQ mismatch color {c} move {m:?} in FEN: {fen}"
                    );
                    assert_eq!(
                        inc_acc.psqt_accumulation[c], ref_acc.psqt_accumulation[c],
                        "PSQT mismatch color {c} move {m:?} in FEN: {fen}"
                    );
                }

                pos.undo_move(m);
            }
        }
    }

    #[test]
    fn test_threat_incremental_vs_refresh_startpos() {
        let model_path = std::path::Path::new("../../models/pikafish.nnue");
        if !model_path.exists() {
            return;
        }
        let model = NnueModel::load(model_path).expect("model load");
        let mut pos = Position::start_pos().expect("start_pos");
        let simd = test_simd();

        let mut prev_acc = Accumulator::new();
        refresh_threat_accumulator(&model, &pos, &mut prev_acc, &simd);

        let ml = generate(&pos, GenType::Legal);
        for i in 0..ml.len() {
            let m = ml.get(i);
            let gives_check = pos.gives_check(m);

            let mirror_before = [
                half_ka_v2_hm::make_feature_bucket(Color::White, &pos).1,
                half_ka_v2_hm::make_feature_bucket(Color::Black, &pos).1,
            ];

            let mut dts = super::super::DirtyThreats::new();
            pos.do_move_with_threats(m, gives_check, &mut dts);

            let mirror_after = [
                half_ka_v2_hm::make_feature_bucket(Color::White, &pos).1,
                half_ka_v2_hm::make_feature_bucket(Color::Black, &pos).1,
            ];
            dts.requires_refresh[0] = mirror_before[0] != mirror_after[0];
            dts.requires_refresh[1] = mirror_before[1] != mirror_after[1];

            let mut inc_acc = Accumulator::new();
            update_threat_accumulator_incremental(
                &model,
                &pos,
                &prev_acc,
                &mut inc_acc,
                &dts,
                &simd,
            );

            let mut ref_acc = Accumulator::new();
            refresh_threat_accumulator(&model, &pos, &mut ref_acc, &simd);

            for c in 0..2 {
                assert_eq!(
                    inc_acc.accumulation[c], ref_acc.accumulation[c],
                    "threat mismatch perspective={c} move={m:?} (move {i})"
                );
            }

            pos.undo_move(m);
        }
    }

    #[test]
    fn test_threat_incremental_vs_refresh_captures() {
        let model_path = std::path::Path::new("../../models/pikafish.nnue");
        if !model_path.exists() {
            return;
        }
        let model = NnueModel::load(model_path).expect("model load");
        let simd = test_simd();

        let fens = [
            "r1ba1a3/4kn3/2n1b4/pNp1p1p1p/4c4/6P2/P1P2R2P/1CcC5/9/2BAKAB2 w - - 0 1",
            "2bak4/9/3a5/p2Np3p/3n1P3/3pc3P/P4r1c1/B2CC2R1/4A4/3AK1B2 b - - 0 1",
            "5a3/3k5/3aR4/9/5r3/5n3/9/3A1A3/5K3/2BC2B2 w - - 0 1",
            // Bench position 7 — triggers mismatch in search
            "2b1ka2r/3na2c1/4b3n/8R/8C/4C1P2/P1P1P3P/4B1N2/1r2A4/2BAK4 w - - 0 1",
        ];

        for fen in &fens {
            let mut pos = Position::from_fen(fen).expect("parse fen");
            let mut prev_acc = Accumulator::new();
            refresh_threat_accumulator(&model, &pos, &mut prev_acc, &simd);

            let ml = generate(&pos, GenType::Legal);
            for i in 0..ml.len() {
                let m = ml.get(i);
                let gives_check = pos.gives_check(m);

                // Compute mirror_before
                let mirror_before = [
                    half_ka_v2_hm::make_feature_bucket(Color::White, &pos).1,
                    half_ka_v2_hm::make_feature_bucket(Color::Black, &pos).1,
                ];

                let mut dts = super::super::DirtyThreats::new();
                pos.do_move_with_threats(m, gives_check, &mut dts);

                let mirror_after = [
                    half_ka_v2_hm::make_feature_bucket(Color::White, &pos).1,
                    half_ka_v2_hm::make_feature_bucket(Color::Black, &pos).1,
                ];
                dts.requires_refresh[0] = mirror_before[0] != mirror_after[0];
                dts.requires_refresh[1] = mirror_before[1] != mirror_after[1];

                let mut inc_acc = Accumulator::new();
                update_threat_accumulator_incremental(
                    &model,
                    &pos,
                    &prev_acc,
                    &mut inc_acc,
                    &dts,
                    &simd,
                );

                let mut ref_acc = Accumulator::new();
                refresh_threat_accumulator(&model, &pos, &mut ref_acc, &simd);

                for c in 0..2 {
                    assert_eq!(
                        inc_acc.accumulation[c], ref_acc.accumulation[c],
                        "threat mismatch perspective={c} move={m:?} fen={fen}"
                    );
                }

                pos.undo_move(m);
            }
        }
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn test_threat_incremental_multi_step() {
        use crate::nnue::features::half_ka_v2_hm;
        use crate::types::Color;

        let model_path = std::path::Path::new("../../models/pikafish.nnue");
        if !model_path.exists() {
            return;
        }
        let model = NnueModel::load(model_path).expect("model load");
        let simd = test_simd();

        // Test 2-ply from bench position 7
        let fen = "2b1ka2r/3na2c1/4b3n/8R/8C/4C1P2/P1P1P3P/4B1N2/1r2A4/2BAK4 w - - 0 1";
        let mut pos = Position::from_fen(fen).expect("parse fen");
        let mut prev_acc = Accumulator::new();
        refresh_threat_accumulator(&model, &pos, &mut prev_acc, &simd);

        let ml1 = generate(&pos, GenType::Legal);
        for i in 0..ml1.len() {
            let m1 = ml1.get(i);
            let gc1 = pos.gives_check(m1);

            let mb1 = [
                half_ka_v2_hm::make_feature_bucket(Color::White, &pos).1,
                half_ka_v2_hm::make_feature_bucket(Color::Black, &pos).1,
            ];
            let mut dts1 = super::super::DirtyThreats::new();
            pos.do_move_with_threats(m1, gc1, &mut dts1);
            let ma1 = [
                half_ka_v2_hm::make_feature_bucket(Color::White, &pos).1,
                half_ka_v2_hm::make_feature_bucket(Color::Black, &pos).1,
            ];
            dts1.requires_refresh[0] = mb1[0] != ma1[0];
            dts1.requires_refresh[1] = mb1[1] != ma1[1];

            let mut acc1 = Accumulator::new();
            update_threat_accumulator_incremental(&model, &pos, &prev_acc, &mut acc1, &dts1, &simd);

            let mut ref1 = Accumulator::new();
            refresh_threat_accumulator(&model, &pos, &mut ref1, &simd);

            for c in 0..2 {
                assert_eq!(
                    acc1.accumulation[c], ref1.accumulation[c],
                    "ply1 mismatch perspective={c} move={m1:?}"
                );
            }

            // Now try all ply-2 moves
            let ml2 = generate(&pos, GenType::Legal);
            for j in 0..ml2.len() {
                let m2 = ml2.get(j);
                let gc2 = pos.gives_check(m2);

                let mb2 = [
                    half_ka_v2_hm::make_feature_bucket(Color::White, &pos).1,
                    half_ka_v2_hm::make_feature_bucket(Color::Black, &pos).1,
                ];
                let mut dts2 = super::super::DirtyThreats::new();
                pos.do_move_with_threats(m2, gc2, &mut dts2);
                let ma2 = [
                    half_ka_v2_hm::make_feature_bucket(Color::White, &pos).1,
                    half_ka_v2_hm::make_feature_bucket(Color::Black, &pos).1,
                ];
                dts2.requires_refresh[0] = mb2[0] != ma2[0];
                dts2.requires_refresh[1] = mb2[1] != ma2[1];

                let mut acc2 = Accumulator::new();
                update_threat_accumulator_incremental(&model, &pos, &acc1, &mut acc2, &dts2, &simd);

                let mut ref2 = Accumulator::new();
                refresh_threat_accumulator(&model, &pos, &mut ref2, &simd);

                for c in 0..2 {
                    assert_eq!(
                        acc2.accumulation[c], ref2.accumulation[c],
                        "ply2 mismatch perspective={c} m1={m1:?} m2={m2:?}"
                    );
                }

                pos.undo_move(m2);
            }

            pos.undo_move(m1);
        }
    }
}
