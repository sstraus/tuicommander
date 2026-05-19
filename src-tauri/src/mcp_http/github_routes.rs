use axum::Json;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use super::types::{
    CiChecksQuery, IssueActionRequest, IssuesQuery, PathQuery, PollRepoRequest, PrDiffQuery,
    SetVisibilityRequest, StartPollingRequest, UpdatePathsRequest,
};
use super::{err_500, validate_repo_path};
use crate::github_poller::PollerCmd;
use crate::state::AppState;

pub(super) async fn repo_github_status(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
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
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    if let Some(cached) = crate::state::AppState::get_cached(
        &state.git_cache.github_status,
        &path,
        crate::state::GITHUB_CACHE_TTL,
    ) {
        return Json(cached).into_response();
    }
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
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    let pr_number = q.pr_number;
    Json(crate::github::get_ci_checks_impl(&path, pr_number, &state).await).into_response()
}

pub(super) async fn repo_approve_pr(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::ApprovePrRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    let path = body.repo_path;
    let pr = body.pr_number;
    match crate::github::approve_pr_impl(&path, pr, &state).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

pub(super) async fn repo_pr_diff(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PrDiffQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    let pr = q.pr;
    match crate::github::get_pr_diff_impl(&path, pr, &state).await {
        Ok(diff) => diff.into_response(),
        Err(e) => (axum::http::StatusCode::BAD_GATEWAY, e).into_response(),
    }
}

pub(super) async fn repo_issues(
    State(state): State<Arc<AppState>>,
    Query(q): Query<IssuesQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    let filter = q.filter;
    match crate::github::get_all_issues_impl(std::slice::from_ref(&path), &filter, &state).await {
        Ok(mut results) => {
            let issues = results.remove(&path).unwrap_or_default();
            Json(issues).into_response()
        }
        Err(e) => err_500(&e),
    }
}

pub(super) async fn repo_close_issue(
    State(state): State<Arc<AppState>>,
    Json(body): Json<IssueActionRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    match crate::github::close_issue_impl(&body.repo_path, body.issue_number, &state).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

pub(super) async fn repo_reopen_issue(
    State(state): State<Arc<AppState>>,
    Json(body): Json<IssueActionRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    match crate::github::reopen_issue_impl(&body.repo_path, body.issue_number, &state).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

// --- GitHub poller HTTP handlers ---

pub(super) async fn poller_start(
    State(state): State<Arc<AppState>>,
    Json(body): Json<StartPollingRequest>,
) -> Response {
    let guard = state.github_poller.lock();
    if let Some(poller) = guard.as_ref() {
        let _ = poller.cmd_tx.try_send(PollerCmd::UpdatePaths(body.paths));
        let _ = poller
            .cmd_tx
            .try_send(PollerCmd::SetIssueFilter(body.issue_filter));
    }
    Json(serde_json::json!({"ok": true})).into_response()
}

pub(super) async fn poller_stop(State(state): State<Arc<AppState>>) -> Response {
    let poller = state.github_poller.lock().take();
    if let Some(p) = poller {
        let _ = p.cmd_tx.send(PollerCmd::Stop).await;
    }
    Json(serde_json::json!({"ok": true})).into_response()
}

pub(super) async fn poller_set_visibility(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SetVisibilityRequest>,
) -> Response {
    if let Some(poller) = state.github_poller.lock().as_ref() {
        let _ = poller
            .cmd_tx
            .try_send(PollerCmd::SetVisibility(body.visible));
    }
    Json(serde_json::json!({"ok": true})).into_response()
}

pub(super) async fn poller_poll_repo(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PollRepoRequest>,
) -> Response {
    if let Some(poller) = state.github_poller.lock().as_ref() {
        let _ = poller.cmd_tx.try_send(PollerCmd::PollRepo(body.path));
    }
    Json(serde_json::json!({"ok": true})).into_response()
}

pub(super) async fn poller_update_paths(
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpdatePathsRequest>,
) -> Response {
    if let Some(poller) = state.github_poller.lock().as_ref() {
        let _ = poller.cmd_tx.try_send(PollerCmd::UpdatePaths(body.paths));
    }
    Json(serde_json::json!({"ok": true})).into_response()
}

pub(super) async fn api_debug_set(
    Json(body): Json<super::types::SetApiDebugRequest>,
) -> Response {
    crate::github_debug::set(body.enabled);
    Json(serde_json::json!({"ok": true, "enabled": body.enabled})).into_response()
}

pub(super) async fn api_debug_get() -> Response {
    let enabled = crate::github_debug::enabled();
    Json(serde_json::json!({"enabled": enabled})).into_response()
}

pub(super) async fn poller_set_issue_filter(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::SetIssueFilterRequest>,
) -> Response {
    if let Some(poller) = state.github_poller.lock().as_ref() {
        let _ = poller
            .cmd_tx
            .try_send(PollerCmd::SetIssueFilter(body.filter));
    }
    Json(serde_json::json!({"ok": true})).into_response()
}
