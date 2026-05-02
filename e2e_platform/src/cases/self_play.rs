use std::time::Instant;

use crate::cases::{TestCase, TestOutcome};
use crate::config::E2eConfig;
use crate::error::E2eResult;
use crate::match_driver::driver::run_match;
use crate::match_driver::match_config::MatchConfig;
use crate::referee::game_result::GameResult;

/// Tests `PikaRust` vs `PikaRust` self-play: full game with move legality validation.
pub struct SelfPlayTest;

impl TestCase for SelfPlayTest {
    fn name(&self) -> &'static str {
        "self_play"
    }

    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome> {
        let start = Instant::now();

        let match_config = MatchConfig {
            white_name: "PikaRust-W".to_owned(),
            white_bin: config.pikarust_bin.clone(),
            white_cwd: config.pikarust_cwd.clone(),
            black_name: "PikaRust-B".to_owned(),
            black_bin: config.pikarust_bin.clone(),
            black_cwd: config.pikarust_cwd.clone(),
            search_depth: config.self_play_depth,
            max_moves: config.max_game_moves,
            response_timeout: config.search_timeout,
        };

        let record = run_match(&match_config)?;

        let passed = !matches!(record.result, GameResult::EngineError { .. });

        let detail = format!("{}, {} moves", record.result, record.move_count);

        Ok(TestOutcome {
            name: self.name().to_owned(),
            passed,
            duration: start.elapsed(),
            detail,
        })
    }
}
