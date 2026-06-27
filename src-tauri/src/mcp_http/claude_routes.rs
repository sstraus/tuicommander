//! HTTP routes for the Claude Usage dashboard (browser/PWA/remote parity, story 063).
//!
//! `get_claude_usage_api` / `get_claude_project_list` are plain async fns callable
//! in both builds. `timeline` / `session_stats` are desktop-only `#[tauri::command]`s,
//! so these handlers call their non-gated `*_impl` siblings instead.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::Response;

use super::json_result;
use super::types::{ClaudeStatsQuery, ClaudeTimelineQuery};
use crate::AppState;

pub(super) async fn claude_usage_api() -> Response {
    json_result(crate::claude_usage::get_claude_usage_api().await)
}

pub(super) async fn claude_project_list() -> Response {
    json_result(crate::claude_usage::get_claude_project_list().await)
}

pub(super) async fn claude_usage_timeline(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ClaudeTimelineQuery>,
) -> Response {
    json_result(crate::claude_usage::get_claude_usage_timeline_impl(&state, q.scope, q.days).await)
}

pub(super) async fn claude_session_stats(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ClaudeStatsQuery>,
) -> Response {
    json_result(crate::claude_usage::get_claude_session_stats_impl(&state, q.scope).await)
}
