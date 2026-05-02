use crate::cases::{TestOutcome, all_cases, execute_case};
use crate::checks::preflight::{self, PreflightResult};
use crate::config::E2eConfig;

/// Aggregated results from a full E2E run.
pub struct RunReport {
    /// Pre-flight check results.
    pub preflight_results: Vec<PreflightResult>,
    /// Test case outcomes.
    pub test_outcomes: Vec<TestOutcome>,
}

/// Execute all E2E tests.
///
/// Runs preflight checks first. If critical checks fail, skips tests.
/// Optionally filters tests by name substring.
pub fn run_all(config: &E2eConfig, filter: Option<&str>) -> RunReport {
    let preflight_results = preflight::run_preflight(config);

    let critical_failed = preflight_results.iter().any(|r| !r.passed);
    if critical_failed {
        return RunReport {
            preflight_results,
            test_outcomes: Vec::new(),
        };
    }

    let pikafish_available = config.pikafish_bin.exists();
    let cases = all_cases();
    let mut test_outcomes = Vec::new();

    for case in &cases {
        if let Some(f) = filter {
            if !case.name().contains(f) {
                continue;
            }
        }

        if case.requires_pikafish() && !pikafish_available {
            test_outcomes.push(TestOutcome {
                name: case.name().to_owned(),
                passed: false,
                duration: std::time::Duration::ZERO,
                detail: "skipped: pikafish binary not available".to_owned(),
            });
            continue;
        }

        log::info!("running test: {}", case.name());
        let outcome = execute_case(case.as_ref(), config);
        log::info!(
            "  {} ({}ms)",
            if outcome.passed { "PASS" } else { "FAIL" },
            outcome.duration.as_millis()
        );
        test_outcomes.push(outcome);
    }

    RunReport {
        preflight_results,
        test_outcomes,
    }
}
