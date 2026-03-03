use std::sync::Arc;

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::relay::{self, AppState};

/// Build the Axum router with all relay endpoints.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ws/{session_id}", get(ws_upgrade))
        .with_state(state)
}

/// Health check endpoint.
async fn health() -> &'static str {
    "ok"
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
