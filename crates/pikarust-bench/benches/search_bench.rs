use criterion::{Criterion, criterion_group, criterion_main};

use pikarust_core::bitboard::ensure_initialized;
use pikarust_core::engine::{Engine, SearchLimits};
use pikarust_core::search::TranspositionTable;
use pikarust_core::search::evaluate::evaluate_simple;
use pikarust_core::types::{Bound, Move, Square};

const STARTPOS: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";

fn bench_search_depth1(c: &mut Criterion) {
    ensure_initialized();
    let mut engine = Engine::new().expect("engine init");

    c.bench_function("search_startpos_depth1", |b| {
        b.iter(|| {
            engine.set_position(STARTPOS, &[]).expect("set pos");
            let limits = SearchLimits {
                depth: Some(1),
                ..SearchLimits::default()
            };
            criterion::black_box(engine.go(&limits).wait())
        });
    });
}

fn bench_search_depth2(c: &mut Criterion) {
    ensure_initialized();
    let mut engine = Engine::new().expect("engine init");

    c.bench_function("search_startpos_depth2", |b| {
        b.iter(|| {
            engine.set_position(STARTPOS, &[]).expect("set pos");
            let limits = SearchLimits {
                depth: Some(2),
                ..SearchLimits::default()
            };
            criterion::black_box(engine.go(&limits).wait())
        });
    });
}

fn bench_search_depth3(c: &mut Criterion) {
    ensure_initialized();
    let mut engine = Engine::new().expect("engine init");

    let mut group = c.benchmark_group("search_depth3");
    group.sample_size(10);
    group.bench_function("search_startpos_depth3", |b| {
        b.iter(|| {
            engine.set_position(STARTPOS, &[]).expect("set pos");
            let limits = SearchLimits {
                depth: Some(3),
                ..SearchLimits::default()
            };
            criterion::black_box(engine.go(&limits).wait())
        });
    });
    group.finish();
}

fn bench_evaluate_simple(c: &mut Criterion) {
    ensure_initialized();
    let pos = pikarust_core::position::Position::from_fen(STARTPOS).expect("valid fen");

    c.bench_function("evaluate_simple_startpos", |b| {
        b.iter(|| criterion::black_box(evaluate_simple(criterion::black_box(&pos), 0)));
    });
}

fn bench_tt_probe(c: &mut Criterion) {
    let tt = TranspositionTable::new(16);
    let key: u64 = 0xDEAD_BEEF_1234_5678;
    let m = Move::make(Square::SQ_E0, Square::SQ_E1);

    let result = tt.probe(key);
    result
        .writer
        .write(key, 100, true, Bound::Exact, 5, m, 50, tt.generation());

    c.bench_function("tt_probe_hit", |b| {
        b.iter(|| {
            let r = tt.probe(criterion::black_box(key));
            criterion::black_box(r.found)
        });
    });
}

fn bench_tt_probe_miss(c: &mut Criterion) {
    let tt = TranspositionTable::new(16);

    c.bench_function("tt_probe_miss", |b| {
        b.iter(|| {
            let r = tt.probe(criterion::black_box(0xCAFE_BABE_0000_0001));
            criterion::black_box(r.found)
        });
    });
}

fn bench_tt_write(c: &mut Criterion) {
    let tt = TranspositionTable::new(16);
    let m = Move::make(Square::SQ_E0, Square::SQ_E1);

    c.bench_function("tt_write", |b| {
        let mut key: u64 = 0;
        b.iter(|| {
            let r = tt.probe(key);
            r.writer
                .write(key, 100, false, Bound::Lower, 5, m, 50, tt.generation());
            key = key.wrapping_add(1);
        });
    });
}

criterion_group!(
    benches,
    bench_search_depth1,
    bench_search_depth2,
    bench_search_depth3,
    bench_evaluate_simple,
    bench_tt_probe,
    bench_tt_probe_miss,
    bench_tt_write,
);
criterion_main!(benches);
