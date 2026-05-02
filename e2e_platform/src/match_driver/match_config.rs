use std::path::PathBuf;
use std::time::Duration;

/// Configuration for a single match between two engines.
pub struct MatchConfig {
    /// Display name for engine playing White (Red).
    pub white_name: String,
    /// Binary path for White engine.
    pub white_bin: PathBuf,
    /// Working directory for White engine.
    pub white_cwd: PathBuf,
    /// Display name for engine playing Black.
    pub black_name: String,
    /// Binary path for Black engine.
    pub black_bin: PathBuf,
    /// Working directory for Black engine.
    pub black_cwd: PathBuf,
    /// Search depth per move.
    pub search_depth: u32,
    /// Maximum number of full moves before declaring draw.
    pub max_moves: u32,
    /// Timeout for each engine response.
    pub response_timeout: Duration,
}
