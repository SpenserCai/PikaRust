#![forbid(unsafe_code)]

use axum::{
    Router,
    extract::{State, WebSocketUpgrade, ws::Message},
    response::IntoResponse,
    routing::get,
};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
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

#[allow(
    clippy::redundant_pub_crate,
    reason = "Tokio select emits private helper types that trigger MSRV Clippy"
)]
async fn handle_socket_inner(
    socket: axum::extract::ws::WebSocket,
    engine_path: &Path,
) -> anyhow::Result<()> {
    let mut child = tokio::process::Command::new(engine_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        // Cover every early-return and cancelled connection path.
        .kill_on_drop(true)
        .spawn()?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("engine stdin unavailable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("engine stdout unavailable"))?;
    let mut reader = BufReader::new(stdout).lines();

    stdin.write_all(b"uci\n").await?;

    let (mut ws_tx, mut ws_rx) = socket.split();

    // Both streams belong to this connection. Engine exit and socket closure
    // terminate the other direction without leaving a detached forwarding task.
    loop {
        tokio::select! {
            line = reader.next_line() => match line? {
                Some(line) => ws_tx.send(Message::Text(line.into())).await?,
                None => break,
            },
            message = ws_rx.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    stdin.write_all(text.as_bytes()).await?;
                    if !text.ends_with('\n') {
                        stdin.write_all(b"\n").await?;
                    }
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
    let _ = stdin.write_all(b"stop\nquit\n").await;
    drop(stdin);
    if tokio::time::timeout(Duration::from_secs(2), child.wait())
        .await
        .is_err()
    {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    Ok(())
}
