use std::io;

use thiserror::Error;

/// Top-level error type for the E2E platform.
#[derive(Debug, Error)]
pub enum E2eError {
    /// Engine process failed to start or communicate.
    #[error("engine error ({engine}): {message}")]
    Engine {
        /// Which engine produced the error.
        engine: String,
        /// Description of the failure.
        message: String,
    },

    /// UCI protocol violation.
    #[error("UCI protocol error ({engine}): expected {expected}, got {actual}")]
    Protocol {
        /// Which engine violated the protocol.
        engine: String,
        /// What was expected.
        expected: String,
        /// What was actually received.
        actual: String,
    },

    /// Timeout waiting for engine response.
    #[error("timeout ({engine}): no response within {timeout_ms}ms for {context}")]
    Timeout {
        /// Which engine timed out.
        engine: String,
        /// How long we waited.
        timeout_ms: u64,
        /// What we were waiting for.
        context: String,
    },

    /// Move legality violation detected by referee.
    #[error("illegal move: engine {engine} played {uci_move} in position {fen}")]
    IllegalMove {
        /// Which engine played the illegal move.
        engine: String,
        /// The illegal move string.
        uci_move: String,
        /// The FEN of the position.
        fen: String,
    },

    /// Pre-flight check failure.
    #[error("preflight: {0}")]
    Preflight(String),

    /// IO error.
    #[error("io: {0}")]
    Io(#[from] io::Error),

    /// Position/FEN parsing error.
    #[error("position: {0}")]
    Position(String),
}

/// Alias for E2E results.
pub type E2eResult<T> = Result<T, E2eError>;
