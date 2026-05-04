use std::path::PathBuf;
use std::time::Duration;

/// How the engine should search each move.
#[derive(Clone)]
pub enum SearchMode {
    /// Fixed depth search.
    Depth(u32),
    /// Fixed time per move in milliseconds.
    Movetime(u64),
}

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
    /// How each engine searches.
    pub search_mode: SearchMode,
    /// Maximum number of full moves before declaring draw.
    pub max_moves: u32,
    /// Timeout for each engine response.
    pub response_timeout: Duration,
    /// Hash table size in MB.
    pub hash_mb: u32,
    /// Starting position FEN (None = startpos).
    pub start_fen: Option<String>,
}
