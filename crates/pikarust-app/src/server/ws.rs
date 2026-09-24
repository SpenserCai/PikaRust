use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::SinkExt;
use futures_util::stream::StreamExt;
use pikarust_core::engine::{SearchHandle, SearchLimits, SearchResult};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::AppState;
use super::session::Session;

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_session(socket, state))
}

async fn handle_session(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    let Some(session) = initialize_session(&mut sender, &state).await else {
        return;
    };

    let (info_tx, mut info_rx) = mpsc::channel::<String>(64);

    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = info_rx.recv().await {
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    let mut active: Option<(Option<String>, SearchHandle)> = None;
    let mut poll = tokio::time::interval(std::time::Duration::from_millis(5));
    loop {
        let msg = tokio::select! {
            _ = &mut send_task => break,
            _ = poll.tick(), if active.is_some() => {
                if let Some((id, handle)) = &active {
                    match handle.try_result() {
                        Ok(Some(result)) => {
                            send_bestmove(id.clone(), &result, &info_tx).await;
                            active = None;
                        }
                        Err(error) => {
                            send_error(id.clone(), "SEARCH_ERROR", error.to_string(), &info_tx).await;
                            active = None;
                        }
                        Ok(None) => {}
                    }
                }
                continue;
            }
            msg = receiver.next() => match msg {
                Some(Ok(msg)) => msg,
                _ => break,
            },
        };
        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };
        let cmd: WsCommand = match serde_json::from_str(&text) {
            Ok(c) => c,
            Err(error) => {
                send_error(None, "PARSE_ERROR", error.to_string(), &info_tx).await;
                continue;
            }
        };
        session.touch().await;
        // Start and mutate in wire order. Spawning each `go` as an independent
        // task allows a following `stop`/`position` to overtake its startup.
        if matches!(
            cmd.cmd.as_str(),
            "go" | "stop" | "position" | "ucinewgame" | "setoption"
        ) {
            finish_active(&mut active, &info_tx).await;
        }
        match cmd.cmd.as_str() {
            "position" => handle_position(&session, &cmd, &info_tx).await,
            "go" => match start_search(&session, &cmd).await {
                Ok(handle) => active = Some((cmd.id, handle)),
                Err(error) => send_error(cmd.id, "SEARCH_ERROR", error, &info_tx).await,
            },
            "stop" => {}
            "ponderhit" => {
                if let Some((_, handle)) = &active {
                    handle.ponderhit();
                }
            }
            "ucinewgame" => handle_new_game(&session, &cmd, &info_tx).await,
            "setoption" => handle_set_option(&session, &cmd, &info_tx).await,
            _ => {
                send_error(
                    cmd.id,
                    "UNKNOWN_COMMAND",
                    format!("unknown command: {}", cmd.cmd),
                    &info_tx,
                )
                .await;
            }
        }
    }
    // Cancellation belongs to the connection, including an unfinished `go`.
    drop(active);
    if !send_task.is_finished() {
        send_task.abort();
        let _ = send_task.await;
    }
    state.session_mgr.destroy_session(&session.id).await;
    drop(session);
}

async fn initialize_session(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    state: &AppState,
) -> Option<Arc<Session>> {
    let created = state.session_mgr.create_session().await;
    let session = match created {
        Ok(s) => s,
        Err(e) => {
            let err = serde_json::to_string(&WsResponse::Error {
                request_id: None,
                code: "SESSION_ERROR".to_owned(),
                message: e.to_string(),
            })
            .unwrap_or_default();
            let _ = sender.send(Message::Text(err.into())).await;
            return None;
        }
    };

    let session_msg = serde_json::to_string(&WsResponse::Session {
        session_id: session.id.clone(),
        engine_version: env!("CARGO_PKG_VERSION").to_owned(),
    })
    .unwrap_or_default();
    if sender
        .send(Message::Text(session_msg.into()))
        .await
        .is_err()
    {
        state.session_mgr.destroy_session(&session.id).await;
        return None;
    }

    Some(session)
}

async fn send_error(
    request_id: Option<String>,
    code: &str,
    message: String,
    tx: &mpsc::Sender<String>,
) {
    let response = WsResponse::Error {
        request_id,
        code: code.to_owned(),
        message,
    };
    if let Ok(json) = serde_json::to_string(&response) {
        let _ = tx.send(json).await;
    }
}

async fn send_bestmove(
    request_id: Option<String>,
    result: &SearchResult,
    tx: &mpsc::Sender<String>,
) {
    let info = WsResponse::Info {
        request_id: request_id.clone(),
        depth: result.depth,
        seldepth: result.seldepth,
        nodes: result.nodes,
        score_cp: result.score_cp,
        pv: result.pv.iter().map(ToString::to_string).collect(),
    };
    if let Ok(json) = serde_json::to_string(&info) {
        let _ = tx.send(json).await;
    }
    let response = WsResponse::BestMove {
        request_id,
        bestmove: result.best_move.to_string(),
        ponder: result.ponder_move.map(|m| m.to_string()),
    };
    if let Ok(json) = serde_json::to_string(&response) {
        let _ = tx.send(json).await;
    }
}

async fn finish_active(
    active: &mut Option<(Option<String>, SearchHandle)>,
    tx: &mpsc::Sender<String>,
) {
    if let Some((id, handle)) = active.take() {
        handle.stop();
        match super::wait_for_search(handle).await {
            Ok(result) => send_bestmove(id, &result, tx).await,
            Err(error) => send_error(id, "SEARCH_ERROR", error.to_string(), tx).await,
        }
    }
}

async fn handle_position(session: &Session, cmd: &WsCommand, tx: &mpsc::Sender<String>) {
    let fen = cmd
        .fen
        .as_deref()
        .unwrap_or("rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1");
    let moves: Vec<&str> = cmd
        .moves
        .as_ref()
        .map(|v| v.iter().map(String::as_str).collect())
        .unwrap_or_default();

    session.touch().await;
    let mut guard = session.engine.lock().await;
    let Some(engine) = guard.as_mut() else {
        return;
    };
    let result = engine.set_position(fen, &moves);
    drop(guard);

    if let Err(e) = result {
        let err = serde_json::to_string(&WsResponse::Error {
            request_id: cmd.id.clone(),
            code: "INVALID_POSITION".to_owned(),
            message: e.to_string(),
        })
        .unwrap_or_default();
        let _ = tx.send(err).await;
    }
}

async fn start_search(session: &Session, cmd: &WsCommand) -> Result<SearchHandle, String> {
    let params = cmd.params.as_ref();
    let mut limits = SearchLimits {
        depth: params.and_then(|p| p.depth),
        nodes: params.and_then(|p| p.nodes),
        movetime: params.and_then(|p| p.movetime),
        movestogo: params.and_then(|p| p.movestogo),
        time: [params.and_then(|p| p.wtime), params.and_then(|p| p.btime)],
        inc: [params.and_then(|p| p.winc), params.and_then(|p| p.binc)],
        infinite: params.is_some_and(|p| p.infinite.unwrap_or(false)),
        ponder: params.is_some_and(|p| p.ponder.unwrap_or(false)),
        search_moves: params.map(|p| p.searchmoves.clone()).unwrap_or_default(),
    };
    if limits.depth.is_none()
        && limits.nodes.is_none()
        && limits.movetime.is_none()
        && limits.time == [None; 2]
        && !limits.infinite
        && !limits.ponder
    {
        limits.depth = Some(10);
    }
    let mut guard = session.engine.lock().await;
    let engine = guard.as_mut().ok_or("session has expired")?;
    let handle = engine.try_go(&limits);
    drop(guard);
    handle.map_err(|error| error.to_string())
}

async fn handle_new_game(session: &Session, cmd: &WsCommand, tx: &mpsc::Sender<String>) {
    session.touch().await;
    let mut guard = session.engine.lock().await;
    let Some(engine) = guard.as_mut() else {
        return;
    };
    let result = engine.new_game();
    drop(guard);

    if let Err(e) = result {
        let err = serde_json::to_string(&WsResponse::Error {
            request_id: cmd.id.clone(),
            code: "ENGINE_ERROR".to_owned(),
            message: e.to_string(),
        })
        .unwrap_or_default();
        let _ = tx.send(err).await;
    }
}

async fn handle_set_option(session: &Session, cmd: &WsCommand, tx: &mpsc::Sender<String>) {
    let name = cmd.name.as_deref().unwrap_or("");
    let value = cmd.value.as_deref().unwrap_or("");
    if name.eq_ignore_ascii_case("Threads") || name.eq_ignore_ascii_case("Hash") {
        send_error(
            cmd.id.clone(),
            "OPTION_ERROR",
            "Threads and Hash are configured by the server".to_owned(),
            tx,
        )
        .await;
        return;
    }

    session.touch().await;
    let mut guard = session.engine.lock().await;
    let Some(engine) = guard.as_mut() else {
        return;
    };
    let result = engine.set_option(name, value);
    drop(guard);

    if let Err(e) = result {
        let err = serde_json::to_string(&WsResponse::Error {
            request_id: cmd.id.clone(),
            code: "OPTION_ERROR".to_owned(),
            message: e.to_string(),
        })
        .unwrap_or_default();
        let _ = tx.send(err).await;
    }
}

#[derive(Deserialize)]
struct WsCommand {
    #[serde(default)]
    id: Option<String>,
    cmd: String,
    #[serde(default)]
    fen: Option<String>,
    #[serde(default)]
    moves: Option<Vec<String>>,
    #[serde(default)]
    params: Option<GoParams>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    value: Option<String>,
}

#[derive(Deserialize)]
struct GoParams {
    #[serde(default)]
    depth: Option<i32>,
    #[serde(default)]
    nodes: Option<u64>,
    #[serde(default)]
    movestogo: Option<i32>,
    #[serde(default)]
    ponder: Option<bool>,
    #[serde(default)]
    searchmoves: Vec<String>,
    #[serde(default)]
    movetime: Option<i64>,
    #[serde(default)]
    wtime: Option<i64>,
    #[serde(default)]
    btime: Option<i64>,
    #[serde(default)]
    winc: Option<i64>,
    #[serde(default)]
    binc: Option<i64>,
    #[serde(default)]
    infinite: Option<bool>,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum WsResponse {
    #[serde(rename = "session")]
    Session {
        session_id: String,
        engine_version: String,
    },
    #[serde(rename = "info")]
    Info {
        request_id: Option<String>,
        depth: i32,
        seldepth: i32,
        nodes: u64,
        score_cp: i32,
        pv: Vec<String>,
    },
    #[serde(rename = "bestmove")]
    BestMove {
        request_id: Option<String>,
        #[serde(rename = "move")]
        bestmove: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        ponder: Option<String>,
    },
    #[serde(rename = "error")]
    Error {
        request_id: Option<String>,
        code: String,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::session::SessionManager;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn consecutive_searches_and_stop_preserve_request_order() {
        let pool = crate::server::pool::model_free_pool(1);
        let sessions = SessionManager::new(Arc::clone(&pool), 1, Duration::from_secs(60));
        let session = sessions.create_session().await.unwrap();
        let command: WsCommand =
            serde_json::from_str(r#"{"cmd":"go","id":"first","params":{"infinite":true}}"#)
                .unwrap();
        let handle = start_search(&session, &command).await.unwrap();
        let mut active = Some((command.id, handle));
        let (tx, mut rx) = mpsc::channel(4);
        tokio::time::timeout(Duration::from_secs(2), finish_active(&mut active, &tx))
            .await
            .unwrap();
        let response: serde_json::Value = serde_json::from_str(&rx.recv().await.unwrap()).unwrap();
        assert_eq!(response["type"], "info");
        assert_eq!(response["request_id"], "first");
        let response: serde_json::Value = serde_json::from_str(&rx.recv().await.unwrap()).unwrap();
        assert_eq!(response["type"], "bestmove");
        assert_eq!(response["request_id"], "first");
        let command: WsCommand =
            serde_json::from_str(r#"{"cmd":"go","id":"second","params":{"depth":1}}"#).unwrap();
        let handle = start_search(&session, &command).await.unwrap();
        assert_ne!(
            super::super::wait_for_search(handle)
                .await
                .unwrap()
                .best_move,
            pikarust_core::types::Move::NONE
        );
        sessions.destroy_session(&session.id).await;
        drop(session);
        drop(sessions);
        assert_eq!(pool.active_count(), 0);
    }
}
