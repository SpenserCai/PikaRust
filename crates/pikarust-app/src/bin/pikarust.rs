#![forbid(unsafe_code)]

use std::io::{self, BufRead, Write};
use std::time::Duration;

use log::{debug, error, info};

use pikarust_core::engine::{Engine, SearchHandle, SearchLimits};
use pikarust_core::types::is_decisive;
use uci_rs::{GoParams, UciCommand, parse_command};

const ENGINE_NAME: &str = "PikaRust";
const ENGINE_AUTHOR: &str = "PikaRust Team";
const START_FEN: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .target(env_logger::Target::Stderr)
        .init();

    info!("{ENGINE_NAME} starting");

    let mut engine = match Engine::new() {
        Ok(e) => e,
        Err(e) => {
            error!("failed to initialize engine: {e}");
            std::process::exit(1);
        }
    };

    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<UciCommand>();

    std::thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines().map_while(Result::ok) {
            let trimmed = line.trim().to_owned();
            if trimmed.is_empty() {
                continue;
            }
            debug!(">> {trimmed}");
            if let Ok(cmd) = parse_command(&trimmed) {
                if cmd_tx.send(cmd).is_err() {
                    break;
                }
            }
        }
    });

    let mut active: Option<SearchHandle> = None;

    loop {
        if let Some(ref handle) = active {
            if let Some(result) = handle.try_recv() {
                output_result(&result);
                active = None;
            }
        }

        let timeout = if active.is_some() {
            Duration::from_millis(1)
        } else {
            Duration::from_secs(86400)
        };

        match cmd_rx.recv_timeout(timeout) {
            Ok(cmd) => match cmd {
                UciCommand::Uci => handle_uci(),
                UciCommand::IsReady => send("readyok"),
                UciCommand::Go(params) => {
                    stop_active(&mut active);
                    let limits = convert_go_params(&params);
                    active = Some(engine.go(&limits));
                }
                UciCommand::Stop => {
                    if let Some(handle) = active.take() {
                        handle.stop();
                        output_result(&handle.wait());
                    }
                }
                UciCommand::PonderHit => {
                    if let Some(ref handle) = active {
                        handle.ponderhit();
                    }
                }
                UciCommand::Position { fen, moves } => {
                    stop_active(&mut active);
                    handle_position(&mut engine, fen.as_deref(), &moves);
                }
                UciCommand::UciNewGame => {
                    stop_active(&mut active);
                    if let Err(e) = engine.new_game() {
                        error!("new_game failed: {e}");
                    }
                }
                UciCommand::SetOption { name, value } => {
                    stop_active(&mut active);
                    handle_set_option(&mut engine, &name, value.as_deref());
                }
                UciCommand::Quit => {
                    stop_active(&mut active);
                    engine.stop();
                    break;
                }
                UciCommand::Debug(on) => {
                    debug!("debug mode: {on}");
                }
                UciCommand::D => {
                    send(&format!("{}", engine.position()));
                }
                UciCommand::Bench(_params) => {
                    debug!("bench (not yet implemented)");
                }
                UciCommand::Flip => {
                    debug!("flip (not yet implemented)");
                }
            },
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    info!("{ENGINE_NAME} exiting");
}

fn stop_active(active: &mut Option<SearchHandle>) {
    if let Some(handle) = active.take() {
        handle.stop();
        let _ = handle.wait();
    }
}

fn output_result(result: &pikarust_core::engine::SearchResult) {
    let score_str = if is_decisive(result.score) {
        let plies = if result.score > 0 {
            pikarust_core::types::VALUE_MATE - result.score
        } else {
            -(pikarust_core::types::VALUE_MATE + result.score)
        };
        let moves = (plies + i32::from(plies > 0)) / 2;
        format!("score mate {moves}")
    } else {
        format!("score cp {}", result.score_cp)
    };

    let mut info = format!(
        "info depth {} nodes {} {score_str}",
        result.depth, result.nodes
    );
    if let Some((w, d, l)) = result.wdl {
        use std::fmt::Write;
        let _ = write!(info, " wdl {w} {d} {l}");
    }
    send(&info);

    let best = result.best_move.to_string();
    match result.ponder_move {
        Some(p) => send(&format!("bestmove {best} ponder {p}")),
        None => send(&format!("bestmove {best}")),
    }
}

fn handle_uci() {
    send(&format!("id name {ENGINE_NAME}"));
    send(&format!("id author {ENGINE_AUTHOR}"));

    for opt in Engine::uci_options() {
        send(&opt.to_string());
    }

    send("uciok");
}

fn handle_position(engine: &mut Engine, fen: Option<&str>, moves: &[String]) {
    let fen = fen.unwrap_or(START_FEN);
    let move_strs: Vec<&str> = moves.iter().map(String::as_str).collect();

    if let Err(e) = engine.set_position(fen, &move_strs) {
        error!("position error: {e}");
    }
}

fn handle_set_option(engine: &mut Engine, name: &str, value: Option<&str>) {
    let value = value.unwrap_or("");
    if let Err(e) = engine.set_option(name, value) {
        error!("setoption error: {e}");
    }
}

fn convert_go_params(params: &GoParams) -> SearchLimits {
    SearchLimits {
        depth: params.depth.map(|d| d as i32),
        nodes: params.nodes,
        time: [
            params.wtime.map(|t| t as i64),
            params.btime.map(|t| t as i64),
        ],
        inc: [params.winc.map(|t| t as i64), params.binc.map(|t| t as i64)],
        movestogo: params.movestogo.map(|m| m as i32),
        movetime: params.movetime.map(|t| t as i64),
        infinite: params.infinite,
        ponder: params.ponder,
    }
}

fn send(msg: &str) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "{msg}");
    let _ = out.flush();
}
