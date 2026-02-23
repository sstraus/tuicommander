use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::types::*;
use super::validate_repo_path;

pub(super) async fn repo_info(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    Json(crate::git::get_repo_info_impl(&q.path)).into_response()
}

pub(super) async fn repo_diff(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    match crate::git::get_git_diff(q.path, None) {
        Ok(diff) => (StatusCode::OK, Json(serde_json::json!({"diff": diff}))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ).into_response(),
    }
}

pub(super) async fn repo_diff_stats(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    Json(crate::git::get_diff_stats(q.path, None)).into_response()
}

pub(super) async fn repo_changed_files(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    match crate::git::get_changed_files(q.path, None) {
        Ok(files) => (StatusCode::OK, Json(serde_json::json!(files))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ).into_response(),
    }
}

pub(super) async fn repo_branches(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    match crate::git::get_git_branches(q.path) {
        Ok(branches) => (StatusCode::OK, Json(serde_json::json!(branches))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ).into_response(),
    }
}

pub(super) async fn get_file_diff_http(Query(q): Query<FileQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    match crate::git::get_file_diff(q.path, q.file, None) {
        Ok(diff) => (StatusCode::OK, Json(serde_json::json!(diff))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn list_markdown_files_http(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    match crate::list_markdown_files_impl(q.path) {
        Ok(files) => (StatusCode::OK, Json(serde_json::json!(files))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn read_file_http(Query(q): Query<FileQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    match crate::read_file_impl(q.path, q.file) {
        Ok(content) => (StatusCode::OK, Json(serde_json::json!(content))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn rename_branch_http(Json(body): Json<RenameBranchRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) { return e.into_response(); }
    match crate::git::rename_branch_impl(&body.path, &body.old_name, &body.new_name) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
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
    match crate::git::get_recent_commits(q.path, q.count) {
        Ok(commits) => (StatusCode::OK, Json(serde_json::json!(commits))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
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
