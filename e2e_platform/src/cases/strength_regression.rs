//! A reproducible paired non-inferiority experiment against a previous build.
//! Confidence bounds concern this opening distribution and search budget only.
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use pikarust_core::position::{GenType, Position, generate};

use crate::cases::{TestCase, TestOutcome};
use crate::config::E2eConfig;
use crate::error::{E2eError, E2eResult};
use crate::match_driver::driver::run_match;
use crate::match_driver::match_config::{MatchConfig, SearchMode};
use crate::referee::game_result::GameResult;
use crate::referee::game_state::GameState;

const START: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";

fn parameter<T: std::str::FromStr>(name: &str, fallback: T) -> E2eResult<T> {
    std::env::var(name).map_or(Ok(fallback), |value| {
        value
            .parse()
            .map_err(|_| E2eError::Preflight(format!("invalid {name}: {value}")))
    })
}

const fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// Generate distinct legal openings once, then use both colors for every FEN.
fn openings(count: usize, seed: u64) -> E2eResult<Vec<String>> {
    let mut rng = seed;
    let mut result = Vec::with_capacity(count);
    let mut unique_positions = HashSet::new();
    for _ in 0..count.saturating_mul(100) {
        let mut position =
            Position::from_fen(START).map_err(|error| E2eError::Position(error.to_string()))?;
        let plies = 8 + random(&mut rng) % 17;
        for _ in 0..plies {
            let mut moves: Vec<_> = generate(&position, GenType::Legal).as_slice().to_vec();
            if moves.is_empty() {
                break;
            }
            moves.sort_by_key(ToString::to_string);
            let mv = moves[(random(&mut rng) % moves.len() as u64) as usize];
            let gives_check = position.gives_check(mv);
            position.do_move(mv, gives_check);
        }
        let fen = position.fen();
        if GameState::from_fen(&fen)?
            .check_game_end("candidate", "baseline")
            .is_some()
            || !unique_positions.insert(fen.clone())
        {
            continue;
        }
        result.push(fen);
        if result.len() == count {
            return Ok(result);
        }
    }
    Err(E2eError::Preflight(
        "unable to generate enough distinct nonterminal openings".to_owned(),
    ))
}

fn score(result: &GameResult) -> E2eResult<f64> {
    match result {
        GameResult::Checkmate { winner }
        | GameResult::Stalemate { winner }
        | GameResult::RuleViolation { winner } => Ok(if winner == "candidate" { 1.0 } else { 0.0 }),
        GameResult::Draw { .. } | GameResult::MaxMovesReached { .. } => Ok(0.5),
        GameResult::EngineError { engine, message } => Err(E2eError::Engine {
            engine: engine.clone(),
            message: message.clone(),
        }),
    }
}

fn game(
    config: &E2eConfig,
    baseline: &Path,
    fen: &str,
    candidate_red: bool,
    nodes: u64,
    max_moves: u32,
) -> E2eResult<crate::match_driver::driver::MatchRecord> {
    let candidate = ("candidate", &config.pikarust_bin, &config.pikarust_cwd);
    let baseline = ("baseline", baseline, config.baseline_cwd.as_path());
    let (red, black) = if candidate_red {
        (
            (candidate.0, candidate.1.as_path(), candidate.2.as_path()),
            baseline,
        )
    } else {
        (
            baseline,
            (candidate.0, candidate.1.as_path(), candidate.2.as_path()),
        )
    };
    run_match(&MatchConfig {
        white_name: red.0.to_owned(),
        white_bin: red.1.to_path_buf(),
        white_cwd: red.2.to_path_buf(),
        black_name: black.0.to_owned(),
        black_bin: black.1.to_path_buf(),
        black_cwd: black.2.to_path_buf(),
        search_mode: SearchMode::Nodes(nodes),
        max_moves,
        response_timeout: config.search_timeout,
        hash_mb: 16,
        nnue_model: Some(config.nnue_model.clone()),
        start_fen: Some(fen.to_owned()),
    })
}

/// Freeze executables and the model before starting a long experiment. A build
/// in another terminal cannot change the participants halfway through a match.
fn prepare_experiment(
    config: &E2eConfig,
    baseline: &Path,
    candidate_hash: &str,
    baseline_hash: &str,
) -> E2eResult<(E2eConfig, std::io::BufWriter<std::fs::File>)> {
    let report = std::env::var_os("PIKARUST_E2E_REPORT").map_or_else(
        || config.pikarust_cwd.join("target/e2e/report.json"),
        PathBuf::from,
    );
    let directory = report
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("strength-binaries");
    std::fs::create_dir_all(directory.join("models"))?;
    let directory = directory.canonicalize()?;
    let candidate_copy = directory.join(format!("candidate-{candidate_hash}"));
    let baseline_copy = directory.join(format!("baseline-{baseline_hash}"));
    let model_copy = directory.join("models/pikafish.nnue");
    for (source, destination, expected) in [
        (
            &config.pikarust_bin as &Path,
            candidate_copy.as_path(),
            candidate_hash.to_owned(),
        ),
        (baseline, baseline_copy.as_path(), baseline_hash.to_owned()),
        (
            config.nnue_model.as_path(),
            model_copy.as_path(),
            crate::checks::preflight::locked_value("PIKAFISH_NNUE_SHA256")
                .unwrap_or_default()
                .to_owned(),
        ),
    ] {
        if !destination.exists() && source != destination {
            std::fs::copy(source, destination)?;
        }
        if crate::checks::preflight::sha256(destination)? != expected {
            return Err(E2eError::Preflight(
                "input changed while freezing strength participants".to_owned(),
            ));
        }
    }
    let frozen = E2eConfig {
        pikarust_bin: candidate_copy,
        pikarust_cwd: directory.clone(),
        baseline_bin: Some(baseline_copy),
        baseline_cwd: directory,
        nnue_model: model_copy,
        ..config.clone()
    };
    let progress =
        std::io::BufWriter::new(std::fs::File::create(report.with_extension("games.jsonl"))?);
    Ok((frozen, progress))
}

fn run_pairs(
    config: &E2eConfig,
    positions: &[String],
    nodes: u64,
    max_moves: u32,
    jobs: usize,
    progress: std::io::BufWriter<std::fs::File>,
) -> E2eResult<Vec<serde_json::Value>> {
    let progress = Mutex::new(progress);
    let cancelled = AtomicBool::new(false);
    let baseline = config.baseline_bin.as_deref().expect("prepared baseline");
    let mut observations = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for worker in 0..jobs.min(positions.len()) {
            let progress = &progress;
            let cancelled = &cancelled;
            handles.push(scope.spawn(move || {
                let result = (|| -> E2eResult<Vec<(usize, serde_json::Value)>> {
                    let mut results = Vec::new();
                    for (index, fen) in positions.iter().enumerate().skip(worker).step_by(jobs) {
                        if cancelled.load(Ordering::Relaxed) { break; }
                        let red = game(config, baseline, fen, true, nodes, max_moves)?;
                        let black = game(config, baseline, fen, false, nodes, max_moves)?;
                        let pair_score = 0.5_f64.mul_add(score(&red.result)?, 0.5 * score(&black.result)?);
                        let capped = usize::from(matches!(red.result, GameResult::MaxMovesReached { .. }))
                            + usize::from(matches!(black.result, GameResult::MaxMovesReached { .. }));
                        let observation = serde_json::json!({"index":index,"fen":fen,"pair_score":pair_score,"capped_games":capped,
                            "candidate_red":{"result":red.result.to_string(),"moves":red.move_history,"final_fen":red.final_fen},
                            "candidate_black":{"result":black.result.to_string(),"moves":black.move_history,"final_fen":black.final_fen}});
                        let mut writer = progress.lock().map_err(|_| E2eError::Engine { engine:"harness".to_owned(),message:"progress writer poisoned".to_owned() })?;
                        writeln!(writer,"{observation}")?;
                        writer.flush()?;
                        drop(writer);
                        log::info!("strength pair {}/{}: score={pair_score:.2}; red={}, black={}", index + 1, positions.len(), red.result, black.result);
                        results.push((index, observation));
                    }
                    Ok(results)
                })();
                if result.is_err() { cancelled.store(true, Ordering::Relaxed); }
                result
            }));
        }
        let mut results = Vec::new();
        for handle in handles {
            results.extend(handle.join().map_err(|_| E2eError::Engine {
                engine: "harness".to_owned(),
                message: "strength worker panicked".to_owned(),
            })??);
        }
        Ok::<_, E2eError>(results)
    })?;
    observations.sort_by_key(|(index, _)| *index);
    Ok(observations.into_iter().map(|(_, value)| value).collect())
}

/// Distribution-free one-sided 95% Hoeffding lower bound on paired score.
fn lower_bound(mean: f64, pairs: usize) -> f64 {
    (mean - (20.0_f64.ln() / (2.0 * pairs as f64)).sqrt()).max(0.0)
}

pub struct StrengthRegression;
impl TestCase for StrengthRegression {
    fn name(&self) -> &'static str {
        "strength_regression"
    }
    fn is_slow(&self) -> bool {
        true
    }
    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome> {
        let start = Instant::now();
        let baseline = config.baseline_bin.as_deref().ok_or_else(|| {
            E2eError::Preflight(
                "PIKARUST_BASELINE_BIN is required; baseline must be a distinct previous build"
                    .to_owned(),
            )
        })?;
        if !baseline.is_file() || baseline.canonicalize()? == config.pikarust_bin.canonicalize()? {
            return Err(E2eError::Preflight(
                "baseline binary must exist and differ from candidate".to_owned(),
            ));
        }
        let candidate_hash = crate::checks::preflight::sha256(&config.pikarust_bin)?;
        let baseline_hash = crate::checks::preflight::sha256(baseline)?;
        if candidate_hash == baseline_hash {
            return Err(E2eError::Preflight(
                "candidate and baseline have identical binary SHA-256; use distinct builds"
                    .to_owned(),
            ));
        }
        let pairs: usize = parameter("PIKARUST_STRENGTH_PAIRS", 200)?;
        let nodes: u64 = parameter("PIKARUST_STRENGTH_NODES", 2000)?;
        let max_moves: u32 = parameter("PIKARUST_STRENGTH_MAX_MOVES", 150)?;
        let jobs: usize = parameter("PIKARUST_STRENGTH_JOBS", 4)?;
        let minimum: f64 = parameter("PIKARUST_STRENGTH_MIN_SCORE", 0.40)?;
        let seed: u64 = parameter("PIKARUST_STRENGTH_SEED", 20_260_924)?;
        if pairs < 2
            || nodes == 0
            || max_moves == 0
            || !(0.0..1.0).contains(&minimum)
            || seed == 0
            || !(1..=8).contains(&jobs)
        {
            return Err(E2eError::Preflight(
                "invalid strength experiment parameters".to_owned(),
            ));
        }
        let (frozen, mut progress) =
            prepare_experiment(config, baseline, &candidate_hash, &baseline_hash)?;
        writeln!(
            progress,
            "{}",
            serde_json::json!({"kind":"experiment","pairs":pairs,"nodes":nodes,"max_moves":max_moves,
            "seed":seed,"minimum_score":minimum,"candidate_sha256":candidate_hash,"baseline_sha256":baseline_hash,
            "nnue_sha256":crate::checks::preflight::sha256(&frozen.nnue_model)?})
        )?;
        progress.flush()?;
        let observations = run_pairs(
            &frozen,
            &openings(pairs, seed)?,
            nodes,
            max_moves,
            jobs,
            progress,
        )?;
        let total: f64 = observations
            .iter()
            .filter_map(|value| value["pair_score"].as_f64())
            .sum();
        let capped: u64 = observations
            .iter()
            .filter_map(|value| value["capped_games"].as_u64())
            .sum();
        let mean = total / pairs as f64;
        let lower = lower_bound(mean, pairs);
        let upper = 1.0 - lower_bound(1.0 - mean, pairs);
        // Predominantly capped games provide no reliable evidence of strength.
        let enough_results = capped <= pairs as u64;
        let passed = lower >= minimum && enough_results;
        let status = if passed {
            "noninferiority_supported"
        } else if upper < minimum {
            "regression"
        } else {
            "inconclusive"
        };
        Ok(TestOutcome { name: self.name().to_owned(), passed, duration: start.elapsed(), detail: serde_json::json!({
            "status":status, "candidate":config.pikarust_bin, "baseline":baseline,
            "frozen_candidate":frozen.pikarust_bin,"frozen_baseline":frozen.baseline_bin,
            "candidate_sha256":candidate_hash,
            "baseline_sha256":baseline_hash,
            "nnue_sha256":crate::checks::preflight::sha256(&config.nnue_model)?,
            "pairs":pairs,"jobs":jobs,"seed":seed,"nodes_per_move":nodes,"max_moves":max_moves,
            "score":mean,"minimum_score":minimum,"lower_95_one_sided":lower,"upper_95_one_sided":upper,
            "method":"Hoeffding bound on independent sampled opening-pair scores; fixed seed for replay",
            "scope":"this opening generator, move cap and node budget only; no universal Elo claim",
            "capped_games":capped,"maximum_capped_fraction":0.5,"games":observations,
        }).to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tiny_draw_sample_cannot_prove_noninferiority() {
        assert!(lower_bound(0.5, 30) < 0.4);
        assert!(lower_bound(0.5, 200) > 0.4);
    }
    #[test]
    fn opening_sample_is_replayable_and_diverse() {
        let sample = openings(8, 20_260_924).unwrap();
        assert_eq!(sample, openings(8, 20_260_924).unwrap());
        assert_eq!(sample.iter().collect::<HashSet<_>>().len(), 8);
    }
}
