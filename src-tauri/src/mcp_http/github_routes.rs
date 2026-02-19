use std::sync::Arc;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::state::AppState;
use super::types::{CiChecksQuery, PathQuery};
use super::validate_repo_path;

pub(super) async fn repo_github_status(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    match tokio::task::spawn_blocking(move || crate::github::get_github_status_impl(&path)).await {
        Ok(status) => Json(status).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn repo_pr_statuses(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    match tokio::task::spawn_blocking(move || crate::github::get_repo_pr_statuses_impl(&path, &state)).await {
        Ok(statuses) => Json(statuses).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn repo_ci_checks(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CiChecksQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    let pr_number = q.pr_number;
    match tokio::task::spawn_blocking(move || crate::github::get_ci_checks_impl(&path, pr_number, &state)).await {
        Ok(checks) => Json(checks).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}
