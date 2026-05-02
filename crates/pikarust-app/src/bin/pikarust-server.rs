use std::net::SocketAddr;
use std::sync::Arc;

use log::info;
use pikarust_app::server::pool::PoolConfig;
use pikarust_app::server::{ServerConfig, build_router, create_app_state, run_cleanup_task};

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let host: [u8; 4] = [0, 0, 0, 0];
    let port: u16 = parse_env("PIKARUST_PORT", 8080);
    let bind_addr = SocketAddr::from((host, port));

    let config = ServerConfig {
        bind_addr,
        pool: PoolConfig {
            max_engines: parse_env("PIKARUST_MAX_ENGINES", 8),
            threads_per_engine: parse_env("PIKARUST_THREADS_PER_ENGINE", 1),
            hash_mb_per_engine: parse_env("PIKARUST_HASH_MB", 16),
        },
        max_sessions: parse_env("PIKARUST_MAX_SESSIONS", 64),
        idle_timeout_secs: parse_env("PIKARUST_IDLE_TIMEOUT", 600),
    };

    let state = create_app_state(&config);
    let session_mgr = Arc::clone(&state.session_mgr);

    tokio::spawn(run_cleanup_task(session_mgr, 60));

    let router = build_router(state);

    info!("PikaRust server listening on {}", config.bind_addr);

    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .expect("failed to bind address");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    info!("Server shut down");
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install signal handler");
    info!("Shutdown signal received");
}
