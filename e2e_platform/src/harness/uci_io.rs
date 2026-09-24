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
#[derive(Debug, Clone, Default, serde::Serialize, PartialEq, Eq)]
pub struct InfoLine {
    /// Search depth.
    pub depth: Option<u32>,
    /// Bound-only scores cannot establish exact evaluation equivalence.
    pub bounded: bool,
    /// UCI `MultiPV` index (one when omitted).
    pub multipv: Option<u32>,
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
    let lines = engine.read_until(|line| line.trim() == "uciok", timeout)?;
    if !lines.iter().any(|line| line.starts_with("id name ")) {
        return Err(E2eError::Protocol {
            engine: engine.name().to_owned(),
            expected: "id name before uciok".to_owned(),
            actual: lines.join("\n"),
        });
    }
    Ok(lines)
}

/// Send "isready", wait for "readyok".
pub fn sync_engine(engine: &mut EngineProcess, timeout: Duration) -> E2eResult<()> {
    engine.send("isready")?;
    let lines = engine.read_until(|line| line.trim() == "readyok", timeout)?;
    if lines.iter().any(|line| line.starts_with("bestmove ")) {
        return Err(E2eError::Protocol {
            engine: engine.name().to_owned(),
            expected: "readyok without an unsolicited bestmove".to_owned(),
            actual: lines.join("\n"),
        });
    }
    Ok(())
}

/// Send "go depth N", collect info lines, return bestmove + all info.
pub fn go_depth(
    engine: &mut EngineProcess,
    depth: u32,
    timeout: Duration,
) -> E2eResult<(BestMoveResponse, Vec<InfoLine>)> {
    engine.send(&format!("go depth {depth}"))?;
    let (bestmove, infos) = collect_search_result(engine, timeout)?;
    let completed = infos.iter().rev().find(|info| {
        info.depth == Some(depth)
            && !info.bounded
            && info.multipv.unwrap_or(1) == 1
            && (info.score_cp.is_some() ^ info.score_mate.is_some())
            && info.nodes.is_some_and(|nodes| nodes > 0)
            && !info.pv.is_empty()
    });
    if completed.is_none_or(|info| info.pv.first() != Some(&bestmove.best_move)) {
        return Err(E2eError::Protocol {
            engine: engine.name().to_owned(),
            expected: format!(
                "completed depth {depth}, exact score, positive nodes and PV matching bestmove"
            ),
            actual: format!("bestmove={}, info={infos:?}", bestmove.best_move),
        });
    }
    Ok((bestmove, infos))
}

/// Search with a reproducible node budget.
pub fn go_nodes(
    engine: &mut EngineProcess,
    nodes: u64,
    timeout: Duration,
) -> E2eResult<(BestMoveResponse, Vec<InfoLine>)> {
    engine.send(&format!("go nodes {nodes}"))?;
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
    let valid_move = |value: &str| {
        let bytes = value.as_bytes();
        bytes.len() == 4
            && (b'a'..=b'i').contains(&bytes[0])
            && bytes[1].is_ascii_digit()
            && (b'a'..=b'i').contains(&bytes[2])
            && bytes[3].is_ascii_digit()
    };
    if !(tokens.len() == 2 || tokens.len() == 4)
        || tokens.first() != Some(&"bestmove")
        || !tokens
            .get(1)
            .is_some_and(|mv| valid_move(mv) || *mv == "0000" || *mv == "(none)")
        || (tokens.len() == 4 && (tokens[2] != "ponder" || !valid_move(tokens[3])))
    {
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
    let mut tokens = line.split_whitespace().skip(1);
    let mut info = InfoLine {
        multipv: Some(1),
        ..InfoLine::default()
    };
    while let Some(field) = tokens.next() {
        match field {
            "string" => break,
            "lowerbound" | "upperbound" => info.bounded = true,
            "depth" => info.depth = tokens.next().and_then(|value| value.parse().ok()),
            "multipv" => info.multipv = tokens.next().and_then(|value| value.parse().ok()),
            "nodes" => info.nodes = tokens.next().and_then(|value| value.parse().ok()),
            "score" => match tokens.next() {
                Some("cp") => info.score_cp = tokens.next().and_then(|value| value.parse().ok()),
                Some("mate") => {
                    info.score_mate = tokens.next().and_then(|value| value.parse().ok());
                }
                _ => {}
            },
            "pv" => {
                info.pv.extend(tokens.map(str::to_owned));
                break;
            }
            _ => {}
        }
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_bestmove_is_a_protocol_failure() {
        for value in [
            "bestmove",
            "bestmove z0a0",
            "bestmove a0a1 junk",
            "bestmove a0a1 ponder bad",
        ] {
            assert!(parse_bestmove(value, "fixture").is_err(), "{value}");
        }
        assert!(parse_bestmove("bestmove a0a1 ponder a9a8", "fixture").is_ok());
    }

    #[test]
    fn info_strings_cannot_supply_fake_scores() {
        assert!(
            parse_info("info string score cp 123 depth 9")
                .score_cp
                .is_none()
        );
        let info = parse_info("info depth 4 score cp 10 lowerbound nodes 123 pv a0a1");
        assert!(info.bounded);
        assert_eq!(info.nodes, Some(123));
    }
    #[cfg(unix)]
    #[test]
    fn incomplete_depth_or_score_is_not_a_completed_search() {
        for info in [
            "info depth 1 score cp 0 nodes 1 pv b0c2",
            "info depth 5 nodes 1 pv b0c2",
            "info depth 5 score cp 0 lowerbound nodes 1 pv b0c2",
            "info depth 5 score cp 0 nodes 0 pv b0c2",
            "info depth 5 score cp 0 nodes 1 pv h0g2",
        ] {
            let mut engine = EngineProcess::spawn(
                "fixture",
                std::path::Path::new("/bin/sh"),
                std::path::Path::new("/tmp"),
                Duration::from_secs(1),
            )
            .unwrap();
            engine
                .send(&format!("go() {{ echo '{info}'; echo 'bestmove b0c2'; }}"))
                .unwrap();
            assert!(
                matches!(
                    go_depth(&mut engine, 5, Duration::from_secs(1)),
                    Err(E2eError::Protocol { .. })
                ),
                "{info}"
            );
        }
    }
}
