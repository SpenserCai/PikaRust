use crate::runner::RunReport;

/// Print a human-readable report to stdout.
pub fn print_report(report: &RunReport) {
    println!();
    println!("=== PikaRust E2E Test Report ===");
    println!();

    println!("Pre-flight Checks:");
    for r in &report.preflight_results {
        let status = if r.passed { "PASS" } else { "FAIL" };
        println!("  [{status}] {:<30} {}", r.name, r.detail);
    }
    println!();

    if report.test_outcomes.is_empty() {
        println!("No tests executed (pre-flight checks failed).");
        return;
    }

    println!("Test Results:");
    for t in &report.test_outcomes {
        let status = if t.passed { "PASS" } else { "FAIL" };
        let ms = t.duration.as_millis();
        println!(
            "  [{status}] {:<30} ({ms:>6}ms)  {}",
            t.name,
            summarize_detail(&t.detail)
        );
    }
    println!();

    let total = report.test_outcomes.len();
    let passed = report.test_outcomes.iter().filter(|t| t.passed).count();
    let failed = total - passed;
    let total_ms: u128 = report
        .test_outcomes
        .iter()
        .map(|t| t.duration.as_millis())
        .sum();

    println!(
        "Summary: {passed}/{total} passed, {failed} failed ({:.1}s total)",
        total_ms as f64 / 1000.0
    );
    println!();
}

fn summarize_detail(detail: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(detail) else {
        return detail.to_owned();
    };
    if let Some(status) = value["status"].as_str() {
        return format!(
            "{status}; pairs={}, score={}, lower95={}, minimum={}, capped={}",
            value["pairs"],
            value["score"],
            value["lower_95_one_sided"],
            value["minimum_score"],
            value["capped_games"]
        );
    }
    let positions = value["positions"].as_array();
    let count = positions.map_or(0, Vec::len);
    let differences: Vec<_> = positions
        .into_iter()
        .flatten()
        .filter(|position| {
            position["equal"] == false
                || position["repeat_identical"] == false
                || position["delta"].as_i64().is_some_and(|delta| delta != 0)
        })
        .filter_map(|position| position["position"].as_str())
        .collect();
    let inapplicable = positions
        .into_iter()
        .flatten()
        .filter(|position| position["applicable"] == false)
        .count();
    let snapshot = value["snapshot_matches"]
        .as_bool()
        .map_or_else(String::new, |matches| {
            format!("; snapshot_matches={matches}")
        });
    format!(
        "{}; positions={count}; inapplicable={inapplicable}; mismatches={differences:?}{snapshot} (full data in JSON report)",
        value["criterion"].as_str().unwrap_or("see report")
    )
}

/// Returns true if all tests passed.
pub fn all_passed(report: &RunReport) -> bool {
    let preflight_ok = report.preflight_results.iter().all(|r| r.passed);
    let tests_ok = report.test_outcomes.iter().all(|t| t.passed);
    preflight_ok && tests_ok && !report.test_outcomes.is_empty()
}

/// Machine-readable report is written even when a suite fails.
pub fn write_json(report: &RunReport, path: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let preflight: Vec<_> = report
        .preflight_results
        .iter()
        .map(|result| {
            serde_json::json!({
                "name":result.name, "passed":result.passed, "detail":result.detail,
            })
        })
        .collect();
    let tests: Vec<_> = report.test_outcomes.iter().map(|result| serde_json::json!({
        "name":result.name, "passed":result.passed, "duration_ms":result.duration.as_millis(),
        "detail":serde_json::from_str::<serde_json::Value>(&result.detail).unwrap_or_else(|_| serde_json::Value::String(result.detail.clone())),
    })).collect();
    let payload = serde_json::json!({
        "schema_version":1, "passed":all_passed(report),
        "reference_lock":include_str!("../../scripts/reference.lock"),
        "preflight":preflight, "tests":tests,
    });
    let bytes = serde_json::to_vec_pretty(&payload).map_err(std::io::Error::other)?;
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_execution_is_not_success() {
        assert!(!all_passed(&RunReport {
            preflight_results: vec![],
            test_outcomes: vec![]
        }));
    }
}
