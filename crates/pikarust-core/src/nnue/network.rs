use crate::types::{Color, Piece, PieceType, Value};

use super::model::{
    L2_BIG, L3_BIG, LayerStackWeights, NnueModel, OUTPUT_SCALE, PSQT_BUCKETS, TRANSFORMED_DIMS,
    WEIGHT_SCALE_BITS,
};

pub static LAYER_STACK_BUCKETS: [[[[u8; 5]; 5]; 3]; 3] = {
    let mut v = [[[[0u8; 5]; 5]; 3]; 3];
    let mut us_rook: usize = 0;
    while us_rook <= 2 {
        let mut opp_rook: usize = 0;
        while opp_rook <= 2 {
            let mut us_kc: usize = 0;
            while us_kc <= 4 {
                let mut opp_kc: usize = 0;
                while opp_kc <= 4 {
                    v[us_rook][opp_rook][us_kc][opp_kc] = if us_rook == opp_rook {
                        (us_rook * 4
                            + (us_kc + opp_kc >= 4) as usize * 2
                            + (us_kc == opp_kc) as usize) as u8
                    } else if us_rook == 2 && opp_rook == 1 {
                        12
                    } else if us_rook == 1 && opp_rook == 2 {
                        13
                    } else if us_rook > 0 && opp_rook == 0 {
                        14
                    } else {
                        15
                    };
                    opp_kc += 1;
                }
                us_kc += 1;
            }
            opp_rook += 1;
        }
        us_rook += 1;
    }
    v
};

pub fn make_layer_stack_bucket(piece_count: &[u8; Piece::NUM], side_to_move: Color) -> usize {
    let stm = side_to_move;
    let us_rook = piece_count[Piece::make(stm, PieceType::Rook).index()];
    let opp_rook = piece_count[Piece::make(!stm, PieceType::Rook).index()];
    let us_kc = piece_count[Piece::make(stm, PieceType::Knight).index()]
        + piece_count[Piece::make(stm, PieceType::Cannon).index()];
    let opp_kc = piece_count[Piece::make(!stm, PieceType::Knight).index()]
        + piece_count[Piece::make(!stm, PieceType::Cannon).index()];

    LAYER_STACK_BUCKETS[us_rook.min(2) as usize][opp_rook.min(2) as usize][us_kc.min(4) as usize]
        [opp_kc.min(4) as usize] as usize
}

const FC0_OUTPUTS: usize = L2_BIG + 1; // 32

pub struct Network {
    model: NnueModel,
}

impl Network {
    pub const fn new(model: NnueModel) -> Self {
        Self { model }
    }

    pub const fn model(&self) -> &NnueModel {
        &self.model
    }

    pub fn evaluate(
        &self,
        psq_acc: &[[i16; TRANSFORMED_DIMS]; 2],
        threat_acc: &[[i16; TRANSFORMED_DIMS]; 2],
        psqt_psq: &[[i32; PSQT_BUCKETS]; 2],
        psqt_threat: &[[i32; PSQT_BUCKETS]; 2],
        piece_count: &[u8; Piece::NUM],
        side_to_move: Color,
    ) -> (Value, Value) {
        let bucket = make_layer_stack_bucket(piece_count, side_to_move);
        let stm = side_to_move;

        let psqt_val = (psqt_psq[stm as usize][bucket] - psqt_psq[!stm as usize][bucket]
            + psqt_threat[stm as usize][bucket]
            - psqt_threat[!stm as usize][bucket])
            / 2;

        let perspectives = [stm, !stm];
        let mut transformed = [0u8; TRANSFORMED_DIMS];
        transform(psq_acc, threat_acc, perspectives, &mut transformed);

        let ls = &self.model.layer_stacks[bucket];
        let positional = propagate_layers(ls, &transformed);

        (psqt_val / OUTPUT_SCALE, positional)
    }
}

fn transform(
    psq_acc: &[[i16; TRANSFORMED_DIMS]; 2],
    threat_acc: &[[i16; TRANSFORMED_DIMS]; 2],
    perspectives: [Color; 2],
    output: &mut [u8; TRANSFORMED_DIMS],
) {
    for (p, &perspective) in perspectives.iter().enumerate() {
        let offset = p * 512;
        let c = perspective as usize;
        for j in 0..512 {
            let sum0 = i32::from(psq_acc[c][j]) + i32::from(threat_acc[c][j]);
            let sum1 = i32::from(psq_acc[c][j + 512]) + i32::from(threat_acc[c][j + 512]);
            let clamped0 = sum0.clamp(0, 255) as u32;
            let clamped1 = sum1.clamp(0, 255) as u32;
            output[offset + j] = ((clamped0 * clamped1) / 512) as u8;
        }
    }
}

fn propagate_layers(ls: &LayerStackWeights, input: &[u8; TRANSFORMED_DIMS]) -> Value {
    let mut fc0_out = [0i32; FC0_OUTPUTS];
    affine_sparse(
        input,
        &ls.fc0_weights,
        ls.fc0_biases.as_slice(),
        &mut fc0_out,
        TRANSFORMED_DIMS,
        FC0_OUTPUTS,
    );

    let mut sqr_relu_out = [0u8; L2_BIG];
    sqr_clipped_relu(&fc0_out[..L2_BIG], &mut sqr_relu_out);

    let mut relu_out = [0u8; L2_BIG];
    clipped_relu(&fc0_out[..L2_BIG], &mut relu_out);

    // Concatenate: [sqr_relu_out || relu_out] = 62 bytes
    let mut concat = [0u8; L2_BIG * 2]; // 62
    concat[..L2_BIG].copy_from_slice(&sqr_relu_out);
    concat[L2_BIG..].copy_from_slice(&relu_out);

    // fc_1: 62 inputs (padded to 64 in weights) -> 32 outputs
    let mut fc1_out = [0i32; L3_BIG];
    affine_dense(
        &concat,
        &ls.fc1_weights,
        ls.fc1_biases.as_slice(),
        &mut fc1_out,
        64, // padded input dimension
        L3_BIG,
    );

    let mut fc1_relu = [0u8; L3_BIG];
    clipped_relu(&fc1_out, &mut fc1_relu);

    // fc_2: 32 inputs -> 1 output
    let mut fc2_out = [0i32; 1];
    affine_dense(
        &fc1_relu,
        &ls.fc2_weights,
        ls.fc2_biases.as_slice(),
        &mut fc2_out,
        32,
        1,
    );

    // Skip connection: fc_0_out[31] scaled
    let skip = i64::from(fc0_out[L2_BIG]);
    let fwd_out = (skip * (600 * i64::from(OUTPUT_SCALE))) / (127 * (1i64 << WEIGHT_SCALE_BITS));

    (fc2_out[0] + fwd_out as i32) / OUTPUT_SCALE
}

fn affine_sparse(
    input: &[u8],
    weights: &[i8],
    biases: &[i32],
    output: &mut [i32],
    in_dim: usize,
    out_dim: usize,
) {
    output[..out_dim].copy_from_slice(&biases[..out_dim]);

    for i in 0..in_dim {
        if input[i] == 0 {
            continue;
        }
        let in_val = i32::from(input[i]);
        for o in 0..out_dim {
            output[o] += in_val * i32::from(weights[i * out_dim + o]);
        }
    }
}

fn affine_dense(
    input: &[u8],
    weights: &[i8],
    biases: &[i32],
    output: &mut [i32],
    padded_in_dim: usize,
    out_dim: usize,
) {
    output[..out_dim].copy_from_slice(&biases[..out_dim]);

    let actual_in = input.len();
    for i in 0..actual_in {
        if input[i] == 0 {
            continue;
        }
        let in_val = i32::from(input[i]);
        for o in 0..out_dim {
            output[o] += in_val * i32::from(weights[i * out_dim + o]);
        }
    }
    // Padded positions (actual_in..padded_in_dim) have zero input, no contribution.
    let _ = padded_in_dim;
}

fn clipped_relu(input: &[i32], output: &mut [u8]) {
    for (i, &x) in input.iter().enumerate() {
        output[i] = (x >> WEIGHT_SCALE_BITS).clamp(0, 127) as u8;
    }
}

fn sqr_clipped_relu(input: &[i32], output: &mut [u8]) {
    let max_val = 127i32 << WEIGHT_SCALE_BITS;
    for (i, &x) in input.iter().enumerate() {
        let clamped = i64::from(x.clamp(0, max_val));
        let squared = (clamped * clamped) >> (2 * WEIGHT_SCALE_BITS + 7);
        output[i] = squared.min(127) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::model::LAYER_STACKS;

    #[test]
    fn test_layer_stack_buckets_range() {
        for us_rook in 0..3 {
            for opp_rook in 0..3 {
                for us_kc in 0..5 {
                    for opp_kc in 0..5 {
                        let b = LAYER_STACK_BUCKETS[us_rook][opp_rook][us_kc][opp_kc];
                        assert!(
                            (b as usize) < LAYER_STACKS,
                            "bucket {b} out of range for [{us_rook}][{opp_rook}][{us_kc}][{opp_kc}]"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_layer_stack_buckets_equal_rooks() {
        assert_eq!(LAYER_STACK_BUCKETS[0][0][0][0], 1);
        assert_eq!(LAYER_STACK_BUCKETS[0][0][2][2], 3);
        assert_eq!(LAYER_STACK_BUCKETS[1][1][0][0], 5);
        assert_eq!(LAYER_STACK_BUCKETS[2][2][0][0], 9);
    }

    #[test]
    fn test_layer_stack_buckets_unequal_rooks() {
        assert_eq!(LAYER_STACK_BUCKETS[2][1][0][0], 12);
        assert_eq!(LAYER_STACK_BUCKETS[1][2][0][0], 13);
        assert_eq!(LAYER_STACK_BUCKETS[1][0][0][0], 14);
        assert_eq!(LAYER_STACK_BUCKETS[2][0][0][0], 14);
        assert_eq!(LAYER_STACK_BUCKETS[0][1][0][0], 15);
        assert_eq!(LAYER_STACK_BUCKETS[0][2][0][0], 15);
    }

    #[test]
    fn test_clipped_relu_basic() {
        let input = [0i32, 64, 128, -10, 127 * 64 + 100, 8128];
        let mut output = [0u8; 6];
        clipped_relu(&input, &mut output);
        assert_eq!(output[0], 0);
        assert_eq!(output[1], 1);
        assert_eq!(output[2], 2);
        assert_eq!(output[3], 0);
        assert_eq!(output[4], 127);
        assert_eq!(output[5], 127);
    }

    #[test]
    fn test_sqr_clipped_relu_basic() {
        let input = [0i32, -10, 127 * 64];
        let mut output = [0u8; 3];
        sqr_clipped_relu(&input, &mut output);
        assert_eq!(output[0], 0);
        assert_eq!(output[1], 0);
        // (127*64)^2 >> (12+7) = (8128)^2 >> 19 = 66_064_384 >> 19 = 126
        assert_eq!(output[2], 126);
    }

    #[test]
    fn test_transform_basic() {
        let mut psq_acc = [[0i16; TRANSFORMED_DIMS]; 2];
        let mut threat_acc = [[0i16; TRANSFORMED_DIMS]; 2];

        psq_acc[0][0] = 100;
        threat_acc[0][0] = 50;
        psq_acc[0][512] = 200;
        threat_acc[0][512] = 55;

        let perspectives = [Color::White, Color::Black];
        let mut output = [0u8; TRANSFORMED_DIMS];
        transform(&psq_acc, &threat_acc, perspectives, &mut output);

        // sum0 = 150, sum1 = 255 -> (150 * 255) / 512 = 74
        assert_eq!(output[0], 74);
    }

    #[test]
    fn test_transform_clamp_negative() {
        let mut psq_acc = [[0i16; TRANSFORMED_DIMS]; 2];
        let threat_acc = [[0i16; TRANSFORMED_DIMS]; 2];

        psq_acc[0][0] = -100;
        psq_acc[0][512] = 200;

        let perspectives = [Color::White, Color::Black];
        let mut output = [0u8; TRANSFORMED_DIMS];
        transform(&psq_acc, &threat_acc, perspectives, &mut output);

        // sum0 = -100 -> clamped to 0, result = 0
        assert_eq!(output[0], 0);
    }

    #[test]
    fn test_affine_sparse_basic() {
        let input = [0u8, 2, 0, 3];
        let weights: Vec<i8> = vec![
            // input[0] * weights -> skipped (input=0)
            1, 2, // input[1] * weights
            3, 4, // input[2] * weights -> skipped (input=0)
            5, 6, // input[3] * weights
            7, 8,
        ];
        let biases = [10i32, 20];
        let mut output = [0i32; 2];
        affine_sparse(&input, &weights, &biases, &mut output, 4, 2);

        // output[0] = 10 + 2*3 + 3*7 = 10 + 6 + 21 = 37
        // output[1] = 20 + 2*4 + 3*8 = 20 + 8 + 24 = 52
        assert_eq!(output[0], 37);
        assert_eq!(output[1], 52);
    }
}
