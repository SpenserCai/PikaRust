use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use pikarust_core::engine::SearchLimits;
use pikarust_core::position::Position;
use serde::{Deserialize, Serialize};

use super::AppState;
use super::pool::PoolError;

pub fn rest_router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/evaluate", post(evaluate_handler))
        .route("/api/v1/bestmove", post(bestmove_handler))
        .route("/api/v1/fen/validate", post(validate_fen_handler))
        .route("/api/v1/health", get(health_handler))
        .route("/api/v1/status", get(status_handler))
}

#[derive(Deserialize)]
struct EvaluateRequest {
    fen: String,
    #[serde(default = "default_depth")]
    depth: i32,
}

#[derive(Serialize)]
struct EvaluateResponse {
    score: ScoreInfo,
    depth: i32,
    nodes: u64,
    pv: Vec<String>,
}

#[derive(Deserialize)]
struct BestMoveRequest {
    fen: String,
    #[serde(default)]
    depth: Option<i32>,
    #[serde(default)]
    movetime: Option<i64>,
}

#[derive(Serialize)]
struct BestMoveResponse {
    bestmove: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ponder: Option<String>,
    score: ScoreInfo,
    depth: i32,
}

#[derive(Deserialize)]
struct ValidateFenRequest {
    fen: String,
}

#[derive(Serialize)]
struct ValidateFenResponse {
    valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct StatusResponse {
    active_sessions: usize,
    pool_active: usize,
    pool_available: usize,
}

#[derive(Serialize)]
struct ScoreInfo {
    cp: i32,
}

const fn default_depth() -> i32 {
    10
}

async fn evaluate_handler(
    State(state): State<AppState>,
    Json(req): Json<EvaluateRequest>,
) -> Result<Json<EvaluateResponse>, AppError> {
    let mut engine = state.pool.acquire().await.map_err(AppError::Pool)?;

    engine
        .set_position(&req.fen, &[])
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let limits = SearchLimits {
        depth: Some(req.depth),
        ..SearchLimits::default()
    };

    let (engine, search_result) = tokio::task::spawn_blocking(move || {
        let r = engine.go(&limits).wait();
        (engine, r)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Box::pin(state.pool.release(engine)).await;

    Ok(Json(EvaluateResponse {
        score: ScoreInfo {
            cp: search_result.score,
        },
        depth: search_result.depth,
        nodes: search_result.nodes,
        pv: vec![search_result.best_move.to_string()],
    }))
}

async fn bestmove_handler(
    State(state): State<AppState>,
    Json(req): Json<BestMoveRequest>,
) -> Result<Json<BestMoveResponse>, AppError> {
    let mut engine = state.pool.acquire().await.map_err(AppError::Pool)?;

    engine
        .set_position(&req.fen, &[])
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let limits = SearchLimits {
        depth: req.depth,
        movetime: req.movetime,
        ..SearchLimits::default()
    };

    let (engine, search_result) = tokio::task::spawn_blocking(move || {
        let r = engine.go(&limits).wait();
        (engine, r)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Box::pin(state.pool.release(engine)).await;

    Ok(Json(BestMoveResponse {
        bestmove: search_result.best_move.to_string(),
        ponder: search_result.ponder_move.map(|m| m.to_string()),
        score: ScoreInfo {
            cp: search_result.score,
        },
        depth: search_result.depth,
    }))
}

async fn validate_fen_handler(Json(req): Json<ValidateFenRequest>) -> Json<ValidateFenResponse> {
    match Position::from_fen(&req.fen) {
        Ok(_) => Json(ValidateFenResponse {
            valid: true,
            error: None,
        }),
        Err(e) => Json(ValidateFenResponse {
            valid: false,
            error: Some(e.to_string()),
        }),
    }
}

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn status_handler(State(state): State<AppState>) -> Json<StatusResponse> {
    Json(StatusResponse {
        active_sessions: state.session_mgr.session_count(),
        pool_active: state.pool.active_count(),
        pool_available: state.pool.available_count().await,
    })
}

pub enum AppError {
    BadRequest(String),
    Pool(PoolError),
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::Pool(PoolError::Exhausted) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "engine pool exhausted".to_owned(),
            ),
            Self::Pool(PoolError::Engine(e)) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = serde_json::json!({ "error": message });
        (status, Json(body)).into_response()
    }
}
