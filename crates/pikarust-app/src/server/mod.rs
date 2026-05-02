pub mod http;
pub mod pool;
pub mod session;
pub mod ws;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::http::Method;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use self::pool::{EnginePool, PoolConfig, SharedPool};
use self::session::{SessionManager, SharedSessionManager};

#[derive(Clone)]
pub struct AppState {
    pub pool: SharedPool,
    pub session_mgr: SharedSessionManager,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub pool: PoolConfig,
    pub max_sessions: usize,
    pub idle_timeout_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::from(([0, 0, 0, 0], 8080)),
            pool: PoolConfig::default(),
            max_sessions: 64,
            idle_timeout_secs: 600,
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/ws", axum::routing::get(ws::ws_handler))
        .merge(http::rest_router())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST])
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub fn create_app_state(config: &ServerConfig) -> AppState {
    let pool = Arc::new(EnginePool::new(config.pool.clone()));
    let session_mgr = Arc::new(SessionManager::new(
        Arc::clone(&pool),
        config.max_sessions,
        Duration::from_secs(config.idle_timeout_secs),
    ));
    AppState { pool, session_mgr }
}

pub async fn run_cleanup_task(session_mgr: SharedSessionManager, interval_secs: u64) {
    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
    loop {
        interval.tick().await;
        Box::pin(session_mgr.cleanup_idle()).await;
    }
}
