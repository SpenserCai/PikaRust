use super::SimdOps;

pub struct Scalar;

impl SimdOps for Scalar {
    fn vec_add_i16(a: &mut [i16], b: &[i16]) {
        debug_assert_eq!(a.len(), b.len());
        for (x, &y) in a.iter_mut().zip(b.iter()) {
            *x = x.wrapping_add(y);
        }
    }

    fn vec_sub_i16(a: &mut [i16], b: &[i16]) {
        debug_assert_eq!(a.len(), b.len());
        for (x, &y) in a.iter_mut().zip(b.iter()) {
            *x = x.wrapping_sub(y);
        }
    }

    fn vec_add_i32(a: &mut [i32], b: &[i32]) {
        debug_assert_eq!(a.len(), b.len());
        for (x, &y) in a.iter_mut().zip(b.iter()) {
            *x = x.wrapping_add(y);
        }
    }

    fn vec_sub_i32(a: &mut [i32], b: &[i32]) {
        debug_assert_eq!(a.len(), b.len());
        for (x, &y) in a.iter_mut().zip(b.iter()) {
            *x = x.wrapping_sub(y);
        }
    }

    fn transform_features(psq_acc: &[i16], threat_acc: &[i16], output: &mut [u8]) {
        debug_assert_eq!(psq_acc.len(), 1024);
        debug_assert_eq!(threat_acc.len(), 1024);
        debug_assert!(output.len() >= 512);

        for j in 0..512 {
            let sum0 = i32::from(psq_acc[j]) + i32::from(threat_acc[j]);
            let sum1 = i32::from(psq_acc[j + 512]) + i32::from(threat_acc[j + 512]);
            let clamped0 = sum0.clamp(0, 255) as u32;
            let clamped1 = sum1.clamp(0, 255) as u32;
            output[j] = ((clamped0 * clamped1) / 512) as u8;
        }
    }

    fn clipped_relu(input: &[i32], output: &mut [u8], shift: u32) {
        for (i, &x) in input.iter().enumerate() {
            output[i] = (x >> shift).clamp(0, 127) as u8;
        }
    }

    fn sqr_clipped_relu(input: &[i32], output: &mut [u8], shift: u32) {
        for (i, &x) in input.iter().enumerate() {
            let v = i64::from(x);
            let squared = (v * v) >> (2 * shift + 7);
            output[i] = squared.min(127) as u8;
        }
    }

    fn affine_propagate(
        input: &[u8],
        weights: &[i8],
        biases: &[i32],
        output: &mut [i32],
        in_dim: usize,
        out_dim: usize,
    ) {
        output[..out_dim].copy_from_slice(&biases[..out_dim]);

        for i in 0..in_dim.min(input.len()) {
            if input[i] == 0 {
                continue;
            }
            let in_val = i32::from(input[i]);
            for o in 0..out_dim {
                output[o] += in_val * i32::from(weights[i * out_dim + o]);
            }
        }
    }

    fn horizontal_sum_i32(data: &[i32]) -> i32 {
        data.iter().sum()
    }

    fn find_nnz(input: &[u8], nnz_indices: &mut Vec<usize>) {
        nnz_indices.clear();
        let chunks = input.len() / 4;
        for i in 0..chunks {
            let base = i * 4;
            if input[base] != 0
                || input[base + 1] != 0
                || input[base + 2] != 0
                || input[base + 3] != 0
            {
                nnz_indices.push(i);
            }
        }
    }

    fn affine_propagate_sparse(
        input: &[u8],
        weights: &[i8],
        biases: &[i32],
        output: &mut [i32],
        out_dim: usize,
        nnz_indices: &[usize],
    ) {
        output[..out_dim].copy_from_slice(&biases[..out_dim]);

        for &block_idx in nnz_indices {
            let base = block_idx * 4;
            for sub in 0..4 {
                let idx = base + sub;
                if idx >= input.len() {
                    break;
                }
                if input[idx] == 0 {
                    continue;
                }
                let in_val = i32::from(input[idx]);
                for o in 0..out_dim {
                    output[o] += in_val * i32::from(weights[idx * out_dim + o]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scalar_vec_add_i16() {
        let mut a = [1i16, 2, 3, 4, -100, i16::MAX];
        let b = [10i16, 20, 30, 40, 100, 1];
        Scalar::vec_add_i16(&mut a, &b);
        assert_eq!(a, [11, 22, 33, 44, 0, i16::MIN]);
    }

    #[test]
    fn test_scalar_vec_sub_i16() {
        let mut a = [10i16, 20, 30, 0, i16::MIN];
        let b = [1i16, 2, 3, 1, 1];
        Scalar::vec_sub_i16(&mut a, &b);
        assert_eq!(a, [9, 18, 27, -1, i16::MAX]);
    }

    #[test]
    fn test_scalar_transform_features() {
        let mut psq = [0i16; 1024];
        let mut threat = [0i16; 1024];
        psq[0] = 100;
        threat[0] = 50;
        psq[512] = 200;
        threat[512] = 55;

        let mut output = [0u8; 512];
        Scalar::transform_features(&psq, &threat, &mut output);
        // sum0=150, sum1=255 -> (150*255)/512 = 74
        assert_eq!(output[0], 74);
    }

    #[test]
    fn test_scalar_transform_features_negative() {
        let mut psq = [0i16; 1024];
        let threat = [0i16; 1024];
        psq[0] = -100;
        psq[512] = 200;

        let mut output = [0u8; 512];
        Scalar::transform_features(&psq, &threat, &mut output);
        assert_eq!(output[0], 0);
    }

    #[test]
    fn test_scalar_clipped_relu() {
        let input = [0i32, 64, 128, -10, 127 * 64 + 100, 8128];
        let mut output = [0u8; 6];
        Scalar::clipped_relu(&input, &mut output, 6);
        assert_eq!(output, [0, 1, 2, 0, 127, 127]);
    }

    #[test]
    fn test_scalar_sqr_clipped_relu() {
        let input = [0i32, -10, 127 * 64];
        let mut output = [0u8; 3];
        Scalar::sqr_clipped_relu(&input, &mut output, 6);
        assert_eq!(output[0], 0);
        // -10 squared = 100, >> 19 = 0 (small value floors to 0)
        assert_eq!(output[1], 0);
        assert_eq!(output[2], 126);
    }

    /// Verify that `sqr_clipped_relu` matches C++ Pikafish behavior:
    /// the raw value is squared first (so negatives become positive),
    /// then the result is right-shifted and clamped to [0, 127].
    #[test]
    fn test_scalar_sqr_clipped_relu_negative_input_matches_cpp() {
        let shift = 6u32;
        // -8128 = -127 * 64. C++: (-8128)^2 >> 19 = 66_064_384 >> 19 = 126
        let input = [-8128i32, 8128, 0, -100, 100];
        let mut output = [0u8; 5];
        Scalar::sqr_clipped_relu(&input, &mut output, shift);

        // Negative input: squared first, so sign doesn't matter
        assert_eq!(
            output[0], 126,
            "input -8128 with shift=6 should produce 126"
        );
        // Positive input: same magnitude, same result
        assert_eq!(output[1], 126, "input 8128 with shift=6 should produce 126");
        // Zero input
        assert_eq!(output[2], 0, "input 0 should produce 0");
        // Symmetric: negative and positive of same magnitude must match
        assert_eq!(
            output[3], output[4],
            "sqr_clipped_relu must be symmetric around zero"
        );
    }

    #[test]
    fn test_scalar_affine_propagate() {
        let input = [0u8, 2, 0, 3];
        let weights: Vec<i8> = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let biases = [10i32, 20];
        let mut output = [0i32; 2];
        Scalar::affine_propagate(&input, &weights, &biases, &mut output, 4, 2);
        assert_eq!(output[0], 37);
        assert_eq!(output[1], 52);
    }

    #[test]
    fn test_scalar_horizontal_sum() {
        let data = [1i32, 2, 3, 4, 5];
        assert_eq!(Scalar::horizontal_sum_i32(&data), 15);
    }

    #[test]
    fn test_scalar_find_nnz() {
        let mut input = [0u8; 16];
        input[1] = 5;
        input[8] = 1;
        input[11] = 2;
        let mut nnz = Vec::new();
        Scalar::find_nnz(&input, &mut nnz);
        assert_eq!(nnz, vec![0, 2]);
    }

    #[test]
    fn test_scalar_affine_sparse() {
        let input = [0u8, 2, 0, 3, 0, 0, 0, 0];
        let weights: Vec<i8> = vec![1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 0, 0, 0, 0];
        let biases = [10i32, 20];
        let mut output = [0i32; 2];
        let nnz = vec![0usize];
        Scalar::affine_propagate_sparse(&input, &weights, &biases, &mut output, 2, &nnz);
        assert_eq!(output[0], 37);
        assert_eq!(output[1], 52);
    }
}
