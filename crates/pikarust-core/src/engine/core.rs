use std::time::Instant;

use thiserror::Error;

use crate::position::{FenError, Position};
use crate::search::ThreadPool;
use crate::search::time::SearchLimits as InternalSearchLimits;
use crate::types::{Depth, Move, Square, Value};

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

pub struct Engine {
    options: EngineOptions,
    position: Position,
    thread_pool: Option<ThreadPool>,
}

impl Engine {
    pub fn new() -> Result<Self, EngineError> {
        let position = Position::from_fen(START_FEN)?;
        Ok(Self {
            options: EngineOptions::default(),
            position,
            thread_pool: None,
        })
    }

    fn ensure_thread_pool(&mut self) {
        if self.thread_pool.is_none() {
            self.thread_pool = Some(ThreadPool::new(self.options.threads, self.options.hash_mb));
        }
    }

    pub fn set_option(&mut self, name: &str, value: &str) -> Result<(), EngineError> {
        let old_threads = self.options.threads;
        let old_hash = self.options.hash_mb;
        self.options.set(name, value)?;

        if self.thread_pool.is_some()
            && (self.options.threads != old_threads || self.options.hash_mb != old_hash)
        {
            self.thread_pool = Some(ThreadPool::new(self.options.threads, self.options.hash_mb));
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
        let tp = self.thread_pool.as_mut().expect("thread pool initialized");

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
}
