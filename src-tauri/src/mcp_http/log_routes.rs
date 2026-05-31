//! HTTP endpoints for the application log ring buffer.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use std::sync::Arc;

use crate::AppState;
use crate::app_logger::LogEntry;

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
pub(crate) async fn clear_logs(State(state): State<Arc<AppState>>) -> StatusCode {
    let mut buf = state.log_buffer.lock();
    buf.clear();
    StatusCode::NO_CONTENT
}

// ---------------------------------------------------------------------------
// Diagnostic mode toggle
// ---------------------------------------------------------------------------

/// GET /diagnostics — current diagnostic mode state.
pub(crate) async fn diagnostics_get() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "enabled": crate::cpu_watchdog::diagnostic_mode(),
    }))
}

/// POST /diagnostics — toggle diagnostic mode. Body: `{ "enabled": true }`.
pub(crate) async fn diagnostics_set(
    Json(body): Json<super::types::SetApiDebugRequest>,
) -> Json<serde_json::Value> {
    crate::cpu_watchdog::set_diagnostic_mode(body.enabled);
    Json(serde_json::json!({
        "ok": true,
        "enabled": body.enabled,
    }))
}
