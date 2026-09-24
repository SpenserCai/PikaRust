use std::path::{Path, PathBuf};
use std::time::Duration;

/// Central configuration for the E2E platform.
#[derive(Clone)]
pub struct E2eConfig {
    /// Path to the `PikaRust` binary.
    pub pikarust_bin: PathBuf,
    /// Working directory for `PikaRust` (project root, so it finds models/).
    pub pikarust_cwd: PathBuf,
    /// Path to the Pikafish binary.
    pub pikafish_bin: PathBuf,
    /// Working directory for Pikafish (its bin dir, so it finds pikafish.nnue).
    pub pikafish_cwd: PathBuf,
    /// Path to the NNUE model file.
    pub nnue_model: PathBuf,
    /// Default timeout for UCI responses.
    pub default_timeout: Duration,
    /// Timeout for search operations.
    pub search_timeout: Duration,
    /// Maximum full moves per game before declaring draw.
    pub max_game_moves: u32,
    /// Fixed-node budget for search diagnostics.
    pub comparison_nodes: u64,
    /// Optional previous `PikaRust` build for strength regression games.
    pub baseline_bin: Option<PathBuf>,
    /// Working directory for the baseline engine.
    pub baseline_cwd: PathBuf,
    /// Search depth for self-play.
    pub self_play_depth: u32,
    /// Search depth for cross-engine play.
    pub cross_engine_depth: u32,
}

impl E2eConfig {
    /// Build config from the project root path.
    pub fn from_project_root(root: &Path) -> Self {
        Self {
            pikarust_bin: std::env::var_os("PIKARUST_BIN")
                .map_or_else(|| root.join("target/release/pikarust"), PathBuf::from),
            pikarust_cwd: root.to_path_buf(),
            pikafish_bin: root.join("tests/fixtures/pikafish/bin/pikafish"),
            pikafish_cwd: root.join("tests/fixtures/pikafish/bin"),
            nnue_model: std::env::var_os("PIKARUST_NNUE_MODEL")
                .map_or_else(|| root.join("models/pikafish.nnue"), PathBuf::from),
            default_timeout: Duration::from_secs(10),
            search_timeout: Duration::from_secs(60),
            max_game_moves: 200,
            comparison_nodes: 10_000,
            baseline_bin: std::env::var_os("PIKARUST_BASELINE_BIN").map(PathBuf::from),
            baseline_cwd: std::env::var_os("PIKARUST_BASELINE_CWD")
                .map_or_else(|| root.to_path_buf(), PathBuf::from),
            self_play_depth: 6,
            cross_engine_depth: 8,
        }
    }
}
