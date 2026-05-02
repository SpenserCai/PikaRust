use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use log::info;
use thiserror::Error;

use crate::nnue::{Network, NnueModel};
use crate::position::{FenError, Position};
use crate::search::ThreadPool;
use crate::search::time::SearchLimits as InternalSearchLimits;
use crate::types::{Depth, Move, Square, VALUE_ZERO, Value};

use super::options::{EngineOptions, OptionError, UciOption};

const START_FEN: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("invalid FEN: {0}")]
    Fen(#[from] FenError),
    #[error("option error: {0}")]
    Option(#[from] OptionError),
    #[error("illegal move: {0}")]
    IllegalMove(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchLimits {
    pub depth: Option<Depth>,
    pub nodes: Option<u64>,
    pub time: [Option<i64>; 2],
    pub inc: [Option<i64>; 2],
    pub movestogo: Option<i32>,
    pub movetime: Option<i64>,
    pub infinite: bool,
    pub ponder: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub best_move: Move,
    pub ponder_move: Option<Move>,
    pub score: Value,
    pub depth: Depth,
    pub nodes: u64,
}

impl Default for SearchResult {
    fn default() -> Self {
        Self {
            best_move: Move::NONE,
            ponder_move: None,
            score: VALUE_ZERO,
            depth: 0,
            nodes: 0,
        }
    }
}

pub struct Engine {
    options: EngineOptions,
    position: Position,
    thread_pool: Option<ThreadPool>,
    network: Option<Arc<Network>>,
}

const DEFAULT_NNUE_FILE: &str = "pikafish.nnue";

const NNUE_SEARCH_PATHS: &[&str] = &["models", "."];

fn find_nnue_model() -> Option<Arc<Network>> {
    for dir in NNUE_SEARCH_PATHS {
        let path = Path::new(dir).join(DEFAULT_NNUE_FILE);
        if path.exists() {
            match NnueModel::load(&path) {
                Ok(model) => {
                    info!("NNUE model loaded from {}", path.display());
                    return Some(Arc::new(Network::new(model)));
                }
                Err(e) => {
                    info!("Failed to load NNUE from {}: {e}", path.display());
                }
            }
        }
    }
    info!("No NNUE model found, using material-only evaluation");
    None
}

impl Engine {
    pub fn new() -> Result<Self, EngineError> {
        let position = Position::from_fen(START_FEN)?;
        let network = find_nnue_model();
        Ok(Self {
            options: EngineOptions::default(),
            position,
            thread_pool: None,
            network,
        })
    }

    fn ensure_thread_pool(&mut self) {
        if self.thread_pool.is_none() {
            self.thread_pool = Some(ThreadPool::new(
                self.options.threads,
                self.options.hash_mb,
                self.network.clone(),
            ));
        }
    }

    pub fn set_option(&mut self, name: &str, value: &str) -> Result<(), EngineError> {
        let old_threads = self.options.threads;
        let old_hash = self.options.hash_mb;
        self.options.set(name, value)?;

        if self.thread_pool.is_some()
            && (self.options.threads != old_threads || self.options.hash_mb != old_hash)
        {
            self.thread_pool = Some(ThreadPool::new(
                self.options.threads,
                self.options.hash_mb,
                self.network.clone(),
            ));
        }
        Ok(())
    }

    pub const fn options(&self) -> &EngineOptions {
        &self.options
    }

    pub fn new_game(&mut self) -> Result<(), EngineError> {
        self.position = Position::from_fen(START_FEN)?;
        if let Some(tp) = &mut self.thread_pool {
            tp.clear();
        }
        Ok(())
    }

    pub fn set_position(&mut self, fen: &str, moves: &[&str]) -> Result<(), EngineError> {
        let mut pos = Position::from_fen(fen)?;

        for &move_str in moves {
            let m = parse_uci_move(&pos, move_str)
                .ok_or_else(|| EngineError::IllegalMove(move_str.to_owned()))?;
            let gives_check = pos.gives_check(m);
            pos.do_move(m, gives_check);
        }

        self.position = pos;
        Ok(())
    }

    pub const fn position(&self) -> &Position {
        &self.position
    }

    pub fn go(&mut self, limits: &SearchLimits) -> SearchResult {
        self.ensure_thread_pool();
        let Some(tp) = self.thread_pool.as_mut() else {
            return SearchResult::default();
        };

        let search_limits = convert_limits(limits);
        tp.start_search(&self.position, &search_limits);
        tp.wait_for_search();

        let best_move = tp.best_move().unwrap_or(Move::NONE);
        let score = tp.best_score();
        let nodes = tp.nodes_searched();

        let idx = tp.best_thread_idx();
        let depth = tp.worker(idx).completed_depth;

        let ponder_move = if best_move == Move::NONE {
            None
        } else {
            let w = tp.worker(idx);
            if !w.root_moves.is_empty() && w.root_moves[0].pv.len() > 1 {
                Some(w.root_moves[0].pv[1])
            } else {
                None
            }
        };

        SearchResult {
            best_move,
            ponder_move,
            score,
            depth,
            nodes,
        }
    }

    pub fn stop(&self) {
        if let Some(tp) = &self.thread_pool {
            tp.stop();
        }
    }

    pub fn uci_options() -> Vec<UciOption> {
        EngineOptions::uci_options()
    }
}

fn convert_limits(limits: &SearchLimits) -> InternalSearchLimits {
    let mut sl = InternalSearchLimits::new();
    sl.start_time = Instant::now();

    if let Some(d) = limits.depth {
        sl.depth = d;
    }
    if let Some(n) = limits.nodes {
        sl.nodes = n;
    }
    for i in 0..2 {
        if let Some(t) = limits.time[i] {
            sl.time[i] = t.max(0) as u64;
        }
        if let Some(inc) = limits.inc[i] {
            sl.inc[i] = inc.max(0) as u64;
        }
    }
    if let Some(mtg) = limits.movestogo {
        sl.movestogo = mtg;
    }
    if let Some(mt) = limits.movetime {
        sl.movetime = mt.max(0) as u64;
    }
    sl.infinite = limits.infinite;
    sl.ponder_mode = limits.ponder;
    sl
}

fn parse_uci_move(pos: &Position, s: &str) -> Option<Move> {
    if s.len() != 4 {
        return None;
    }

    let bytes = s.as_bytes();
    let from_file = bytes[0].checked_sub(b'a')?;
    let from_rank = bytes[1].checked_sub(b'0')?;
    let to_file = bytes[2].checked_sub(b'a')?;
    let to_rank = bytes[3].checked_sub(b'0')?;

    if from_file > 8 || from_rank > 9 || to_file > 8 || to_rank > 9 {
        return None;
    }

    let from = Square::make(from_file.try_into().ok()?, from_rank.try_into().ok()?);
    let to = Square::make(to_file.try_into().ok()?, to_rank.try_into().ok()?);
    let m = Move::make(from, to);

    if pos.is_legal(m) { Some(m) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{VALUE_MATE_IN_MAX_PLY, VALUE_MATED_IN_MAX_PLY};

    #[test]
    fn test_engine_new() {
        let engine = Engine::new().unwrap();
        assert_eq!(engine.options().hash_mb, 16);
        assert_eq!(engine.options().threads, 1);
        assert!(engine.thread_pool.is_none());
    }

    #[test]
    fn test_engine_set_option() {
        let mut engine = Engine::new().unwrap();
        engine.set_option("Hash", "256").unwrap();
        assert_eq!(engine.options().hash_mb, 256);
    }

    #[test]
    fn test_engine_set_option_error() {
        let mut engine = Engine::new().unwrap();
        assert!(engine.set_option("Nonexistent", "42").is_err());
    }

    #[test]
    fn test_engine_set_option_recreates_pool() {
        let mut engine = Engine::new().unwrap();
        engine.ensure_thread_pool();
        assert!(engine.thread_pool.is_some());
        engine.set_option("Threads", "2").unwrap();
        assert!(engine.thread_pool.is_some());
        assert_eq!(engine.options().threads, 2);
    }

    #[test]
    fn test_engine_new_game() {
        let mut engine = Engine::new().unwrap();
        engine.set_position(START_FEN, &["b0c2"]).unwrap();
        engine.new_game().unwrap();
        assert_eq!(engine.position().fen(), START_FEN);
    }

    #[test]
    fn test_engine_set_position_startpos() {
        let mut engine = Engine::new().unwrap();
        engine.set_position(START_FEN, &[]).unwrap();
        assert_eq!(engine.position().fen(), START_FEN);
    }

    #[test]
    fn test_engine_set_position_with_moves() {
        let mut engine = Engine::new().unwrap();
        engine.set_position(START_FEN, &["b0c2", "b9c7"]).unwrap();
        let fen = engine.position().fen();
        assert_ne!(fen, START_FEN);
    }

    #[test]
    fn test_engine_set_position_invalid_fen() {
        let mut engine = Engine::new().unwrap();
        assert!(engine.set_position("invalid fen", &[]).is_err());
    }

    #[test]
    fn test_engine_set_position_illegal_move() {
        let mut engine = Engine::new().unwrap();
        assert!(engine.set_position(START_FEN, &["e0e5"]).is_err());
    }

    #[test]
    fn test_engine_go_finds_legal_move() {
        let mut engine = Engine::new().unwrap();
        let limits = SearchLimits {
            depth: Some(1),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits);
        assert_ne!(result.best_move, Move::NONE);
        assert!(result.depth >= 1);
    }

    #[test]
    fn test_engine_stop() {
        let mut engine = Engine::new().unwrap();
        engine.stop();
        engine.ensure_thread_pool();
        engine.stop();
    }

    #[test]
    fn test_engine_uci_options() {
        let options = Engine::uci_options();
        assert_eq!(options.len(), 6);
    }

    #[test]
    fn test_search_limits_default() {
        let limits = SearchLimits::default();
        assert!(limits.depth.is_none());
        assert!(limits.nodes.is_none());
        assert_eq!(limits.time, [None; 2]);
        assert_eq!(limits.inc, [None; 2]);
        assert!(limits.movestogo.is_none());
        assert!(limits.movetime.is_none());
        assert!(!limits.infinite);
        assert!(!limits.ponder);
    }

    #[test]
    fn test_convert_limits_depth() {
        let limits = SearchLimits {
            depth: Some(5),
            ..SearchLimits::default()
        };
        let sl = convert_limits(&limits);
        assert_eq!(sl.depth, 5);
        assert_eq!(sl.nodes, 0);
        assert!(!sl.infinite);
    }

    #[test]
    fn test_convert_limits_time() {
        let limits = SearchLimits {
            time: [Some(60000), Some(30000)],
            inc: [Some(1000), Some(500)],
            movestogo: Some(20),
            ..SearchLimits::default()
        };
        let sl = convert_limits(&limits);
        assert_eq!(sl.time[0], 60000);
        assert_eq!(sl.time[1], 30000);
        assert_eq!(sl.inc[0], 1000);
        assert_eq!(sl.inc[1], 500);
        assert_eq!(sl.movestogo, 20);
    }

    #[test]
    fn test_convert_limits_infinite() {
        let limits = SearchLimits {
            infinite: true,
            ..SearchLimits::default()
        };
        let sl = convert_limits(&limits);
        assert!(sl.infinite);
    }

    #[test]
    fn test_parse_uci_move_valid() {
        let pos = Position::from_fen(START_FEN).unwrap();
        let m = parse_uci_move(&pos, "b0c2");
        assert!(m.is_some());
        let m = m.unwrap();
        assert!(m.is_ok());
    }

    #[test]
    fn test_parse_uci_move_invalid_format() {
        let pos = Position::from_fen(START_FEN).unwrap();
        assert!(parse_uci_move(&pos, "").is_none());
        assert!(parse_uci_move(&pos, "abc").is_none());
        assert!(parse_uci_move(&pos, "abcde").is_none());
    }

    #[test]
    fn test_parse_uci_move_out_of_range() {
        let pos = Position::from_fen(START_FEN).unwrap();
        assert!(parse_uci_move(&pos, "z0a0").is_none());
    }

    #[test]
    fn test_parse_uci_move_illegal() {
        let pos = Position::from_fen(START_FEN).unwrap();
        assert!(parse_uci_move(&pos, "e0e5").is_none());
    }

    // -------------------------------------------------------------------
    // Search integration tests
    // -------------------------------------------------------------------

    #[test]
    fn test_search_depth1_returns_legal_move() {
        let mut engine = Engine::new().unwrap();
        let limits = SearchLimits {
            depth: Some(1),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits);
        assert_ne!(result.best_move, Move::NONE, "depth 1 should return a move");
        assert!(result.best_move.is_ok(), "returned move should be valid");

        // Verify the move is actually legal in the current position
        let pos = engine.position();
        assert!(
            pos.is_legal(result.best_move),
            "returned move should be legal"
        );
    }

    #[test]
    fn test_search_depth3_returns_valid_move() {
        let mut engine = Engine::new().unwrap();
        let limits = SearchLimits {
            depth: Some(3),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits);
        assert_ne!(result.best_move, Move::NONE);
        assert!(result.depth >= 3, "should reach at least depth 3");
        assert!(result.nodes > 0, "should search some nodes");

        // Verify legality
        let pos = engine.position();
        assert!(pos.is_legal(result.best_move));
    }

    #[test]
    fn test_search_depth5_completes() {
        let mut engine = Engine::new().unwrap();
        let limits = SearchLimits {
            depth: Some(5),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits);
        assert_ne!(result.best_move, Move::NONE);
        assert!(result.depth >= 5);
        assert!(result.nodes > 100, "depth 5 should search many nodes");
    }

    #[test]
    fn test_search_midgame_position() {
        let mut engine = Engine::new().unwrap();
        let fen = "r1ba1a3/4kn3/2n1b4/pNp1p1p1p/4c4/6P2/P1P2R2P/1CcC5/9/2BAKAB2 w - - 0 1";
        engine.set_position(fen, &[]).unwrap();

        let limits = SearchLimits {
            depth: Some(3),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits);
        assert_ne!(result.best_move, Move::NONE);
        assert!(result.best_move.is_ok());
    }

    #[test]
    fn test_search_after_moves() {
        let mut engine = Engine::new().unwrap();
        engine.set_position(START_FEN, &["b0c2", "b9c7"]).unwrap();

        let limits = SearchLimits {
            depth: Some(3),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits);
        assert_ne!(result.best_move, Move::NONE);
    }

    #[test]
    fn test_search_node_limited() {
        let mut engine = Engine::new().unwrap();
        let limits = SearchLimits {
            nodes: Some(1000),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits);
        assert_ne!(result.best_move, Move::NONE);
        // Node limit is approximate, but should be in the right ballpark
        assert!(
            result.nodes < 10000,
            "node-limited search should not vastly exceed limit, got {}",
            result.nodes
        );
    }

    #[test]
    fn test_search_consecutive_searches() {
        let mut engine = Engine::new().unwrap();
        let limits = SearchLimits {
            depth: Some(2),
            ..SearchLimits::default()
        };

        // First search
        let r1 = engine.go(&limits);
        assert_ne!(r1.best_move, Move::NONE);

        // Second search on same position should also work
        let r2 = engine.go(&limits);
        assert_ne!(r2.best_move, Move::NONE);
    }

    #[test]
    fn test_search_new_game_then_search() {
        let mut engine = Engine::new().unwrap();

        // Search once
        let limits = SearchLimits {
            depth: Some(2),
            ..SearchLimits::default()
        };
        let _ = engine.go(&limits);

        // New game, then search again
        engine.new_game().unwrap();
        let result = engine.go(&limits);
        assert_ne!(result.best_move, Move::NONE);
    }

    #[test]
    fn test_search_endgame_position() {
        let mut engine = Engine::new().unwrap();
        let fen = "5a3/3k5/3aR4/9/5r3/5n3/9/3A1A3/5K3/2BC2B2 w - - 0 1";
        engine.set_position(fen, &[]).unwrap();

        let limits = SearchLimits {
            depth: Some(3),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits);
        assert_ne!(result.best_move, Move::NONE);
    }

    #[test]
    fn test_search_score_is_bounded() {
        let mut engine = Engine::new().unwrap();
        let limits = SearchLimits {
            depth: Some(5),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits);
        // Score should be within valid range
        assert!(
            result.score > VALUE_MATED_IN_MAX_PLY - 100
                && result.score < VALUE_MATE_IN_MAX_PLY + 100,
            "score {} should be in reasonable range",
            result.score
        );
    }
}
