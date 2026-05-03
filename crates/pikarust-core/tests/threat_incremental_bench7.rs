use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;

use pikarust_core::nnue::{Network, NnueModel};
use pikarust_core::position::{GenType, Position, generate};
use pikarust_core::search::search::{RootMove, Worker};
use pikarust_core::search::time::SearchLimits;
use pikarust_core::search::tt::TranspositionTable;

fn load_network() -> Option<Arc<Network>> {
    for p in &["../models/pikafish.nnue", "../../models/pikafish.nnue", "models/pikafish.nnue"] {
        let path = std::path::Path::new(p);
        if path.exists() {
            return NnueModel::load(path).ok().map(|m| Arc::new(Network::new(m)));
        }
    }
    eprintln!("CWD: {:?}", std::env::current_dir().unwrap());
    None
}

fn make_worker(fen: &str, depth: i32, network: Option<Arc<Network>>) -> Worker {
    let stop = Arc::new(AtomicBool::new(false));
    let ponder = Arc::new(AtomicBool::new(false));
    let tt = Arc::new(TranspositionTable::new(16));
    let increase_depth = Arc::new(AtomicBool::new(true));
    let tot_best_move_changes = Arc::new(AtomicU64::new(0));

    let mut w = Worker::new(0, stop, ponder, tt, increase_depth, tot_best_move_changes, 1, network);

    let pos = Position::from_fen(fen).expect("valid FEN");
    let legal_moves = generate(&pos, GenType::Legal);
    let mut root_moves = Vec::new();
    for i in 0..legal_moves.len() {
        root_moves.push(RootMove::new(legal_moves.get(i)));
    }

    let mut limits = SearchLimits::new();
    limits.depth = depth;
    limits.start_time = std::time::Instant::now();

    w.root_pos = pos;
    w.root_moves = root_moves;
    w.limits = limits;
    w
}

/// Bench position 7 — the position that produces wrong node counts at depth 13
/// when threat incremental update is wired into search.
///
/// The debug_assertions validation in evaluate_pos will fire an assertion
/// if incremental != refresh at any node.
#[test]
fn test_threat_incremental_bench7_depth5() {
    let Some(net) = load_network() else {
        eprintln!("NNUE model not found, skipping");
        return;
    };
    let fen = "2b1ka2r/3na2c1/4b3n/8R/8C/4C1P2/P1P1P3P/4B1N2/1r2A4/2BAK4 w - - 0 1";
    let mut w = make_worker(fen, 5, Some(net));
    let result = w.iterative_deepening();
    assert!(result.is_some(), "search should return a move");
    eprintln!("depth 5 completed, nodes={}", w.node_count());
}

#[test]
fn test_threat_incremental_bench7_depth8() {
    let Some(net) = load_network() else {
        eprintln!("NNUE model not found, skipping");
        return;
    };
    let fen = "2b1ka2r/3na2c1/4b3n/8R/8C/4C1P2/P1P1P3P/4B1N2/1r2A4/2BAK4 w - - 0 1";
    let mut w = make_worker(fen, 8, Some(net));
    let result = w.iterative_deepening();
    assert!(result.is_some(), "search should return a move");
    eprintln!("depth 8 completed, nodes={}", w.node_count());
}

#[test]
fn test_threat_incremental_bench7_depth13() {
    let Some(net) = load_network() else {
        eprintln!("NNUE model not found, skipping");
        return;
    };
    let fen = "2b1ka2r/3na2c1/4b3n/8R/8C/4C1P2/P1P1P3P/4B1N2/1r2A4/2BAK4 w - - 0 1";
    let mut w = make_worker(fen, 13, Some(net));
    let result = w.iterative_deepening();
    assert!(result.is_some(), "search should return a move");
    eprintln!("depth 13 completed, nodes={}", w.node_count());
}
