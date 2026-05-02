use crate::position::Position;
use crate::types::Color;

use super::accumulator::Accumulator;
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
}
