use criterion::criterion_main;

mod nnue_benches {
    use criterion::{Criterion, criterion_group};

    fn bench_placeholder(c: &mut Criterion) {
        c.bench_function("nnue_placeholder", |b| b.iter(|| 1 + 1));
    }

    criterion_group!(benches, bench_placeholder);
}

criterion_main!(nnue_benches::benches);
