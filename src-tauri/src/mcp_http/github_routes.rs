use axum::Json;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

#[derive(serde::Deserialize)]
pub(super) struct CircleCiSetTokenRequest {
    token: String,
}

use super::types::{
    ChangelogQuery, CiChecksQuery, CiFailureLogsQuery, GithubBindRepoRequest,
    GithubPollLoginRequest, GithubRemoveAccountRequest, GithubRepoPathBody, GithubResolveRepoQuery,
    GithubResolveReposRequest, GithubSetHideDraftsRequest, IssueActionRequest, IssuesQuery,
    PathQuery, PollRepoRequest, PrDiffQuery, SetVisibilityRequest, StartPollingRequest,
    UpdatePathsRequest,
};
use super::{err_500, json_result, upstream_json_result, validate_repo_path};
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
    if let Some(cached) = state.git_cache.github_status.get(&path) {
        return Json(&*cached).into_response();
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

pub(super) async fn repo_pr_review_threads(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CiChecksQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    json_result(crate::github::get_pr_review_threads_impl(&q.path, q.pr_number, &state).await)
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
    upstream_json_result(
        crate::github::approve_pr_impl(&path, pr, &state)
            .await
            .map(|()| serde_json::json!({"ok": true})),
    )
}

pub(super) async fn repo_update_pr_branch(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::UpdatePrBranchRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    upstream_json_result(
        crate::github::update_pr_branch_impl(
            &body.repo_path,
            body.pr_number,
            &body.expected_head_sha,
            &state,
        )
        .await
        .map(|()| serde_json::json!({"ok": true})),
    )
}

pub(super) async fn repo_close_pr(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::ApprovePrRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    upstream_json_result(
        crate::github::close_pr_impl(&body.repo_path, body.pr_number, &state)
            .await
            .map(|()| serde_json::json!({"ok": true})),
    )
}

pub(super) async fn repo_create_pr(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::CreatePrRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    upstream_json_result(
        crate::github::create_pr_impl(
            &body.repo_path,
            &body.title,
            &body.body,
            &body.base,
            &body.head,
            body.draft,
            &state,
        )
        .await,
    )
}

pub(super) async fn repo_create_issue(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::CreateIssueRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    upstream_json_result(
        crate::github::create_issue_impl(&body.repo_path, &body.title, &body.body, &state).await,
    )
}

pub(super) async fn repo_post_pr_review(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::PostPrReviewRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    upstream_json_result(
        crate::github::post_pr_review_impl(
            &body.repo_path,
            body.pr_number,
            &body.body,
            body.event.as_deref(),
            &body.comments,
            &state,
        )
        .await,
    )
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
    upstream_json_result(crate::github::get_pr_diff_pinned_impl(&path, pr, &state).await)
}

pub(super) async fn repo_merged_prs(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ChangelogQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    upstream_json_result(
        crate::github::get_merged_prs_impl(&q.path, q.since_tag.as_deref(), &state).await,
    )
}

/// The three ego-driven repo features, restored on ACP by story 795-320b.
///
/// They sit here rather than under an `/ai/` prefix because the prefix belonged
/// to the embedded engine that story 784 deleted.
///
/// Which servers answer them is not what "desktop-only" would suggest, and the
/// distinction is between two *binaries*, not two features. They are mounted in
/// `build_router`, so the desktop app serves them —
/// nothing here needs a window, since the work happens in an ego process
/// reached over ACP. They are **not** in `shared_routes`, so
/// `build_remote_router` does not carry them and the `tuic-remote` daemon
/// answers 404. Moving them is a decision about the remote surface, not a
/// tidy-up: check `shared_routes_surface_is_locked_and_desktop_only_excluded`
/// and its `must_exist` list before assuming either way (#810-4986).
pub(super) async fn repo_generate_changelog(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ChangelogQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    upstream_json_result(
        crate::changelog::generate_changelog_impl(&q.path, q.since_tag.as_deref(), &state).await,
    )
}

pub(super) async fn repo_pr_review(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::PrReviewRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    upstream_json_result(
        crate::pr_review::run_pr_review_impl(body.repo_path, body.pr_number, &state).await,
    )
}

pub(super) async fn repo_improvement_scan(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::ImprovementScanRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    upstream_json_result(
        crate::improvement_scan::run_improvement_scan_impl(body.repo_path, body.focus, &state)
            .await,
    )
}

pub(super) async fn repo_create_issue_from_proposal(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::CreateIssueFromProposalRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    upstream_json_result(
        crate::improvement_scan::create_issue_from_proposal_impl(
            &body.repo_path,
            &body.proposal,
            &state,
        )
        .await,
    )
}

pub(super) async fn repo_conflict_assist(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::ConflictAssistRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    upstream_json_result(
        crate::conflict_assist::start_conflict_assist_impl(body.repo_path, body.pr_number, &state)
            .await,
    )
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

pub(super) async fn repo_issue_detail(
    State(state): State<Arc<AppState>>,
    Query(q): Query<super::types::IssueDetailQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    json_result(crate::github::get_issue_detail_impl(&q.repo_path, q.issue_number, &state).await)
}

pub(super) async fn repo_close_issue(
    State(state): State<Arc<AppState>>,
    Json(body): Json<IssueActionRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    upstream_json_result(
        crate::github::close_issue_impl(&body.repo_path, body.issue_number, &state)
            .await
            .map(|()| serde_json::json!({"ok": true})),
    )
}

pub(super) async fn repo_reopen_issue(
    State(state): State<Arc<AppState>>,
    Json(body): Json<IssueActionRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    upstream_json_result(
        crate::github::reopen_issue_impl(&body.repo_path, body.issue_number, &state)
            .await
            .map(|()| serde_json::json!({"ok": true})),
    )
}

// --- GitHub poller HTTP handlers ---

pub(super) async fn poller_start(
    State(state): State<Arc<AppState>>,
    Json(body): Json<StartPollingRequest>,
) -> Response {
    let StartPollingRequest {
        paths,
        issue_filter,
        pr_hide_drafts,
    } = body;
    crate::github_poller::ensure_polling(&state, paths, issue_filter, pr_hide_drafts);
    Json(serde_json::json!({"ok": true})).into_response()
}

pub(super) async fn poller_stop(State(state): State<Arc<AppState>>) -> Response {
    poller_response(crate::github_poller::stop_poller(&state).await)
}

/// Shared shape for the poller control routes: `{"ok": true}` on delivery, a 500
/// with `{"error": …}` when the command went nowhere. They used to answer
/// `{"ok": true}` unconditionally, including when no poller was running.
fn poller_response(result: Result<(), String>) -> Response {
    match result {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn poller_set_visibility(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SetVisibilityRequest>,
) -> Response {
    poller_response(crate::github_poller::send_poller_cmd(
        &state,
        PollerCmd::SetVisibility(body.visible),
    ))
}

pub(super) async fn poller_poll_repo(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PollRepoRequest>,
) -> Response {
    poller_response(crate::github_poller::send_poller_cmd(
        &state,
        PollerCmd::PollRepo(body.path),
    ))
}

pub(super) async fn poller_update_paths(
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpdatePathsRequest>,
) -> Response {
    poller_response(crate::github_poller::send_poller_cmd(
        &state,
        PollerCmd::UpdatePaths(body.paths),
    ))
}

pub(super) async fn api_debug_set(Json(body): Json<super::types::SetApiDebugRequest>) -> Response {
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
    poller_response(crate::github_poller::send_poller_cmd(
        &state,
        PollerCmd::SetIssueFilter(body.filter),
    ))
}

pub(super) async fn github_viewer_login(State(state): State<Arc<AppState>>) -> Response {
    json_result(crate::github::get_viewer_login(&state).await)
}

pub(super) async fn ci_failure_logs(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CiFailureLogsQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    json_result(
        crate::github::fetch_ci_failure_logs_with_state(
            q.repo_path,
            q.branch,
            q.check_url,
            q.head_sha,
            state,
        )
        .await,
    )
}

pub(super) async fn circleci_token_status() -> Response {
    json_result(crate::circleci::circleci_token_status().await)
}

pub(super) async fn circleci_set_token(Json(body): Json<CircleCiSetTokenRequest>) -> Response {
    json_result(crate::circleci::circleci_set_token(body.token).await)
}

pub(super) async fn circleci_delete_token() -> Response {
    json_result(crate::circleci::circleci_delete_token().await)
}

pub(super) async fn github_set_hide_drafts(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GithubSetHideDraftsRequest>,
) -> Response {
    json_result(crate::github_poller::github_set_pr_hide_drafts_impl(
        &state, body.hide,
    ))
}

pub(super) async fn github_start_login(State(state): State<Arc<AppState>>) -> Response {
    json_result(crate::github_auth::start_device_flow(&state.http_client).await)
}

pub(super) async fn github_poll_login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GithubPollLoginRequest>,
) -> Response {
    json_result(crate::github_auth::github_poll_login_impl(&state, body.device_code).await)
}

pub(super) async fn github_poll_add_account(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GithubPollLoginRequest>,
) -> Response {
    json_result(crate::github_auth::github_poll_add_account_impl(&state, body.device_code).await)
}

pub(super) async fn github_logout(State(state): State<Arc<AppState>>) -> Response {
    json_result(crate::github_auth::github_logout_impl(&state).await)
}

pub(super) async fn github_disconnect(State(state): State<Arc<AppState>>) -> Response {
    json_result(crate::github_auth::github_disconnect_impl(&state).await)
}

pub(super) async fn github_auth_status(State(state): State<Arc<AppState>>) -> Response {
    json_result(crate::github_auth::github_auth_status_impl(&state).await)
}

pub(super) async fn github_diagnostics(State(state): State<Arc<AppState>>) -> Response {
    json_result(crate::github_auth::github_diagnostics_impl(&state, false).await)
}

/// Explicit manual refresh releases retry exclusions; ordinary GET remains read-only.
pub(super) async fn github_refresh_diagnostics(State(state): State<Arc<AppState>>) -> Response {
    json_result(crate::github_auth::github_diagnostics_impl(&state, true).await)
}

// --- Multi-account: accounts + repo bindings (IPC/HTTP parity) ---

pub(super) async fn github_list_accounts() -> Response {
    Json(
        crate::github_account::GitHubAccountRegistry::load()
            .list()
            .to_vec(),
    )
    .into_response()
}

// Desktop-only: the add/remove/resolve impls in github_account.rs are
// #[cfg(feature = "desktop")] (keychain-backed). The remote daemon never serves
// GitHub account routes (build_remote_router omits them), so gate these handlers to match
// their impls and keep the always-compiled lib building for tuic-remote (#094-ec55).
#[cfg(feature = "desktop")]
pub(super) async fn github_remove_account(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GithubRemoveAccountRequest>,
) -> Response {
    json_result(crate::github_account::github_remove_account_impl(
        &state, body.id,
    ))
}

pub(super) async fn github_list_bindings() -> Response {
    Json(crate::github_account::RepoBindingStore::load().entries()).into_response()
}

pub(super) async fn github_bind_repo(Json(body): Json<GithubBindRepoRequest>) -> Response {
    json_result(crate::github_account::bind_repo_to_account(
        std::path::Path::new(&body.repo_path),
        &body.account_id,
        &body.remote_name,
    ))
}

pub(super) async fn github_unbind_repo(Json(body): Json<GithubRepoPathBody>) -> Response {
    json_result(crate::github_account::unbind_repo(std::path::Path::new(
        &body.repo_path,
    )))
}

#[cfg(feature = "desktop")]
pub(super) async fn github_resolve_repo(
    State(state): State<Arc<AppState>>,
    Query(q): Query<GithubResolveRepoQuery>,
) -> Response {
    json_result(crate::github_account::github_resolve_repo_impl(
        &state,
        q.repo_path,
    ))
}

#[cfg(feature = "desktop")]
pub(super) async fn github_resolve_repos(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GithubResolveReposRequest>,
) -> Response {
    Json(crate::github_account::github_resolve_repos_impl(
        &state,
        body.repo_paths,
    ))
    .into_response()
}

/// Kills the surviving mutants of #1366-ea64: a handler that returns `Response::default()`
/// (200, empty body) instead of the result of the GitHub call it wraps.
#[cfg(test)]
mod pr_action_route_tests {
    use super::*;
    use crate::mcp_http::types::{ApprovePrRequest, MergePrRequest, UpdatePrBranchRequest};
    use axum::http::StatusCode;

    fn state() -> Arc<AppState> {
        Arc::new(crate::state::tests_support::make_test_app_state())
    }

    async fn error_body(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("a JSON body");
        json["error"].as_str().expect("an error string").to_string()
    }

    /// Catches (repo_pr_review_threads -> Default): a failed lookup answering 200 with no body.
    #[tokio::test]
    async fn review_threads_route_returns_the_lookup_error() {
        let response = repo_pr_review_threads(
            State(state()),
            Query(CiChecksQuery {
                path: "/nonexistent".into(),
                pr_number: 1,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error_body(response).await, "No GitHub remote");
    }

    /// Catches (repo_update_pr_branch -> Default): a refused update answering 200 with no body,
    /// which the panel reads as "branch updated".
    #[tokio::test]
    async fn update_branch_route_returns_the_upstream_error() {
        let response = repo_update_pr_branch(
            State(state()),
            Json(UpdatePrBranchRequest {
                repo_path: "/nonexistent".into(),
                pr_number: 1,
                expected_head_sha: String::new(),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        assert!(error_body(response).await.contains("head commit"));
    }

    /// Catches (repo_close_pr -> Default): a PR that could not be closed answering 200.
    #[tokio::test]
    async fn close_route_returns_the_upstream_error() {
        let response = repo_close_pr(
            State(state()),
            Json(ApprovePrRequest {
                repo_path: "/nonexistent".into(),
                pr_number: 1,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        assert_ne!(error_body(response).await, "");
    }

    /// Catches (merge_pr_via_github_http -> Default): a refused merge answering 200 with no
    /// `sha`, which the caller reads as merged.
    #[tokio::test]
    async fn merge_route_returns_the_merge_error() {
        let response = crate::mcp_http::worktree_routes::merge_pr_via_github_http(
            State(state()),
            Json(MergePrRequest {
                repo_path: "/nonexistent".into(),
                pr_number: 1,
                merge_method: "squash".into(),
                expected_head_sha: String::new(),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(error_body(response).await.contains("head commit"));
    }
}
