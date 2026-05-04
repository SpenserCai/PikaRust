use std::path::Path;
use std::time::Instant;

use crate::cases::{TestCase, TestOutcome};
use crate::config::E2eConfig;
use crate::error::E2eResult;
use crate::match_driver::driver::run_match;
use crate::match_driver::match_config::{MatchConfig, SearchMode};
use crate::referee::game_result::GameResult;

/// Standard Chinese chess opening positions for gauntlet diversity.
/// Uses valid positions from the bench suite to ensure correctness.
const OPENING_FENS: &[(&str, &str)] = &[
    // Startpos
    (
        "startpos",
        "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w",
    ),
    // Early midgame — open files
    (
        "midgame_open",
        "r1ba1a3/4kn3/2n1b4/pNp1p1p1p/4c4/6P2/P1P2R2P/1CcC5/9/2BAKAB2 w",
    ),
    // Midgame — tactical
    (
        "midgame_tactical",
        "1r1akabr1/1c7/2n1b1n2/p1p1p3p/6p2/PN3R3/1cP1P1P1P/2C1C1N2/1R7/2BAKAB2 b",
    ),
    // Complex midgame
    (
        "midgame_complex",
        "2bakab2/6r2/2n1c1nc1/p1p2rp1p/4p4/2PN2PC1/P3P3P/6N2/3CA4/1RBAK1B1R w",
    ),
    // Endgame
    (
        "endgame",
        "5a3/3k5/3aR4/9/5r3/5n3/9/3A1A3/5K3/2BC2B2 w",
    ),
];

/// Gauntlet search time per move (ms).
const MOVETIME_MS: u64 = 500;
/// Hash table size for gauntlet games.
const HASH_MB: u32 = 32;
/// Max moves per game.
const MAX_MOVES: u32 = 150;

/// Per-game result for statistics.
#[allow(dead_code)]
struct GameScore {
    opening: &'static str,
    white_name: String,
    result: GameResult,
    moves: u32,
}

impl GameScore {
    /// Score from the perspective of `engine_name`: 1.0 win, 0.5 draw, 0.0 loss.
    fn score_for(&self, engine_name: &str) -> f64 {
        match &self.result {
            GameResult::Checkmate { winner } => {
                if winner == engine_name { 1.0 } else { 0.0 }
            }
            GameResult::Draw { .. } | GameResult::MaxMovesReached { .. } => 0.5,
            GameResult::EngineError { engine, .. } => {
                if engine == engine_name { 0.0 } else { 1.0 }
            }
        }
    }
}

/// Compute Elo difference from score percentage.
/// `score` is in [0, 1]. Returns None if score is 0 or 1 (infinite Elo diff).
fn elo_diff(score: f64) -> Option<f64> {
    if score <= 0.0 || score >= 1.0 {
        return None;
    }
    Some(-400.0 * (1.0 / score - 1.0).log10())
}

/// Run a gauntlet: `white_engine` vs `opponent` across all openings, both colors.
fn run_gauntlet(
    white_name: &str,
    white_bin: &Path,
    white_cwd: &Path,
    opp_name: &str,
    opp_bin: &Path,
    opp_cwd: &Path,
    config: &E2eConfig,
) -> E2eResult<Vec<GameScore>> {
    let mut results = Vec::new();

    for &(opening_name, fen) in OPENING_FENS {
        // Game 1: first engine as White (Red)
        let cfg = MatchConfig {
            white_name: white_name.to_owned(),
            white_bin: white_bin.to_path_buf(),
            white_cwd: white_cwd.to_path_buf(),
            black_name: opp_name.to_owned(),
            black_bin: opp_bin.to_path_buf(),
            black_cwd: opp_cwd.to_path_buf(),
            search_mode: SearchMode::Movetime(MOVETIME_MS),
            max_moves: MAX_MOVES,
            response_timeout: config.search_timeout,
            hash_mb: HASH_MB,
            start_fen: Some(fen.to_owned()),
        };
        let record = run_match(&cfg)?;
        log::info!(
            "  {opening_name} ({white_name} as Red): {}, {} moves",
            record.result, record.move_count
        );
        results.push(GameScore {
            opening: opening_name,
            white_name: white_name.to_owned(),
            result: record.result,
            moves: record.move_count,
        });

        // Game 2: first engine as Black (swap colors)
        let cfg = MatchConfig {
            white_name: opp_name.to_owned(),
            white_bin: opp_bin.to_path_buf(),
            white_cwd: opp_cwd.to_path_buf(),
            black_name: white_name.to_owned(),
            black_bin: white_bin.to_path_buf(),
            black_cwd: white_cwd.to_path_buf(),
            search_mode: SearchMode::Movetime(MOVETIME_MS),
            max_moves: MAX_MOVES,
            response_timeout: config.search_timeout,
            hash_mb: HASH_MB,
            start_fen: Some(fen.to_owned()),
        };
        let record = run_match(&cfg)?;
        log::info!(
            "  {opening_name} ({white_name} as Black): {}, {} moves",
            record.result, record.move_count
        );
        results.push(GameScore {
            opening: opening_name,
            white_name: opp_name.to_owned(),
            result: record.result,
            moves: record.move_count,
        });
    }

    Ok(results)
}

/// Format gauntlet results into a summary string.
fn summarize(engine_a: &str, engine_b: &str, results: &[GameScore]) -> String {
    let mut wins = 0u32;
    let mut draws = 0u32;
    let mut losses = 0u32;
    let mut has_error = false;

    for g in results {
        if matches!(g.result, GameResult::EngineError { .. }) {
            has_error = true;
        }
        let s = g.score_for(engine_a);
        #[allow(clippy::float_cmp)]
        if s == 1.0 {
            wins += 1;
        } else if s == 0.0 {
            losses += 1;
        } else {
            draws += 1;
        }
    }

    let total = results.len() as f64;
    let score = 0.5f64.mul_add(f64::from(draws), f64::from(wins)) / total;
    let pct = score * 100.0;

    let elo_str = elo_diff(score).map_or_else(
        || "Elo: N/A".to_owned(),
        |e| format!("Elo diff: {e:+.0}"),
    );

    let error_note = if has_error { " [ENGINE ERROR]" } else { "" };

    format!(
        "{engine_a} vs {engine_b}: +{wins} ={draws} -{losses} ({pct:.1}%), {elo_str}{error_note}"
    )
}

// ---------------------------------------------------------------------------
// Test case: PikaRust vs Pikafish
// ---------------------------------------------------------------------------

pub struct StrengthGauntlet;

impl TestCase for StrengthGauntlet {
    fn name(&self) -> &'static str {
        "strength_gauntlet"
    }

    fn requires_pikafish(&self) -> bool {
        true
    }

    fn is_slow(&self) -> bool {
        true
    }

    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome> {
        let start = Instant::now();

        let results = run_gauntlet(
            "PikaRust",
            &config.pikarust_bin,
            &config.pikarust_cwd,
            "Pikafish",
            &config.pikafish_bin,
            &config.pikafish_cwd,
            config,
        )?;

        let has_error = results
            .iter()
            .any(|g| matches!(g.result, GameResult::EngineError { .. }));

        let detail = summarize("PikaRust", "Pikafish", &results);

        Ok(TestOutcome {
            name: self.name().to_owned(),
            passed: !has_error,
            duration: start.elapsed(),
            detail,
        })
    }
}

// ---------------------------------------------------------------------------
// Test case: PikaRust vs PikaRust (self-gauntlet, baseline sanity)
// ---------------------------------------------------------------------------

pub struct StrengthGauntletSelf;

impl TestCase for StrengthGauntletSelf {
    fn name(&self) -> &'static str {
        "strength_gauntlet_self"
    }

    fn is_slow(&self) -> bool {
        true
    }

    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome> {
        let start = Instant::now();

        let results = run_gauntlet(
            "PikaRust-A",
            &config.pikarust_bin,
            &config.pikarust_cwd,
            "PikaRust-B",
            &config.pikarust_bin,
            &config.pikarust_cwd,
            config,
        )?;

        let has_error = results
            .iter()
            .any(|g| matches!(g.result, GameResult::EngineError { .. }));

        let detail = summarize("PikaRust-A", "PikaRust-B", &results);

        Ok(TestOutcome {
            name: self.name().to_owned(),
            passed: !has_error,
            duration: start.elapsed(),
            detail,
        })
    }
}

// ---------------------------------------------------------------------------
// Test case: Pikafish vs Pikafish (reference baseline)
// ---------------------------------------------------------------------------

pub struct StrengthGauntletRef;

impl TestCase for StrengthGauntletRef {
    fn name(&self) -> &'static str {
        "strength_gauntlet_ref"
    }

    fn requires_pikafish(&self) -> bool {
        true
    }

    fn is_slow(&self) -> bool {
        true
    }

    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome> {
        let start = Instant::now();

        let results = run_gauntlet(
            "Pikafish-A",
            &config.pikafish_bin,
            &config.pikafish_cwd,
            "Pikafish-B",
            &config.pikafish_bin,
            &config.pikafish_cwd,
            config,
        )?;

        let has_error = results
            .iter()
            .any(|g| matches!(g.result, GameResult::EngineError { .. }));

        let detail = summarize("Pikafish-A", "Pikafish-B", &results);

        Ok(TestOutcome {
            name: self.name().to_owned(),
            passed: !has_error,
            duration: start.elapsed(),
            detail,
        })
    }
}
