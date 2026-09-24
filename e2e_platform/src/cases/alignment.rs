//! Independent conformance layers; search agreement is reported, never inferred
//! from a centipawn tolerance or treated as proof of playing strength.
use std::collections::BTreeMap;
use std::time::Instant;

use serde::Serialize;

use crate::cases::{TestCase, TestOutcome};
use crate::config::E2eConfig;
use crate::error::{E2eError, E2eResult};
use crate::harness::engine_process::EngineProcess;
use crate::harness::uci_io::{self, InfoLine};
use crate::referee::game_state::GameState;

pub fn positions() -> impl Iterator<Item = (&'static str, &'static str)> {
    include_str!("../../fixtures/positions.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| line.split_once('\t').expect("checked-in position fixture"))
}

fn engines(config: &E2eConfig) -> E2eResult<(EngineProcess, EngineProcess)> {
    let mut candidate = EngineProcess::spawn(
        "PikaRust",
        &config.pikarust_bin,
        &config.pikarust_cwd,
        config.default_timeout,
    )?;
    let mut reference = EngineProcess::spawn(
        "Pikafish",
        &config.pikafish_bin,
        &config.pikafish_cwd,
        config.default_timeout,
    )?;
    for engine in [&mut candidate, &mut reference] {
        uci_io::uci_handshake(engine, config.default_timeout)?;
        uci_io::set_option(engine, "Threads", "1")?;
        uci_io::set_option(engine, "Hash", "16")?;
        uci_io::sync_engine(engine, config.default_timeout)?;
    }
    uci_io::set_option(
        &mut reference,
        "EvalFile",
        &config.nnue_model.to_string_lossy(),
    )?;
    uci_io::sync_engine(&mut reference, config.default_timeout)?;
    Ok((candidate, reference))
}

fn reset(engine: &mut EngineProcess, fen: &str, config: &E2eConfig) -> E2eResult<()> {
    uci_io::new_game(engine)?;
    uci_io::sync_engine(engine, config.default_timeout)?;
    uci_io::set_position(engine, Some(fen), &[])
}

fn protocol(engine: &EngineProcess, expected: &str, lines: &[String]) -> E2eError {
    E2eError::Protocol {
        engine: engine.name().to_owned(),
        expected: expected.to_owned(),
        actual: lines.join("\n"),
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct PerftResult {
    total: u64,
    divide: BTreeMap<String, u64>,
}

fn perft(engine: &mut EngineProcess, depth: u32, config: &E2eConfig) -> E2eResult<PerftResult> {
    engine.send(&format!("go perft {depth}"))?;
    let lines = engine.read_until(
        |line| line.starts_with("Nodes searched:"),
        config.search_timeout,
    )?;
    let total = lines
        .last()
        .and_then(|line| line.strip_prefix("Nodes searched:"))
        .and_then(|value| value.trim().parse::<u64>().ok())
        .ok_or_else(|| protocol(engine, "Nodes searched: <integer>", &lines))?;
    let mut divide = BTreeMap::new();
    for line in &lines {
        let Some((mv, count)) = line.split_once(':') else {
            continue;
        };
        if mv.len() != 4 {
            continue;
        }
        let count = count
            .trim()
            .parse::<u64>()
            .map_err(|_| protocol(engine, "integer perft branch count", &lines))?;
        if divide.insert(mv.to_owned(), count).is_some() {
            return Err(protocol(engine, "each root move exactly once", &lines));
        }
    }
    if divide.is_empty()
        || divide
            .values()
            .try_fold(0_u64, |sum, count| sum.checked_add(*count))
            != Some(total)
    {
        return Err(protocol(
            engine,
            "nonempty perft divide whose sum equals total",
            &lines,
        ));
    }
    Ok(PerftResult { total, divide })
}

pub struct PerftEquivalence;
impl TestCase for PerftEquivalence {
    fn name(&self) -> &'static str {
        "perft_equivalence"
    }
    fn requires_pikafish(&self) -> bool {
        true
    }
    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome> {
        let start = Instant::now();
        let (mut candidate, mut reference) = engines(config)?;
        let mut observations = Vec::new();
        let mut passed = true;
        for (name, fen) in positions() {
            reset(&mut candidate, fen, config)?;
            reset(&mut reference, fen, config)?;
            let actual = perft(&mut candidate, 3, config)?;
            let expected = perft(&mut reference, 3, config)?;
            passed &= actual == expected;
            observations.push(serde_json::json!({"position":name, "depth":3, "candidate":actual, "reference":expected, "equal":actual == expected}));
        }
        candidate.quit()?;
        reference.quit()?;
        Ok(TestOutcome { name: self.name().to_owned(), passed, duration: start.elapsed(), detail: serde_json::json!({"criterion":"exact total and every root move count", "positions":observations}).to_string() })
    }
}

fn raw_eval(engine: &mut EngineProcess, config: &E2eConfig) -> E2eResult<Option<i32>> {
    engine.send("eval")?;
    engine.send("isready")?;
    let lines = engine.read_until(|line| line.trim() == "readyok", config.default_timeout)?;
    if lines
        .iter()
        .any(|line| line.trim() == "Final evaluation: none (in check)")
    {
        return Ok(None);
    }
    lines
        .iter()
        .find_map(|line| {
            let (_, tail) = line.split_once("NNUE evaluation")?;
            let value = tail.strip_suffix("(side to move, internal units)")?;
            value.trim().parse().ok()
        })
        .map(Some)
        .ok_or_else(|| {
            protocol(
                engine,
                "raw NNUE evaluation in side-to-move internal units",
                &lines,
            )
        })
}

pub struct NnueEquivalence;
impl TestCase for NnueEquivalence {
    fn name(&self) -> &'static str {
        "nnue_equivalence"
    }
    fn requires_pikafish(&self) -> bool {
        true
    }
    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome> {
        let start = Instant::now();
        let (mut candidate, mut reference) = engines(config)?;
        let mut passed = true;
        let mut observations = Vec::new();
        for (name, fen) in positions() {
            reset(&mut candidate, fen, config)?;
            reset(&mut reference, fen, config)?;
            let actual = raw_eval(&mut candidate, config)?;
            let expected = raw_eval(&mut reference, config)?;
            passed &= actual == expected;
            observations.push(serde_json::json!({"position":name, "candidate":actual, "reference":expected, "applicable":actual.is_some() && expected.is_some(), "delta":actual.zip(expected).map(|(a,b)| i64::from(a)-i64::from(b))}));
        }
        candidate.quit()?;
        reference.quit()?;
        Ok(TestOutcome { name: self.name().to_owned(), passed, duration: start.elapsed(), detail: serde_json::json!({"criterion":"exact raw NNUE integer; in-check traces are explicitly inapplicable; no search or centipawn conversion", "positions":observations}).to_string() })
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct SearchObservation {
    bestmove: String,
    final_info: InfoLine,
}

fn search(
    engine: &mut EngineProcess,
    fen: &str,
    config: &E2eConfig,
) -> E2eResult<SearchObservation> {
    reset(engine, fen, config)?;
    let (bestmove, infos) =
        uci_io::go_nodes(engine, config.comparison_nodes, config.search_timeout)?;
    observation(engine, fen, bestmove, infos)
}

fn observation(
    engine: &EngineProcess,
    fen: &str,
    bestmove: uci_io::BestMoveResponse,
    infos: Vec<InfoLine>,
) -> E2eResult<SearchObservation> {
    GameState::from_fen(fen)?.apply_uci_move(&bestmove.best_move, engine.name())?;
    let info = infos
        .into_iter()
        .rev()
        .find(|info| {
            !info.bounded
                && info.multipv.unwrap_or(1) == 1
                && info.depth.is_some_and(|depth| depth > 0)
                && (info.score_cp.is_some() ^ info.score_mate.is_some())
                && info.nodes.is_some_and(|nodes| nodes > 0)
                && !info.pv.is_empty()
        })
        .ok_or_else(|| {
            protocol(
                engine,
                "completed search info with depth, exact score, nodes, and PV",
                &[],
            )
        })?;
    let mut state = GameState::from_fen(fen)?;
    for mv in &info.pv {
        state.apply_uci_move(mv, engine.name())?;
    }
    Ok(SearchObservation {
        bestmove: bestmove.best_move,
        final_info: info,
    })
}

/// Validates reproducibility and legal output; agreement with upstream is a metric.
pub struct SearchComparison;
impl TestCase for SearchComparison {
    fn name(&self) -> &'static str {
        "search_comparison"
    }
    fn requires_pikafish(&self) -> bool {
        true
    }
    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome> {
        let start = Instant::now();
        let (mut candidate, mut reference) = engines(config)?;
        let mut passed = true;
        let mut observations = Vec::new();
        for (name, fen) in positions() {
            let actual = search(&mut candidate, fen, config)?;
            let repeated = search(&mut candidate, fen, config)?;
            let expected = search(&mut reference, fen, config)?;
            passed &= actual == repeated;
            observations.push(serde_json::json!({"position":name, "candidate":actual, "reference":expected,
                "repeat_identical":actual == repeated, "bestmove_equal": actual.bestmove == expected.bestmove,
                "score_equal":actual.final_info.score_cp == expected.final_info.score_cp && actual.final_info.score_mate == expected.final_info.score_mate}));
        }
        candidate.quit()?;
        reference.quit()?;
        Ok(TestOutcome { name: self.name().to_owned(), passed, duration: start.elapsed(), detail: serde_json::json!({"criterion":"legal output and deterministic candidate replay; upstream search equality is NOT a pass criterion", "nodes":config.comparison_nodes, "positions":observations}).to_string() })
    }
}

/// Collect a candidate-only fixed-depth snapshot. Updating the checked-in
/// baseline is a deliberate reviewed change, separate from normal test runs.
pub fn record_search_baseline(config: &E2eConfig) -> E2eResult<serde_json::Value> {
    let engine_name = if config.pikarust_bin == config.pikafish_bin {
        "Pikafish"
    } else {
        "PikaRust"
    };
    let mut engine = EngineProcess::spawn(
        engine_name,
        &config.pikarust_bin,
        &config.pikarust_cwd,
        config.default_timeout,
    )?;
    let handshake = uci_io::uci_handshake(&mut engine, config.default_timeout)?;
    if handshake
        .iter()
        .any(|line| line.starts_with("option name EvalFile "))
    {
        uci_io::set_option(
            &mut engine,
            "EvalFile",
            &config.nnue_model.to_string_lossy(),
        )?;
    }
    uci_io::set_option(&mut engine, "Threads", "1")?;
    uci_io::set_option(&mut engine, "Hash", "16")?;
    let mut observations = Vec::new();
    let searches = positions()
        .flat_map(|(name, fen)| [5, 8].map(|depth| (format!("{name}_depth{depth}"), fen, depth)));
    for (name, fen, depth) in searches {
        reset(&mut engine, fen, config)?;
        let (bestmove, infos) = uci_io::go_depth(&mut engine, depth, config.search_timeout)?;
        let result =
            observation(&engine, fen, bestmove, infos).map_err(|error| E2eError::Engine {
                engine: engine.name().to_owned(),
                message: format!(
                    "fixture={name}, requested_depth={depth}, root_fen={fen}: {error}"
                ),
            })?;
        if result.final_info.depth != Some(depth) {
            return Err(protocol(
                &engine,
                "completed requested depth for baseline",
                &[],
            ));
        }
        observations.push(serde_json::json!({"position":name,"fen":fen,"search":result}));
    }
    engine.quit()?;
    Ok(
        serde_json::json!({"schema_version":1,"criterion":"exact candidate fixed-depth regression snapshot; not upstream parity",
        "reference_lock":include_str!("../../../scripts/reference.lock"), "depths":[5,8],"threads":1,"hash_mb":16,"positions":observations}),
    )
}

pub struct SearchRegression;
impl TestCase for SearchRegression {
    fn name(&self) -> &'static str {
        "search_regression"
    }
    fn requires_pikafish(&self) -> bool {
        true
    }
    fn run(&self, config: &E2eConfig) -> E2eResult<TestOutcome> {
        let start = Instant::now();
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../fixtures/search-baseline.json"
        ))
        .map_err(|error| E2eError::Preflight(format!("invalid search baseline: {error}")))?;
        let actual = record_search_baseline(config)?;
        let reference_config = E2eConfig {
            pikarust_bin: config.pikafish_bin.clone(),
            pikarust_cwd: config.pikafish_cwd.clone(),
            ..config.clone()
        };
        let reference = record_search_baseline(&reference_config)?;
        let mut comparisons = Vec::new();
        let mut reference_parity = true;
        for (candidate, upstream) in actual["positions"]
            .as_array()
            .unwrap()
            .iter()
            .zip(reference["positions"].as_array().unwrap())
        {
            let observed = &candidate["search"];
            let oracle = &upstream["search"];
            let equal = observed["bestmove"] == oracle["bestmove"]
                && ["depth", "nodes", "score_cp", "score_mate"]
                    .iter()
                    .all(|field| observed["final_info"][field] == oracle["final_info"][field]);
            reference_parity &= equal;
            comparisons.push(serde_json::json!({"position":candidate["position"],"equal":equal,
                "candidate":observed,"reference":oracle,"pv_equal":observed["final_info"]["pv"] == oracle["final_info"]["pv"]}));
        }
        let snapshot_matches = actual == expected;
        let passed = snapshot_matches && reference_parity;
        Ok(TestOutcome { name:self.name().to_owned(), passed, duration:start.elapsed(), detail:serde_json::json!({
            "criterion":"official bestmove, exact score and nodes parity at depths 5 and 8 AND candidate full-PV reviewed snapshot",
            "snapshot_matches":snapshot_matches,"reference_parity":reference_parity,"positions":comparisons,
            "expected":expected,"actual":actual,
        }).to_string() })
    }
}
