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
        println!("  [{status}] {:<30} ({ms:>6}ms)  {}", t.name, t.detail);
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

/// Returns true if all tests passed.
pub fn all_passed(report: &RunReport) -> bool {
    let preflight_ok = report.preflight_results.iter().all(|r| r.passed);
    let tests_ok = report.test_outcomes.iter().all(|t| t.passed);
    preflight_ok && tests_ok
}
