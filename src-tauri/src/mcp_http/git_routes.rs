use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::types::*;
use super::validate_repo_path;

pub(super) async fn repo_info(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    match tokio::task::spawn_blocking(move || crate::git::get_repo_info_impl(&path)).await {
        Ok(info) => Json(info).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn repo_diff(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    match tokio::task::spawn_blocking(move || crate::git::get_git_diff(path, None)).await {
        Ok(Ok(diff)) => (StatusCode::OK, Json(serde_json::json!({"diff": diff}))).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn repo_diff_stats(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    match tokio::task::spawn_blocking(move || crate::git::get_diff_stats(path, None)).await {
        Ok(stats) => Json(stats).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn repo_changed_files(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    match tokio::task::spawn_blocking(move || crate::git::get_changed_files(path, None)).await {
        Ok(Ok(files)) => (StatusCode::OK, Json(serde_json::json!(files))).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn repo_branches(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    match tokio::task::spawn_blocking(move || crate::git::get_git_branches(path)).await {
        Ok(Ok(branches)) => (StatusCode::OK, Json(serde_json::json!(branches))).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn get_file_diff_http(Query(q): Query<FileQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    let file = q.file;
    match tokio::task::spawn_blocking(move || crate::git::get_file_diff(path, file, None)).await {
        Ok(Ok(diff)) => (StatusCode::OK, Json(serde_json::json!(diff))).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn list_markdown_files_http(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    match tokio::task::spawn_blocking(move || crate::list_markdown_files_impl(path)).await {
        Ok(Ok(files)) => (StatusCode::OK, Json(serde_json::json!(files))).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn read_file_http(Query(q): Query<FileQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    let file = q.file;
    match tokio::task::spawn_blocking(move || crate::read_file_impl(path, file)).await {
        Ok(Ok(content)) => (StatusCode::OK, Json(serde_json::json!(content))).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn rename_branch_http(Json(body): Json<RenameBranchRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) { return e.into_response(); }
    let path = body.path;
    let old_name = body.old_name;
    let new_name = body.new_name;
    match tokio::task::spawn_blocking(move || crate::git::rename_branch_impl(&path, &old_name, &new_name)).await {
        Ok(Ok(())) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn get_initials_http(Query(q): Query<NameQuery>) -> impl IntoResponse {
    Json(crate::git::get_initials(q.name))
}

pub(super) async fn check_is_main_branch_http(Query(q): Query<BranchQuery>) -> impl IntoResponse {
    Json(crate::git::check_is_main_branch(q.branch))
}

pub(super) async fn get_recent_commits_http(Query(q): Query<RecentCommitsQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    let count = q.count;
    match tokio::task::spawn_blocking(move || crate::git::get_recent_commits(path, count)).await {
        Ok(Ok(commits)) => (StatusCode::OK, Json(serde_json::json!(commits))).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn get_local_ips_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
) -> impl axum::response::IntoResponse {
    Json(crate::get_local_ips_impl(&state))
}

pub(super) async fn list_user_plugins_http() -> impl axum::response::IntoResponse {
    Json(serde_json::json!(crate::plugins::list_user_plugins()))
}
