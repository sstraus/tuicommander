use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use crate::AppState;
use super::types::PathQuery;
use super::{err_500, validate_repo_path};

/// Start a repo watcher for a repository via HTTP (browser-only mode).
/// The unified watcher covers HEAD, git state, and working tree changes.
pub(super) async fn start_repo_watcher_http(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let app_handle = state.app_handle.read().clone();
    match crate::repo_watcher::start_watching(&q.path, app_handle.as_ref(), &state) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

/// Stop a repo watcher for a repository via HTTP.
pub(super) async fn stop_repo_watcher_http(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    crate::repo_watcher::stop_watching(&q.path, &state);
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

/// Start a directory watcher via HTTP (browser-only mode).
pub(super) async fn start_dir_watcher_http(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let app_handle = state.app_handle.read().clone();
    match crate::dir_watcher::start_watching(&q.path, app_handle.as_ref(), &state) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

/// Stop a directory watcher via HTTP.
pub(super) async fn stop_dir_watcher_http(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    crate::dir_watcher::stop_watching(&q.path, &state);
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}
