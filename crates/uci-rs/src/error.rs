//! UCI protocol error types.

use thiserror::Error;

/// Errors that can occur during UCI command parsing.
#[derive(Debug, Clone, Error)]
pub enum UciError {
    #[error("unknown command: {0}")]
    UnknownCommand(String),

    #[error("invalid position: {0}")]
    InvalidPosition(String),

    #[error("parse error: {0}")]
    ParseError(String),
}
