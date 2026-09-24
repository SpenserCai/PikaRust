#![forbid(unsafe_code)]

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::Duration;

use log::{debug, error, info};

use pikarust_core::engine::{Engine, SearchHandle, SearchLimits};
use pikarust_core::position::rule_judge::RuleJudgeResult;
use pikarust_core::position::{GenType, Position, generate};
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

    let mut engine = match initialize_engine() {
        Ok(e) => e,
        Err(e) => {
            error!("failed to initialize engine: {e}");
            std::process::exit(1);
        }
    };

    let cmd_rx = spawn_command_reader();

    let mut active: Option<SearchHandle> = None;

    loop {
        if let Some(ref handle) = active {
            match handle.try_result() {
                Ok(Some(result)) => {
                    output_result(&result);
                    active = None;
                }
                Err(error) => {
                    send(&format!("info string error: {error}"));
                    send("bestmove 0000");
                    active = None;
                }
                Ok(None) => {}
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
                    if let Some(depth) = params.perft {
                        handle_perft(&engine, depth);
                    } else {
                        match convert_go_params(&params)
                            .and_then(|limits| engine.try_go(&limits).map_err(|e| e.to_string()))
                        {
                            Ok(handle) => active = Some(handle),
                            Err(err) => send(&format!("info string error: {err}")),
                        }
                    }
                }
                UciCommand::Stop => {
                    stop_active(&mut active);
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
                UciCommand::D => handle_display(&engine),
                UciCommand::Eval => {
                    stop_active(&mut active);
                    handle_eval(&engine);
                }
                UciCommand::Bench(_params) => {
                    send("info string error: bench is provided by the pikarust-bench binary");
                }
                UciCommand::Flip => {
                    send("info string error: flip is unsupported");
                }
            },
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    stop_active(&mut active);
    info!("{ENGINE_NAME} exiting");
}

fn spawn_command_reader() -> std::sync::mpsc::Receiver<UciCommand> {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::sync_channel::<UciCommand>(64);

    std::thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines().map_while(Result::ok) {
            let trimmed = line.trim().to_owned();
            if trimmed.is_empty() {
                continue;
            }
            debug!(">> {trimmed}");
            match parse_command(&trimmed) {
                Ok(cmd) => {
                    let quitting = cmd == UciCommand::Quit;
                    if cmd_tx.send(cmd).is_err() || quitting {
                        break;
                    }
                }
                Err(err) => send(&format!("info string error: {err}")),
            }
        }
    });

    cmd_rx
}

fn handle_eval(engine: &Engine) {
    if !engine.position().checkers().is_empty() {
        send("Final evaluation: none (in check)");
        return;
    }
    let evaluation = engine.evaluate();
    if let Some(nnue) = evaluation.nnue {
        send(&format!(
            "(Big net) NNUE evaluation {nnue:+} (side to move, internal units)"
        ));
    } else {
        send("info string NNUE unavailable; material-only evaluation");
    }
    send(&format!(
        "Final evaluation {:+} (side to move, internal units)",
        evaluation.value
    ));
}

fn initialize_engine() -> Result<Engine, String> {
    let mut args = std::env::args_os().skip(1);
    let mut eval_file = std::env::var_os("PIKARUST_NNUE_FILE").map(PathBuf::from);
    let mut material_only = false;
    let mut explicit_eval_file = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--eval-file") => {
                explicit_eval_file = true;
                eval_file = Some(PathBuf::from(
                    args.next().ok_or("--eval-file requires a path")?,
                ));
            }
            Some("--material-only") => material_only = true,
            Some("--version") => {
                println!("{ENGINE_NAME} {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            Some("--help" | "-h") => {
                println!(
                    "Usage: pikarust [--eval-file PATH | --material-only]\n\nNNUE path may also be set with PIKARUST_NNUE_FILE.\nWithout a path, discovers models/pikafish.nnue or ./pikafish.nnue."
                );
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {}", arg.to_string_lossy())),
        }
    }
    if material_only && explicit_eval_file {
        return Err("--eval-file and --material-only cannot be combined".to_owned());
    }
    if material_only {
        Engine::without_nnue()
    } else if let Some(path) = eval_file {
        Engine::with_nnue_file(path)
    } else {
        Engine::new()
    }
    .map_err(|error| error.to_string())
}

fn handle_perft(engine: &Engine, depth: u32) {
    let Ok(depth) = i32::try_from(depth) else {
        send("info string error: perft depth out of range");
        return;
    };
    if !(0..=10).contains(&depth) {
        send("info string error: perft depth must be in 0..=10");
        return;
    }
    if depth == 0 {
        send("Nodes searched: 1");
        return;
    }
    let mut position = engine.position().clone();
    let mut total = 0;
    for (mv, nodes) in pikarust_core::position::perft::perft_divide(&mut position, depth) {
        send(&format!("{mv}: {nodes}"));
        total += nodes;
    }
    send(&format!("Nodes searched: {total}"));
}

fn stop_active(active: &mut Option<SearchHandle>) {
    if let Some(handle) = active.take() {
        handle.stop();
        match handle.wait_result() {
            Ok(result) => output_result(&result),
            Err(error) => {
                send(&format!("info string error: {error}"));
                send("bestmove 0000");
            }
        }
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
        "info depth {} seldepth {} nodes {} hashfull {} {score_str}",
        result.depth, result.seldepth, result.nodes, result.hashfull
    );
    if let Some((w, d, l)) = result.wdl {
        use std::fmt::Write;
        let _ = write!(info, " wdl {w} {d} {l}");
    }
    if !result.pv.is_empty() {
        use std::fmt::Write;
        let _ = write!(
            info,
            " pv {}",
            result
                .pv
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        );
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
        send(&format!("info string error: {e}"));
    }
}

fn handle_display(engine: &Engine) {
    send(&format!("{}", engine.position()));
    send(&format!("Game status: {}", game_status(engine.position())));
}

/// Adjudicate the played position from the side to move's perspective.
fn game_status(position: &Position) -> &'static str {
    // Repetition adjudication temporarily rewinds history. Keep diagnostics
    // independent from the live engine position, including its NNUE state.
    let mut position = position.clone();
    // At the game root, ply zero excludes search-only twofold adjudication.
    // Preserve the rule judge's precedence over the no-legal-move fallback.
    match position.rule_judge(0) {
        RuleJudgeResult::Definitive(value) => match value.cmp(&0) {
            std::cmp::Ordering::Greater => "win",
            std::cmp::Ordering::Equal => "draw",
            std::cmp::Ordering::Less => "loss",
        },
        RuleJudgeResult::None | RuleJudgeResult::TwoFold(_) => {
            if generate(&position, GenType::Legal).is_empty() {
                "loss"
            } else {
                "ongoing"
            }
        }
    }
}

fn handle_set_option(engine: &mut Engine, name: &str, value: Option<&str>) {
    let value = value.unwrap_or("");
    if let Err(e) = engine.set_option(name, value) {
        error!("setoption error: {e}");
        send(&format!("info string error: {e}"));
    }
}

fn convert_go_params(params: &GoParams) -> Result<SearchLimits, String> {
    let signed = |value: Option<u64>| -> Result<Option<i64>, String> {
        value
            .map(i64::try_from)
            .transpose()
            .map_err(|_| "time value is too large".to_owned())
    };
    let limits = SearchLimits {
        depth: params
            .depth
            .map(i32::try_from)
            .transpose()
            .map_err(|_| "depth is too large")?,
        nodes: params.nodes,
        time: [signed(params.wtime)?, signed(params.btime)?],
        inc: [signed(params.winc)?, signed(params.binc)?],
        movestogo: params
            .movestogo
            .map(i32::try_from)
            .transpose()
            .map_err(|_| "movestogo is too large")?,
        movetime: signed(params.movetime)?,
        infinite: params.infinite,
        ponder: params.ponder,
        search_moves: params.searchmoves.clone(),
    };
    limits.validate().map_err(|error| error.to_string())?;
    Ok(limits)
}

fn send(msg: &str) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "{msg}");
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use pikarust_core::types::{Move, Square};

    fn play(position: &mut Position, from: Square, to: Square) {
        let movement = Move::make(from, to);
        assert!(position.is_legal_move(movement));
        position.do_move(movement, position.gives_check(movement));
    }

    #[test]
    fn game_status_reports_played_threefold_without_mutating_history() {
        let mut position = Position::start_pos().unwrap();
        assert_eq!(game_status(&position), "ongoing");
        let cycle = [
            (Square::SQ_A0, Square::SQ_A1),
            (Square::SQ_A9, Square::SQ_A8),
            (Square::SQ_A1, Square::SQ_A0),
            (Square::SQ_A8, Square::SQ_A9),
        ];
        for (from, to) in cycle {
            play(&mut position, from, to);
        }
        assert_eq!(game_status(&position), "ongoing");
        for (from, to) in cycle {
            play(&mut position, from, to);
        }
        let fen = position.fen();
        let key = position.key();
        assert_eq!(game_status(&position), "draw");
        assert_eq!(game_status(&position), "draw");
        assert_eq!(position.fen(), fen);
        assert_eq!(position.key(), key);
        assert_eq!(position.rule_judge(0), RuleJudgeResult::Definitive(0));
    }

    #[test]
    fn game_status_reports_rule60_and_material_draws() {
        let at_limit = START_FEN.replace("0 1", "120 1");
        for fen in [at_limit.as_str(), "3k5/9/9/9/9/9/9/9/9/4K4 w - - 0 1"] {
            let position = Position::from_fen(fen).unwrap();
            assert_eq!(game_status(&position), "draw");
            assert_eq!(position.fen(), fen);
        }
    }

    #[test]
    fn game_status_reports_mate_and_stalemate_as_losses() {
        for (fen, in_check) in [
            ("3k5/9/9/9/9/9/9/4r4/9/3rK4 w - - 0 1", true),
            ("3k5/9/9/9/9/9/9/9/3r1r3/4K4 w - - 0 1", false),
            ("3k5/9/9/9/9/9/9/4r4/9/3rK4 w - - 120 1", true),
        ] {
            let position = Position::from_fen(fen).unwrap();
            assert_eq!(position.checkers().is_not_empty(), in_check);
            assert!(generate(&position, GenType::Legal).is_empty());
            assert_eq!(game_status(&position), "loss");
            assert_eq!(position.fen(), fen);
        }
    }

    #[test]
    fn game_status_perpetual_check_uses_side_to_move_perspective() {
        let mut position = Position::from_fen("3k5/3R5/9/9/9/4P4/9/9/9/4K4 b - - 0 1").unwrap();
        let cycle = [
            (Square::SQ_D9, Square::SQ_E9),
            (Square::SQ_D8, Square::SQ_E8),
            (Square::SQ_E9, Square::SQ_D9),
            (Square::SQ_E8, Square::SQ_D8),
        ];
        for (from, to) in cycle {
            play(&mut position, from, to);
        }
        assert_eq!(game_status(&position), "ongoing");
        for (from, to) in cycle {
            play(&mut position, from, to);
        }
        assert_eq!(game_status(&position), "win");
        play(&mut position, Square::SQ_D9, Square::SQ_E9);
        assert_eq!(game_status(&position), "loss");
    }

    #[test]
    fn rejects_overflow_and_unbounded_zero_limits() {
        for command in [
            "go depth 4294967295",
            "go movetime 18446744073709551615",
            "go nodes 0",
            "go depth 0",
        ] {
            let UciCommand::Go(params) = parse_command(command).unwrap() else {
                panic!("expected go")
            };
            assert!(convert_go_params(&params).is_err(), "{command}");
        }
    }

    #[test]
    fn forwards_root_move_restrictions() {
        let UciCommand::Go(params) = parse_command("go depth 1 searchmoves b0c2").unwrap() else {
            panic!("expected go")
        };
        assert_eq!(convert_go_params(&params).unwrap().search_moves, ["b0c2"]);
    }
}
