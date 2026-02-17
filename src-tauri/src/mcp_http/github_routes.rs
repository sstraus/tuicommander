use axum::extract::Query;
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::types::PathQuery;
use super::validate_repo_path;

pub(super) async fn repo_github_status(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    Json(crate::github::get_github_status(q.path)).into_response()
}

pub(super) async fn repo_pr_statuses(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    Json(crate::github::get_repo_pr_statuses_impl(&q.path)).into_response()
}

pub(super) async fn repo_ci_checks(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    Json(crate::github::get_ci_checks(q.path)).into_response()
}
