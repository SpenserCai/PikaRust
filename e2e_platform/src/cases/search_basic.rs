use std::time::{Duration, Instant};

use crate::cases::{TestCase, TestOutcome};
use crate::config::E2eConfig;
use crate::error::E2eResult;
use crate::harness::engine_process::EngineProcess;
use crate::harness::uci_io;
use crate::referee::game_state::GameState;

/// Tests fixed-depth search from startpos.
pub struct SearchDepthTest;

impl TestCase for SearchDepthTest {
    fn name(&self) -> &'static str {
        "search_depth"
    }

    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome> {
        let start = Instant::now();
        let timeout = config.search_timeout;

        let mut engine = EngineProcess::spawn(
            "PikaRust",
            &config.pikarust_bin,
            &config.pikarust_cwd,
            timeout,
        )?;

        uci_io::uci_handshake(&mut engine, config.default_timeout)?;
        uci_io::set_option(&mut engine, "Threads", "1")?;
        uci_io::sync_engine(&mut engine, config.default_timeout)?;

        uci_io::set_position(&mut engine, None, &[])?;
        let (bm, _infos) = uci_io::go_depth(&mut engine, 5, timeout)?;

        let mut state = GameState::new()?;
        state.apply_uci_move(&bm.best_move, "PikaRust")?;

        engine.quit()?;

        Ok(TestOutcome {
            name: self.name().to_owned(),
            passed: true,
            duration: start.elapsed(),
            detail: format!("bestmove={}", bm.best_move),
        })
    }
}

/// Tests movetime search.
pub struct SearchMovetimeTest;

impl TestCase for SearchMovetimeTest {
    fn name(&self) -> &'static str {
        "search_movetime"
    }

    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome> {
        let start = Instant::now();
        let timeout = config.search_timeout;

        let mut engine = EngineProcess::spawn(
            "PikaRust",
            &config.pikarust_bin,
            &config.pikarust_cwd,
            timeout,
        )?;

        uci_io::uci_handshake(&mut engine, config.default_timeout)?;
        uci_io::set_option(&mut engine, "Threads", "1")?;
        uci_io::sync_engine(&mut engine, config.default_timeout)?;

        uci_io::set_position(&mut engine, None, &[])?;
        let (bm, _infos) = uci_io::go_movetime(&mut engine, 1000, timeout)?;

        let elapsed = start.elapsed();
        let mut state = GameState::new()?;
        state.apply_uci_move(&bm.best_move, "PikaRust")?;

        engine.quit()?;

        let within_time = elapsed < Duration::from_secs(5);

        Ok(TestOutcome {
            name: self.name().to_owned(),
            passed: within_time,
            duration: elapsed,
            detail: format!(
                "bestmove={}, elapsed={:.1}s",
                bm.best_move,
                elapsed.as_secs_f64()
            ),
        })
    }
}

/// Tests go infinite + stop.
pub struct SearchStopTest;

impl TestCase for SearchStopTest {
    fn name(&self) -> &'static str {
        "search_stop"
    }

    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome> {
        let start = Instant::now();
        let timeout = config.search_timeout;

        let mut engine = EngineProcess::spawn(
            "PikaRust",
            &config.pikarust_bin,
            &config.pikarust_cwd,
            timeout,
        )?;

        uci_io::uci_handshake(&mut engine, config.default_timeout)?;
        uci_io::set_option(&mut engine, "Threads", "1")?;
        uci_io::sync_engine(&mut engine, config.default_timeout)?;

        uci_io::set_position(&mut engine, None, &[])?;
        uci_io::go_infinite(&mut engine)?;

        std::thread::sleep(Duration::from_millis(500));

        let (bm, _infos) = uci_io::stop_and_collect(&mut engine, timeout)?;

        let mut state = GameState::new()?;
        state.apply_uci_move(&bm.best_move, "PikaRust")?;

        engine.quit()?;

        Ok(TestOutcome {
            name: self.name().to_owned(),
            passed: true,
            duration: start.elapsed(),
            detail: format!("bestmove={}", bm.best_move),
        })
    }
}
