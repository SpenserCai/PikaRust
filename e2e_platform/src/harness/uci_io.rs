use std::time::Duration;

use crate::error::{E2eError, E2eResult};
use crate::harness::engine_process::EngineProcess;

/// Parsed bestmove response.
#[derive(Debug, Clone)]
pub struct BestMoveResponse {
    /// The best move in UCI notation (e.g., "h2e2").
    pub best_move: String,
    /// Optional ponder move.
    pub ponder_move: Option<String>,
}

/// Parsed info line fields relevant to E2E testing.
#[derive(Debug, Clone, Default)]
pub struct InfoLine {
    /// Search depth.
    pub depth: Option<u32>,
    /// Score in centipawns.
    pub score_cp: Option<i32>,
    /// Score as mate-in-N.
    pub score_mate: Option<i32>,
    /// Nodes searched.
    pub nodes: Option<u64>,
    /// Principal variation moves.
    pub pv: Vec<String>,
}

/// Perform the UCI handshake: send "uci", collect until "uciok".
pub fn uci_handshake(engine: &mut EngineProcess, timeout: Duration) -> E2eResult<Vec<String>> {
    engine.send("uci")?;
    engine.read_until(|line| line.trim() == "uciok", timeout)
}

/// Send "isready", wait for "readyok".
pub fn sync_engine(engine: &mut EngineProcess, timeout: Duration) -> E2eResult<()> {
    engine.send("isready")?;
    engine.read_until(|line| line.trim() == "readyok", timeout)?;
    Ok(())
}

/// Send "go depth N", collect info lines, return bestmove + all info.
pub fn go_depth(
    engine: &mut EngineProcess,
    depth: u32,
    timeout: Duration,
) -> E2eResult<(BestMoveResponse, Vec<InfoLine>)> {
    engine.send(&format!("go depth {depth}"))?;
    collect_search_result(engine, timeout)
}

/// Send "go movetime N", collect info lines, return bestmove + all info.
pub fn go_movetime(
    engine: &mut EngineProcess,
    movetime_ms: u64,
    timeout: Duration,
) -> E2eResult<(BestMoveResponse, Vec<InfoLine>)> {
    engine.send(&format!("go movetime {movetime_ms}"))?;
    collect_search_result(engine, timeout)
}

/// Send "go infinite" (caller must later send "stop").
pub fn go_infinite(engine: &mut EngineProcess) -> E2eResult<()> {
    engine.send("go infinite")
}

/// Send "stop", collect until bestmove.
pub fn stop_and_collect(
    engine: &mut EngineProcess,
    timeout: Duration,
) -> E2eResult<(BestMoveResponse, Vec<InfoLine>)> {
    engine.send("stop")?;
    collect_search_result(engine, timeout)
}

/// Send `position startpos` or `position fen FEN [moves ...]`.
pub fn set_position(
    engine: &mut EngineProcess,
    fen: Option<&str>,
    moves: &[String],
) -> E2eResult<()> {
    let mut cmd = fen.map_or_else(
        || "position startpos".to_owned(),
        |f| format!("position fen {f}"),
    );
    if !moves.is_empty() {
        cmd.push_str(" moves ");
        cmd.push_str(&moves.join(" "));
    }
    engine.send(&cmd)
}

/// Send `setoption name NAME value VALUE`.
pub fn set_option(engine: &mut EngineProcess, name: &str, value: &str) -> E2eResult<()> {
    engine.send(&format!("setoption name {name} value {value}"))
}

/// Send "ucinewgame".
pub fn new_game(engine: &mut EngineProcess) -> E2eResult<()> {
    engine.send("ucinewgame")
}

/// Collect lines until "bestmove", parsing info lines along the way.
fn collect_search_result(
    engine: &EngineProcess,
    timeout: Duration,
) -> E2eResult<(BestMoveResponse, Vec<InfoLine>)> {
    let lines = engine.read_until(|line| line.starts_with("bestmove"), timeout)?;

    let mut infos = Vec::new();
    let mut bestmove = None;

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with("info ") {
            infos.push(parse_info(trimmed));
        } else if trimmed.starts_with("bestmove") {
            bestmove = Some(parse_bestmove(trimmed, engine.name())?);
        }
    }

    let bm = bestmove.ok_or_else(|| E2eError::Protocol {
        engine: engine.name().to_owned(),
        expected: "bestmove line".to_owned(),
        actual: "no bestmove found".to_owned(),
    })?;

    Ok((bm, infos))
}

/// Parse a "bestmove <move> [ponder <move>]" line.
fn parse_bestmove(line: &str, engine_name: &str) -> E2eResult<BestMoveResponse> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 2 {
        return Err(E2eError::Protocol {
            engine: engine_name.to_owned(),
            expected: "bestmove <move>".to_owned(),
            actual: line.to_owned(),
        });
    }

    let best_move = tokens[1].to_owned();
    let ponder_move = if tokens.len() >= 4 && tokens[2] == "ponder" {
        Some(tokens[3].to_owned())
    } else {
        None
    };

    Ok(BestMoveResponse {
        best_move,
        ponder_move,
    })
}

/// Parse an "info" line into structured fields.
fn parse_info(line: &str) -> InfoLine {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut info = InfoLine::default();
    let mut i = 1; // skip "info"

    while i < tokens.len() {
        match tokens[i] {
            "depth" => {
                i += 1;
                if i < tokens.len() {
                    info.depth = tokens[i].parse().ok();
                }
            }
            "score" => {
                i += 1;
                if i < tokens.len() {
                    match tokens[i] {
                        "cp" => {
                            i += 1;
                            if i < tokens.len() {
                                info.score_cp = tokens[i].parse().ok();
                            }
                        }
                        "mate" => {
                            i += 1;
                            if i < tokens.len() {
                                info.score_mate = tokens[i].parse().ok();
                            }
                        }
                        _ => {}
                    }
                }
            }
            "nodes" => {
                i += 1;
                if i < tokens.len() {
                    info.nodes = tokens[i].parse().ok();
                }
            }
            "pv" => {
                i += 1;
                while i < tokens.len() {
                    info.pv.push(tokens[i].to_owned());
                    i += 1;
                }
                break;
            }
            _ => {}
        }
        i += 1;
    }

    info
}
