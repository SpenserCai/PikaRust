use std::time::Instant;

use crate::cases::{TestCase, TestOutcome};
use crate::config::E2eConfig;
use crate::error::E2eResult;
use crate::harness::engine_process::EngineProcess;
use crate::harness::uci_io;

/// Test positions for equivalence comparison.
const TEST_POSITIONS: &[(&str, &str)] = &[
    (
        "startpos",
        "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1",
    ),
    (
        "midgame",
        "r1ba1a3/4kn3/2n1b4/pNp1p1p1p/4c4/6P2/P1P2R2P/1CcC5/9/2BAKAB2 w - - 0 1",
    ),
    ("endgame", "2bak4/4a4/4b4/9/9/3R5/9/4B4/4A4/2BAK4 w - - 0 1"),
    (
        "complex",
        "r2akab2/9/1cn1b1n2/p1p1p3p/6p2/2P6/P3P1c1P/N1C1C1N2/9/R1BAKAB1R w - - 0 1",
    ),
];

/// Tests evaluation score equivalence between `PikaRust` and Pikafish.
pub struct EvalEquivalenceTest;

impl TestCase for EvalEquivalenceTest {
    fn name(&self) -> &'static str {
        "eval_equivalence"
    }

    fn requires_pikafish(&self) -> bool {
        true
    }

    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome> {
        let start = Instant::now();
        let timeout = config.search_timeout;
        let depth = config.equiv_depth;

        let mut pikarust = EngineProcess::spawn(
            "PikaRust",
            &config.pikarust_bin,
            &config.pikarust_cwd,
            timeout,
        )?;
        let mut pikafish = EngineProcess::spawn(
            "Pikafish",
            &config.pikafish_bin,
            &config.pikafish_cwd,
            timeout,
        )?;

        uci_io::uci_handshake(&mut pikarust, config.default_timeout)?;
        uci_io::uci_handshake(&mut pikafish, config.default_timeout)?;

        uci_io::set_option(&mut pikarust, "Threads", "1")?;
        uci_io::set_option(&mut pikarust, "Hash", "16")?;
        uci_io::set_option(&mut pikafish, "Threads", "1")?;
        uci_io::set_option(&mut pikafish, "Hash", "16")?;

        uci_io::sync_engine(&mut pikarust, config.default_timeout)?;
        uci_io::sync_engine(&mut pikafish, config.default_timeout)?;

        let mut diffs = Vec::new();
        let mut max_diff: i32 = 0;

        for (pos_name, fen) in TEST_POSITIONS {
            uci_io::new_game(&mut pikarust)?;
            uci_io::new_game(&mut pikafish)?;
            uci_io::sync_engine(&mut pikarust, config.default_timeout)?;
            uci_io::sync_engine(&mut pikafish, config.default_timeout)?;

            uci_io::set_position(&mut pikarust, Some(fen), &[])?;
            uci_io::set_position(&mut pikafish, Some(fen), &[])?;

            let (_bm_pr, pikarust_infos) = uci_io::go_depth(&mut pikarust, depth, timeout)?;
            let (_bm_pf, pikafish_infos) = uci_io::go_depth(&mut pikafish, depth, timeout)?;

            let pikarust_score = find_deepest_score(&pikarust_infos, depth);
            let pikafish_score = find_deepest_score(&pikafish_infos, depth);

            let diff = match (pikarust_score, pikafish_score) {
                (Some(a), Some(b)) => (a - b).abs(),
                _ => -1,
            };

            if diff > max_diff {
                max_diff = diff;
            }

            diffs.push(format!(
                "{pos_name}: pikarust={}, pikafish={}, diff={diff}",
                pikarust_score.map_or_else(|| "?".to_owned(), |v| v.to_string()),
                pikafish_score.map_or_else(|| "?".to_owned(), |v| v.to_string()),
            ));
        }

        pikarust.quit()?;
        pikafish.quit()?;

        // Allow tolerance of ±5cp for minor implementation differences
        let tolerance = 5;
        let passed = max_diff <= tolerance;

        let detail = if passed {
            format!("max score diff: {max_diff}cp (tolerance: ±{tolerance}cp)")
        } else {
            format!(
                "FAILED: max diff {max_diff}cp exceeds ±{tolerance}cp\n  {}",
                diffs.join("\n  ")
            )
        };

        Ok(TestOutcome {
            name: self.name().to_owned(),
            passed,
            duration: start.elapsed(),
            detail,
        })
    }
}

/// Find the score from the deepest info line at or near the target depth.
fn find_deepest_score(
    infos: &[crate::harness::uci_io::InfoLine],
    target_depth: u32,
) -> Option<i32> {
    // First try exact depth match
    for info in infos.iter().rev() {
        if info.depth == Some(target_depth) {
            if let Some(cp) = info.score_cp {
                return Some(cp);
            }
            if let Some(mate) = info.score_mate {
                return Some(mate * 10000);
            }
        }
    }
    // Fallback: deepest available
    for info in infos.iter().rev() {
        if info.score_cp.is_some() || info.score_mate.is_some() {
            if let Some(cp) = info.score_cp {
                return Some(cp);
            }
            if let Some(mate) = info.score_mate {
                return Some(mate * 10000);
            }
        }
    }
    None
}
