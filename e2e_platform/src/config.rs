use std::path::{Path, PathBuf};
use std::time::Duration;

/// Central configuration for the E2E platform.
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
    /// Search depth for equivalence tests.
    pub equiv_depth: u32,
    /// Search depth for self-play.
    pub self_play_depth: u32,
    /// Search depth for cross-engine play.
    pub cross_engine_depth: u32,
}

impl E2eConfig {
    /// Build config from the project root path.
    pub fn from_project_root(root: &Path) -> Self {
        Self {
            pikarust_bin: root.join("target/release/pikarust"),
            pikarust_cwd: root.to_path_buf(),
            pikafish_bin: root.join("tests/fixtures/pikafish/bin/pikafish"),
            pikafish_cwd: root.join("tests/fixtures/pikafish/bin"),
            nnue_model: root.join("models/pikafish.nnue"),
            default_timeout: Duration::from_secs(10),
            search_timeout: Duration::from_secs(60),
            max_game_moves: 200,
            equiv_depth: 5,
            self_play_depth: 4,
            cross_engine_depth: 6,
        }
    }
}
