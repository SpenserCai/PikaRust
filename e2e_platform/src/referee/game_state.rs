use pikarust_core::position::rule_judge::RuleJudgeResult;
use pikarust_core::position::{GenType, Position, generate};
use pikarust_core::types::{File, Move, Rank, Square, VALUE_DRAW};

use crate::error::{E2eError, E2eResult};

use super::game_result::{DrawReason, GameResult};

const START_FEN: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";

/// Uses the core rules to adjudicate tests. This is not an independent referee;
/// exact upstream perft checks separately validate legal move generation.
pub struct GameState {
    position: Position,
    move_history: Vec<String>,
}

impl GameState {
    /// Create a new game from the starting position.
    pub fn new() -> E2eResult<Self> {
        Self::from_fen(START_FEN)
    }

    /// Create from a specific FEN.
    pub fn from_fen(fen: &str) -> E2eResult<Self> {
        let position = Position::from_fen(fen).map_err(|e| E2eError::Position(e.to_string()))?;
        Ok(Self {
            position,
            move_history: Vec::new(),
        })
    }

    /// Validate and apply a UCI move string (e.g., "h2e2").
    /// Returns an error if the move is illegal.
    pub fn apply_uci_move(&mut self, uci_move: &str, engine_name: &str) -> E2eResult<()> {
        let m = parse_uci_move(&self.position, uci_move).ok_or_else(|| E2eError::IllegalMove {
            engine: engine_name.to_owned(),
            uci_move: uci_move.to_owned(),
            fen: self.position.fen(),
        })?;

        let gives_check = self.position.gives_check(m);
        self.position.do_move(m, gives_check);
        self.move_history.push(uci_move.to_owned());
        Ok(())
    }

    /// Check if the game has ended (checkmate, stalemate, draw).
    /// `white_name` and `black_name` are used to identify the winner.
    pub fn check_game_end(&mut self, white_name: &str, black_name: &str) -> Option<GameResult> {
        let legal_moves = generate(&self.position, GenType::Legal);

        if legal_moves.is_empty() {
            let winner = if self.position.side_to_move() == pikarust_core::types::Color::White {
                black_name.to_owned()
            } else {
                white_name.to_owned()
            };
            return Some(if self.position.checkers().is_not_empty() {
                GameResult::Checkmate { winner }
            } else {
                GameResult::Stalemate { winner }
            });
        }

        if let RuleJudgeResult::Definitive(value) = self.position.rule_judge(0) {
            if value == VALUE_DRAW {
                let reason = if self.position.rule60_count() >= 120 {
                    DrawReason::SixtyMoveRule
                } else {
                    DrawReason::Repetition
                };
                return Some(GameResult::Draw { reason });
            }
            let white_wins = (value > VALUE_DRAW)
                == (self.position.side_to_move() == pikarust_core::types::Color::White);
            return Some(GameResult::RuleViolation {
                winner: if white_wins {
                    white_name.to_owned()
                } else {
                    black_name.to_owned()
                },
            });
        }

        None
    }

    /// Current FEN string.
    pub fn fen(&self) -> String {
        self.position.fen()
    }

    /// Current side to move.
    pub const fn side_to_move(&self) -> pikarust_core::types::Color {
        self.position.side_to_move()
    }

    /// Number of half-moves played.
    pub fn half_move_count(&self) -> usize {
        self.move_history.len()
    }

    /// Full move history as UCI strings.
    pub fn move_history(&self) -> &[String] {
        &self.move_history
    }
}

/// Parse a 4-character UCI move string into a `Move`, checking legality.
/// Replicates the logic from `engine/core.rs:parse_uci_move`.
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

    let from = Square::make(
        File::try_from(from_file).ok()?,
        Rank::try_from(from_rank).ok()?,
    );
    let to = Square::make(File::try_from(to_file).ok()?, Rank::try_from(to_rank).ok()?);
    let m = Move::make(from, to);

    if pos.is_legal_move(m) { Some(m) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xiangqi_stalemate_is_a_loss() {
        // Red rook on d8 controls d9/e8; f8 controls f9, without checking e9.
        let mut game = GameState::from_fen("4k4/3R1R3/9/9/4P4/9/9/9/9/4K4 b - - 0 1").unwrap();
        assert!(game.position.checkers().is_empty());
        assert!(
            matches!(game.check_game_end("red", "black"), Some(GameResult::Stalemate { winner }) if winner == "red")
        );
    }
    #[test]
    fn referee_rejects_non_pseudo_legal_moves_without_panicking() {
        for mv in ["b0b5", "a9a8", "a0b0", "a1a2"] {
            let mut game = GameState::new().unwrap();
            assert!(game.apply_uci_move(mv, "fixture").is_err(), "{mv}");
        }
    }
}
