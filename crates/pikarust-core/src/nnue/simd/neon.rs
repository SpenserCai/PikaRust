#[allow(clippy::wildcard_imports)]
use std::arch::aarch64::*;

use super::SimdOps;

pub struct Neon;

impl SimdOps for Neon {
    fn vec_add_i16(a: &mut [i16], b: &[i16]) {
        debug_assert_eq!(a.len(), b.len());
        let len = a.len();
        let chunks = len / 8;
        let remainder = chunks * 8;

        // SAFETY: NEON is always available on aarch64. Pointers are valid for
        // the slice lengths and we process only complete 8-element chunks.
        unsafe {
            let a_ptr = a.as_mut_ptr();
            let b_ptr = b.as_ptr();
            for i in 0..chunks {
                let offset = i * 8;
                let va = vld1q_s16(a_ptr.add(offset));
                let vb = vld1q_s16(b_ptr.add(offset));
                vst1q_s16(a_ptr.add(offset), vaddq_s16(va, vb));
            }
        }

        for i in remainder..len {
            a[i] = a[i].wrapping_add(b[i]);
        }
    }

    fn vec_sub_i16(a: &mut [i16], b: &[i16]) {
        debug_assert_eq!(a.len(), b.len());
        let len = a.len();
        let chunks = len / 8;
        let remainder = chunks * 8;

        // SAFETY: NEON is always available on aarch64. Pointers are valid for
        // the slice lengths and we process only complete 8-element chunks.
        unsafe {
            let a_ptr = a.as_mut_ptr();
            let b_ptr = b.as_ptr();
            for i in 0..chunks {
                let offset = i * 8;
                let va = vld1q_s16(a_ptr.add(offset));
                let vb = vld1q_s16(b_ptr.add(offset));
                vst1q_s16(a_ptr.add(offset), vsubq_s16(va, vb));
            }
        }

        for i in remainder..len {
            a[i] = a[i].wrapping_sub(b[i]);
        }
    }

    fn vec_add_i16_widening(acc: &mut [i16], weights: &[i8]) {
        let len = acc.len().min(weights.len());
        let chunks = len / 16;

        // SAFETY: NEON is always available on aarch64. We process 16 elements
        // per iteration: load 16×i8, widening-add to two 8×i16 vectors.
        unsafe {
            let acc_ptr = acc.as_mut_ptr();
            let w_ptr = weights.as_ptr();
            for i in 0..chunks {
                let off = i * 16;
                let w8 = vld1q_s8(w_ptr.add(off));
                let a_lo = vld1q_s16(acc_ptr.add(off));
                let a_hi = vld1q_s16(acc_ptr.add(off + 8));
                vst1q_s16(acc_ptr.add(off), vaddw_s8(a_lo, vget_low_s8(w8)));
                vst1q_s16(acc_ptr.add(off + 8), vaddw_high_s8(a_hi, w8));
            }
        }

        for i in (chunks * 16)..len {
            acc[i] += i16::from(weights[i]);
        }
    }

    fn vec_sub_i16_widening(acc: &mut [i16], weights: &[i8]) {
        let len = acc.len().min(weights.len());
        let chunks = len / 16;

        // SAFETY: Same as vec_add_i16_widening but with vsubw_s8.
        unsafe {
            let acc_ptr = acc.as_mut_ptr();
            let w_ptr = weights.as_ptr();
            for i in 0..chunks {
                let off = i * 16;
                let w8 = vld1q_s8(w_ptr.add(off));
                let a_lo = vld1q_s16(acc_ptr.add(off));
                let a_hi = vld1q_s16(acc_ptr.add(off + 8));
                vst1q_s16(acc_ptr.add(off), vsubw_s8(a_lo, vget_low_s8(w8)));
                vst1q_s16(acc_ptr.add(off + 8), vsubw_high_s8(a_hi, w8));
            }
        }

        for i in (chunks * 16)..len {
            acc[i] -= i16::from(weights[i]);
        }
    }

    fn vec_add_i32(a: &mut [i32], b: &[i32]) {
        debug_assert_eq!(a.len(), b.len());
        let len = a.len();
        let chunks = len / 4;
        let remainder = chunks * 4;

        // SAFETY: NEON is always available on aarch64. Pointers are valid.
        unsafe {
            let a_ptr = a.as_mut_ptr();
            let b_ptr = b.as_ptr();
            for i in 0..chunks {
                let offset = i * 4;
                let va = vld1q_s32(a_ptr.add(offset));
                let vb = vld1q_s32(b_ptr.add(offset));
                vst1q_s32(a_ptr.add(offset), vaddq_s32(va, vb));
            }
        }

        for i in remainder..len {
            a[i] = a[i].wrapping_add(b[i]);
        }
    }

    fn vec_sub_i32(a: &mut [i32], b: &[i32]) {
        debug_assert_eq!(a.len(), b.len());
        let len = a.len();
        let chunks = len / 4;
        let remainder = chunks * 4;

        // SAFETY: NEON is always available on aarch64. Pointers are valid.
        unsafe {
            let a_ptr = a.as_mut_ptr();
            let b_ptr = b.as_ptr();
            for i in 0..chunks {
                let offset = i * 4;
                let va = vld1q_s32(a_ptr.add(offset));
                let vb = vld1q_s32(b_ptr.add(offset));
                vst1q_s32(a_ptr.add(offset), vsubq_s32(va, vb));
            }
        }

        for i in remainder..len {
            a[i] = a[i].wrapping_sub(b[i]);
        }
    }

    fn transform_features(psq_acc: &[i16], threat_acc: &[i16], output: &mut [u8]) {
        debug_assert_eq!(psq_acc.len(), 1024);
        debug_assert_eq!(threat_acc.len(), 1024);
        debug_assert!(output.len() >= 512);

        // SAFETY: NEON is always available on aarch64. All pointer accesses are
        // within the validated slice bounds (512 i16 pairs -> 512 u8 outputs).
        unsafe {
            let psq_ptr = psq_acc.as_ptr();
            let threat_ptr = threat_acc.as_ptr();
            let out_ptr = output.as_mut_ptr();

            let zero = vdupq_n_s16(0);
            let max_val = vdupq_n_s16(255);

            let mut j = 0;
            while j + 8 <= 512 {
                let p0 = vld1q_s16(psq_ptr.add(j));
                let t0 = vld1q_s16(threat_ptr.add(j));
                let sum0 = vaddq_s16(p0, t0);
                let clamped0 = vminq_s16(vmaxq_s16(sum0, zero), max_val);

                let p1 = vld1q_s16(psq_ptr.add(j + 512));
                let t1 = vld1q_s16(threat_ptr.add(j + 512));
                let sum1 = vaddq_s16(p1, t1);
                let clamped1 = vminq_s16(vmaxq_s16(sum1, zero), max_val);

                let c0_lo = vget_low_s16(clamped0);
                let c0_hi = vget_high_s16(clamped0);
                let c1_lo = vget_low_s16(clamped1);
                let c1_hi = vget_high_s16(clamped1);

                let prod_lo = vmull_s16(c0_lo, c1_lo);
                let prod_hi = vmull_s16(c0_hi, c1_hi);

                let div_lo = vshrn_n_s32(prod_lo, 9);
                let div_hi = vshrn_n_s32(prod_hi, 9);
                let combined = vcombine_s16(div_lo, div_hi);

                let narrow = vqmovun_s16(combined);
                vst1_u8(out_ptr.add(j), narrow);

                j += 8;
            }

            for k in j..512 {
                let s0 = i32::from(*psq_ptr.add(k)) + i32::from(*threat_ptr.add(k));
                let s1 = i32::from(*psq_ptr.add(k + 512)) + i32::from(*threat_ptr.add(k + 512));
                let c0 = s0.clamp(0, 255) as u32;
                let c1 = s1.clamp(0, 255) as u32;
                *out_ptr.add(k) = ((c0 * c1) / 512) as u8;
            }
        }
    }

    fn clipped_relu(input: &[i32], output: &mut [u8], shift: u32) {
        debug_assert_eq!(shift, 6);
        let len = input.len();
        let chunks = len / 16;
        let remainder = chunks * 16;

        // SAFETY: NEON is always available on aarch64. We process 16 i32 elements
        // at a time, narrowing to 16 u8 values. All accesses are within bounds.
        unsafe {
            let in_ptr = input.as_ptr();
            let out_ptr = output.as_mut_ptr();

            for i in 0..chunks {
                let base = i * 16;
                let v0 = vshrq_n_s32::<6>(vld1q_s32(in_ptr.add(base)));
                let v1 = vshrq_n_s32::<6>(vld1q_s32(in_ptr.add(base + 4)));
                let v2 = vshrq_n_s32::<6>(vld1q_s32(in_ptr.add(base + 8)));
                let v3 = vshrq_n_s32::<6>(vld1q_s32(in_ptr.add(base + 12)));

                let max127 = vdupq_n_s32(127);
                let v0 = vminq_s32(v0, max127);
                let v1 = vminq_s32(v1, max127);
                let v2 = vminq_s32(v2, max127);
                let v3 = vminq_s32(v3, max127);

                let lo16 = vqmovn_s32(v0);
                let hi16 = vqmovn_s32(v1);
                let s16_0 = vcombine_s16(lo16, hi16);

                let lo16_2 = vqmovn_s32(v2);
                let hi16_2 = vqmovn_s32(v3);
                let s16_1 = vcombine_s16(lo16_2, hi16_2);

                let u8_0 = vqmovun_s16(s16_0);
                let u8_1 = vqmovun_s16(s16_1);
                let result = vcombine_u8(u8_0, u8_1);

                vst1q_u8(out_ptr.add(base), result);
            }
        }

        for i in remainder..len {
            output[i] = (input[i] >> 6).clamp(0, 127) as u8;
        }
    }

    fn sqr_clipped_relu(input: &[i32], output: &mut [u8], shift: u32) {
        debug_assert_eq!(shift, 6);
        let len = input.len();

        // Scalar formula: (v * v) >> (2*shift + 7) = (v * v) >> 19, clamped to [0, 127]
        // We use scalar code to match Pikafish's NEON path exactly (no SIMD for
        // sqr_clipped_relu on ARM in Pikafish). This avoids i32 overflow issues
        // that would require i64 multiplication.

        for i in 0..len {
            let v = i64::from(input[i]);
            output[i] = ((v * v) >> 19).min(127) as u8;
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

        if out_dim >= 16 {
            let out_chunks = out_dim / 16;

            // SAFETY: NEON is always available on aarch64. We process 16 output
            // elements at a time. For each non-zero input, we broadcast the input
            // value, widen 16 weight bytes to i16, multiply, widen to i32, and
            // accumulate. All pointer arithmetic stays within slice bounds.
            unsafe {
                let out_ptr = output.as_mut_ptr();

                for (i, &in_val_byte) in input.iter().enumerate().take(in_dim.min(input.len())) {
                    if in_val_byte == 0 {
                        continue;
                    }
                    let in_val = vdupq_n_s16(i16::from(in_val_byte));
                    let w_base = i * out_dim;

                    for c in 0..out_chunks {
                        let o = c * 16;
                        let w_ptr = weights.as_ptr().add(w_base + o);

                        let w8 = vld1q_s8(w_ptr);
                        let w16_lo = vmovl_s8(vget_low_s8(w8));
                        let w16_hi = vmovl_high_s8(w8);

                        let prod_lo_lo = vmull_s16(vget_low_s16(in_val), vget_low_s16(w16_lo));
                        let prod_lo_hi = vmull_s16(vget_low_s16(in_val), vget_high_s16(w16_lo));
                        let prod_hi_lo = vmull_s16(vget_low_s16(in_val), vget_low_s16(w16_hi));
                        let prod_hi_hi = vmull_s16(vget_low_s16(in_val), vget_high_s16(w16_hi));

                        let a0 = vld1q_s32(out_ptr.add(o));
                        let a1 = vld1q_s32(out_ptr.add(o + 4));
                        let a2 = vld1q_s32(out_ptr.add(o + 8));
                        let a3 = vld1q_s32(out_ptr.add(o + 12));

                        vst1q_s32(out_ptr.add(o), vaddq_s32(a0, prod_lo_lo));
                        vst1q_s32(out_ptr.add(o + 4), vaddq_s32(a1, prod_lo_hi));
                        vst1q_s32(out_ptr.add(o + 8), vaddq_s32(a2, prod_hi_lo));
                        vst1q_s32(out_ptr.add(o + 12), vaddq_s32(a3, prod_hi_hi));
                    }
                }
            }

            let simd_end = out_chunks * 16;
            for i in 0..in_dim.min(input.len()) {
                if input[i] == 0 {
                    continue;
                }
                let in_val = i32::from(input[i]);
                for o in simd_end..out_dim {
                    output[o] += in_val * i32::from(weights[i * out_dim + o]);
                }
            }
        } else {
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
    }

    fn horizontal_sum_i32(data: &[i32]) -> i32 {
        let len = data.len();
        let chunks = len / 4;
        let remainder = chunks * 4;
        // SAFETY: NEON is always available on aarch64. We load 4 i32 at a time
        // and reduce with vaddvq_s32. All accesses are within bounds.
        let mut sum: i32 = unsafe {
            let ptr = data.as_ptr();
            let mut acc = vdupq_n_s32(0);
            for i in 0..chunks {
                let v = vld1q_s32(ptr.add(i * 4));
                acc = vaddq_s32(acc, v);
            }
            vaddvq_s32(acc)
        };

        for &x in &data[remainder..] {
            sum = sum.wrapping_add(x);
        }
        sum
    }

    fn find_nnz(input: &[u8], nnz_indices: &mut [usize; super::MAX_NNZ]) -> usize {
        let chunks = input.len() / 4;
        debug_assert!(chunks <= super::MAX_NNZ);
        let mut count = 0;
        for i in 0..chunks {
            let base = i * 4;
            let word = u32::from_ne_bytes([
                input[base],
                input[base + 1],
                input[base + 2],
                input[base + 3],
            ]);
            if word != 0 {
                nnz_indices[count] = i;
                count += 1;
            }
        }
        count
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

        if out_dim >= 16 {
            let out_chunks = out_dim / 16;

            // SAFETY: NEON is always available on aarch64. Same pattern as
            // affine_propagate but only visiting non-zero input blocks.
            unsafe {
                let out_ptr = output.as_mut_ptr();

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
                        let in_val = vdupq_n_s16(i16::from(input[idx]));
                        let w_base = idx * out_dim;

                        for c in 0..out_chunks {
                            let o = c * 16;
                            let w_ptr = weights.as_ptr().add(w_base + o);

                            let w8 = vld1q_s8(w_ptr);
                            let w16_lo = vmovl_s8(vget_low_s8(w8));
                            let w16_hi = vmovl_high_s8(w8);

                            let p0 = vmull_s16(vget_low_s16(in_val), vget_low_s16(w16_lo));
                            let p1 = vmull_s16(vget_low_s16(in_val), vget_high_s16(w16_lo));
                            let p2 = vmull_s16(vget_low_s16(in_val), vget_low_s16(w16_hi));
                            let p3 = vmull_s16(vget_low_s16(in_val), vget_high_s16(w16_hi));

                            let a0 = vld1q_s32(out_ptr.add(o));
                            let a1 = vld1q_s32(out_ptr.add(o + 4));
                            let a2 = vld1q_s32(out_ptr.add(o + 8));
                            let a3 = vld1q_s32(out_ptr.add(o + 12));

                            vst1q_s32(out_ptr.add(o), vaddq_s32(a0, p0));
                            vst1q_s32(out_ptr.add(o + 4), vaddq_s32(a1, p1));
                            vst1q_s32(out_ptr.add(o + 8), vaddq_s32(a2, p2));
                            vst1q_s32(out_ptr.add(o + 12), vaddq_s32(a3, p3));
                        }
                    }
                }
            }

            let simd_end = out_chunks * 16;
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
                    for o in simd_end..out_dim {
                        output[o] += in_val * i32::from(weights[idx * out_dim + o]);
                    }
                }
            }
        } else {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nnue::simd::SimdOps;
    use crate::nnue::simd::scalar::Scalar;

    #[test]
    fn test_neon_vec_add_i16_matches_scalar() {
        let orig = [1i16, -2, 100, -100, 0, i16::MAX, i16::MIN, 42, 7, 13];
        let b = [10i16, 20, -50, 50, 0, 1, -1, -42, 3, -13];

        let mut a_scalar = orig;
        let mut a_neon = orig;
        Scalar::vec_add_i16(&mut a_scalar, &b);
        Neon::vec_add_i16(&mut a_neon, &b);
        assert_eq!(a_scalar, a_neon);
    }

    #[test]
    fn test_neon_vec_sub_i16_matches_scalar() {
        let orig = [10i16, 20, 30, 0, i16::MIN, i16::MAX, -1, 100, 50, 25];
        let b = [1i16, 2, 3, 1, 1, -1, 1, 100, 50, 25];

        let mut a_scalar = orig;
        let mut a_neon = orig;
        Scalar::vec_sub_i16(&mut a_scalar, &b);
        Neon::vec_sub_i16(&mut a_neon, &b);
        assert_eq!(a_scalar, a_neon);
    }

    #[test]
    fn test_neon_vec_add_i16_1024() {
        let mut a_scalar = [0i16; 1024];
        let mut a_neon = [0i16; 1024];
        let b = [0i16; 1024];
        for i in 0..1024 {
            a_scalar[i] = (i as i16).wrapping_mul(3);
            a_neon[i] = a_scalar[i];
        }
        let mut b = b;
        for (i, val) in b.iter_mut().enumerate() {
            *val = -(i as i16);
        }
        Scalar::vec_add_i16(&mut a_scalar, &b);
        Neon::vec_add_i16(&mut a_neon, &b);
        assert_eq!(a_scalar[..], a_neon[..]);
    }

    #[test]
    fn test_neon_transform_features_matches_scalar() {
        let mut psq = [0i16; 1024];
        let mut threat = [0i16; 1024];
        for i in 0..1024 {
            psq[i] = ((i * 7 + 3) % 300) as i16 - 50;
            threat[i] = ((i * 11 + 5) % 200) as i16 - 30;
        }

        let mut out_scalar = [0u8; 512];
        let mut out_neon = [0u8; 512];
        Scalar::transform_features(&psq, &threat, &mut out_scalar);
        Neon::transform_features(&psq, &threat, &mut out_neon);
        assert_eq!(out_scalar[..], out_neon[..]);
    }

    #[test]
    fn test_neon_clipped_relu_matches_scalar() {
        let mut input = [0i32; 32];
        for (i, val) in input.iter_mut().enumerate() {
            *val = (i as i32) * 100 - 500;
        }

        let mut out_scalar = [0u8; 32];
        let mut out_neon = [0u8; 32];
        Scalar::clipped_relu(&input, &mut out_scalar, 6);
        Neon::clipped_relu(&input, &mut out_neon, 6);
        assert_eq!(out_scalar, out_neon);
    }

    #[test]
    fn test_neon_horizontal_sum_matches_scalar() {
        let data: Vec<i32> = (0..33).map(|i| i * 7 - 100).collect();
        let s = Scalar::horizontal_sum_i32(&data);
        let n = Neon::horizontal_sum_i32(&data);
        assert_eq!(s, n);
    }
}
