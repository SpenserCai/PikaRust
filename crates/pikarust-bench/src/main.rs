#![forbid(unsafe_code)]

use std::time::Instant;

use pikarust_core::bitboard::ensure_initialized;
use pikarust_core::engine::{Engine, SearchLimits};
use pikarust_core::position::{GenType, Position, generate};

const STARTPOS: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";

/// Pikafish default bench positions (50 FENs from benchmark.cpp).
const BENCH_FENS: &[&str] = &[
    // Initial position
    "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w",
    // Middle game
    "r1ba1a3/4kn3/2n1b4/pNp1p1p1p/4c4/6P2/P1P2R2P/1CcC5/9/2BAKAB2 w",
    "1cbak4/9/n2a5/2p1p3p/5cp2/2n2N3/6PCP/3AB4/2C6/3A1K1N1 w",
    "5a3/3k5/3aR4/9/5r3/5n3/9/3A1A3/5K3/2BC2B2 w",
    "2bak4/9/3a5/p2Np3p/3n1P3/3pc3P/P4r1c1/B2CC2R1/4A4/3AK1B2 b",
    "1r1akabr1/1c7/2n1b1n2/p1p1p3p/6p2/PN3R3/1cP1P1P1P/2C1C1N2/1R7/2BAKAB2 b",
    "2b1ka2r/3na2c1/4b3n/8R/8C/4C1P2/P1P1P3P/4B1N2/1r2A4/2BAK4 w",
    "2bckab2/4a4/5n3/CR3N2p/5r3/P3P1B2/9/2n1B4/4A4/3AK1C2 w",
    "2b1kab1C/1N2a4/n3ccn2/p5r1p/4p4/P1P2RN2/2r1P3P/C3B4/4A4/2BAK2R1 w",
    "2bakab2/9/2n1c1R1c/3r4p/4N4/r8/6P1P/6C1C/4A4/1RBAK1B2 w",
    "2bak1b1r/4a4/2n4cn/p6C1/4pN3/P2N4R/4P1P1P/3CB4/4A2r1/c1BAKR3 w",
    "1r2kabr1/4a4/2C1b2c1/p3p3p/1c3n3/2p3R2/P3P3P/N3C1N2/7R1/2BAKAB2 b",
    "3ak1b2/4a4/2n1b1R2/p1N1pc2p/7r1/2PN1r3/P3P3P/3RB4/4A4/1C2KAB1c w",
    "2baka1r1/9/c5n1c/p3p1CCp/2p3p2/4P4/P6RP/2r1B1N2/4A4/1RB1KA3 w",
    "3akabr1/9/4c4/p1pRn2Cp/4rcp2/2P1p4/P3P1P1P/3CB1N2/9/3AKABR1 w",
    "3akab2/3r5/8n/8p/2P1C1b2/8P/cR2N2r1/2n1B1N2/4A4/2B1KR3 w",
    "2bak4/4a1R2/2n1ccn1b/p3p1C1p/9/2p3P2/P1r1P3P/2N1BCN2/4A4/2BAK4 w",
    "C3kab2/4a4/2Rnb3n/8p/6p2/1p2c3r/P5P2/4B3N/3CA4/2BAK4 w",
    "4kabr1/4a4/2n1b3n/p1C1p3p/6p2/PNP6/4P1P2/1C2B4/4A4/1R2KAB1c w",
    "3ak1bn1/4a4/1c2b1c2/r3p1N1p/p1p6/6P2/n1P1P3P/N1C1C3B/3R5/2BAKA3 w",
    "1rb1kabr1/4a4/1c7/p1p1R3p/7n1/2P3p2/P3P1c1P/C1N6/4N4/1RBAKAB2 w",
    "r1b1kabr1/4a1c2/1cn3n2/p1p1pR2p/3NP4/2P6/P5p1P/1C2C4/9/RNBAKAB2 b",
    "rn1akab2/9/1c2b1n1c/3Pp1p1p/p8/6P2/P1N1P3P/2C1C4/3rN4/R2AKAB1R w",
    "2bakab2/6r2/2n1c1nc1/p1p2rp1p/4p4/2PN2PC1/P3P3P/6N2/3CA4/1RBAK1B1R w",
    "rn2kab2/3ra4/2c1b2cn/p5p1C/9/2p6/P3P1P1P/NC4N2/9/1RBAKABR1 w",
    "rnbakab2/2r6/1c4nc1/p3p1C1p/2p3p2/2P6/P3P1P1P/1CN3N2/8R/R1BAKAB2 b",
    "r2akabr1/4n4/4b1nc1/p1N1p1R1p/6p2/2p3P2/Pc2P3P/2C1C1N2/9/R1BAKAB2 b",
    "3ak1b1r/4a4/b1n2c3/p3C2Rp/5np2/2P6/P2rP1P1P/3C2N2/4A4/R1B1KAB2 b",
    "rnba1aCn1/4k4/8r/p1p1p3p/1c4P2/2P6/P3c3P/1C4N2/4K4/RNBA1AB1R b",
    "4kab2/4a4/n3b4/p5p1p/2r1C4/2N1P2r1/P4nPcP/N3B4/2R6/2RAKAB2 w",
    "4kab2/4a4/2n1bcc2/p1N1p1Crp/5RP2/2P2N3/P3r4/4B4/C3A2n1/2BAK3R w",
    "r2akabr1/4n1c2/4b1c2/pC2C3p/2P1P4/9/P3N1p1P/9/9/RNBAKAB2 w",
    "2b1ka1r1/4a4/4b1n2/p3p1p1p/3n5/2p1P1P2/PR6P/2cCBRN2/3rA4/1NBAK4 b",
    "4k2n1/9/1c2b4/p3p1N1p/7r1/6P2/P1R1P3P/4B4/4A4/2BAK4 b",
    "2c1kab2/4a4/2n1b3c/p1pN4R/3r2p1p/2P6/P3P1P1n/4BC2N/4A4/2C1KAB2 w",
    "6b2/3ka1N2/5a3/p3p4/1n3P3/P1N5C/5nc2/3AB4/9/3AK1B2 b",
    "3k1a3/2P1aP3/4b1n2/8C/6b2/1R5R1/9/9/1rcpr4/3c1K3 w",
    "4ka3/3Pa4/r6R1/2C4C1/9/9/8n/9/4p3r/3K3R1 w",
    "4ka3/4a4/N8/p8/C8/9/9/8B/3p2ppc/4K4 w",
    "9/4k4/3aba3/3P5/1cb6/2BC5/n3N4/B2A5/9/3AK4 w",
    "3ak4/3Pa4/4b3b/5r3/1R3N3/9/9/B8/2p1A4/2B1KA3 w",
    "4k1b2/4a4/5a3/6P1C/9/p4Nn2/2n6/9/4K4/5AB2 b",
    // Complicated checks and evasions
    "CRN1k1b2/3ca4/4ba3/9/2nr5/9/9/4B4/4A4/4KA3 w",
    "R1N1k1b2/9/3aba3/9/2nr5/2B6/9/4B4/4A4/4KA3 w",
    "C1nNk4/9/9/9/9/9/n1pp5/B3C4/9/3A1K3 w",
    "4ka3/4a4/9/9/4N4/p8/9/4C3c/7n1/2BK5 w",
    "2b1ka3/9/b3N4/4n4/9/9/9/4C4/2p6/2BK5 w",
    "1C2ka3/9/C1Nab1n2/p3p3p/6p2/9/P3P3P/3AB4/3p2c2/c1BAK4 w",
    "CnN1k1b2/c3a4/4ba3/9/2nr5/9/9/4C4/4A4/4KA3 w",
];

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

fn bench_perft() {
    ensure_initialized();
    let pos = Position::from_fen(STARTPOS).expect("valid fen");

    println!("=== Perft Benchmark ===");
    for depth in 1..=4 {
        let start = Instant::now();
        let nodes = perft(&pos, depth);
        let elapsed = start.elapsed();
        let nps = if elapsed.as_secs_f64() > 0.0 {
            nodes as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };
        println!(
            "  Perft({depth}): {nodes:>12} nodes in {elapsed:>10.3?}  ({nps:>12.0} nodes/sec)"
        );
    }
}

fn bench_nps() {
    ensure_initialized();
    let mut engine = Engine::new().expect("engine init");

    println!("\n=== NPS Benchmark ===");
    let limits = SearchLimits {
        movetime: Some(5000),
        ..SearchLimits::default()
    };

    let start = Instant::now();
    let result = engine.go(&limits).wait();
    let elapsed = start.elapsed();

    let nps = if elapsed.as_secs_f64() > 0.0 {
        result.nodes as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };

    println!("  Depth reached: {}", result.depth);
    println!("  Nodes searched: {}", result.nodes);
    println!("  Time: {elapsed:.3?}");
    println!("  NPS: {nps:.0}");
    println!("  Best move: {}", result.best_move);
}

/// Pikafish-aligned bench: 50 positions × depth 13, deterministic node count.
fn bench() {
    ensure_initialized();
    let mut engine = Engine::new().expect("engine init");

    // Warm-up: ensure ThreadPool + TT allocated before timing.
    let warmup = SearchLimits { depth: Some(1), ..SearchLimits::default() };
    let _ = engine.go(&warmup).wait();

    // Clear TT once (matches Pikafish ucinewgame before bench).
    engine.new_game().expect("new_game");

    let limits = SearchLimits { depth: Some(13), ..SearchLimits::default() };
    let mut total_nodes: u64 = 0;

    let elapsed = Instant::now();

    for (i, fen) in BENCH_FENS.iter().enumerate() {
        engine.set_position(fen, &[]).expect("valid fen");
        let result = engine.go(&limits).wait();
        total_nodes += result.nodes;
        eprintln!(
            "Position {}/{}: {} nodes  bestmove {}",
            i + 1,
            BENCH_FENS.len(),
            result.nodes,
            result.best_move
        );
    }

    let elapsed_ms = elapsed.elapsed().as_millis().max(1) as u64;
    let nps = 1000 * total_nodes / elapsed_ms;

    eprintln!();
    eprintln!("===========================");
    eprintln!("Total time (ms) : {elapsed_ms}");
    eprintln!("Nodes searched  : {total_nodes}");
    eprintln!("Nodes/second    : {nps}");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "bench" {
        bench();
        return;
    }

    println!("PikaRust Macro Benchmarks");
    println!("========================\n");

    bench_perft();
    bench_nps();

    println!("\nDone.");
}
