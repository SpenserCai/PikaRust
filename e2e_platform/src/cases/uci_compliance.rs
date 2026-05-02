use std::time::Instant;

use crate::cases::{TestCase, TestOutcome};
use crate::config::E2eConfig;
use crate::error::E2eResult;
use crate::harness::engine_process::EngineProcess;
use crate::harness::uci_io;

/// Tests UCI protocol compliance: handshake, options, isready.
pub struct UciComplianceTest;

impl TestCase for UciComplianceTest {
    fn name(&self) -> &'static str {
        "uci_compliance"
    }

    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome> {
        let start = Instant::now();
        let timeout = config.default_timeout;

        let mut engine = EngineProcess::spawn(
            "PikaRust",
            &config.pikarust_bin,
            &config.pikarust_cwd,
            timeout,
        )?;

        let lines = uci_io::uci_handshake(&mut engine, timeout)?;

        let has_id_name = lines.iter().any(|l| l.starts_with("id name"));
        let has_uciok = lines.iter().any(|l| l.trim() == "uciok");

        if !has_id_name || !has_uciok {
            engine.quit()?;
            return Ok(TestOutcome {
                name: self.name().to_owned(),
                passed: false,
                duration: start.elapsed(),
                detail: format!("missing id name ({has_id_name}) or uciok ({has_uciok})"),
            });
        }

        uci_io::set_option(&mut engine, "Hash", "32")?;
        uci_io::sync_engine(&mut engine, timeout)?;

        uci_io::set_option(&mut engine, "Threads", "1")?;
        uci_io::sync_engine(&mut engine, timeout)?;

        uci_io::new_game(&mut engine)?;
        uci_io::sync_engine(&mut engine, timeout)?;

        engine.quit()?;

        Ok(TestOutcome {
            name: self.name().to_owned(),
            passed: true,
            duration: start.elapsed(),
            detail: "handshake, setoption, ucinewgame all OK".to_owned(),
        })
    }
}
