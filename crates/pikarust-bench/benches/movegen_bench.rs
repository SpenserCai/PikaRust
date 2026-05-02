use criterion::{Criterion, criterion_group, criterion_main};

use pikarust_core::bitboard::ensure_initialized;
use pikarust_core::position::{GenType, Position, generate};

const STARTPOS: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";
const MIDGAME: &str = "r1bakab1r/9/2n1c2n1/p1p1p1p1p/9/2P6/P3P1P1P/2N1C1N2/9/R1BAKAB1R w - - 0 5";

fn bench_legal_movegen_startpos(c: &mut Criterion) {
    ensure_initialized();
    let pos = Position::from_fen(STARTPOS).expect("valid fen");

    c.bench_function("movegen_legal_startpos", |b| {
        b.iter(|| criterion::black_box(generate(criterion::black_box(&pos), GenType::Legal)));
    });
}

fn bench_legal_movegen_midgame(c: &mut Criterion) {
    ensure_initialized();
    let pos = Position::from_fen(MIDGAME).expect("valid fen");

    c.bench_function("movegen_legal_midgame", |b| {
        b.iter(|| criterion::black_box(generate(criterion::black_box(&pos), GenType::Legal)));
    });
}

fn bench_capture_gen_startpos(c: &mut Criterion) {
    ensure_initialized();
    let pos = Position::from_fen(STARTPOS).expect("valid fen");

    c.bench_function("movegen_captures_startpos", |b| {
        b.iter(|| criterion::black_box(generate(criterion::black_box(&pos), GenType::Captures)));
    });
}

fn bench_quiet_gen_startpos(c: &mut Criterion) {
    ensure_initialized();
    let pos = Position::from_fen(STARTPOS).expect("valid fen");

    c.bench_function("movegen_quiets_startpos", |b| {
        b.iter(|| criterion::black_box(generate(criterion::black_box(&pos), GenType::Quiets)));
    });
}

fn bench_pseudolegal_gen_midgame(c: &mut Criterion) {
    ensure_initialized();
    let pos = Position::from_fen(MIDGAME).expect("valid fen");

    c.bench_function("movegen_pseudolegal_midgame", |b| {
        b.iter(|| criterion::black_box(generate(criterion::black_box(&pos), GenType::PseudoLegal)));
    });
}

fn bench_perft_startpos_depth2(c: &mut Criterion) {
    ensure_initialized();
    let pos = Position::from_fen(STARTPOS).expect("valid fen");

    c.bench_function("perft_startpos_depth2", |b| {
        b.iter(|| criterion::black_box(perft(criterion::black_box(&pos), 2)));
    });
}

fn perft(pos: &Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }

    let moves = generate(pos, GenType::Legal);
    if depth == 1 {
        return moves.len() as u64;
    }

    let mut nodes = 0u64;
    for i in 0..moves.len() {
        let m = moves.get(i);
        let mut child = pos.clone();
        let gives_check = child.gives_check(m);
        child.do_move(m, gives_check);
        nodes += perft(&child, depth - 1);
    }
    nodes
}

criterion_group!(
    benches,
    bench_legal_movegen_startpos,
    bench_legal_movegen_midgame,
    bench_capture_gen_startpos,
    bench_quiet_gen_startpos,
    bench_pseudolegal_gen_midgame,
    bench_perft_startpos_depth2,
);
criterion_main!(benches);
