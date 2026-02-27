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
    Query(q): Query<OptionalRepoQuery>,
) -> impl IntoResponse {
    let dir = match q.repo_path {
        Some(rp) => crate::worktree::resolve_worktree_dir_for_repo(
            std::path::Path::new(&rp),
            &state.worktrees_dir,
        )
        .to_string_lossy()
        .to_string(),
        None => state.worktrees_dir.to_string_lossy().to_string(),
    };
    Json(serde_json::json!({"dir": dir}))
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
    // Model provides only branch_name and optionally base_ref (start point).
    // Storage path and strategy come entirely from user config via resolve_worktree_dir_for_repo.
    let config = crate::worktree::WorktreeConfig {
        task_name: body.branch_name.clone(),
        base_repo: body.base_repo.clone(),
        branch: Some(body.branch_name),
        create_branch: true, // Always create a new branch — model must not control this
    };
    let worktrees_dir = crate::worktree::resolve_worktree_dir_for_repo(
        std::path::Path::new(&config.base_repo),
        &state.worktrees_dir,
    );
    match crate::worktree::create_worktree_internal(&worktrees_dir, &config, body.base_ref.as_deref()) {
        Ok(wt) => {
            state.invalidate_repo_caches(&body.base_repo);
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "name": wt.name,
                    "path": wt.path.to_string_lossy(),
                    "branch": wt.branch,
                    "base_repo": wt.base_repo.to_string_lossy(),
                })),
            )
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))),
    }
}

pub(super) async fn remove_worktree_http(
    Path(branch): Path<String>,
    Query(q): Query<RemoveWorktreeQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) { return e.into_response(); }
    match crate::worktree::remove_worktree_by_branch(&q.repo_path, &branch, q.delete_branch.unwrap_or(true)) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn detect_orphan_worktrees_http(Query(q): Query<OptionalRepoQuery>) -> Response {
    let repo_path = match q.repo_path {
        Some(p) => p,
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "repoPath required"}))).into_response(),
    };
    if let Err(e) = validate_repo_path(&repo_path) { return e.into_response(); }
    match crate::worktree::detect_orphan_worktrees(repo_path) {
        Ok(paths) => (StatusCode::OK, Json(serde_json::json!(paths))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn remove_orphan_worktree_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::RemoveOrphanRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    let worktree = crate::state::WorktreeInfo {
        name: std::path::Path::new(&body.worktree_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| body.worktree_path.clone()),
        path: std::path::PathBuf::from(&body.worktree_path),
        branch: None,
        base_repo: std::path::PathBuf::from(&body.repo_path),
    };
    match crate::worktree::remove_worktree_internal(&worktree) {
        Ok(()) => {
            state.invalidate_repo_caches(&body.repo_path);
            (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn generate_worktree_name_http(
    Json(body): Json<GenerateWorktreeNameRequest>,
) -> impl IntoResponse {
    Json(crate::worktree::generate_worktree_name_cmd(body.existing_names))
}

pub(super) async fn list_local_branches_http(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    match crate::worktree::list_local_branches(q.path) {
        Ok(branches) => (StatusCode::OK, Json(serde_json::json!(branches))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn checkout_remote_branch_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::CheckoutRemoteRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    let repo = std::path::PathBuf::from(&body.repo_path);
    let remote_ref = format!("origin/{}", body.branch_name);
    match crate::git_cli::git_cmd(&repo)
        .args(&["checkout", "-b", &body.branch_name, &remote_ref])
        .run()
    {
        Ok(_) => {
            state.invalidate_repo_caches(&body.repo_path);
            (StatusCode::OK, Json(serde_json::json!(null))).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response()
        }
    }
}

pub(super) async fn merge_pr_via_github_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<MergePrRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    match crate::github::merge_pr_github_impl(&body.repo_path, body.pr_number, &body.merge_method, &state) {
        Ok(sha) => (StatusCode::OK, Json(serde_json::json!({"sha": sha}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn finalize_merged_worktree_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<FinalizeMergeRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    let repo_path = body.repo_path.clone();
    let base_repo = std::path::PathBuf::from(&repo_path);
    let result = match body.action.as_str() {
        "archive" => crate::worktree::archive_worktree(&base_repo, &body.branch_name)
            .map(|ap| serde_json::json!({"merged": true, "action": "archived", "archive_path": ap})),
        "delete" => crate::worktree::remove_worktree_by_branch(&repo_path, &body.branch_name, true)
            .map(|_| serde_json::json!({"merged": true, "action": "deleted", "archive_path": null})),
        other => Err(format!("Unknown action '{other}': expected 'archive' or 'delete'")),
    };
    match result {
        Ok(json) => {
            state.invalidate_repo_caches(&repo_path);
            (StatusCode::OK, Json(json)).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}
