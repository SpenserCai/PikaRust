use std::arch::x86_64::*;

use super::SimdOps;

pub struct Avx2;

impl SimdOps for Avx2 {
    fn vec_add_i16(a: &mut [i16], b: &[i16]) {
        debug_assert_eq!(a.len(), b.len());
        let len = a.len();
        let chunks = len / 16;
        let remainder = chunks * 16;

        // SAFETY: Caller guarantees AVX2 is available (checked at dispatch).
        // Pointers are valid for the slice lengths; we process complete 16-element chunks.
        unsafe {
            let a_ptr = a.as_mut_ptr();
            let b_ptr = b.as_ptr();
            for i in 0..chunks {
                let offset = i * 16;
                let va = _mm256_loadu_si256(a_ptr.add(offset).cast());
                let vb = _mm256_loadu_si256(b_ptr.add(offset).cast());
                _mm256_storeu_si256(a_ptr.add(offset).cast(), _mm256_add_epi16(va, vb));
            }
        }

        for i in remainder..len {
            a[i] = a[i].wrapping_add(b[i]);
        }
    }

    fn vec_sub_i16(a: &mut [i16], b: &[i16]) {
        debug_assert_eq!(a.len(), b.len());
        let len = a.len();
        let chunks = len / 16;
        let remainder = chunks * 16;

        // SAFETY: Caller guarantees AVX2 is available. Same bounds reasoning.
        unsafe {
            let a_ptr = a.as_mut_ptr();
            let b_ptr = b.as_ptr();
            for i in 0..chunks {
                let offset = i * 16;
                let va = _mm256_loadu_si256(a_ptr.add(offset).cast());
                let vb = _mm256_loadu_si256(b_ptr.add(offset).cast());
                _mm256_storeu_si256(a_ptr.add(offset).cast(), _mm256_sub_epi16(va, vb));
            }
        }

        for i in remainder..len {
            a[i] = a[i].wrapping_sub(b[i]);
        }
    }

    fn vec_add_i32(a: &mut [i32], b: &[i32]) {
        debug_assert_eq!(a.len(), b.len());
        let len = a.len();
        let chunks = len / 8;
        let remainder = chunks * 8;

        // SAFETY: Caller guarantees AVX2 is available. Pointers are valid.
        unsafe {
            let a_ptr = a.as_mut_ptr();
            let b_ptr = b.as_ptr();
            for i in 0..chunks {
                let offset = i * 8;
                let va = _mm256_loadu_si256(a_ptr.add(offset).cast());
                let vb = _mm256_loadu_si256(b_ptr.add(offset).cast());
                _mm256_storeu_si256(a_ptr.add(offset).cast(), _mm256_add_epi32(va, vb));
            }
        }

        for i in remainder..len {
            a[i] = a[i].wrapping_add(b[i]);
        }
    }

    fn vec_sub_i32(a: &mut [i32], b: &[i32]) {
        debug_assert_eq!(a.len(), b.len());
        let len = a.len();
        let chunks = len / 8;
        let remainder = chunks * 8;

        // SAFETY: Caller guarantees AVX2 is available. Pointers are valid.
        unsafe {
            let a_ptr = a.as_mut_ptr();
            let b_ptr = b.as_ptr();
            for i in 0..chunks {
                let offset = i * 8;
                let va = _mm256_loadu_si256(a_ptr.add(offset).cast());
                let vb = _mm256_loadu_si256(b_ptr.add(offset).cast());
                _mm256_storeu_si256(a_ptr.add(offset).cast(), _mm256_sub_epi32(va, vb));
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

        // SAFETY: Caller guarantees AVX2 is available. All pointer accesses are
        // within the validated slice bounds (512 i16 pairs -> 512 u8 outputs).
        unsafe {
            let psq_ptr = psq_acc.as_ptr();
            let threat_ptr = threat_acc.as_ptr();
            let out_ptr = output.as_mut_ptr();

            let zero = _mm256_setzero_si256();
            let max_val = _mm256_set1_epi16(255);

            let mut j = 0;
            while j + 16 <= 512 {
                let p0 = _mm256_loadu_si256(psq_ptr.add(j).cast());
                let t0 = _mm256_loadu_si256(threat_ptr.add(j).cast());
                let sum0 = _mm256_add_epi16(p0, t0);
                let clamped0 = _mm256_min_epi16(_mm256_max_epi16(sum0, zero), max_val);

                let p1 = _mm256_loadu_si256(psq_ptr.add(j + 512).cast());
                let t1 = _mm256_loadu_si256(threat_ptr.add(j + 512).cast());
                let sum1 = _mm256_add_epi16(p1, t1);
                let clamped1 = _mm256_min_epi16(_mm256_max_epi16(sum1, zero), max_val);

                // Multiply and divide by 512: shift left 7, mulhi shifts right 16,
                // net effect is >> 9 = / 512
                let shifted0 = _mm256_slli_epi16(clamped0, 7);
                let product = _mm256_mulhi_epi16(shifted0, clamped1);

                // Pack i16 -> u8 with saturation
                let packed = _mm256_packus_epi16(product, product);
                // AVX2 packus interleaves lanes: need permute to get contiguous bytes
                let permuted = _mm256_permute4x64_epi64(packed, 0b11_01_10_00);

                // Store lower 16 bytes
                _mm_storeu_si128(out_ptr.add(j).cast(), _mm256_castsi256_si128(permuted));

                j += 16;
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
        let len = input.len();
        let chunks = len / 32;
        let remainder = chunks * 32;

        // SAFETY: Caller guarantees AVX2 is available. We process 32 i32 elements
        // at a time, narrowing to 32 u8 values. All accesses are within bounds.
        unsafe {
            let in_ptr = input.as_ptr();
            let out_ptr = output.as_mut_ptr();
            let max127 = _mm256_set1_epi32(127);

            for i in 0..chunks {
                let base = i * 32;
                let v0 = _mm256_srai_epi32(_mm256_loadu_si256(in_ptr.add(base).cast()), 6);
                let v1 = _mm256_srai_epi32(_mm256_loadu_si256(in_ptr.add(base + 8).cast()), 6);
                let v2 = _mm256_srai_epi32(_mm256_loadu_si256(in_ptr.add(base + 16).cast()), 6);
                let v3 = _mm256_srai_epi32(_mm256_loadu_si256(in_ptr.add(base + 24).cast()), 6);

                let v0 = _mm256_min_epi32(v0, max127);
                let v1 = _mm256_min_epi32(v1, max127);
                let v2 = _mm256_min_epi32(v2, max127);
                let v3 = _mm256_min_epi32(v3, max127);

                // packus_epi32 saturates negative to 0 and clamps to u16 range
                let packed01 = _mm256_packus_epi32(v0, v1);
                let packed23 = _mm256_packus_epi32(v2, v3);

                // packus_epi16 saturates to [0, 255]
                let packed = _mm256_packus_epi16(packed01, packed23);

                // Fix AVX2 lane interleaving
                let permuted =
                    _mm256_permutevar8x32_epi32(packed, _mm256_setr_epi32(0, 4, 1, 5, 2, 6, 3, 7));

                _mm256_storeu_si256(out_ptr.add(base).cast(), permuted);
            }
        }

        let _ = shift;
        for i in remainder..len {
            output[i] = (input[i] >> 6).clamp(0, 127) as u8;
        }
    }

    fn sqr_clipped_relu(input: &[i32], output: &mut [u8], shift: u32) {
        let max_val = 127i32 << shift;
        for (i, &x) in input.iter().enumerate() {
            let clamped = i64::from(x.clamp(0, max_val));
            let squared = (clamped * clamped) >> (2 * shift + 7);
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
        let len = data.len();
        let chunks = len / 8;
        let remainder = chunks * 8;
        let mut sum: i32;

        // SAFETY: Caller guarantees AVX2 is available. We load 8 i32 at a time.
        unsafe {
            let ptr = data.as_ptr();
            let mut acc = _mm256_setzero_si256();
            for i in 0..chunks {
                let v = _mm256_loadu_si256(ptr.add(i * 8).cast());
                acc = _mm256_add_epi32(acc, v);
            }
            // Horizontal reduction: 8 -> 4 -> 2 -> 1
            let hi128 = _mm256_extracti128_si256(acc, 1);
            let lo128 = _mm256_castsi256_si128(acc);
            let sum128 = _mm_add_epi32(lo128, hi128);
            let hi64 = _mm_unpackhi_epi64(sum128, sum128);
            let sum64 = _mm_add_epi32(sum128, hi64);
            let hi32 = _mm_shuffle_epi32(sum64, 1);
            let sum32 = _mm_add_epi32(sum64, hi32);
            sum = _mm_cvtsi128_si32(sum32);
        }

        for &x in &data[remainder..] {
            sum = sum.wrapping_add(x);
        }
        sum
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
