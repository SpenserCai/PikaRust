#![forbid(unsafe_code)]

use std::time::Instant;

use pikarust_core::bitboard::ensure_initialized;
use pikarust_core::engine::{Engine, SearchLimits};
use pikarust_core::position::{GenType, Position, generate};

const STARTPOS: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";

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
    let result = engine.go(&limits);
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

fn main() {
    println!("PikaRust Macro Benchmarks");
    println!("========================\n");

    bench_perft();
    bench_nps();

    println!("\nDone.");
}
