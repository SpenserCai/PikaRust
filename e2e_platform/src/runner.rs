use crate::cases::{TestOutcome, all_cases, execute_case};
use crate::checks::preflight::{self, PreflightResult};
use crate::config::E2eConfig;

/// Aggregated results, including selection errors rather than vacuous success.
pub struct RunReport {
    pub preflight_results: Vec<PreflightResult>,
    pub test_outcomes: Vec<TestOutcome>,
}

/// Execute an explicitly selected suite. Empty selections fail before spawning.
pub fn run_all(config: &E2eConfig, filter: Option<&str>, suite: &str) -> RunReport {
    let cases = all_cases();
    let has_exact = filter.is_some_and(|filter| cases.iter().any(|case| case.name() == filter));
    let selected: Vec<_> = cases
        .iter()
        .filter(|case| {
            if let Some(filter) = filter {
                return if has_exact {
                    case.name() == filter
                } else {
                    case.name().contains(filter)
                };
            }
            match suite {
                "smoke" => matches!(
                    case.name(),
                    "uci_compliance" | "search_depth" | "search_movetime" | "search_stop"
                ),
                "alignment" => matches!(
                    case.name(),
                    "perft_equivalence"
                        | "nnue_equivalence"
                        | "search_comparison"
                        | "search_regression"
                ),
                "all" => !case.is_slow(),
                _ => false,
            }
        })
        .collect();
    if selected.is_empty() {
        return RunReport {
            preflight_results: vec![PreflightResult {
                name: "test selection".to_owned(),
                passed: false,
                detail: format!("no cases selected: suite={suite}, filter={filter:?}"),
            }],
            test_outcomes: Vec::new(),
        };
    }
    let needs_reference = selected.iter().any(|case| case.requires_pikafish());
    let preflight_results = preflight::run_preflight(config, needs_reference);
    if preflight_results.iter().any(|result| !result.passed) {
        return RunReport {
            preflight_results,
            test_outcomes: Vec::new(),
        };
    }
    let test_outcomes = selected
        .iter()
        .map(|case| {
            log::info!("running test: {}", case.name());
            let outcome = execute_case(case.as_ref(), config);
            log::info!(
                "{}: {} ({}ms)",
                outcome.name,
                if outcome.passed { "PASS" } else { "FAIL" },
                outcome.duration.as_millis()
            );
            outcome
        })
        .collect();
    RunReport {
        preflight_results,
        test_outcomes,
    }
}
