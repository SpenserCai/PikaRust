use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use log::{info, warn};
use thiserror::Error;

use crate::nnue::{Network, NnueError, NnueModel};
use crate::position::{FenError, Position};
use crate::search::ThreadPool;
use crate::search::thread::SearchResult as ThreadSearchResult;
use crate::search::time::SearchLimits as InternalSearchLimits;
use crate::types::{Depth, Move, Square, VALUE_ZERO, Value, is_decisive};

use super::options::{EngineOptions, OptionError, UciOption};

const START_FEN: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("invalid FEN: {0}")]
    Fen(#[from] FenError),
    #[error("option error: {0}")]
    Option(#[from] OptionError),
    #[error("NNUE model: {0}")]
    Nnue(#[from] NnueError),
    #[error("search worker disconnected before returning a result")]
    SearchFailed,
    #[error("invalid search limits: {0}")]
    InvalidLimits(String),
    #[error("illegal move: {0}")]
    IllegalMove(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchLimits {
    pub depth: Option<Depth>,
    pub nodes: Option<u64>,
    /// Remaining milliseconds for each side. With any clock supplied, zero or
    /// an omitted side means an exhausted clock (a minimum one-millisecond
    /// search budget); `[None, None]` means there is no clock limit.
    pub time: [Option<i64>; 2],
    pub inc: [Option<i64>; 2],
    pub movestogo: Option<i32>,
    pub movetime: Option<i64>,
    pub infinite: bool,
    pub ponder: bool,
    /// Restrict the search to these legal UCI root moves.
    pub search_moves: Vec<String>,
}

/// Static scores in internal engine units, from the side-to-move perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaticEvaluation {
    /// Raw neural-network score, absent in explicit material-only mode.
    pub nnue: Option<Value>,
    /// Evaluation after material scaling and rule-60 adjustment.
    pub value: Value,
}

impl SearchLimits {
    /// Validate limits supplied by an application or language binding.
    pub fn validate(&self) -> Result<(), EngineError> {
        if self
            .depth
            .is_some_and(|d| !(1..crate::types::MAX_PLY).contains(&d))
        {
            return Err(EngineError::InvalidLimits(format!(
                "depth must be in 1..{}",
                crate::types::MAX_PLY
            )));
        }
        if self.nodes == Some(0)
            || self.movetime.is_some_and(|v| v <= 0)
            || self.movestogo.is_some_and(|v| v <= 0)
            || self.time.iter().chain(&self.inc).flatten().any(|&v| v < 0)
        {
            return Err(EngineError::InvalidLimits(
                "node, move-time and moves-to-go limits must be positive; clocks must be nonnegative".to_owned()
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub best_move: Move,
    pub ponder_move: Option<Move>,
    pub score: Value,
    pub score_cp: i32,
    pub wdl: Option<(i32, i32, i32)>,
    pub depth: Depth,
    pub seldepth: i32,
    pub nodes: u64,
    pub hashfull: i32,
    pub pv: Vec<Move>,
}

impl Default for SearchResult {
    fn default() -> Self {
        Self {
            best_move: Move::NONE,
            ponder_move: None,
            score: VALUE_ZERO,
            score_cp: 0,
            wdl: None,
            depth: 0,
            seldepth: 0,
            nodes: 0,
            hashfull: 0,
            pv: Vec::new(),
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
                    warn!("Failed to load NNUE from {}: {e}", path.display());
                }
            }
        }
    }
    warn!(
        "No NNUE model found; material-only evaluation has no competitive playing-strength guarantee"
    );
    None
}

impl Engine {
    /// Discover `models/pikafish.nnue` or `pikafish.nnue` in the working directory.
    /// A warning is logged when discovery falls back to material evaluation.
    /// Applications requiring NNUE should use [`Self::with_nnue_file`].
    pub fn new() -> Result<Self, EngineError> {
        Self::from_network(find_nnue_model())
    }

    /// Load the specified NNUE file. Loading errors never fall back to material.
    pub fn with_nnue_file(path: impl AsRef<Path>) -> Result<Self, EngineError> {
        Self::with_network(Arc::new(Network::new(NnueModel::load(path.as_ref())?)))
    }

    /// Share immutable model weights between independent engine instances.
    pub fn with_network(network: Arc<Network>) -> Result<Self, EngineError> {
        Self::from_network(Some(network))
    }

    /// Construct a model-free engine for rule tests and lightweight diagnostics.
    pub fn without_nnue() -> Result<Self, EngineError> {
        Self::from_network(None)
    }

    fn from_network(network: Option<Arc<Network>>) -> Result<Self, EngineError> {
        let position = Position::from_fen(START_FEN)?;
        Ok(Self {
            options: EngineOptions::default(),
            position,
            thread_pool: None,
            network,
        })
    }

    pub const fn has_nnue(&self) -> bool {
        self.network.is_some()
    }

    /// Access immutable weights for reuse by an engine pool or binding.
    pub fn network(&self) -> Option<Arc<Network>> {
        self.network.clone()
    }

    pub fn evaluate(&self) -> StaticEvaluation {
        self.network.as_ref().map_or_else(
            || StaticEvaluation {
                nnue: None,
                value: crate::search::evaluate::evaluate_simple(&self.position, 0),
            },
            |network| {
                let (psqt, positional) = network.evaluate_position(&self.position);
                StaticEvaluation {
                    nnue: Some(psqt + positional),
                    value: crate::search::evaluate::evaluate(&self.position, psqt, positional, 0),
                }
            },
        )
    }

    fn ensure_thread_pool(&mut self) {
        if self.thread_pool.is_none() {
            let mut tp = ThreadPool::new(
                self.options.threads,
                self.options.hash_mb,
                self.network.clone(),
            );
            tp.clear();
            self.thread_pool = Some(tp);
        }
    }

    pub fn set_option(&mut self, name: &str, value: &str) -> Result<(), EngineError> {
        let old_threads = self.options.threads;
        let old_hash = self.options.hash_mb;
        self.options.set(name, value)?;

        if self.thread_pool.is_some()
            && (self.options.threads != old_threads || self.options.hash_mb != old_hash)
        {
            // Join the previous search before allocating its replacement. All
            // new pools must receive the same history initialization as the
            // first search, including nonzero capture/pawn-history seeds.
            drop(self.thread_pool.take());
            self.ensure_thread_pool();
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

    /// Start a search and return a control handle. Non-blocking.
    /// The caller controls the search lifecycle through the returned
    /// [`SearchHandle`] (stop, wait, ponderhit).
    /// Validate untrusted limits and root moves before starting a search.
    pub fn try_go(&mut self, limits: &SearchLimits) -> Result<SearchHandle, EngineError> {
        limits.validate()?;
        for m in &limits.search_moves {
            if parse_uci_move(&self.position, m).is_none() {
                return Err(EngineError::IllegalMove(m.clone()));
            }
        }
        Ok(self.go(limits))
    }

    pub fn go(&mut self, limits: &SearchLimits) -> SearchHandle {
        self.ensure_thread_pool();
        let piece_counts = *self.position.piece_count_array();
        let show_wdl = self.options.show_wdl;
        let Some(tp) = self.thread_pool.as_mut() else {
            let (tx, rx) = mpsc::sync_channel(1);
            let _ = tx.send(ThreadSearchResult::default());
            return SearchHandle {
                stop: Arc::new(AtomicBool::new(true)),
                ponder: Arc::new(AtomicBool::new(false)),
                rx,
                piece_counts,
                show_wdl,
            };
        };

        let mut search_limits = convert_limits(limits);
        search_limits.move_overhead = self.options.move_overhead as u64;
        let rx = tp.start_search_async(&self.position, &search_limits);
        let stop = Arc::clone(tp.stop_flag());
        let ponder = Arc::clone(tp.ponder_flag());

        SearchHandle {
            stop,
            ponder,
            rx,
            piece_counts,
            show_wdl,
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

/// Search control handle. Dropping it cancels this search without blocking.
///
/// Waiting consumes the handle; stopping or dropping an older handle cannot
/// affect a subsequent search. The engine joins its workers on shutdown.
#[must_use = "dropping a search handle cancels the search"]
pub struct SearchHandle {
    stop: Arc<AtomicBool>,
    ponder: Arc<AtomicBool>,
    rx: mpsc::Receiver<ThreadSearchResult>,
    piece_counts: [u8; 16],
    show_wdl: bool,
}

impl SearchHandle {
    /// Block until the search completes, consuming the handle.
    pub fn wait(self) -> SearchResult {
        let raw = self.rx.recv().unwrap_or_default();
        self.finalize(&raw)
    }

    /// Wait for a result, preserving worker failures for applications.
    pub fn wait_result(self) -> Result<SearchResult, EngineError> {
        let raw = self.rx.recv().map_err(|_| EngineError::SearchFailed)?;
        Ok(self.finalize(&raw))
    }

    /// Poll a result without conflating worker failure with an active search.
    pub fn try_result(&self) -> Result<Option<SearchResult>, EngineError> {
        match self.rx.try_recv() {
            Ok(raw) => Ok(Some(self.finalize(&raw))),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(EngineError::SearchFailed),
        }
    }

    /// Non-blocking check for search completion.
    pub fn try_recv(&self) -> Option<SearchResult> {
        self.rx.try_recv().ok().map(|raw| self.finalize(&raw))
    }

    /// Send stop signal. The search threads will stop at the next checkpoint.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// Ponderhit: transition from ponder mode to normal search.
    /// Time management starts taking effect.
    pub fn ponderhit(&self) {
        self.ponder.store(false, Ordering::SeqCst);
    }

    fn finalize(&self, raw: &ThreadSearchResult) -> SearchResult {
        let (score_cp, wdl) = if is_decisive(raw.score) {
            (raw.score, None)
        } else {
            let cp = crate::search::to_cp(raw.score, &self.piece_counts);
            let wdl_triple = if self.show_wdl {
                Some(crate::search::wdl(raw.score, &self.piece_counts))
            } else {
                None
            };
            (cp, wdl_triple)
        };

        SearchResult {
            best_move: raw.best_move,
            ponder_move: raw.ponder_move,
            score: raw.score,
            score_cp,
            wdl,
            depth: raw.depth,
            seldepth: raw.seldepth,
            nodes: raw.nodes,
            hashfull: raw.hashfull,
            pv: raw.pv.clone(),
        }
    }
}

impl Drop for SearchHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn convert_limits(limits: &SearchLimits) -> InternalSearchLimits {
    let mut sl = InternalSearchLimits::new();
    sl.start_time = Instant::now();

    if let Some(d) = limits.depth {
        sl.depth = d.clamp(1, crate::types::MAX_PLY - 1);
    }
    if let Some(n) = limits.nodes {
        sl.nodes = n;
    }
    // The search layer uses zero as "no clock". Preserve the distinction
    // between absent clocks and an explicitly exhausted (or omitted-side)
    // clock, so `go wtime 0 btime 0` cannot become an unbounded search.
    let has_clock = limits.time.iter().any(Option::is_some);
    for i in 0..2 {
        if has_clock {
            sl.time[i] = limits.time[i].unwrap_or_default().max(1) as u64;
        }
        if let Some(inc) = limits.inc[i] {
            sl.inc[i] = inc.max(0) as u64;
        }
    }
    if let Some(mtg) = limits.movestogo {
        sl.movestogo = mtg.max(1);
    }
    if let Some(mt) = limits.movetime {
        sl.movetime = mt.max(1) as u64;
    }
    sl.infinite = limits.infinite;
    sl.ponder_mode = limits.ponder;
    sl.search_moves.clone_from(&limits.search_moves);
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

    if pos.is_legal_move(m) { Some(m) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{VALUE_MATE_IN_MAX_PLY, VALUE_MATED_IN_MAX_PLY};

    #[test]
    fn test_engine_new() {
        let engine = Engine::without_nnue().unwrap();
        assert_eq!(engine.options().hash_mb, 16);
        assert_eq!(engine.options().threads, 1);
        assert!(engine.thread_pool.is_none());
    }

    #[test]
    fn test_engine_set_option() {
        let mut engine = Engine::without_nnue().unwrap();
        engine.set_option("Hash", "256").unwrap();
        assert_eq!(engine.options().hash_mb, 256);
    }

    #[test]
    fn test_engine_set_option_error() {
        let mut engine = Engine::without_nnue().unwrap();
        assert!(engine.set_option("Nonexistent", "42").is_err());
    }

    #[test]
    fn test_engine_set_option_recreates_pool() {
        let mut engine = Engine::without_nnue().unwrap();
        engine.ensure_thread_pool();
        assert!(engine.thread_pool.is_some());
        engine.set_option("Threads", "2").unwrap();
        assert!(engine.thread_pool.is_some());
        assert_eq!(engine.options().threads, 2);
    }

    #[test]
    fn test_engine_new_game() {
        let mut engine = Engine::without_nnue().unwrap();
        engine.set_position(START_FEN, &["b0c2"]).unwrap();
        engine.new_game().unwrap();
        assert_eq!(engine.position().fen(), START_FEN);
    }

    #[test]
    fn test_engine_set_position_startpos() {
        let mut engine = Engine::without_nnue().unwrap();
        engine.set_position(START_FEN, &[]).unwrap();
        assert_eq!(engine.position().fen(), START_FEN);
    }

    #[test]
    fn test_engine_set_position_with_moves() {
        let mut engine = Engine::without_nnue().unwrap();
        engine.set_position(START_FEN, &["b0c2", "b9c7"]).unwrap();
        let fen = engine.position().fen();
        assert_ne!(fen, START_FEN);
    }

    #[test]
    fn test_engine_set_position_invalid_fen() {
        let mut engine = Engine::without_nnue().unwrap();
        assert!(engine.set_position("invalid fen", &[]).is_err());
    }

    #[test]
    fn test_engine_set_position_illegal_move() {
        let mut engine = Engine::without_nnue().unwrap();
        assert!(engine.set_position(START_FEN, &["e0e5"]).is_err());
    }

    #[test]
    fn test_engine_go_finds_legal_move() {
        let mut engine = Engine::without_nnue().unwrap();
        let limits = SearchLimits {
            depth: Some(1),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits).wait();
        assert_ne!(result.best_move, Move::NONE);
        assert!(result.depth >= 1);
    }

    #[test]
    fn test_engine_stop() {
        let mut engine = Engine::without_nnue().unwrap();
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
        let mut engine = Engine::without_nnue().unwrap();
        let limits = SearchLimits {
            depth: Some(1),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits).wait();
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
        let mut engine = Engine::without_nnue().unwrap();
        let limits = SearchLimits {
            depth: Some(3),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits).wait();
        assert_ne!(result.best_move, Move::NONE);
        assert!(result.depth >= 3, "should reach at least depth 3");
        assert!(result.nodes > 0, "should search some nodes");

        // Verify legality
        let pos = engine.position();
        assert!(pos.is_legal(result.best_move));
    }

    #[test]
    fn test_search_depth5_completes() {
        let mut engine = Engine::without_nnue().unwrap();
        let limits = SearchLimits {
            depth: Some(5),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits).wait();
        assert_ne!(result.best_move, Move::NONE);
        assert!(result.depth >= 5);
        assert!(result.nodes > 100, "depth 5 should search many nodes");
    }

    #[test]
    fn test_search_midgame_position() {
        let mut engine = Engine::without_nnue().unwrap();
        let fen = "r1ba1a3/4kn3/2n1b4/pNp1p1p1p/4c4/6P2/P1P2R2P/1CcC5/9/2BAKAB2 w - - 0 1";
        engine.set_position(fen, &[]).unwrap();

        let limits = SearchLimits {
            depth: Some(3),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits).wait();
        assert_ne!(result.best_move, Move::NONE);
        assert!(result.best_move.is_ok());
    }

    #[test]
    fn test_search_after_moves() {
        let mut engine = Engine::without_nnue().unwrap();
        engine.set_position(START_FEN, &["b0c2", "b9c7"]).unwrap();

        let limits = SearchLimits {
            depth: Some(3),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits).wait();
        assert_ne!(result.best_move, Move::NONE);
    }

    #[test]
    fn test_search_node_limited() {
        let mut engine = Engine::without_nnue().unwrap();
        let limits = SearchLimits {
            nodes: Some(1000),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits).wait();
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
        let mut engine = Engine::without_nnue().unwrap();
        let limits = SearchLimits {
            depth: Some(2),
            ..SearchLimits::default()
        };

        // First search
        let r1 = engine.go(&limits).wait();
        assert_ne!(r1.best_move, Move::NONE);

        // Second search on same position should also work
        let r2 = engine.go(&limits).wait();
        assert_ne!(r2.best_move, Move::NONE);
    }

    #[test]
    fn test_search_new_game_then_search() {
        let mut engine = Engine::without_nnue().unwrap();

        // Search once
        let limits = SearchLimits {
            depth: Some(2),
            ..SearchLimits::default()
        };
        let _ = engine.go(&limits).wait();

        // New game, then search again
        engine.new_game().unwrap();
        let result = engine.go(&limits).wait();
        assert_ne!(result.best_move, Move::NONE);
    }

    #[test]
    fn test_search_endgame_position() {
        let mut engine = Engine::without_nnue().unwrap();
        let fen = "5a3/3k5/3aR4/9/5r3/5n3/9/3A1A3/5K3/2BC2B2 w - - 0 1";
        engine.set_position(fen, &[]).unwrap();

        let limits = SearchLimits {
            depth: Some(3),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits).wait();
        assert_ne!(result.best_move, Move::NONE);
    }

    #[test]
    fn test_search_score_is_bounded() {
        let mut engine = Engine::without_nnue().unwrap();
        let limits = SearchLimits {
            depth: Some(5),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits).wait();
        // Score should be within valid range
        assert!(
            result.score > VALUE_MATED_IN_MAX_PLY - 100
                && result.score < VALUE_MATE_IN_MAX_PLY + 100,
            "score {} should be in reasonable range",
            result.score
        );
    }

    // -------------------------------------------------------------------
    // SearchHandle tests
    // -------------------------------------------------------------------

    #[test]
    fn test_search_handle_stop_terminates_search() {
        let mut engine = Engine::without_nnue().unwrap();
        let limits = SearchLimits {
            depth: Some(100),
            ..SearchLimits::default()
        };
        let handle = engine.go(&limits);
        handle.stop();
        let result = handle.wait();
        assert_ne!(result.best_move, Move::NONE);
    }

    #[test]
    fn test_search_handle_try_recv_none_during_search() {
        let mut engine = Engine::without_nnue().unwrap();
        let limits = SearchLimits {
            depth: Some(100),
            ..SearchLimits::default()
        };
        let handle = engine.go(&limits);
        let immediate = handle.try_recv();
        assert!(
            immediate.is_none(),
            "deep search should not complete instantly"
        );
        handle.stop();
        let _ = handle.wait();
    }

    #[test]
    fn test_search_handle_wait_after_completion() {
        let mut engine = Engine::without_nnue().unwrap();
        let limits = SearchLimits {
            depth: Some(1),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits).wait();
        assert_ne!(result.best_move, Move::NONE);
    }

    #[test]
    fn test_search_handle_ponder_waits_for_stop() {
        let mut engine = Engine::without_nnue().unwrap();
        let limits = SearchLimits {
            depth: Some(1),
            ponder: true,
            ..SearchLimits::default()
        };
        let handle = engine.go(&limits);
        std::thread::sleep(std::time::Duration::from_millis(200));
        let poll = handle.try_recv();
        assert!(poll.is_none(), "ponder search should not return until stop");
        handle.stop();
        let result = handle.wait();
        assert_ne!(result.best_move, Move::NONE);
    }

    #[test]
    fn test_search_handle_ponderhit_transitions_to_normal() {
        let mut engine = Engine::without_nnue().unwrap();
        let limits = SearchLimits {
            depth: Some(1),
            ponder: true,
            ..SearchLimits::default()
        };
        let handle = engine.go(&limits);
        std::thread::sleep(std::time::Duration::from_millis(100));
        handle.ponderhit();
        let result = handle.wait();
        assert_ne!(result.best_move, Move::NONE);
    }

    #[test]
    fn test_search_handle_infinite_waits_for_stop() {
        let mut engine = Engine::without_nnue().unwrap();
        let limits = SearchLimits {
            infinite: true,
            ..SearchLimits::default()
        };
        let handle = engine.go(&limits);
        std::thread::sleep(std::time::Duration::from_millis(200));
        let poll = handle.try_recv();
        assert!(
            poll.is_none(),
            "infinite search should not return until stop"
        );
        handle.stop();
        let result = handle.wait();
        assert_ne!(result.best_move, Move::NONE);
    }

    #[test]
    fn test_thread_pool_drop_stops_active_search() {
        let mut engine = Engine::without_nnue().unwrap();
        let limits = SearchLimits {
            depth: Some(100),
            ..SearchLimits::default()
        };
        let _handle = engine.go(&limits);
        drop(engine);
    }

    #[test]
    fn test_consecutive_go_stops_previous() {
        let mut engine = Engine::without_nnue().unwrap();
        let limits1 = SearchLimits {
            depth: Some(100),
            ..SearchLimits::default()
        };
        let _handle1 = engine.go(&limits1);

        let limits2 = SearchLimits {
            depth: Some(1),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits2).wait();
        assert_ne!(result.best_move, Move::NONE);
    }

    #[test]
    fn test_worker_recovery_after_search() {
        let mut engine = Engine::without_nnue().unwrap();
        let limits = SearchLimits {
            depth: Some(3),
            ..SearchLimits::default()
        };
        let r1 = engine.go(&limits).wait();
        assert_ne!(r1.best_move, Move::NONE);

        let r2 = engine.go(&limits).wait();
        assert_ne!(r2.best_move, Move::NONE);
    }

    #[test]
    fn test_search_handle_no_legal_moves() {
        let mut engine = Engine::without_nnue().unwrap();
        let fen = "3k5/9/3R5/4R4/9/9/9/9/9/4K4 b - - 0 1";
        if engine.set_position(fen, &[]).is_ok() {
            let limits = SearchLimits {
                depth: Some(1),
                ..SearchLimits::default()
            };
            let result = engine.go(&limits).wait();
            assert!(
                result.best_move == Move::NONE || result.best_move.is_ok(),
                "should return NONE or a valid move"
            );
        }
    }
    #[test]
    fn explicit_model_loading_fails_without_fallback() {
        assert!(matches!(
            Engine::with_nnue_file("__nonexistent_pikarust_model__.nnue"),
            Err(EngineError::Nnue(_))
        ));
        assert!(!Engine::without_nnue().unwrap().has_nnue());
    }

    #[test]
    fn stale_handle_cannot_cancel_a_new_search() {
        let mut engine = Engine::without_nnue().unwrap();
        let first = engine.go(&SearchLimits {
            depth: Some(1),
            ..SearchLimits::default()
        });
        let second = engine.go(&SearchLimits {
            depth: Some(1),
            ponder: true,
            ..SearchLimits::default()
        });
        assert!(!Arc::ptr_eq(&first.stop, &second.stop));
        first.stop();
        drop(first);
        assert!(!second.stop.load(Ordering::SeqCst));
        second.ponderhit();
        assert_ne!(second.wait().best_move, Move::NONE);
    }

    #[test]
    fn dropping_handle_requests_cancellation() {
        let mut engine = Engine::without_nnue().unwrap();
        let handle = engine.go(&SearchLimits {
            infinite: true,
            ..SearchLimits::default()
        });
        let stopped = Arc::clone(&handle.stop);
        drop(handle);
        assert!(stopped.load(Ordering::SeqCst));
        // Reusing the engine joins the cancelled search before starting another.
        assert_ne!(
            engine
                .go(&SearchLimits {
                    depth: Some(1),
                    ..SearchLimits::default()
                })
                .wait()
                .best_move,
            Move::NONE
        );
    }

    #[test]
    fn validated_search_restricts_root_moves() {
        let mut engine = Engine::without_nnue().unwrap();
        let mut limits = SearchLimits {
            depth: Some(2),
            search_moves: vec!["b0c2".to_owned()],
            ..SearchLimits::default()
        };
        assert_eq!(
            engine.try_go(&limits).unwrap().wait().best_move.to_string(),
            "b0c2"
        );
        limits.search_moves = vec!["a0a9".to_owned()];
        assert!(matches!(
            engine.try_go(&limits),
            Err(EngineError::IllegalMove(_))
        ));
        limits.search_moves.clear();
        limits.depth = Some(0);
        assert!(matches!(
            engine.try_go(&limits),
            Err(EngineError::InvalidLimits(_))
        ));
    }

    #[test]
    fn untrusted_move_input_must_pass_piece_movement_rules() {
        let mut engine = Engine::without_nnue().unwrap();
        for movement in ["a0a9", "a1a2", "a9a8", "a0b0", "b0b3", "e0e0", "a0e9"] {
            assert!(
                matches!(
                    engine.set_position(START_FEN, &[movement]),
                    Err(EngineError::IllegalMove(_))
                ),
                "{movement}"
            );
            assert_eq!(engine.position().fen(), START_FEN);
        }
    }
    #[test]
    fn clock_conversion_distinguishes_absent_and_exhausted_clocks() {
        assert_eq!(convert_limits(&SearchLimits::default()).time, [0, 0]);
        for (time, expected) in [
            ([Some(0), Some(0)], [1, 1]),
            ([Some(0), None], [1, 1]),
            ([None, Some(5000)], [1, 5000]),
            ([Some(5000), None], [5000, 1]),
        ] {
            let internal = convert_limits(&SearchLimits {
                time,
                ..SearchLimits::default()
            });
            assert_eq!(internal.time, expected);
            assert!(internal.use_time_management());
        }
    }

    #[test]
    fn exhausted_and_missing_side_clocks_finish_without_stop() {
        let mut engine = Engine::without_nnue().unwrap();
        for (black, time) in [
            (false, [Some(0), Some(0)]),
            (false, [Some(0), None]),
            (false, [None, Some(5000)]),
            (true, [Some(5000), None]),
        ] {
            let fen = if black {
                START_FEN.replace(" w ", " b ")
            } else {
                START_FEN.to_owned()
            };
            engine.set_position(&fen, &[]).unwrap();
            let handle = engine
                .try_go(&SearchLimits {
                    time,
                    ..SearchLimits::default()
                })
                .unwrap();
            let result = handle
                .rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .expect("an exhausted clock must complete without an external stop");
            assert_ne!(result.best_move, Move::NONE);
            assert!(engine.position().is_legal_move(result.best_move));
        }
    }
    #[test]
    fn worker_failure_is_reported_and_next_search_recovers() {
        for threads in [1, 2] {
            let mut engine = Engine::without_nnue().unwrap();
            engine.set_option("Threads", &threads.to_string()).unwrap();
            engine.ensure_thread_pool();
            engine.thread_pool.as_mut().unwrap().inject_worker_failure();
            let limits = SearchLimits {
                depth: Some(1),
                ..SearchLimits::default()
            };
            let handle = engine.try_go(&limits).unwrap();
            assert!(
                matches!(handle.wait_result(), Err(EngineError::SearchFailed)),
                "{threads} workers"
            );
            let result = engine.try_go(&limits).unwrap().wait_result().unwrap();
            assert_ne!(result.best_move, Move::NONE);
            assert!(engine.position().is_legal_move(result.best_move));
        }
    }

    #[test]
    fn polling_reports_worker_failure_instead_of_pending_forever() {
        let mut engine = Engine::without_nnue().unwrap();
        engine.ensure_thread_pool();
        engine.thread_pool.as_mut().unwrap().inject_worker_failure();
        let handle = engine
            .try_go(&SearchLimits {
                depth: Some(1),
                ..SearchLimits::default()
            })
            .unwrap();
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match handle.try_result() {
                Err(EngineError::SearchFailed) => break,
                Ok(None) => {
                    assert!(Instant::now() < deadline, "failed search remained pending");
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                other => panic!("expected an explicit search failure, got {other:?}"),
            }
        }
    }
    #[test]
    fn changing_pool_options_joins_an_active_search() {
        let mut engine = Engine::without_nnue().unwrap();
        let active = engine.go(&SearchLimits {
            infinite: true,
            ..SearchLimits::default()
        });
        engine.set_option("Hash", "32").unwrap();
        assert!(active.stop.load(Ordering::SeqCst));
        let completed = active
            .try_result()
            .unwrap()
            .expect("pool replacement must join its previous search");
        assert!(engine.position().is_legal_move(completed.best_move));
        assert_eq!(engine.options().hash_mb, 32);
    }

    #[test]
    fn nnue_pool_reconfiguration_matches_fresh_engine_without_new_game() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models/pikafish.nnue");
        let model = NnueModel::load(&path).expect(
            "NNUE regression requires the pinned model; run git lfs pull --include=models/pikafish.nnue"
        );
        let network = Arc::new(Network::new(model));
        let limits = SearchLimits {
            depth: Some(5),
            ..SearchLimits::default()
        };
        let mut fresh = Engine::with_network(Arc::clone(&network)).unwrap();
        fresh.set_option("Hash", "32").unwrap();
        let expected = fresh.try_go(&limits).unwrap().wait_result().unwrap();
        drop(fresh);

        let mut reconfigured = Engine::with_network(network).unwrap();
        let _ = reconfigured.try_go(&limits).unwrap().wait_result().unwrap();
        reconfigured.set_option("Hash", "32").unwrap();
        reconfigured.set_position(START_FEN, &[]).unwrap();
        let after_hash_change = reconfigured.try_go(&limits).unwrap().wait_result().unwrap();
        assert_eq!(
            after_hash_change, expected,
            "Hash replacement must initialize the same histories as a fresh pool"
        );

        // Compare at one thread: lazy-SMP node totals intentionally depend on
        // scheduling, so the intermediate two-thread pool is rebuilt again.
        reconfigured.set_option("Threads", "2").unwrap();
        reconfigured.set_option("Threads", "1").unwrap();
        reconfigured.set_position(START_FEN, &[]).unwrap();
        let after_thread_change = reconfigured.try_go(&limits).unwrap().wait_result().unwrap();
        assert_eq!(
            after_thread_change, expected,
            "Threads replacement must initialize the same histories as a fresh pool"
        );
    }
}
