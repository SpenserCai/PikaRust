use criterion::{Criterion, criterion_group, criterion_main};

use pikarust_core::bitboard::{
    Bitboard, attacks_bb_cannon, attacks_bb_knight, attacks_bb_rook, ensure_initialized,
};
use pikarust_core::types::Square;

fn bench_magic_rook_attacks(c: &mut Criterion) {
    ensure_initialized();
    let sq = Square::SQ_E4;
    let occ = Bitboard::new(0x0040_2010_0804_0201);

    c.bench_function("magic_rook_attacks", |b| {
        b.iter(|| {
            criterion::black_box(attacks_bb_rook(
                criterion::black_box(sq),
                criterion::black_box(occ),
            ))
        });
    });
}

fn bench_magic_rook_all_squares(c: &mut Criterion) {
    ensure_initialized();
    let occ = Bitboard::new(0x0040_2010_0804_0201);

    c.bench_function("magic_rook_all_squares", |b| {
        b.iter(|| {
            for i in 0..90u8 {
                let sq = Square::from_raw_unchecked(i);
                criterion::black_box(attacks_bb_rook(sq, occ));
            }
        });
    });
}

fn bench_magic_cannon_attacks(c: &mut Criterion) {
    ensure_initialized();
    let sq = Square::SQ_B2;
    let occ = Bitboard::new(0x0123_4567_89AB_CDEF);

    c.bench_function("magic_cannon_attacks", |b| {
        b.iter(|| {
            criterion::black_box(attacks_bb_cannon(
                criterion::black_box(sq),
                criterion::black_box(occ),
            ))
        });
    });
}

fn bench_magic_knight_attacks(c: &mut Criterion) {
    ensure_initialized();
    let sq = Square::SQ_E4;
    let occ = Bitboard::new(0x0040_2010_0804_0201);

    c.bench_function("magic_knight_attacks", |b| {
        b.iter(|| {
            criterion::black_box(attacks_bb_knight(
                criterion::black_box(sq),
                criterion::black_box(occ),
            ))
        });
    });
}

fn bench_bitboard_popcount(c: &mut Criterion) {
    let bb = Bitboard::new(0x0123_4567_89AB_CDEF_0123);

    c.bench_function("bitboard_popcount", |b| {
        b.iter(|| criterion::black_box(criterion::black_box(bb).popcount()));
    });
}

fn bench_bitboard_lsb(c: &mut Criterion) {
    let bb = Bitboard::new(0x0123_4567_89AB_CDEF_0100);

    c.bench_function("bitboard_lsb", |b| {
        b.iter(|| criterion::black_box(criterion::black_box(bb).lsb()));
    });
}

fn bench_bitboard_iteration(c: &mut Criterion) {
    let bb = Bitboard::new(0x0055_AA55_AA55_AA55_AA55);

    c.bench_function("bitboard_iterate_squares", |b| {
        b.iter(|| {
            let mut count = 0u32;
            for sq in criterion::black_box(bb) {
                criterion::black_box(sq);
                count += 1;
            }
            criterion::black_box(count)
        });
    });
}

fn bench_bitboard_ops(c: &mut Criterion) {
    let bb_a = Bitboard::new(0x00FF_FF00_FF00_FF00_FF00);
    let bb_b = Bitboard::new(0x0055_AA55_AA55_AA55_AA55);

    c.bench_function("bitboard_and_or_xor", |bench| {
        bench.iter(|| {
            let and = criterion::black_box(bb_a) & criterion::black_box(bb_b);
            let or = criterion::black_box(bb_a) | criterion::black_box(bb_b);
            let xor = criterion::black_box(bb_a) ^ criterion::black_box(bb_b);
            criterion::black_box((and, or, xor))
        });
    });
}

criterion_group!(
    benches,
    bench_magic_rook_attacks,
    bench_magic_rook_all_squares,
    bench_magic_cannon_attacks,
    bench_magic_knight_attacks,
    bench_bitboard_popcount,
    bench_bitboard_lsb,
    bench_bitboard_iteration,
    bench_bitboard_ops,
);
criterion_main!(benches);
