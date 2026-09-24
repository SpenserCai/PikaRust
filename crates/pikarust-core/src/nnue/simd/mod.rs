#[cfg(target_arch = "x86_64")]
mod avx2;
#[cfg(target_arch = "aarch64")]
mod neon;
pub mod scalar;

pub const MAX_NNZ: usize = super::model::TRANSFORMED_DIMS / 4;

/// NNUE arithmetic with identical slice contracts across backends.
///
/// Vector operations require equal lengths and use wrapping integer arithmetic.
/// Feature transforms require two 1024-element accumulators and at least 512
/// output bytes. Activation output must cover the input; ordinary `ReLU` accepts
/// shifts below 32, and squared `ReLU` accepts shifts up to 28.
///
/// Affine operations require complete input-major weight matrices and enough
/// biases/output for `out_dim`. Sparse block indices must refer to input blocks
/// of four bytes. `find_nnz` requires complete blocks and at most `MAX_NNZ`
/// blocks. Violating these shape contracts panics before writing output.
pub trait SimdOps {
    fn vec_add_i16(a: &mut [i16], b: &[i16]);
    fn vec_sub_i16(a: &mut [i16], b: &[i16]);
    fn vec_add_i32(a: &mut [i32], b: &[i32]);
    fn vec_sub_i32(a: &mut [i32], b: &[i32]);

    /// Widening add: `acc[i] += i16::from(weights[i])`
    fn vec_add_i16_widening(acc: &mut [i16], weights: &[i8]);
    /// Widening sub: `acc[i] -= i16::from(weights[i])`
    fn vec_sub_i16_widening(acc: &mut [i16], weights: &[i8]);

    fn transform_features(psq_acc: &[i16], threat_acc: &[i16], output: &mut [u8]);

    fn clipped_relu(input: &[i32], output: &mut [u8], shift: u32);
    fn sqr_clipped_relu(input: &[i32], output: &mut [u8], shift: u32);

    fn affine_propagate(
        input: &[u8],
        weights: &[i8],
        biases: &[i32],
        output: &mut [i32],
        in_dim: usize,
        out_dim: usize,
    );

    fn horizontal_sum_i32(data: &[i32]) -> i32;

    fn find_nnz(input: &[u8], nnz_indices: &mut [usize; MAX_NNZ]) -> usize;

    fn affine_propagate_sparse(
        input: &[u8],
        weights: &[i8],
        biases: &[i32],
        output: &mut [i32],
        out_dim: usize,
        nnz_indices: &[usize],
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdBackend {
    Scalar,
    #[cfg(target_arch = "aarch64")]
    Neon,
    #[cfg(target_arch = "x86_64")]
    Avx2,
}

impl SimdBackend {
    /// Whether this CPU can safely execute the backend's instructions.
    pub fn is_supported(self) -> bool {
        match self {
            Self::Scalar => true,
            #[cfg(target_arch = "aarch64")]
            Self::Neon => std::arch::is_aarch64_feature_detected!("neon"),
            #[cfg(target_arch = "x86_64")]
            Self::Avx2 => is_x86_feature_detected!("avx2"),
        }
    }
}

impl std::fmt::Display for SimdBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scalar => write!(f, "Scalar"),
            #[cfg(target_arch = "aarch64")]
            Self::Neon => write!(f, "NEON"),
            #[cfg(target_arch = "x86_64")]
            Self::Avx2 => write!(f, "AVX2"),
        }
    }
}

#[allow(clippy::missing_const_for_fn)]
pub fn detect_backend() -> SimdBackend {
    // Priority: simd-none > simd-neon > simd-avx2 > simd-auto
    if cfg!(feature = "simd-none") {
        return SimdBackend::Scalar;
    }

    if cfg!(feature = "simd-neon") {
        #[cfg(target_arch = "aarch64")]
        if SimdBackend::Neon.is_supported() {
            return SimdBackend::Neon;
        }
        return SimdBackend::Scalar;
    }

    if cfg!(feature = "simd-avx2") {
        #[cfg(target_arch = "x86_64")]
        if SimdBackend::Avx2.is_supported() {
            return SimdBackend::Avx2;
        }
        return SimdBackend::Scalar;
    }

    // simd-auto: runtime detection
    #[cfg(target_arch = "aarch64")]
    {
        if SimdBackend::Neon.is_supported() {
            return SimdBackend::Neon;
        }
        SimdBackend::Scalar
    }

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            return SimdBackend::Avx2;
        }
        SimdBackend::Scalar
    }

    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    SimdBackend::Scalar
}

macro_rules! dispatch {
    ($backend:expr, $method:ident $(, $arg:expr)*) => {
        match $backend {
            SimdBackend::Scalar => scalar::Scalar::$method($($arg),*),
            #[cfg(target_arch = "aarch64")]
            SimdBackend::Neon => neon::Neon::$method($($arg),*),
            #[cfg(target_arch = "x86_64")]
            // SAFETY: Dispatch's private backend is set only by CPU detection
            // or with_backend's support check. AVX2 kernels validate their own
            // slice contracts; the caller need not uphold any memory invariant.
            SimdBackend::Avx2 => unsafe { avx2::Avx2::$method($($arg),*) },
        }
    };
}

/// CPU-checked NNUE operations with the input contracts documented by [`SimdOps`].
///
/// Architecture-specific code is isolated behind runtime detection; callers do
/// not need global CPU flags or an unsafe API to use a portable release build.
pub struct Dispatch {
    backend: SimdBackend,
}

impl Dispatch {
    pub fn new() -> Self {
        Self {
            backend: detect_backend(),
        }
    }

    /// Selects an explicit backend.
    ///
    /// # Panics
    /// Panics if the current CPU does not support `backend`. Use [`Self::new`]
    /// to select an available backend automatically.
    pub fn with_backend(backend: SimdBackend) -> Self {
        assert!(
            backend.is_supported(),
            "unsupported SIMD backend: {backend}"
        );
        Self { backend }
    }

    pub const fn backend(&self) -> SimdBackend {
        self.backend
    }

    #[inline]
    pub fn vec_add_i16(&self, a: &mut [i16], b: &[i16]) {
        dispatch!(self.backend, vec_add_i16, a, b);
    }

    #[inline]
    pub fn vec_sub_i16(&self, a: &mut [i16], b: &[i16]) {
        dispatch!(self.backend, vec_sub_i16, a, b);
    }

    #[inline]
    pub fn vec_add_i16_widening(&self, acc: &mut [i16], weights: &[i8]) {
        dispatch!(self.backend, vec_add_i16_widening, acc, weights);
    }

    #[inline]
    pub fn vec_sub_i16_widening(&self, acc: &mut [i16], weights: &[i8]) {
        dispatch!(self.backend, vec_sub_i16_widening, acc, weights);
    }

    #[inline]
    pub fn vec_add_i32(&self, a: &mut [i32], b: &[i32]) {
        dispatch!(self.backend, vec_add_i32, a, b);
    }

    #[inline]
    pub fn vec_sub_i32(&self, a: &mut [i32], b: &[i32]) {
        dispatch!(self.backend, vec_sub_i32, a, b);
    }

    #[inline]
    pub fn transform_features(&self, psq_acc: &[i16], threat_acc: &[i16], output: &mut [u8]) {
        dispatch!(
            self.backend,
            transform_features,
            psq_acc,
            threat_acc,
            output
        );
    }

    #[inline]
    pub fn clipped_relu(&self, input: &[i32], output: &mut [u8], shift: u32) {
        dispatch!(self.backend, clipped_relu, input, output, shift);
    }

    #[inline]
    pub fn sqr_clipped_relu(&self, input: &[i32], output: &mut [u8], shift: u32) {
        dispatch!(self.backend, sqr_clipped_relu, input, output, shift);
    }

    #[inline]
    pub fn affine_propagate(
        &self,
        input: &[u8],
        weights: &[i8],
        biases: &[i32],
        output: &mut [i32],
        in_dim: usize,
        out_dim: usize,
    ) {
        dispatch!(
            self.backend,
            affine_propagate,
            input,
            weights,
            biases,
            output,
            in_dim,
            out_dim
        );
    }

    #[inline]
    pub fn horizontal_sum_i32(&self, data: &[i32]) -> i32 {
        dispatch!(self.backend, horizontal_sum_i32, data)
    }

    #[inline]
    pub fn find_nnz(&self, input: &[u8], nnz_indices: &mut [usize; MAX_NNZ]) -> usize {
        dispatch!(self.backend, find_nnz, input, nnz_indices)
    }

    #[inline]
    pub fn affine_propagate_sparse(
        &self,
        input: &[u8],
        weights: &[i8],
        biases: &[i32],
        output: &mut [i32],
        out_dim: usize,
        nnz_indices: &[usize],
    ) {
        dispatch!(
            self.backend,
            affine_propagate_sparse,
            input,
            weights,
            biases,
            output,
            out_dim,
            nnz_indices
        );
    }
}

impl Default for Dispatch {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_affine_dimensions(
    input: &[u8],
    weights: &[i8],
    biases: &[i32],
    output: &[i32],
    in_dim: usize,
    out_dim: usize,
) {
    let weight_count = in_dim
        .checked_mul(out_dim)
        .expect("affine matrix dimensions overflow");
    assert!(input.len() >= in_dim, "affine input is too short");
    assert!(
        weights.len() >= weight_count,
        "affine weights are too short"
    );
    assert!(biases.len() >= out_dim, "affine biases are too short");
    assert!(output.len() >= out_dim, "affine output is too short");
}

fn validate_sparse_indices(input_len: usize, nnz_indices: &[usize]) {
    let blocks = input_len.div_ceil(4);
    assert!(
        nnz_indices.iter().all(|&index| index < blocks),
        "sparse block index is outside the input"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supported_dispatches() -> Vec<Dispatch> {
        [
            SimdBackend::Scalar,
            #[cfg(target_arch = "x86_64")]
            SimdBackend::Avx2,
            #[cfg(target_arch = "aarch64")]
            SimdBackend::Neon,
        ]
        .into_iter()
        .filter(|backend| backend.is_supported())
        .map(Dispatch::with_backend)
        .collect()
    }

    fn assert_rejects_unchanged<T: Copy + Eq + std::fmt::Debug>(
        output: &mut [T],
        operation: impl FnOnce(&mut [T]),
    ) {
        let original = output.to_vec();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(output))).is_err()
        );
        assert_eq!(output, original);
    }

    #[test]
    fn test_detect_backend() {
        let backend = detect_backend();
        #[cfg(feature = "simd-none")]
        assert_eq!(backend, SimdBackend::Scalar);

        assert!(backend.is_supported());
    }

    #[test]
    fn test_dispatch_vec_add_i16() {
        let d = Dispatch::new();
        let mut a = [1i16, 2, 3, 4, 5, 6, 7, 8];
        let b = [10i16, 20, 30, 40, 50, 60, 70, 80];
        d.vec_add_i16(&mut a, &b);
        assert_eq!(a, [11, 22, 33, 44, 55, 66, 77, 88]);
    }

    #[test]
    fn test_dispatch_vec_sub_i16() {
        let d = Dispatch::new();
        let mut a = [10i16, 20, 30, 40, 50, 60, 70, 80];
        let b = [1i16, 2, 3, 4, 5, 6, 7, 8];
        d.vec_sub_i16(&mut a, &b);
        assert_eq!(a, [9, 18, 27, 36, 45, 54, 63, 72]);
    }

    #[test]
    fn test_dispatch_transform_features() {
        let d = Dispatch::new();
        let mut psq = [0i16; 1024];
        let mut threat = [0i16; 1024];
        psq[0] = 100;
        threat[0] = 50;
        psq[512] = 200;
        threat[512] = 55;

        let mut output = [0u8; 512];
        d.transform_features(&psq, &threat, &mut output);
        assert_eq!(output[0], 74);
    }

    #[test]
    fn test_dispatch_clipped_relu() {
        let d = Dispatch::new();
        let input = [0i32, 64, 128, -10, 127 * 64 + 100, 8128];
        let mut output = [0u8; 6];
        d.clipped_relu(&input, &mut output, 6);
        assert_eq!(output, [0, 1, 2, 0, 127, 127]);
    }

    #[test]
    fn test_dispatch_sqr_clipped_relu() {
        let d = Dispatch::new();
        let input = [0i32, -10, 127 * 64];
        let mut output = [0u8; 3];
        d.sqr_clipped_relu(&input, &mut output, 6);
        assert_eq!(output[0], 0);
        assert_eq!(output[1], 0);
        assert_eq!(output[2], 126);
    }

    #[test]
    fn test_dispatch_affine_propagate() {
        let d = Dispatch::new();
        let input = [0u8, 2, 0, 3];
        let weights: Vec<i8> = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let biases = [10i32, 20];
        let mut output = [0i32; 2];
        d.affine_propagate(&input, &weights, &biases, &mut output, 4, 2);
        assert_eq!(output[0], 37);
        assert_eq!(output[1], 52);
    }

    #[test]
    fn test_dispatch_horizontal_sum() {
        let d = Dispatch::new();
        let data = [1i32, 2, 3, 4, 5];
        assert_eq!(d.horizontal_sum_i32(&data), 15);
    }

    #[test]
    fn test_dispatch_find_nnz() {
        let d = Dispatch::new();
        let mut input = [0u8; 16];
        input[1] = 5;
        input[8] = 1;
        input[11] = 2;
        let mut nnz = [0usize; MAX_NNZ];
        let count = d.find_nnz(&input, &mut nnz);
        assert_eq!(&nnz[..count], &[0, 2]);
    }

    #[test]
    fn test_dispatch_backend_display() {
        let d = Dispatch::new();
        let s = format!("{}", d.backend());
        assert!(!s.is_empty());
    }

    #[test]
    fn explicit_backend_requires_cpu_support() {
        let backends = [
            SimdBackend::Scalar,
            #[cfg(target_arch = "x86_64")]
            SimdBackend::Avx2,
            #[cfg(target_arch = "aarch64")]
            SimdBackend::Neon,
        ];
        for backend in backends {
            let result = std::panic::catch_unwind(|| Dispatch::with_backend(backend));
            assert_eq!(result.is_ok(), backend.is_supported());
        }
    }

    #[test]
    fn invalid_vector_shapes_panic_before_writing() {
        for d in supported_dispatches() {
            let mut a16 = [42_i16; 33];
            let mut a32 = [42_i32; 33];
            assert_rejects_unchanged(&mut a16, |a| d.vec_add_i16(a, &[]));
            assert_rejects_unchanged(&mut a16, |a| d.vec_sub_i16(a, &[]));
            assert_rejects_unchanged(&mut a32, |a| d.vec_add_i32(a, &[]));
            assert_rejects_unchanged(&mut a32, |a| d.vec_sub_i32(a, &[]));
            assert_rejects_unchanged(&mut a16, |a| d.vec_add_i16_widening(a, &[]));
            assert_rejects_unchanged(&mut a16, |a| d.vec_sub_i16_widening(a, &[]));
        }
    }

    #[test]
    fn invalid_transform_and_activation_shapes_panic_before_writing() {
        for d in supported_dispatches() {
            let mut output = [42_u8; 512];
            assert_rejects_unchanged(&mut output, |out| {
                d.transform_features(&[0; 1023], &[0; 1024], out);
            });
            assert_rejects_unchanged(&mut output, |out| {
                d.transform_features(&[0; 1024], &[0; 1023], out);
            });
            assert_rejects_unchanged(&mut output[..511], |out| {
                d.transform_features(&[0; 1024], &[0; 1024], out);
            });
            assert_rejects_unchanged(&mut output[..31], |out| d.clipped_relu(&[64; 32], out, 6));
            assert_rejects_unchanged(&mut output[..31], |out| {
                d.sqr_clipped_relu(&[64; 32], out, 6);
            });
            assert_rejects_unchanged(&mut output, |out| d.clipped_relu(&[64; 32], out, 32));
            assert_rejects_unchanged(&mut output, |out| d.sqr_clipped_relu(&[64; 32], out, 29));
        }
    }

    #[test]
    fn invalid_affine_shapes_panic_before_writing() {
        for d in supported_dispatches() {
            let mut output = [42_i32; 16];
            assert_rejects_unchanged(&mut output, |out| {
                d.affine_propagate(&[1; 4], &[1; 63], &[0; 16], out, 4, 16);
            });
            assert_rejects_unchanged(&mut output, |out| {
                d.affine_propagate(&[1; 3], &[1; 64], &[0; 16], out, 4, 16);
            });
            assert_rejects_unchanged(&mut output, |out| {
                d.affine_propagate(&[1; 4], &[1; 64], &[0; 15], out, 4, 16);
            });
            assert_rejects_unchanged(&mut output[..15], |out| {
                d.affine_propagate(&[1; 4], &[1; 64], &[0; 16], out, 4, 16);
            });
            assert_rejects_unchanged(&mut output, |out| {
                d.affine_propagate(&[], &[], &[], out, usize::MAX, 16);
            });
            assert_rejects_unchanged(&mut output, |out| {
                d.affine_propagate_sparse(&[1; 4], &[1; 63], &[0; 16], out, 16, &[0]);
            });
            assert_rejects_unchanged(&mut output, |out| {
                d.affine_propagate_sparse(&[1; 4], &[1; 64], &[0; 16], out, 16, &[1]);
            });
            assert_rejects_unchanged(&mut output, |out| {
                d.affine_propagate_sparse(&[1; 4], &[1; 64], &[0; 16], out, 16, &[usize::MAX]);
            });
        }
    }

    #[test]
    fn invalid_nnz_input_is_rejected() {
        for d in supported_dispatches() {
            let mut indices = [0; MAX_NNZ];
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    d.find_nnz(&[1; MAX_NNZ * 4 + 4], &mut indices)
                }))
                .is_err()
            );
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    d.find_nnz(&[1; 3], &mut indices)
                }))
                .is_err()
            );
        }
    }

    #[test]
    fn backends_match_scalar_for_unaligned_vectors_and_wrapping_tails() {
        for d in supported_dispatches() {
            for len in [0, 1, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65] {
                let mut actual = vec![i16::MAX; len + 2];
                let mut expected = actual.clone();
                let weights = vec![1_i8; len + 2];
                d.vec_add_i16_widening(&mut actual[1..=len], &weights[1..=len]);
                scalar::Scalar::vec_add_i16_widening(&mut expected[1..=len], &weights[1..=len]);
                assert_eq!(actual, expected, "{} length {len}", d.backend());
                d.vec_sub_i16_widening(&mut actual[1..=len], &weights[1..=len]);
                scalar::Scalar::vec_sub_i16_widening(&mut expected[1..=len], &weights[1..=len]);
                assert_eq!(actual, expected, "{} length {len}", d.backend());
                let data = vec![i32::MAX; len + 2];
                assert_eq!(
                    d.horizontal_sum_i32(&data[1..=len]),
                    scalar::Scalar::horizontal_sum_i32(&data[1..=len])
                );
            }
        }
    }

    #[test]
    fn backends_match_scalar_for_activation_shifts() {
        let input: Vec<_> = (0..65).map(|i| i * 811 - 16384).collect();
        for d in supported_dispatches() {
            for shift in [0, 1, 6, 12, 28] {
                let mut actual = [42; 67];
                let mut expected = actual;
                d.clipped_relu(&input, &mut actual[1..66], shift);
                scalar::Scalar::clipped_relu(&input, &mut expected[1..66], shift);
                assert_eq!(actual, expected, "{} shift {shift}", d.backend());
                d.sqr_clipped_relu(&input, &mut actual[1..66], shift);
                scalar::Scalar::sqr_clipped_relu(&input, &mut expected[1..66], shift);
                assert_eq!(actual, expected, "{} shift {shift}", d.backend());
            }
        }
    }

    #[test]
    fn backends_match_scalar_for_extreme_feature_sums() {
        let values = [i16::MIN, i16::MAX, -100, 0, 100, 255];
        let psq: Vec<_> = (0..1024).map(|i| values[i % values.len()]).collect();
        let threat: Vec<_> = (0..1024)
            .map(|i| values[(i / values.len()) % values.len()])
            .collect();
        let mut expected = [0; 512];
        scalar::Scalar::transform_features(&psq, &threat, &mut expected);
        for d in supported_dispatches() {
            let mut actual = [0; 512];
            d.transform_features(&psq, &threat, &mut actual);
            assert_eq!(actual, expected, "{}", d.backend());
        }
    }

    #[test]
    fn backends_match_scalar_for_all_vector_operations() {
        let values16 = [i16::MIN, i16::MAX, -128, -1, 0, 1, 127];
        let values32 = [i32::MIN, i32::MAX, -1, 0, 1];
        for d in supported_dispatches() {
            for len in [0, 1, 7, 8, 15, 16, 17, 31, 32, 33, 65, 1024] {
                let mut actual16: Vec<_> =
                    (0..len + 2).map(|i| values16[i % values16.len()]).collect();
                let mut expected16 = actual16.clone();
                let rhs16: Vec<_> = (0..len + 2)
                    .map(|i| values16[(i + 3) % values16.len()])
                    .collect();
                let weights: Vec<_> = (0..len + 2).map(|i| (i * 97) as i8).collect();
                let range = 1..=len;
                d.vec_add_i16(&mut actual16[range.clone()], &rhs16[range.clone()]);
                scalar::Scalar::vec_add_i16(&mut expected16[range.clone()], &rhs16[range.clone()]);
                assert_eq!(actual16, expected16, "{} add length {len}", d.backend());
                d.vec_sub_i16(&mut actual16[range.clone()], &rhs16[range.clone()]);
                scalar::Scalar::vec_sub_i16(&mut expected16[range.clone()], &rhs16[range.clone()]);
                assert_eq!(actual16, expected16, "{} sub length {len}", d.backend());
                d.vec_add_i16_widening(&mut actual16[range.clone()], &weights[range.clone()]);
                scalar::Scalar::vec_add_i16_widening(
                    &mut expected16[range.clone()],
                    &weights[range.clone()],
                );
                assert_eq!(
                    actual16,
                    expected16,
                    "{} widen add length {len}",
                    d.backend()
                );
                d.vec_sub_i16_widening(&mut actual16[range.clone()], &weights[range.clone()]);
                scalar::Scalar::vec_sub_i16_widening(
                    &mut expected16[range.clone()],
                    &weights[range.clone()],
                );
                assert_eq!(
                    actual16,
                    expected16,
                    "{} widen sub length {len}",
                    d.backend()
                );

                let mut actual32: Vec<_> =
                    (0..len + 2).map(|i| values32[i % values32.len()]).collect();
                let mut expected32 = actual32.clone();
                let rhs32: Vec<_> = (0..len + 2)
                    .map(|i| values32[(i + 2) % values32.len()])
                    .collect();
                d.vec_add_i32(&mut actual32[range.clone()], &rhs32[range.clone()]);
                scalar::Scalar::vec_add_i32(&mut expected32[range.clone()], &rhs32[range.clone()]);
                assert_eq!(actual32, expected32, "{} add length {len}", d.backend());
                assert_eq!(
                    d.horizontal_sum_i32(&actual32[range.clone()]),
                    scalar::Scalar::horizontal_sum_i32(&expected32[range.clone()]),
                );
                d.vec_sub_i32(&mut actual32[range.clone()], &rhs32[range.clone()]);
                scalar::Scalar::vec_sub_i32(&mut expected32[range.clone()], &rhs32[range]);
                assert_eq!(actual32, expected32, "{} sub length {len}", d.backend());
            }
        }
    }

    #[test]
    fn backends_match_scalar_for_unaligned_transform_and_extreme_activations() {
        let values = [i16::MIN, i16::MAX, -100, 0, 100, 255];
        let psq: Vec<_> = (0..1026).map(|i| values[i % values.len()]).collect();
        let threat: Vec<_> = (0..1026)
            .map(|i| values[(i / values.len()) % values.len()])
            .collect();
        let extremes = [i32::MIN, i32::MAX, -8128, -1, 0, 1, 8128];
        let input: Vec<_> = (0..67).map(|i| extremes[i % extremes.len()]).collect();
        for d in supported_dispatches() {
            let mut actual = [42; 514];
            let mut expected = actual;
            d.transform_features(&psq[1..1025], &threat[1..1025], &mut actual[1..513]);
            scalar::Scalar::transform_features(
                &psq[1..1025],
                &threat[1..1025],
                &mut expected[1..513],
            );
            assert_eq!(actual, expected, "{} transform", d.backend());
            for shift in [0, 1, 6, 12, 28, 31] {
                d.clipped_relu(&input[1..66], &mut actual[1..66], shift);
                scalar::Scalar::clipped_relu(&input[1..66], &mut expected[1..66], shift);
                assert_eq!(actual, expected, "{} activation {shift}", d.backend());
                if shift <= 28 {
                    d.sqr_clipped_relu(&input[1..66], &mut actual[1..66], shift);
                    scalar::Scalar::sqr_clipped_relu(&input[1..66], &mut expected[1..66], shift);
                    assert_eq!(actual, expected, "{} squared {shift}", d.backend());
                }
            }
        }
    }

    #[test]
    fn backends_match_scalar_for_unaligned_dense_and_sparse_affine() {
        for d in supported_dispatches() {
            for (in_dim, out_dim) in [
                (0, 0),
                (0, 32),
                (3, 17),
                (31, 32),
                (32, 1),
                (64, 32),
                (1024, 32),
            ] {
                let input: Vec<_> = (0..in_dim + 2)
                    .map(|i| if i % 5 == 0 { 0 } else { (i * 97) as u8 })
                    .collect();
                let weights: Vec<_> = (0..in_dim * out_dim + 2).map(|i| (i * 71) as i8).collect();
                let biases: Vec<_> = (0..out_dim + 2)
                    .map(|i| if i % 2 == 0 { i32::MAX } else { i32::MIN })
                    .collect();
                let input = &input[1..=in_dim];
                let weights = &weights[1..=in_dim * out_dim];
                let biases = &biases[1..=out_dim];
                let mut actual = vec![42; out_dim + 2];
                let mut expected = actual.clone();
                d.affine_propagate(
                    input,
                    weights,
                    biases,
                    &mut actual[1..=out_dim],
                    in_dim,
                    out_dim,
                );
                scalar::Scalar::affine_propagate(
                    input,
                    weights,
                    biases,
                    &mut expected[1..=out_dim],
                    in_dim,
                    out_dim,
                );
                assert_eq!(actual, expected, "{} dense {in_dim}x{out_dim}", d.backend());
                let indices: Vec<_> = (0..in_dim.div_ceil(4)).collect();
                actual.fill(42);
                d.affine_propagate_sparse(
                    input,
                    weights,
                    biases,
                    &mut actual[1..=out_dim],
                    out_dim,
                    &indices,
                );
                assert_eq!(
                    actual,
                    expected,
                    "{} sparse {in_dim}x{out_dim}",
                    d.backend()
                );
            }
        }
    }

    #[test]
    fn backends_match_scalar_for_unaligned_nnz_blocks() {
        for d in supported_dispatches() {
            for len in [0, 4, 8, 16, 32, 64, 1024] {
                let input: Vec<_> = (0..len + 2)
                    .map(|i| if i % 11 == 0 { 255_u8 } else { 0 })
                    .collect();
                let mut actual = [usize::MAX; MAX_NNZ];
                let mut expected = actual;
                let count = d.find_nnz(&input[1..=len], &mut actual);
                let expected_count = scalar::Scalar::find_nnz(&input[1..=len], &mut expected);
                assert_eq!(count, expected_count);
                assert_eq!(actual, expected, "{} length {len}", d.backend());
            }
        }
    }
}
