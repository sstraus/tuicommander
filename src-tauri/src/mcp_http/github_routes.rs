use std::sync::Arc;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::state::AppState;
use super::types::{CiChecksQuery, PathQuery};
use super::validate_repo_path;

pub(super) async fn repo_github_status(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    Json(crate::github::get_github_status(q.path)).into_response()
}

pub(super) async fn repo_pr_statuses(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    Json(crate::github::get_repo_pr_statuses_impl(
        &q.path,
        &state.http_client,
        state.github_token.as_deref(),
    )).into_response()
}

pub(super) async fn repo_ci_checks(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CiChecksQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    Json(crate::github::get_ci_checks_impl(
        &q.path,
        q.pr_number,
        &state.http_client,
        state.github_token.as_deref(),
    )).into_response()
}
