use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use log::{debug, error, info};

use pikarust_core::engine::{Engine, SearchLimits};
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

    let searching = Arc::new(AtomicBool::new(false));
    let stdin = io::stdin();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                error!("stdin read error: {e}");
                break;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        debug!(">> {trimmed}");

        let cmd = match parse_command(trimmed) {
            Ok(c) => c,
            Err(e) => {
                debug!("parse error: {e}");
                continue;
            }
        };

        match cmd {
            UciCommand::Uci => handle_uci(),
            UciCommand::IsReady => send("readyok"),
            UciCommand::UciNewGame => {
                if let Err(e) = engine.new_game() {
                    error!("new_game failed: {e}");
                }
            }
            UciCommand::Position { fen, moves } => {
                handle_position(&mut engine, fen.as_deref(), &moves);
            }
            UciCommand::Go(params) => {
                handle_go(&mut engine, &params, &searching);
            }
            UciCommand::Stop => {
                engine.stop();
            }
            UciCommand::SetOption { name, value } => {
                handle_set_option(&mut engine, &name, value.as_deref());
            }
            UciCommand::Quit => {
                engine.stop();
                break;
            }
            UciCommand::Debug(on) => {
                debug!("debug mode: {on}");
            }
            UciCommand::D => {
                send(&format!("{}", engine.position()));
            }
            UciCommand::PonderHit => {
                debug!("ponderhit (not yet supported)");
            }
            UciCommand::Bench(_params) => {
                debug!("bench (not yet implemented)");
            }
            UciCommand::Flip => {
                debug!("flip (not yet implemented)");
            }
        }
    }

    info!("{ENGINE_NAME} exiting");
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

fn handle_go(engine: &mut Engine, params: &GoParams, searching: &Arc<AtomicBool>) {
    if searching.load(Ordering::SeqCst) {
        engine.stop();
    }

    let limits = convert_go_params(params);
    searching.store(true, Ordering::SeqCst);

    let result = engine.go(&limits);
    searching.store(false, Ordering::SeqCst);

    let best = result.best_move.to_string();
    let ponder = result.ponder_move.map(|m| m.to_string());

    if let Some(p) = &ponder {
        send(&format!("bestmove {best} ponder {p}"));
    } else {
        send(&format!("bestmove {best}"));
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
