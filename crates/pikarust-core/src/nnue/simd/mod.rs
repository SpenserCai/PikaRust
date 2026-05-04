#[cfg(target_arch = "x86_64")]
mod avx2;
#[cfg(target_arch = "aarch64")]
mod neon;
pub mod scalar;

pub const MAX_NNZ: usize = super::model::TRANSFORMED_DIMS / 4;

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
        return SimdBackend::Neon;
        #[cfg(not(target_arch = "aarch64"))]
        return SimdBackend::Scalar;
    }

    if cfg!(feature = "simd-avx2") {
        #[cfg(target_arch = "x86_64")]
        return SimdBackend::Avx2;
        #[cfg(not(target_arch = "x86_64"))]
        return SimdBackend::Scalar;
    }

    // simd-auto: runtime detection
    #[cfg(target_arch = "aarch64")]
    {
        SimdBackend::Neon
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
            SimdBackend::Avx2 => avx2::Avx2::$method($($arg),*),
        }
    };
}

pub struct Dispatch {
    backend: SimdBackend,
}

impl Dispatch {
    pub fn new() -> Self {
        Self {
            backend: detect_backend(),
        }
    }

    pub const fn with_backend(backend: SimdBackend) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_backend() {
        let backend = detect_backend();
        #[cfg(feature = "simd-none")]
        assert_eq!(backend, SimdBackend::Scalar);

        #[cfg(all(feature = "simd-auto", target_arch = "aarch64"))]
        assert_eq!(backend, SimdBackend::Neon);

        let _ = backend;
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
}
