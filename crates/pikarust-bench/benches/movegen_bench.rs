use criterion::criterion_main;

mod movegen_benches {
    use criterion::{Criterion, criterion_group};

    fn bench_placeholder(c: &mut Criterion) {
        c.bench_function("movegen_placeholder", |b| b.iter(|| 1 + 1));
    }

    criterion_group!(benches, bench_placeholder);
}

criterion_main!(movegen_benches::benches);
