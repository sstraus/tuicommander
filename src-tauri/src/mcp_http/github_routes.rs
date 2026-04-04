use std::sync::Arc;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::state::AppState;
use super::types::{CiChecksQuery, PathQuery, PrDiffQuery};
use super::{err_500, validate_repo_path};

pub(super) async fn repo_github_status(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    match tokio::task::spawn_blocking(move || crate::github::get_github_status_impl(&path)).await {
        Ok(status) => Json(status).into_response(),
        Err(e) => err_500(&format!("Task failed: {e}")),
    }
}

pub(super) async fn repo_pr_statuses(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    match crate::github::get_repo_pr_statuses_impl(&path, false, &state).await {
        Ok(statuses) => Json(statuses).into_response(),
        Err(e) => err_500(&format!("Task failed: {e}")),
    }
}

pub(super) async fn repo_all_pr_statuses(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::GetAllPrStatusesRequest>,
) -> Response {
    let paths = body.paths;
    let include_merged = body.include_merged;
    match crate::github::get_all_pr_statuses_impl(&paths, include_merged, &state).await {
        Ok(statuses) => Json(statuses).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn repo_ci_checks(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CiChecksQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    let pr_number = q.pr_number;
    Json(crate::github::get_ci_checks_impl(&path, pr_number, &state).await).into_response()
}

pub(super) async fn repo_approve_pr(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::ApprovePrRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    let path = body.repo_path;
    let pr = body.pr_number;
    match crate::github::approve_pr_impl(&path, pr, &state).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (axum::http::StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn repo_pr_diff(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PrDiffQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    let pr = q.pr;
    match crate::github::get_pr_diff_impl(&path, pr, &state).await {
        Ok(diff) => diff.into_response(),
        Err(e) => (axum::http::StatusCode::BAD_GATEWAY, e).into_response(),
    }
}
