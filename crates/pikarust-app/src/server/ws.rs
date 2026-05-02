use std::sync::Arc;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::SinkExt;
use futures_util::stream::StreamExt;
use pikarust_core::engine::SearchLimits;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::AppState;
use super::session::Session;

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_session(socket, state))
}

async fn handle_session(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    let session = match state.session_mgr.create_session().await {
        Ok(s) => s,
        Err(e) => {
            let err = serde_json::to_string(&WsResponse::Error {
                request_id: None,
                code: "SESSION_ERROR".to_owned(),
                message: e.to_string(),
            })
            .unwrap_or_default();
            let _ = sender.send(Message::Text(err.into())).await;
            return;
        }
    };

    let session_msg = serde_json::to_string(&WsResponse::Session {
        session_id: session.id.clone(),
        engine_version: env!("CARGO_PKG_VERSION").to_owned(),
    })
    .unwrap_or_default();
    let _ = sender.send(Message::Text(session_msg.into())).await;

    let (info_tx, mut info_rx) = mpsc::channel::<String>(64);

    let send_task = tokio::spawn(async move {
        while let Some(msg) = info_rx.recv().await {
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = receiver.next().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };

        let cmd: WsCommand = match serde_json::from_str(&text) {
            Ok(c) => c,
            Err(e) => {
                let err = serde_json::to_string(&WsResponse::Error {
                    request_id: None,
                    code: "PARSE_ERROR".to_owned(),
                    message: e.to_string(),
                })
                .unwrap_or_default();
                let _ = info_tx.send(err).await;
                continue;
            }
        };

        match cmd.cmd.as_str() {
            "position" => {
                handle_position(&session, &cmd, &info_tx).await;
            }
            "go" => {
                let sess = Arc::clone(&session);
                let tx = info_tx.clone();
                tokio::spawn(async move {
                    handle_go(&sess, &cmd, &tx).await;
                });
            }
            "stop" => {
                handle_stop(&session).await;
            }
            "ucinewgame" => {
                handle_new_game(&session, &cmd, &info_tx).await;
            }
            "setoption" => {
                handle_set_option(&session, &cmd, &info_tx).await;
            }
            _ => {
                let err = serde_json::to_string(&WsResponse::Error {
                    request_id: cmd.id.clone(),
                    code: "UNKNOWN_COMMAND".to_owned(),
                    message: format!("unknown command: {}", cmd.cmd),
                })
                .unwrap_or_default();
                let _ = info_tx.send(err).await;
            }
        }
    }

    send_task.abort();
    Box::pin(state.session_mgr.destroy_session(&session.id)).await;
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

async fn handle_go(session: &Session, cmd: &WsCommand, tx: &mpsc::Sender<String>) {
    let request_id = cmd.id.clone();
    let params = cmd.params.as_ref();

    let limits = SearchLimits {
        depth: params.and_then(|p| p.depth),
        movetime: params.and_then(|p| p.movetime),
        time: [params.and_then(|p| p.wtime), params.and_then(|p| p.btime)],
        inc: [params.and_then(|p| p.winc), params.and_then(|p| p.binc)],
        infinite: params.is_some_and(|p| p.infinite.unwrap_or(false)),
        ..SearchLimits::default()
    };

    session.touch().await;
    let result = {
        let mut guard = session.engine.lock().await;
        let Some(engine) = guard.as_mut() else {
            return;
        };
        let handle = engine.go(&limits);
        drop(guard);
        tokio::task::spawn_blocking(move || handle.wait())
            .await
            .unwrap_or_default()
    };

    let resp = serde_json::to_string(&WsResponse::BestMove {
        request_id,
        bestmove: result.best_move.to_string(),
        ponder: result.ponder_move.map(|m| m.to_string()),
    })
    .unwrap_or_default();
    let _ = tx.send(resp).await;
}

async fn handle_stop(session: &Session) {
    let guard = session.engine.lock().await;
    if let Some(engine) = guard.as_ref() {
        engine.stop();
    }
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
