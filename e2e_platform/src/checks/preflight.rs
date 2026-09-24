use sha2::{Digest, Sha256};
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
pub fn run_preflight(config: &E2eConfig, needs_reference: bool) -> Vec<PreflightResult> {
    let mut results = Vec::new();

    results.push(check_file("PikaRust binary", &config.pikarust_bin));
    if needs_reference {
        results.push(check_file("Pikafish binary", &config.pikafish_bin));
    }
    results.push(check_file("NNUE model", &config.nnue_model));
    results.push(check_model(&config.nnue_model));
    if needs_reference {
        results.push(check_reference(config));
        results.push(check_model(&config.pikafish_cwd.join("pikafish.nnue")));
    }
    if config.baseline_bin.is_some() {
        results.push(check_model(
            &config.baseline_cwd.join("models/pikafish.nnue"),
        ));
    }

    if results.iter().any(|r| !r.passed) {
        return results;
    }

    results.push(check_engine_handshake(
        "PikaRust",
        &config.pikarust_bin,
        &config.pikarust_cwd,
        config.default_timeout,
    ));
    if needs_reference {
        results.push(check_engine_handshake(
            "Pikafish",
            &config.pikafish_bin,
            &config.pikafish_cwd,
            config.default_timeout,
        ));
    }

    results
}

/// Check that a file exists.
fn check_file(name: &str, path: &Path) -> PreflightResult {
    if path.is_file() {
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

pub(crate) fn locked_value(key: &str) -> Option<&'static str> {
    include_str!("../../../scripts/reference.lock")
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name == key).then_some(value)
        })
}

pub(crate) fn sha256(path: &Path) -> std::io::Result<String> {
    sha256_reader(std::fs::File::open(path)?)
}

fn sha256_reader(mut reader: impl std::io::Read) -> std::io::Result<String> {
    use std::fmt::Write;

    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let len = reader.read(&mut buffer)?;
        if len == 0 {
            break;
        }
        hash.update(&buffer[..len]);
    }
    let mut encoded = String::with_capacity(64);
    for byte in hash.finalize() {
        write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(encoded)
}

fn check_model(path: &Path) -> PreflightResult {
    let result = sha256(path);
    let expected = locked_value("PIKAFISH_NNUE_SHA256").unwrap_or("");
    let passed = result.as_ref().is_ok_and(|actual| actual == expected);
    PreflightResult {
        name: "NNUE SHA-256".to_owned(),
        passed,
        detail: format!(
            "{}: expected={expected}, actual={}",
            path.display(),
            result.unwrap_or_else(|error| error.to_string())
        ),
    }
}

fn check_reference(config: &E2eConfig) -> PreflightResult {
    let path = config
        .pikafish_cwd
        .parent()
        .unwrap_or(&config.pikafish_cwd)
        .join("reference.json");
    let verify = || -> Result<(), String> {
        let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
        let metadata: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        let expected_commit = locked_value("PIKAFISH_COMMIT").unwrap_or("");
        let expected_nnue = locked_value("PIKAFISH_NNUE_SHA256").unwrap_or("");
        let binary_hash = sha256(&config.pikafish_bin).map_err(|error| error.to_string())?;
        if metadata["commit"].as_str() != Some(expected_commit)
            || metadata["nnue_sha256"].as_str() != Some(expected_nnue)
            || metadata["binary_sha256"].as_str() != Some(binary_hash.as_str())
        {
            return Err(
                "reference metadata/binary mismatch; run scripts/setup-pikafish.sh".to_owned(),
            );
        }
        Ok(())
    };
    let result = verify();
    PreflightResult {
        name: "reference provenance".to_owned(),
        passed: result.is_ok(),
        detail: result.map_or_else(|error| error, |()| path.display().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::sha256_reader;
    use std::io::Read;

    #[test]
    fn sha256_matches_known_vectors_including_leading_zero_bytes() {
        for (input, expected) in [
            (
                "",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                "abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
        ] {
            assert_eq!(sha256_reader(input.as_bytes()).unwrap(), expected);
        }
        assert_eq!(
            sha256_reader(std::io::repeat(b'a').take(1_000_000)).unwrap(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }
}
