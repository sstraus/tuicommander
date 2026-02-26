//! HTTP endpoints for the application log ring buffer.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

use crate::app_logger::LogEntry;
use crate::AppState;

#[derive(Deserialize)]
pub(crate) struct GetLogsQuery {
    #[serde(default)]
    limit: usize,
    /// Optional minimum level filter: "debug", "info", "warn", "error"
    #[serde(default)]
    level: Option<String>,
    /// Optional source filter: "app", "plugin", "git", "terminal", etc.
    #[serde(default)]
    source: Option<String>,
}

/// GET /logs — retrieve log entries from the ring buffer.
pub(crate) async fn get_logs(
    State(state): State<Arc<AppState>>,
    Query(q): Query<GetLogsQuery>,
) -> Json<Vec<LogEntry>> {
    let buf = state.log_buffer.lock();
    let mut entries = buf.get_entries(q.limit);

    // Apply optional filters
    if let Some(ref level) = q.level {
        entries.retain(|e| e.level == *level);
    }
    if let Some(ref source) = q.source {
        entries.retain(|e| e.source == *source);
    }

    Json(entries)
}

#[derive(Deserialize)]
pub(crate) struct PushLogBody {
    level: String,
    source: String,
    message: String,
    data_json: Option<String>,
}

/// POST /logs — push a log entry into the ring buffer.
pub(crate) async fn push_log(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PushLogBody>,
) -> StatusCode {
    let mut buf = state.log_buffer.lock();
    buf.push(body.level, body.source, body.message, body.data_json);
    StatusCode::NO_CONTENT
}

/// DELETE /logs — clear all log entries.
pub(crate) async fn clear_logs(
    State(state): State<Arc<AppState>>,
) -> StatusCode {
    let mut buf = state.log_buffer.lock();
    buf.clear();
    StatusCode::NO_CONTENT
}
