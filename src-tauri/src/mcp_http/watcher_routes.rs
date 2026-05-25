use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use super::types::PathQuery;
use super::{err_500, validate_repo_path};
use crate::AppState;

/// Start a repo watcher for a repository via HTTP (browser-only mode).
/// The unified watcher covers HEAD, git state, and working tree changes.
pub(super) async fn start_repo_watcher_http(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    match crate::repo_watcher::start_watching(&q.path, &state) {
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
    match crate::dir_watcher::start_watching(&q.path, &state) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

/// Update the set of hot (active-terminal) repo paths via HTTP.
pub(super) async fn set_hot_repos_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let paths: Vec<String> = body
        .get("paths")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let mut hot = state.hot_repo_paths.write();
    hot.clear();
    hot.extend(paths);
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
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
