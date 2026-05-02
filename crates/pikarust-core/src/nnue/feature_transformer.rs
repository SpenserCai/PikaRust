use crate::position::Position;
use crate::types::{Color, Piece, Square};

use super::accumulator::{Accumulator, DirtyPiece};
use super::features::IndexList;
use super::features::full_threats;
use super::features::half_ka_v2_hm;
use super::model::{NnueModel, PSQT_BUCKETS, TRANSFORMED_DIMS};

pub fn refresh_psq_accumulator(model: &NnueModel, pos: &Position, acc: &mut Accumulator) {
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
                for j in 0..TRANSFORMED_DIMS {
                    acc.accumulation[c][j] += i16::from(model.ft.weights[offset + j]);
                }
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
                    for j in 0..TRANSFORMED_DIMS {
                        acc.accumulation[c][j] += i16::from(model.ft.weights[offset + j]);
                    }
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
                    for j in 0..TRANSFORMED_DIMS {
                        acc.accumulation[c][j] -= i16::from(model.ft.weights[offset + j]);
                    }
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
                    for j in 0..TRANSFORMED_DIMS {
                        acc.accumulation[c][j] += i16::from(model.ft.weights[offset + j]);
                    }
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

pub fn refresh_threat_accumulator(model: &NnueModel, pos: &Position, acc: &mut Accumulator) {
    for &perspective in &Color::ALL {
        let mut active = IndexList::new();
        full_threats::append_active_indices(pos, perspective, &mut active);

        let c = perspective as usize;
        acc.accumulation[c] = *model.ft.biases;
        acc.psqt_accumulation[c] = [0i32; PSQT_BUCKETS];

        for &idx in active.as_slice() {
            let offset = idx as usize * TRANSFORMED_DIMS;
            let psqt_offset = idx as usize * PSQT_BUCKETS;

            if offset + TRANSFORMED_DIMS <= model.ft.threat_weights.len() {
                for j in 0..TRANSFORMED_DIMS {
                    acc.accumulation[c][j] += i16::from(model.ft.threat_weights[offset + j]);
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::{GenType, generate};
    use crate::types::PieceType;

    #[test]
    fn test_refresh_psq_accumulator_start_pos() {
        let model_path = std::path::Path::new("../../models/pikafish.nnue");
        if !model_path.exists() {
            return;
        }
        let model = NnueModel::load(model_path).expect("model load");
        let pos = Position::start_pos().expect("start_pos");
        let mut acc = Accumulator::new();
        refresh_psq_accumulator(&model, &pos, &mut acc);
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
        let mut acc = Accumulator::new();
        refresh_threat_accumulator(&model, &pos, &mut acc);
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

        let mut prev_acc = Accumulator::new();
        refresh_psq_accumulator(&model, &pos, &mut prev_acc);

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
            update_psq_accumulator_incremental(&model, &pos, &prev_acc, &mut inc_acc, &dirty);

            let mut ref_acc = Accumulator::new();
            refresh_psq_accumulator(&model, &pos, &mut ref_acc);

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
            let mut prev_acc = Accumulator::new();
            refresh_psq_accumulator(&model, &pos, &mut prev_acc);

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
                update_psq_accumulator_incremental(&model, &pos, &prev_acc, &mut inc_acc, &dirty);

                let mut ref_acc = Accumulator::new();
                refresh_psq_accumulator(&model, &pos, &mut ref_acc);

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
}
