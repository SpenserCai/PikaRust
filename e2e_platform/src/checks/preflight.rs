use std::path::Path;
use std::time::Duration;

use crate::config::E2eConfig;
use crate::error::E2eResult;
use crate::harness::engine_process::EngineProcess;
use crate::harness::uci_io;

/// Result of a single pre-flight check.
#[derive(Debug, Clone)]
pub struct PreflightResult {
    /// Name of the check.
    pub name: String,
    /// Whether it passed.
    pub passed: bool,
    /// Human-readable detail.
    pub detail: String,
}

/// Run all pre-flight checks. Returns the list of results.
/// Critical failures (missing binaries/models) are included as failed checks.
pub fn run_preflight(config: &E2eConfig) -> Vec<PreflightResult> {
    let mut results = Vec::new();

    results.push(check_file("PikaRust binary", &config.pikarust_bin));
    results.push(check_file("Pikafish binary", &config.pikafish_bin));
    results.push(check_file("NNUE model", &config.nnue_model));

    if results.iter().any(|r| !r.passed) {
        return results;
    }

    results.push(check_engine_handshake(
        "PikaRust",
        &config.pikarust_bin,
        &config.pikarust_cwd,
        config.default_timeout,
    ));
    results.push(check_engine_handshake(
        "Pikafish",
        &config.pikafish_bin,
        &config.pikafish_cwd,
        config.default_timeout,
    ));

    results
}

/// Check that a file exists.
fn check_file(name: &str, path: &Path) -> PreflightResult {
    if path.exists() {
        PreflightResult {
            name: format!("{name} exists"),
            passed: true,
            detail: path.display().to_string(),
        }
    } else {
        PreflightResult {
            name: format!("{name} exists"),
            passed: false,
            detail: format!("not found: {}", path.display()),
        }
    }
}

/// Spawn engine, do UCI handshake, verify readyok.
fn check_engine_handshake(
    name: &str,
    binary: &Path,
    cwd: &Path,
    timeout: Duration,
) -> PreflightResult {
    let result = do_handshake_check(name, binary, cwd, timeout);
    match result {
        Ok(id_line) => PreflightResult {
            name: format!("{name} UCI handshake"),
            passed: true,
            detail: id_line,
        },
        Err(e) => PreflightResult {
            name: format!("{name} UCI handshake"),
            passed: false,
            detail: e.to_string(),
        },
    }
}

/// Internal: perform handshake and return the "id name" line.
fn do_handshake_check(
    name: &str,
    binary: &Path,
    cwd: &Path,
    timeout: Duration,
) -> E2eResult<String> {
    let mut engine = EngineProcess::spawn(name, binary, cwd, timeout)?;
    let lines = uci_io::uci_handshake(&mut engine, timeout)?;
    uci_io::sync_engine(&mut engine, timeout)?;
    engine.quit()?;

    let id_line = lines
        .iter()
        .find(|l| l.starts_with("id name"))
        .cloned()
        .unwrap_or_else(|| "id name unknown".to_owned());

    Ok(id_line)
}
