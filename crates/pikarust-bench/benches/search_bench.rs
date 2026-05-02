use criterion::criterion_main;

mod search_benches {
    use criterion::{Criterion, criterion_group};

    fn bench_placeholder(c: &mut Criterion) {
        c.bench_function("search_placeholder", |b| b.iter(|| 1 + 1));
    }

    criterion_group!(benches, bench_placeholder);
}

criterion_main!(search_benches::benches);
