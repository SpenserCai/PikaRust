#![forbid(unsafe_code)]

use axum::{
    Router,
    extract::{State, WebSocketUpgrade, ws::Message},
    response::IntoResponse,
    routing::get,
};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tower_http::{cors::CorsLayer, services::ServeDir};

#[derive(Parser)]
struct Args {
    /// Path to the UCI engine binary
    #[arg(long, env = "PIKARUST_ENGINE_PATH")]
    engine_path: PathBuf,

    /// Port to listen on
    #[arg(long, default_value_t = 9000)]
    port: u16,

    /// Directory to serve static files from
    #[arg(long, default_value = "./dist")]
    static_dir: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let engine_path = Arc::new(args.engine_path);

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(engine_path)
        .fallback_service(ServeDir::new(&args.static_dir))
        .layer(CorsLayer::permissive());

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    tracing::info!("listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[allow(clippy::unused_async)]
async fn ws_handler(
    State(engine_path): State<Arc<PathBuf>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, engine_path))
}

async fn handle_socket(socket: axum::extract::ws::WebSocket, engine_path: Arc<PathBuf>) {
    if let Err(e) = handle_socket_inner(socket, &engine_path).await {
        tracing::error!("websocket error: {e}");
    }
}

async fn handle_socket_inner(
    socket: axum::extract::ws::WebSocket,
    engine_path: &PathBuf,
) -> anyhow::Result<()> {
    let mut child = tokio::process::Command::new(engine_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout).lines();

    stdin.write_all(b"uci\n").await?;

    let (mut ws_tx, mut ws_rx) = socket.split();

    // Forward lines until uciok
    while let Some(line) = reader.next_line().await? {
        let done = line.contains("uciok");
        ws_tx.send(Message::Text(line.into())).await?;
        if done {
            break;
        }
    }

    // Spawn task: engine stdout -> websocket
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let forward_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                line = reader.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            if ws_tx.send(Message::Text(l.into())).await.is_err() {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
                _ = &mut shutdown_rx => break,
            }
        }
    });

    // Client -> engine stdin
    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            Message::Text(text) => {
                let mut cmd = text.to_string();
                if !cmd.ends_with('\n') {
                    cmd.push('\n');
                }
                if stdin.write_all(cmd.as_bytes()).await.is_err() {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    // Cleanup
    let _ = stdin.write_all(b"quit\n").await;
    drop(stdin);
    let _ = shutdown_tx.send(());
    let _ = forward_task.await;
    let _ = child.kill().await;
    Ok(())
}
