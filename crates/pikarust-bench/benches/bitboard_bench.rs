use criterion::criterion_main;

mod bitboard_benches {
    use criterion::{Criterion, criterion_group};

    fn bench_placeholder(c: &mut Criterion) {
        c.bench_function("bitboard_placeholder", |b| b.iter(|| 1 + 1));
    }

    criterion_group!(benches, bench_placeholder);
}

criterion_main!(bitboard_benches::benches);
