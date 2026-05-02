use std::time::Instant;

use crate::cases::{TestCase, TestOutcome};
use crate::config::E2eConfig;
use crate::error::E2eResult;
use crate::match_driver::driver::run_match;
use crate::match_driver::match_config::MatchConfig;
use crate::referee::game_result::GameResult;

/// Tests `PikaRust` vs Pikafish cross-engine play with move legality validation.
pub struct CrossEngineTest;

impl TestCase for CrossEngineTest {
    fn name(&self) -> &'static str {
        "cross_engine"
    }

    fn requires_pikafish(&self) -> bool {
        true
    }

    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome> {
        let start = Instant::now();

        let match_config = MatchConfig {
            white_name: "PikaRust".to_owned(),
            white_bin: config.pikarust_bin.clone(),
            white_cwd: config.pikarust_cwd.clone(),
            black_name: "Pikafish".to_owned(),
            black_bin: config.pikafish_bin.clone(),
            black_cwd: config.pikafish_cwd.clone(),
            search_depth: config.cross_engine_depth,
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
