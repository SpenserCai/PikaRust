use thiserror::Error;

use crate::position::{FenError, Position};
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
}

impl Engine {
    pub fn new() -> Result<Self, EngineError> {
        let position = Position::from_fen(START_FEN)?;
        Ok(Self {
            options: EngineOptions::default(),
            position,
        })
    }

    pub fn set_option(&mut self, name: &str, value: &str) -> Result<(), EngineError> {
        self.options.set(name, value)?;
        Ok(())
    }

    pub const fn options(&self) -> &EngineOptions {
        &self.options
    }

    pub fn new_game(&mut self) -> Result<(), EngineError> {
        self.position = Position::from_fen(START_FEN)?;
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

    pub const fn go(&self, _limits: &SearchLimits) -> SearchResult {
        SearchResult {
            best_move: Move::NONE,
            ponder_move: None,
            score: 0,
            depth: 0,
            nodes: 0,
        }
    }

    pub const fn stop(&self) {}

    pub fn uci_options() -> Vec<UciOption> {
        EngineOptions::uci_options()
    }
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
    fn test_engine_go_placeholder() {
        let engine = Engine::new().unwrap();
        let result = engine.go(&SearchLimits::default());
        assert_eq!(result.best_move, Move::NONE);
        assert_eq!(result.score, 0);
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
