use std::fmt;

/// Outcome of a completed game.
#[derive(Debug, Clone)]
pub enum GameResult {
    /// One side wins by checkmate.
    Checkmate {
        /// Name of the winning engine.
        winner: String,
    },
    /// Draw by a specific rule.
    Draw {
        /// Why the game was drawn.
        reason: DrawReason,
    },
    /// Game exceeded maximum move limit.
    MaxMovesReached {
        /// Number of full moves played.
        move_count: u32,
    },
    /// Engine crashed or protocol error during game.
    EngineError {
        /// Which engine failed.
        engine: String,
        /// What went wrong.
        message: String,
    },
}

/// Reason for a draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawReason {
    /// Position repeated (3-fold or perpetual).
    Repetition,
    /// 60-move rule (120 half-moves without capture/pawn move).
    SixtyMoveRule,
    /// Stalemate (no legal moves, not in check).
    Stalemate,
}

impl fmt::Display for GameResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Checkmate { winner } => write!(f, "{winner} wins by checkmate"),
            Self::Draw { reason } => write!(f, "draw by {reason}"),
            Self::MaxMovesReached { move_count } => {
                write!(f, "draw by max moves ({move_count})")
            }
            Self::EngineError { engine, message } => {
                write!(f, "engine error ({engine}): {message}")
            }
        }
    }
}

impl fmt::Display for DrawReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repetition => f.write_str("repetition"),
            Self::SixtyMoveRule => f.write_str("60-move rule"),
            Self::Stalemate => f.write_str("stalemate"),
        }
    }
}
