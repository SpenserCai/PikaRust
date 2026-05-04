use std::time::Duration;

use pikarust_core::types::Color;

use crate::error::E2eResult;
use crate::harness::engine_process::EngineProcess;
use crate::harness::uci_io;
use crate::referee::game_result::GameResult;
use crate::referee::game_state::GameState;

use super::match_config::{MatchConfig, SearchMode};

/// Record of a completed match.
pub struct MatchRecord {
    /// Game outcome.
    pub result: GameResult,
    /// Number of full moves played.
    pub move_count: u32,
    /// Full move history in UCI notation.
    pub move_history: Vec<String>,
    /// Final position FEN.
    pub final_fen: String,
}

/// Outcome of a single half-move attempt.
enum MoveOutcome {
    /// The game continues.
    Continue,
    /// The game ended — return this record.
    Finished(MatchRecord),
}

/// Execute one side's move, returning whether the game ended.
#[allow(clippy::too_many_arguments)]
fn play_one_move(
    engine: &mut EngineProcess,
    engine_name: &str,
    white_name: &str,
    black_name: &str,
    state: &mut GameState,
    search_mode: &SearchMode,
    timeout: Duration,
    move_num: u32,
    start_fen: Option<&str>,
) -> E2eResult<MoveOutcome> {
    uci_io::set_position(engine, start_fen, state.move_history())?;

    let (bm, _infos) = match search_mode {
        SearchMode::Depth(d) => uci_io::go_depth(engine, *d, timeout)?,
        SearchMode::Movetime(ms) => {
            // Timeout = movetime + generous buffer for engine overhead
            let effective_timeout = Duration::from_millis(*ms) + timeout;
            uci_io::go_movetime(engine, *ms, effective_timeout)?
        }
    };

    if bm.best_move == "0000" || bm.best_move == "(none)" {
        let result = state
            .check_game_end(white_name, black_name)
            .unwrap_or_else(|| GameResult::EngineError {
                engine: engine_name.to_owned(),
                message: "returned null move".to_owned(),
            });
        return Ok(MoveOutcome::Finished(MatchRecord {
            result,
            move_count: move_num,
            move_history: state.move_history().to_vec(),
            final_fen: state.fen(),
        }));
    }

    if let Err(e) = state.apply_uci_move(&bm.best_move, engine_name) {
        return Ok(MoveOutcome::Finished(MatchRecord {
            result: GameResult::EngineError {
                engine: engine_name.to_owned(),
                message: e.to_string(),
            },
            move_count: move_num,
            move_history: state.move_history().to_vec(),
            final_fen: state.fen(),
        }));
    }

    if let Some(game_result) = state.check_game_end(white_name, black_name) {
        return Ok(MoveOutcome::Finished(MatchRecord {
            result: game_result,
            move_count: move_num,
            move_history: state.move_history().to_vec(),
            final_fen: state.fen(),
        }));
    }

    Ok(MoveOutcome::Continue)
}

/// Run a complete game between two engines.
///
/// Both engines are spawned, configured with Threads=1 and the specified Hash,
/// then alternate moves until the game ends.
pub fn run_match(config: &MatchConfig) -> E2eResult<MatchRecord> {
    let mut white = EngineProcess::spawn(
        &config.white_name,
        &config.white_bin,
        &config.white_cwd,
        config.response_timeout,
    )?;
    let mut black = EngineProcess::spawn(
        &config.black_name,
        &config.black_bin,
        &config.black_cwd,
        config.response_timeout,
    )?;

    let timeout = config.response_timeout;

    uci_io::uci_handshake(&mut white, timeout)?;
    uci_io::uci_handshake(&mut black, timeout)?;

    uci_io::set_option(&mut white, "Threads", "1")?;
    uci_io::set_option(&mut white, "Hash", &config.hash_mb.to_string())?;
    uci_io::set_option(&mut black, "Threads", "1")?;
    uci_io::set_option(&mut black, "Hash", &config.hash_mb.to_string())?;

    uci_io::new_game(&mut white)?;
    uci_io::new_game(&mut black)?;
    uci_io::sync_engine(&mut white, timeout)?;
    uci_io::sync_engine(&mut black, timeout)?;

    let mut state = if let Some(ref fen) = config.start_fen {
        GameState::from_fen(fen)?
    } else {
        GameState::new()?
    };

    let start_fen = config.start_fen.as_deref();

    for move_num in 1..=config.max_moves {
        for side in &[Color::White, Color::Black] {
            if state.side_to_move() != *side {
                continue;
            }

            let (engine, engine_name) = if *side == Color::White {
                (&mut white, config.white_name.as_str())
            } else {
                (&mut black, config.black_name.as_str())
            };

            let outcome = play_one_move(
                engine,
                engine_name,
                &config.white_name,
                &config.black_name,
                &mut state,
                &config.search_mode,
                timeout,
                move_num,
                start_fen,
            )?;

            if let MoveOutcome::Finished(record) = outcome {
                white.quit()?;
                black.quit()?;
                return Ok(record);
            }
        }

        if move_num % 20 == 0 {
            log::info!("match in progress: move {move_num}, fen={}", state.fen());
        }
    }

    white.quit()?;
    black.quit()?;

    Ok(MatchRecord {
        result: GameResult::MaxMovesReached {
            move_count: config.max_moves,
        },
        move_count: config.max_moves,
        move_history: state.move_history().to_vec(),
        final_fen: state.fen(),
    })
}
