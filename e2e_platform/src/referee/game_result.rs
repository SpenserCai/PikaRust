use std::fmt;

/// Outcome of a completed game.
#[derive(Debug, Clone)]
pub enum GameResult {
    /// One side wins by checkmate.
    Checkmate {
        /// Name of the winning engine.
        winner: String,
    },
    /// Xiangqi stalemate is a loss for the side with no legal moves.
    Stalemate { winner: String },
    /// A decisive repetition ruling (for example perpetual check).
    RuleViolation { winner: String },
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
}

impl fmt::Display for GameResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Checkmate { winner } => write!(f, "{winner} wins by checkmate"),
            Self::Stalemate { winner } => write!(f, "{winner} wins by stalemate"),
            Self::RuleViolation { winner } => write!(f, "{winner} wins by repetition ruling"),
            Self::Draw { reason } => write!(f, "draw by {reason}"),
            Self::MaxMovesReached { move_count } => {
                write!(f, "adjudicated draw at test move cap ({move_count})")
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
        }
    }
}
