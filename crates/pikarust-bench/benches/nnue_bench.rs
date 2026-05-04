use criterion::{Criterion, criterion_group, criterion_main};

use pikarust_core::nnue::simd::Dispatch;

fn bench_vec_add_i16(c: &mut Criterion) {
    let d = Dispatch::new();
    let b_data = vec![1i16; 1024];

    c.bench_function("simd_vec_add_i16_1024", |b| {
        b.iter_batched(
            || vec![0i16; 1024],
            |mut a| {
                d.vec_add_i16(&mut a, &b_data);
                criterion::black_box(a)
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_vec_sub_i16(c: &mut Criterion) {
    let d = Dispatch::new();
    let b_data = vec![1i16; 1024];

    c.bench_function("simd_vec_sub_i16_1024", |b| {
        b.iter_batched(
            || vec![100i16; 1024],
            |mut a| {
                d.vec_sub_i16(&mut a, &b_data);
                criterion::black_box(a)
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_clipped_relu(c: &mut Criterion) {
    let d = Dispatch::new();
    let input: Vec<i32> = (0..512).map(|i| (i - 256) * 64).collect();

    c.bench_function("simd_clipped_relu_512", |b| {
        b.iter_batched(
            || vec![0u8; 512],
            |mut output| {
                d.clipped_relu(criterion::black_box(&input), &mut output, 6);
                criterion::black_box(output)
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_sqr_clipped_relu(c: &mut Criterion) {
    let d = Dispatch::new();
    let input: Vec<i32> = (0..512).map(|i| (i - 256) * 64).collect();

    c.bench_function("simd_sqr_clipped_relu_512", |b| {
        b.iter_batched(
            || vec![0u8; 512],
            |mut output| {
                d.sqr_clipped_relu(criterion::black_box(&input), &mut output, 6);
                criterion::black_box(output)
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_affine_propagate(c: &mut Criterion) {
    let d = Dispatch::new();
    let in_dim = 512;
    let out_dim = 32;
    let input = vec![128u8; in_dim];
    let weights = vec![1i8; in_dim * out_dim];
    let biases = vec![0i32; out_dim];

    c.bench_function("simd_affine_512x32", |b| {
        b.iter_batched(
            || vec![0i32; out_dim],
            |mut output| {
                d.affine_propagate(
                    criterion::black_box(&input),
                    &weights,
                    &biases,
                    &mut output,
                    in_dim,
                    out_dim,
                );
                criterion::black_box(output)
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_transform_features(c: &mut Criterion) {
    let d = Dispatch::new();
    let psq_acc: Vec<i16> = (0..1024).map(|i| (i % 256) as i16).collect();
    let threat_acc: Vec<i16> = (0..1024).map(|i| ((i + 128) % 256) as i16).collect();

    c.bench_function("simd_transform_features_1024", |b| {
        b.iter_batched(
            || vec![0u8; 512],
            |mut output| {
                d.transform_features(criterion::black_box(&psq_acc), &threat_acc, &mut output);
                criterion::black_box(output)
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_horizontal_sum(c: &mut Criterion) {
    let d = Dispatch::new();
    let data: Vec<i32> = (0..256).collect();

    c.bench_function("simd_horizontal_sum_256", |b| {
        b.iter(|| criterion::black_box(d.horizontal_sum_i32(criterion::black_box(&data))));
    });
}

fn bench_find_nnz(c: &mut Criterion) {
    let d = Dispatch::new();
    let mut input = vec![0u8; 512];
    for (i, v) in input.iter_mut().enumerate() {
        if i % 3 == 0 {
            *v = (i % 255 + 1) as u8;
        }
    }

    c.bench_function("simd_find_nnz_512", |b| {
        b.iter(|| {
            let mut nnz = [0usize; pikarust_core::nnue::simd::MAX_NNZ];
            let count = d.find_nnz(criterion::black_box(&input), &mut nnz);
            criterion::black_box(count)
        });
    });
}

criterion_group!(
    benches,
    bench_vec_add_i16,
    bench_vec_sub_i16,
    bench_clipped_relu,
    bench_sqr_clipped_relu,
    bench_affine_propagate,
    bench_transform_features,
    bench_horizontal_sum,
    bench_find_nnz,
);
criterion_main!(benches);
