use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use pikarust_core::engine::SearchLimits;

use super::wait_for_search;
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

    let handle = engine
        .try_go(&limits)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let search_result = wait_for_search(handle)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    drop(engine);

    Ok(Json(EvaluateResponse {
        score: ScoreInfo {
            cp: search_result.score_cp,
        },
        depth: search_result.depth,
        nodes: search_result.nodes,
        pv: search_result.pv.iter().map(ToString::to_string).collect(),
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
        depth: req
            .depth
            .or_else(|| req.movetime.is_none().then_some(default_depth())),
        movetime: req.movetime,
        ..SearchLimits::default()
    };

    let handle = engine
        .try_go(&limits)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let search_result = wait_for_search(handle)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    drop(engine);

    Ok(Json(BestMoveResponse {
        bestmove: search_result.best_move.to_string(),
        ponder: search_result.ponder_move.map(|m| m.to_string()),
        score: ScoreInfo {
            cp: search_result.score_cp,
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
        pool_available: state.pool.available_count(),
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
            Self::Pool(PoolError::Initialization(msg)) | Self::Internal(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, msg)
            }
        };

        let body = serde_json::json!({ "error": message });
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::session::SessionManager;
    use std::sync::Arc;
    use std::time::Duration;

    fn state() -> AppState {
        let pool = crate::server::pool::model_free_pool(1);
        AppState {
            session_mgr: Arc::new(SessionManager::new(
                Arc::clone(&pool),
                1,
                Duration::from_secs(60),
            )),
            pool,
        }
    }

    #[tokio::test]
    async fn invalid_requests_do_not_exhaust_the_pool() {
        let state = state();
        for _ in 0..3 {
            let error = bestmove_handler(
                State(state.clone()),
                Json(BestMoveRequest {
                    fen: "invalid".to_owned(),
                    depth: Some(1),
                    movetime: None,
                }),
            )
            .await;
            assert!(matches!(error, Err(AppError::BadRequest(_))));
            assert_eq!(state.pool.active_count(), 0);
        }
        let result = bestmove_handler(
            State(state.clone()),
            Json(BestMoveRequest {
                fen: Position::start_pos().unwrap().fen(),
                depth: Some(1),
                movetime: None,
            }),
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(state.pool.active_count(), 0);
        drop(state);
    }

    #[tokio::test]
    async fn zero_depth_is_a_bad_request_instead_of_unbounded_search() {
        let state = state();
        let error = evaluate_handler(
            State(state.clone()),
            Json(EvaluateRequest {
                fen: Position::start_pos().unwrap().fen(),
                depth: 0,
            }),
        )
        .await;
        assert!(matches!(error, Err(AppError::BadRequest(_))));
        assert_eq!(state.pool.active_count(), 0);
        drop(state);
    }
}
