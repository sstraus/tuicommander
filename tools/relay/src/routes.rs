use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::relay::{self, AppState};
use crate::types::PushSubscription;
use crate::{auth, db};

/// Build the Axum router with all relay endpoints.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/register", post(register))
        .route("/stats", get(stats))
        .route("/push/subscribe", post(push_subscribe).delete(push_unsubscribe))
        .route("/ws/{session_id}", get(ws_upgrade))
        .with_state(state)
}

/// Health check endpoint.
async fn health() -> &'static str {
    "ok"
}

/// Registration response.
#[derive(Serialize)]
struct RegisterResponse {
    token: String,
}

/// Self-registration: generate a new bearer token.
async fn register(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> Response {
    if !state.check_rate_limit(addr.ip()) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let token = auth::generate_token();
    let hash = match auth::hash_token(&token) {
        Ok(h) => h,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    // Store in DB if available
    if let Some(conn) = &state.db {
        if let Err(e) = db::insert_token(conn, &hash).await {
            tracing::error!(error = %e, "failed to store token");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    // Cache for fast WS auth lookup
    state.token_cache.insert(token.clone(), hash);

    (StatusCode::CREATED, Json(RegisterResponse { token })).into_response()
}

/// Bearer token extraction from Authorization header.
fn extract_bearer(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

/// Verify a bearer token against the cache.
fn verify_bearer(state: &AppState, token: &str) -> Option<String> {
    // Look up in cache: token → hash
    state
        .token_cache
        .get(token)
        .map(|entry| entry.value().clone())
}

/// Stats response.
#[derive(Serialize)]
struct StatsResponse {
    total_sessions: i64,
    total_bytes: i64,
    created_at: i64,
    last_seen: i64,
}

/// Per-user stats endpoint (requires bearer auth).
async fn stats(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Response {
    let Some(token) = extract_bearer(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(hash) = verify_bearer(&state, token) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let Some(conn) = &state.db else {
        return (StatusCode::OK, Json(StatsResponse {
            total_sessions: 0,
            total_bytes: 0,
            created_at: 0,
            last_seen: 0,
        }))
        .into_response();
    };

    match db::token_stats(conn, &hash).await {
        Ok(Some(s)) => (
            StatusCode::OK,
            Json(StatsResponse {
                total_sessions: s.total_sessions,
                total_bytes: s.total_bytes,
                created_at: s.created_at,
                last_seen: s.last_seen,
            }),
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "stats query failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Register a browser push subscription (requires bearer auth).
async fn push_subscribe(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(body): Json<PushSubscription>,
) -> Response {
    let Some(token) = extract_bearer(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(hash) = verify_bearer(&state, token) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(conn) = &state.db else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };

    match db::insert_push_sub(conn, &hash, &body.endpoint, &body.p256dh, &body.auth).await {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to store push subscription");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Request body for unsubscribing: only the endpoint is needed.
#[derive(Deserialize)]
struct PushUnsubscribeRequest {
    endpoint: String,
}

/// Remove a browser push subscription (requires bearer auth).
async fn push_unsubscribe(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(body): Json<PushUnsubscribeRequest>,
) -> Response {
    let Some(token) = extract_bearer(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(hash) = verify_bearer(&state, token) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(conn) = &state.db else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };

    match db::delete_push_sub(conn, &hash, &body.endpoint).await {
        Ok(true) => StatusCode::OK.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to delete push subscription");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// WebSocket upgrade handler for relay sessions.
async fn ws_upgrade(
    ws: WebSocketUpgrade,
    Path(session_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    if session_id.is_empty() || session_id.len() > 64 {
        return StatusCode::BAD_REQUEST.into_response();
    }

    ws.on_upgrade(move |socket| relay::handle_ws(state, session_id, socket))
}
