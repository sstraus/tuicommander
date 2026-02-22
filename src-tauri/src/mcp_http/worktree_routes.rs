use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::sync::Arc;

use super::types::*;
use super::validate_repo_path;

pub(super) async fn list_worktrees_http(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let worktrees: Vec<serde_json::Value> = state
        .sessions
        .iter()
        .filter_map(|entry| {
            let session = entry.value().lock();
            session.worktree.as_ref().map(|wt| {
                serde_json::json!({
                    "session_id": entry.key(),
                    "name": wt.name,
                    "path": wt.path.to_string_lossy(),
                    "branch": wt.branch,
                    "base_repo": wt.base_repo.to_string_lossy(),
                })
            })
        })
        .collect();
    Json(worktrees)
}

pub(super) async fn get_worktrees_dir_http(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    Json(serde_json::json!({"dir": state.worktrees_dir.to_string_lossy()}))
}

pub(super) async fn get_worktree_paths_http(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    match crate::worktree::get_worktree_paths(q.path) {
        Ok(paths) => (StatusCode::OK, Json(serde_json::json!(paths))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn create_worktree_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateWorktreeRequest>,
) -> impl IntoResponse {
    let config = crate::worktree::WorktreeConfig {
        task_name: body.branch_name.clone(),
        base_repo: body.base_repo,
        branch: Some(body.branch_name),
        create_branch: true,
    };
    match crate::worktree::create_worktree_internal(&state.worktrees_dir, &config) {
        Ok(wt) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "name": wt.name,
                "path": wt.path.to_string_lossy(),
                "branch": wt.branch,
                "base_repo": wt.base_repo.to_string_lossy(),
            })),
        ),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))),
    }
}

pub(super) async fn remove_worktree_http(
    Path(branch): Path<String>,
    Query(q): Query<RemoveWorktreeQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) { return e.into_response(); }
    match crate::worktree::remove_worktree_by_branch(&q.repo_path, &branch) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn generate_worktree_name_http(
    Json(body): Json<GenerateWorktreeNameRequest>,
) -> impl IntoResponse {
    Json(crate::worktree::generate_worktree_name_cmd(body.existing_names))
}
