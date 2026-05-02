pub mod cross_engine;
pub mod eval_equivalence;
pub mod search_basic;
pub mod self_play;
pub mod uci_compliance;

use std::time::{Duration, Instant};

use crate::config::E2eConfig;
use crate::error::E2eResult;

/// Outcome of a single test case.
#[derive(Debug, Clone)]
pub struct TestOutcome {
    /// Test name.
    pub name: String,
    /// Whether the test passed.
    pub passed: bool,
    /// How long the test took.
    pub duration: Duration,
    /// Human-readable detail (pass reason or failure message).
    pub detail: String,
}

/// Trait for all E2E test cases.
pub trait TestCase: Send + Sync {
    /// Human-readable test name.
    fn name(&self) -> &'static str;

    /// Whether this test requires the Pikafish binary.
    fn requires_pikafish(&self) -> bool {
        false
    }

    /// Run the test, returning the outcome.
    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome>;
}

/// Run a test case and capture timing, converting errors to failed outcomes.
pub fn execute_case(case: &dyn TestCase, config: &E2eConfig) -> TestOutcome {
    let start = Instant::now();
    match case.run(config) {
        Ok(outcome) => outcome,
        Err(e) => TestOutcome {
            name: case.name().to_owned(),
            passed: false,
            duration: start.elapsed(),
            detail: e.to_string(),
        },
    }
}

/// Registry of all test cases.
pub fn all_cases() -> Vec<Box<dyn TestCase>> {
    vec![
        Box::new(uci_compliance::UciComplianceTest),
        Box::new(search_basic::SearchDepthTest),
        Box::new(search_basic::SearchMovetimeTest),
        Box::new(search_basic::SearchStopTest),
        Box::new(eval_equivalence::EvalEquivalenceTest),
        Box::new(self_play::SelfPlayTest),
        Box::new(cross_engine::CrossEngineTest),
    ]
}
