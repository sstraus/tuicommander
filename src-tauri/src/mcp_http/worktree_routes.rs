use crate::AppState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use super::types::*;
use super::{err_500, json_result, validate_repo_path};

struct PendingWarmGuard {
    destination: std::path::PathBuf,
    token: u64,
    armed: bool,
}

impl Drop for PendingWarmGuard {
    fn drop(&mut self) {
        if self.armed {
            crate::worktree::finish_warm(
                &self.destination,
                self.token,
                serde_json::json!({"status": "failed", "reason": "creation request cancelled before warming started"}),
            );
        }
    }
}

async fn run_setup_then_warm(
    script: Option<String>,
    source: std::path::PathBuf,
    destination: std::path::PathBuf,
    token: u64,
    warm: impl FnOnce(&std::path::Path, &std::path::Path) -> crate::cow::WarmingReport + Send + 'static,
) -> (
    Option<serde_json::Value>,
    Option<serde_json::Value>,
    tokio::task::JoinHandle<()>,
) {
    let mut pending_guard = PendingWarmGuard {
        destination: destination.clone(),
        token,
        armed: true,
    };
    let mut setup_result = None;
    let mut setup_error = None;
    if let Some(script) = script {
        let cwd = destination.to_string_lossy().into_owned();
        match tokio::task::spawn_blocking(move || crate::worktree::run_setup_script(script, cwd))
            .await
        {
            Ok(Ok(result)) => setup_result = Some(result),
            Ok(Err(error)) => setup_error = Some(serde_json::json!(error)),
            Err(error) => setup_error = Some(serde_json::json!(format!("task panic: {error}"))),
        }
    }
    let task = crate::worktree::spawn_background_warm(source, destination, token, warm);
    pending_guard.armed = false;
    (setup_result, setup_error, task)
}

pub(super) struct CreatedWorktree {
    pub worktree: crate::state::WorktreeInfo,
    pub path: String,
    /// What the caller needs in order to USE this workspace: the dirty policy
    /// applied, the warm artifacts, and the isolation semantics of the
    /// mechanism it actually got. The only instruction channel there is
    /// (#734-ca73) — there is no enforcement layer behind it.
    pub instructions: serde_json::Value,
    /// The id the caller must use to address this workspace afterwards — removal,
    /// dirtiness and finalize all take an id.
    pub workspace_id: String,
    pub branch: String,
    pub setup_script: Option<serde_json::Value>,
    pub setup_script_error: Option<serde_json::Value>,
}

pub(super) async fn list_worktrees_http(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let worktrees: Vec<serde_json::Value> = state
        .session_maps
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
                    "warm_artifacts": crate::worktree::warm_status(&wt.path),
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
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    let result =
        tokio::task::spawn_blocking(move || crate::worktree::get_worktree_paths(path)).await;
    match result {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

pub(super) async fn create_worktree_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateWorktreeRequest>,
) -> impl IntoResponse {
    let created = match create_worktree_shared(
        &state,
        body.base_repo.clone(),
        body.branch_name.clone(),
        body.base_ref.clone(),
        None,
        false,
    )
    .await
    {
        Ok(created) => created,
        Err(response) => return response,
    };

    let mut response = serde_json::json!({
        "name": created.worktree.name,
        "path": &created.path,
        // The instruction payload rides on both transports identically: the
        // model reading it over MCP and the client reading it over HTTP need
        // the same isolation semantics (#734-ca73).
        "instructions": &created.instructions,
        "worktree_rename": &created.instructions["worktree_rename"],
        // How the caller addresses this workspace from here on. `branch` remains
        // explicit display data even though linked worktree ids currently match it.
        "workspace_id": &created.workspace_id,
        "branch": created.worktree.branch,
        "base_repo": created.worktree.base_repo.to_string_lossy(),
    });
    if let Some(setup_script) = created.setup_script {
        response["setup_script"] = setup_script;
    }
    if let Some(setup_script_error) = created.setup_script_error {
        response["setup_script_error"] = setup_script_error;
    }

    (StatusCode::CREATED, Json(response))
}

pub(super) async fn create_worktree_shared(
    state: &Arc<AppState>,
    base_repo: String,
    branch_name: String,
    base_ref: Option<String>,
    creator_session: Option<String>,
    spawn_session: bool,
) -> Result<CreatedWorktree, (StatusCode, Json<serde_json::Value>)> {
    validate_repo_path(&base_repo)?;
    // Model provides only branch_name and optionally base_ref (start point).
    // Storage path and strategy come entirely from user config via resolve_worktree_dir_for_repo.
    let config = crate::worktree::WorktreeConfig {
        task_name: branch_name.clone(),
        base_repo: base_repo.clone(),
        branch: Some(branch_name),
        create_branch: true, // Always create a new branch — model must not control this
    };
    let worktrees_dir = crate::worktree::resolve_worktree_dir_for_repo(
        std::path::Path::new(&config.base_repo),
        &state.worktrees_dir,
    );
    // `create_workspace` picks the mechanism and degrades rather than failing;
    // for the worktree path it still goes through the stale-recovery wrapper, so
    // MCP clients keep healing automatically when an orphaned directory is
    // sitting where the new one should land. Off-loaded onto spawn_blocking
    // because a clone or a `git worktree add` can take seconds.
    let config_bg = config.clone();
    let worktrees_dir_bg = worktrees_dir.clone();
    let allocation_state = Arc::clone(state);
    let result = match tokio::task::spawn_blocking(move || {
        crate::worktree::allocate_workspace(
            &allocation_state,
            &worktrees_dir_bg,
            &config_bg,
            base_ref.as_deref(),
        )
    })
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("task panic: {e}")})),
            ));
        }
    };
    match result {
        Ok((workspace, recycled)) => {
            let wt_path = workspace.path.to_string_lossy().to_string();
            let branch_name = workspace.branch.clone();
            let workspace_id = workspace.workspace_id.clone();
            let warm_token = crate::worktree::begin_warm(&workspace.path);
            let mut pending_guard = PendingWarmGuard {
                destination: workspace.path.clone(),
                token: warm_token,
                armed: true,
            };
            // Built before the setup script runs: the payload describes what the
            // workspace ARRIVED with, and a script that installs something does
            // not change what was already warm.
            let mut instructions = workspace.instruction_payload_pending();
            instructions["worktree_rename"] = serde_json::json!(recycled);
            let reused = recycled.is_some();
            state.notify_worktree_created(crate::state::WorktreeCreatedPayload {
                creator_session,
                spawn_session,
                repo_path: base_repo.clone(),
                workspace_id: workspace_id.clone(),
                branch: branch_name.clone(),
                worktree_path: wt_path.clone(),
                kind: workspace.kind,
            });
            let warm_source = std::path::PathBuf::from(&base_repo);
            let warm_destination = workspace.path.clone();
            let repo_for_script = base_repo.clone();
            let script = tokio::task::spawn_blocking(move || {
                crate::config::resolve_effective_setup_script(&repo_for_script)
            })
            .await
            .ok()
            .flatten();
            pending_guard.armed = false;
            let (setup_script, setup_script_error, _warm_task) = run_setup_then_warm(
                script,
                warm_source,
                warm_destination,
                warm_token,
                move |source, destination| {
                    if reused {
                        crate::cow::WarmingReport::default()
                    } else {
                        crate::cow::warm_worktree(source, destination)
                    }
                },
            )
            .await;
            Ok(CreatedWorktree {
                worktree: crate::state::WorktreeInfo {
                    name: workspace
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| branch_name.clone()),
                    path: workspace.path,
                    branch: Some(branch_name.clone()),
                    base_repo: std::path::PathBuf::from(&base_repo),
                },
                path: wt_path,
                instructions,
                workspace_id,
                branch: branch_name,
                setup_script,
                setup_script_error,
            })
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        )),
    }
}

pub(super) async fn remove_worktree_http(
    State(state): State<Arc<AppState>>,
    Path(workspace_id): Path<String>,
    Query(q): Query<RemoveWorktreeQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let repo_path = q.repo_path.clone();
    let force = q.force.unwrap_or(false);
    let delete_branch = q.delete_branch.unwrap_or(!force);
    let override_lock = q.override_lock.unwrap_or(false);
    let expected_fingerprint = q.expected_fingerprint.clone();
    let confirm_missing_checkout = q.confirm_missing_checkout.unwrap_or(false);
    if force && expected_fingerprint.is_none() && !confirm_missing_checkout {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "force requires expectedFingerprint from the confirmed lifecycle status"})),
        )
            .into_response();
    }
    let id_for_event = workspace_id.clone();
    let preview_state = Arc::clone(&state);
    let result = tokio::task::spawn_blocking(move || {
        let warnings = crate::worktree::inspect_worktree_removal(
            &preview_state,
            std::path::Path::new(&repo_path),
            &workspace_id,
        )
        .warnings;
        let archive = crate::worktree::resolve_archive_script(&repo_path);
        let outcome = crate::worktree::remove_worktree_with_presence_confirmation(
            &preview_state,
            &repo_path,
            &workspace_id,
            delete_branch,
            archive.as_deref(),
            force,
            override_lock,
            expected_fingerprint.as_deref(),
            confirm_missing_checkout,
        )?;
        Ok::<_, String>((outcome, warnings))
    })
    .await;
    // The branch comes off the outcome: it was read from the record before the
    // checkout was removed, and nothing can resolve the id afterwards.
    if let Ok(Ok((ref outcome, _))) = result {
        state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
            repo_path: q.repo_path.clone(),
            workspace_id: id_for_event,
            branch: outcome.branch.clone(),
        });
    }
    match result {
        Ok(Ok((outcome, warnings))) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "branch_delete_warning": outcome.branch_delete_warning,
                "removal_rule": outcome.removal_rule,
                "warnings": warnings,
            })),
        )
            .into_response(),
        Ok(Err(e)) => err_500(&e),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

pub(super) async fn detect_orphan_worktrees_http(Query(q): Query<OptionalRepoQuery>) -> Response {
    let repo_path = match q.repo_path {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "repoPath required"})),
            )
                .into_response();
        }
    };
    if let Err(e) = validate_repo_path(&repo_path) {
        return e.into_response();
    }
    json_result(crate::worktree::detect_orphan_worktrees(repo_path).await)
}

pub(super) async fn assess_orphan_cleanup_http(
    State(state): State<Arc<AppState>>,
    Query(q): Query<OptionalRepoQuery>,
) -> Response {
    let repo_path = match q.repo_path {
        Some(path) if !path.is_empty() => path,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "repoPath required"})),
            )
                .into_response();
        }
    };
    if let Err(error) = validate_repo_path(&repo_path) {
        return error.into_response();
    }
    json_result(crate::worktree::assess_orphan_cleanup_internal(state, repo_path).await)
}

pub(super) async fn begin_orphan_cleanup_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<BeginOrphanCleanupRequest>,
) -> Response {
    if let Err(error) = validate_repo_path(&body.repo_path) {
        return error.into_response();
    }
    let repo_path = body.repo_path;
    let paths = body.paths;
    match tokio::task::spawn_blocking(move || {
        crate::worktree::begin_orphan_cleanup_internal(&state, &repo_path, paths)
    })
    .await
    {
        Ok(Ok(())) => Json(serde_json::Value::Null).into_response(),
        Ok(Err(error)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error})),
        )
            .into_response(),
        Err(error) => err_500(&format!("orphan cleanup registration task failed: {error}")),
    }
}

pub(super) async fn pending_orphan_cleanup_http(
    State(state): State<Arc<AppState>>,
    Query(q): Query<OptionalRepoQuery>,
) -> Response {
    let Some(repo_path) = q.repo_path else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "repoPath required"})),
        )
            .into_response();
    };
    if let Err(error) = validate_repo_path(&repo_path) {
        return error.into_response();
    }
    let answer = state
        .pending_orphan_cleanup
        .get(&repo_path)
        .and_then(|entry| entry.answer);
    Json(answer).into_response()
}

pub(super) async fn answer_orphan_cleanup_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AnswerOrphanCleanupRequest>,
) -> Response {
    if let Err(error) = validate_repo_path(&body.repo_path) {
        return error.into_response();
    }
    let remove = match body.decision.as_str() {
        "remove" => true,
        "keep" => false,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "decision must be remove or keep"})),
            )
                .into_response();
        }
    };
    let repo_path = body.repo_path;
    match tokio::task::spawn_blocking(move || {
        crate::worktree::answer_orphan_cleanup_internal(&state, &repo_path, remove)
    })
    .await
    {
        Ok(Ok(())) => Json(serde_json::json!({"ok": true})).into_response(),
        Ok(Err(error)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error})),
        )
            .into_response(),
        Err(error) => err_500(&format!("orphan cleanup answer task failed: {error}")),
    }
}

pub(super) async fn clear_orphan_cleanup_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ClearOrphanCleanupRequest>,
) -> Response {
    if let Err(error) = validate_repo_path(&body.repo_path) {
        return error.into_response();
    }
    crate::worktree::clear_orphan_cleanup_internal(&state, &body.repo_path, body.kept);
    Json(serde_json::Value::Null).into_response()
}

pub(super) async fn remove_orphan_worktree_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::RemoveOrphanRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    let repo_path = body.repo_path.clone();
    let worktree_path = body.worktree_path.clone();
    let safe_only = body.safe_only;
    let confirmed_sessions = body.confirmed_sessions.clone();
    let guard_state = state.clone();
    // A refused guard is the caller's to act on (400); a removal that fails
    // after the guard keeps its old mapping.
    let result = tokio::task::spawn_blocking(move || {
        crate::worktree::validate_worktree_path(&repo_path, &worktree_path)
            .and_then(|()| {
                crate::worktree::orphan_removal_guard(
                    &guard_state,
                    &repo_path,
                    &worktree_path,
                    safe_only,
                    &confirmed_sessions,
                )
            })
            .map_err(|error| (StatusCode::BAD_REQUEST, error))?;
        let worktree = crate::state::WorktreeInfo {
            name: std::path::Path::new(&worktree_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| worktree_path.clone()),
            path: std::path::PathBuf::from(&worktree_path),
            branch: None,
            base_repo: std::path::PathBuf::from(&repo_path),
        };
        tuic_git::worktree::remove_orphan_worktree_internal(&worktree).map_err(|error| {
            let status = if safe_only {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, error)
        })
    })
    .await;
    match result {
        Ok(Ok(())) => {
            state.invalidate_repo_caches(&body.repo_path);
            (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
        }
        Ok(Err((status, error))) => {
            (status, Json(serde_json::json!({"error": error}))).into_response()
        }
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

pub(super) async fn generate_worktree_name_http(
    Json(body): Json<GenerateWorktreeNameRequest>,
) -> impl IntoResponse {
    Json(crate::worktree::generate_worktree_name_cmd(
        body.existing_names,
    ))
}

pub(super) async fn list_local_branches_http(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    let result =
        tokio::task::spawn_blocking(move || crate::worktree::list_local_branches(path)).await;
    match result {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

pub(super) async fn checkout_remote_branch_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<super::types::CheckoutRemoteRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    let repo = std::path::PathBuf::from(&body.repo_path);
    let branch_name = body.branch_name.clone();
    let remote_ref = format!("origin/{}", body.branch_name);
    let result = tokio::task::spawn_blocking(move || {
        crate::git_cli::git_cmd(&repo)
            .args(["checkout", "-b", &branch_name, &remote_ref])
            .run()
            .map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(_)) => {
            state.invalidate_repo_caches(&body.repo_path);
            (StatusCode::OK, Json(serde_json::json!(null))).into_response()
        }
        Ok(Err(e)) => err_500(&e),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

pub(super) async fn merge_pr_via_github_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<MergePrRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    match crate::github::merge_pr_github_impl(
        &body.repo_path,
        body.pr_number,
        &body.merge_method,
        &body.expected_head_sha,
        &state,
    )
    .await
    {
        Ok(sha) => (StatusCode::OK, Json(serde_json::json!({"sha": sha}))).into_response(),
        Err(e) => err_500(&e),
    }
}

/// Fresh linked-worktree removal preflight.
pub(super) async fn workspace_lifecycle_http(
    State(state): State<Arc<AppState>>,
    Query(q): Query<WorkspaceIdQuery>,
) -> Response {
    if let Err(error) = validate_repo_path(&q.repo_path) {
        return error.into_response();
    }
    let result = tokio::task::spawn_blocking(move || {
        crate::worktree::inspect_worktree_removal(
            &state,
            std::path::Path::new(&q.repo_path),
            &q.workspace_id,
        )
    })
    .await;
    match result {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(error) => err_500(&format!("task panic: {error}")),
    }
}

pub(super) async fn finalize_merged_worktree_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<FinalizeMergeRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    let FinalizeMergeRequest {
        repo_path,
        workspace_id,
        action,
        force,
        expected_fingerprint,
    } = body;
    if force.unwrap_or(false) && expected_fingerprint.is_none() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "force requires expectedFingerprint from the confirmed lifecycle status"}))).into_response();
    }
    // Shares `finalize_merged_worktree_impl` with the Tauri command: the dirty-worktree
    // gate and the "worktree removed" notification live there, once, for both transports.
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::finalize_merged_worktree_impl_with_confirmation(
            &state,
            repo_path,
            workspace_id,
            action,
            force.unwrap_or(false),
            expected_fingerprint.as_deref(),
        )
    })
    .await;
    match res {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

pub(super) async fn switch_branch_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SwitchBranchRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    let SwitchBranchRequest {
        repo_path,
        branch_name,
        force,
        stash,
    } = body;
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::switch_branch_impl(&state, repo_path, branch_name, force, stash)
    })
    .await;
    match res {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

pub(super) async fn merge_and_archive_worktree_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<MergeArchiveRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    let MergeArchiveRequest {
        repo_path,
        branch_name,
        workspace_id,
        target_branch,
        after_merge,
        force,
        expected_fingerprint,
    } = body;
    if force.unwrap_or(false) && expected_fingerprint.is_none() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "force requires expectedFingerprint from the confirmed lifecycle status"}))).into_response();
    }
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::merge_and_archive_worktree_impl_with_confirmation(
            &state,
            repo_path,
            branch_name,
            workspace_id,
            target_branch,
            after_merge,
            force.unwrap_or(false),
            expected_fingerprint.as_deref(),
        )
    })
    .await;
    match res {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

/// Body of `POST /worktrees/run-script`.
#[derive(serde::Deserialize)]
pub(super) struct RunSetupScriptRequest {
    pub script: String,
    pub cwd: String,
}

/// `POST /worktrees/run-script` — mirror of the `run_setup_script` command.
/// The script runs a real process, so it goes to a blocking pool rather than
/// stalling the axum worker for its whole duration.
pub(super) async fn run_setup_script_http(
    Json(body): Json<RunSetupScriptRequest>,
) -> impl IntoResponse {
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::run_setup_script(body.script, body.cwd)
    })
    .await;
    match res {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("task panic: {e}")),
    }
}

#[cfg(test)]
mod warm_tests {
    use super::*;

    // Catches: safe removal destroys reusable artifacts or allocation leaves stale lifecycle paths.
    #[cfg(unix)]
    #[tokio::test]
    async fn recycling_removal_and_creation_return_rename_and_retract_old_workspace() {
        let temp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = setup_repo(temp.path());
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let directory = temp.path().join("worktrees");
        let first_config = crate::worktree::WorktreeConfig {
            task_name: "first".into(),
            base_repo: repo.to_string_lossy().into_owned(),
            branch: Some("first".into()),
            create_branch: true,
        };
        let (first, _) =
            crate::worktree::allocate_workspace(&state, &directory, &first_config, None).unwrap();
        let outcome = crate::worktree::remove_worktree_by_workspace_id_with_confirmation(
            &state,
            &first_config.base_repo,
            "first",
            true,
            None,
            false,
            false,
            None,
        )
        .unwrap();
        assert_eq!(outcome.removal_rule, "recyclable");
        assert!(!first.path.exists());
        let mut events = state.event_bus.subscribe();
        let second_config = crate::worktree::WorktreeConfig {
            task_name: "second".into(),
            branch: Some("second".into()),
            ..first_config
        };
        let (second, rename) =
            crate::worktree::allocate_workspace(&state, &directory, &second_config, None).unwrap();
        assert_eq!(
            rename.as_ref().unwrap()["new_path"],
            second.path.to_string_lossy().as_ref()
        );
        assert!(
            rename.as_ref().unwrap()["old_path"]
                .as_str()
                .unwrap()
                .contains("__archived")
        );
        assert!(
            !std::path::Path::new(rename.as_ref().unwrap()["old_path"].as_str().unwrap()).exists()
        );
        let event = events.try_recv().unwrap();
        assert!(
            matches!(event, crate::state::AppEvent::WorktreeRemoved(ref payload) if payload.workspace_id == "first")
        );
        assert_eq!(
            crate::worktree::get_worktree_paths(second_config.base_repo).unwrap()["second"].path,
            second.path.to_string_lossy()
        );
    }

    #[cfg(unix)]
    pub(super) fn setup_repo(root: &std::path::Path) -> std::path::PathBuf {
        let repo = root.join("repo");
        std::fs::create_dir(&repo).unwrap();
        let git = crate::git_cli::git_cmd(&repo);
        git.args(["init"]).run().unwrap();
        crate::git_cli::git_cmd(&repo)
            .args(["config", "user.email", "test@test.com"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(&repo)
            .args(["config", "user.name", "Test"])
            .run()
            .unwrap();
        std::fs::write(repo.join("README.md"), "base\n").unwrap();
        crate::git_cli::git_cmd(&repo)
            .args(["add", "."])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(&repo)
            .args(["commit", "-m", "base"])
            .run()
            .unwrap();
        repo
    }

    #[cfg(unix)]
    fn set_gated_setup_script(started: &std::path::Path, gate: &std::path::Path) {
        let mut defaults = crate::config::RepoDefaultsConfig::default();
        let finished = gate.with_extension("finished");
        let pid = gate.with_extension("pid");
        defaults.setup_script = format!(
            "echo $$ > '{}'; echo started > '{}'; while [ ! -f '{}' ]; do sleep 0.02; done; echo finished > '{}'",
            pid.display(),
            started.display(),
            gate.display(),
            finished.display()
        );
        crate::config::save_repo_defaults(crate::config::RepoDefaultsConfig::default(), defaults)
            .unwrap();
    }

    #[cfg(unix)]
    async fn wait_for_file(path: &std::path::Path, failure: &str) {
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            while !path.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{failure}"));
    }

    #[cfg(unix)]
    async fn wait_for_setup_exit(pid_file: &std::path::Path) {
        // `echo $$ > pid_file` truncates the file before writing the PID, so
        // polling on existence alone can read it mid-truncate.
        let content = tokio::time::timeout(std::time::Duration::from_secs(60), async {
            loop {
                let content = std::fs::read_to_string(pid_file).unwrap_or_default();
                if !content.trim().is_empty() {
                    return content;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("setup script did not record its PID"));
        let pid: i32 = content.trim().parse().unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            while unsafe { libc::kill(pid, 0) } == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("detached setup shell did not exit");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn create_worktree_shared_reports_pending_while_the_setup_script_runs() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = setup_repo(temp.path());
        let started = temp.path().join("setup.started");
        let gate = temp.path().join("setup.release");
        set_gated_setup_script(&started, &gate);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let repo_path = repo.to_string_lossy().into_owned();
        let state_for_create = Arc::clone(&state);
        let repo_for_create = repo_path.clone();
        let create = tokio::spawn(async move {
            create_worktree_shared(
                &state_for_create,
                repo_for_create,
                "pending-setup".into(),
                None,
                None,
                false,
            )
            .await
        });

        wait_for_file(&started, "setup script did not start").await;
        let paths = crate::worktree::get_worktree_paths(repo_path).unwrap();
        assert_eq!(
            paths["pending-setup"].warm_artifacts.as_ref().unwrap()["status"],
            "pending"
        );
        std::fs::write(&gate, "release").unwrap();
        let created = create
            .await
            .unwrap()
            .unwrap_or_else(|(status, body)| panic!("{status}: {:?}", body.0));
        assert_eq!(created.instructions["warm_artifacts"]["status"], "pending");
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while crate::worktree::warm_status(&created.worktree.path)["status"] == "pending" {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            crate::worktree::warm_status(&created.worktree.path)["status"],
            "done"
        );
        crate::worktree::clear_warm(&created.worktree.path);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancelled_create_marks_pending_warm_failed() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = tempfile::TempDir::new().unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = setup_repo(temp.path());
        let started = temp.path().join("setup.started");
        let gate = temp.path().join("setup.release");
        set_gated_setup_script(&started, &gate);
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let repo_path = repo.to_string_lossy().into_owned();
        let state_for_create = Arc::clone(&state);
        let repo_for_create = repo_path.clone();
        let create = tokio::spawn(async move {
            create_worktree_shared(
                &state_for_create,
                repo_for_create,
                "cancelled-setup".into(),
                None,
                None,
                false,
            )
            .await
        });

        wait_for_file(&started, "setup script did not start").await;
        let paths = crate::worktree::get_worktree_paths(repo_path).unwrap();
        let path = std::path::PathBuf::from(&paths["cancelled-setup"].path);
        assert_eq!(crate::worktree::warm_status(&path)["status"], "pending");
        create.abort();
        let _ = create.await;
        std::fs::write(&gate, "release").unwrap();
        wait_for_file(
            &gate.with_extension("finished"),
            "setup script did not finish",
        )
        .await;
        wait_for_setup_exit(&gate.with_extension("pid")).await;
        assert_eq!(crate::worktree::warm_status(&path)["status"], "failed");
        crate::worktree::clear_warm(&path);
    }

    #[tokio::test]
    async fn a_slow_warm_keeps_the_returned_workspace_pending_until_it_finishes() {
        let temp = tempfile::TempDir::new().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("workspace");
        let (release, wait) = std::sync::mpsc::channel::<()>();

        let token = crate::worktree::begin_warm(&destination);
        let task = crate::worktree::spawn_background_warm(
            source,
            destination.clone(),
            token,
            move |_, _| {
                wait.recv().unwrap();
                crate::cow::WarmingReport::default()
            },
        );

        assert!(!task.is_finished());
        assert_eq!(
            crate::worktree::warm_status(&destination)["status"],
            "pending"
        );
        release.send(()).unwrap();
        task.await.unwrap();
        assert_eq!(crate::worktree::warm_status(&destination)["status"], "done");
        crate::worktree::clear_warm(&destination);
    }

    #[tokio::test]
    async fn setup_finishes_before_warm_reads_the_workspace() {
        let temp = tempfile::TempDir::new().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("workspace");
        std::fs::create_dir(&destination).unwrap();
        let token = crate::worktree::begin_warm(&destination);
        assert_eq!(
            crate::worktree::warm_status(&destination)["status"],
            "pending"
        );
        let (setup, error, warm_task) = run_setup_then_warm(
            Some("echo ready > setup.marker".into()),
            source,
            destination.clone(),
            token,
            |_, destination| {
                assert!(destination.join("setup.marker").exists());
                crate::cow::WarmingReport::default()
            },
        )
        .await;
        assert!(setup.is_some(), "setup result: {error:?}");
        assert!(error.is_none(), "{error:?}");
        warm_task.await.unwrap();
        assert_eq!(crate::worktree::warm_status(&destination)["status"], "done");
        crate::worktree::clear_warm(&destination);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn setup_script_observes_pending_before_warm_starts() {
        let temp = tempfile::TempDir::new().unwrap();
        let destination = temp.path().join("workspace");
        std::fs::create_dir(&destination).unwrap();
        let token = crate::worktree::begin_warm(&destination);
        let (setup, error, task) = run_setup_then_warm(
            Some("sleep 2; echo setup > setup.marker".into()),
            temp.path().to_path_buf(),
            destination.clone(),
            token,
            |_, dest| {
                assert_eq!(crate::worktree::warm_status(dest)["status"], "pending");
                assert!(dest.join("setup.marker").exists());
                crate::cow::WarmingReport::default()
            },
        )
        .await;
        assert!(setup.is_some(), "{error:?}");
        task.await.unwrap();
        crate::worktree::clear_warm(&destination);
    }
}

#[cfg(test)]
mod survivor_tests {
    use super::*;

    async fn json(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Catches: force confirmation predicates rejecting non-force/confirmed requests or allowing unconfirmed force.
    #[tokio::test]
    async fn removal_confirmation_accepts_only_the_required_force_combinations() {
        let temp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        for (force, fingerprint, missing, expected) in [
            (false, None, false, StatusCode::INTERNAL_SERVER_ERROR),
            (
                true,
                Some("confirmed"),
                false,
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (true, None, true, StatusCode::INTERNAL_SERVER_ERROR),
            (true, None, false, StatusCode::BAD_REQUEST),
        ] {
            let response = remove_worktree_http(
                State(state.clone()),
                Path("unknown-workspace".into()),
                Query(RemoveWorktreeQuery {
                    repo_path: temp.path().to_string_lossy().into_owned(),
                    force: Some(force),
                    delete_branch: Some(false),
                    override_lock: None,
                    expected_fingerprint: fingerprint.map(str::to_owned),
                    confirm_missing_checkout: Some(missing),
                }),
            )
            .await;
            assert_eq!(
                response.status(),
                expected,
                "{force} {fingerprint:?} {missing}"
            );
            assert!(json(response).await["error"].is_string());
        }
    }

    /// Catches: empty repoPath falling through to generic path validation instead of the required-field error.
    #[tokio::test]
    async fn orphan_assessment_empty_repo_reports_required_field() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let response = assess_orphan_cleanup_http(
            State(state),
            Query(OptionalRepoQuery {
                repo_path: Some(String::new()),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            json(response).await,
            serde_json::json!({"error": "repoPath required"})
        );
    }

    /// Catches: lifecycle handler returning an empty success instead of a removal preflight document.
    #[tokio::test]
    async fn lifecycle_unknown_workspace_returns_a_preflight_document() {
        let temp = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let response = workspace_lifecycle_http(
            State(state),
            Query(WorkspaceIdQuery {
                repo_path: temp.path().to_string_lossy().into_owned(),
                workspace_id: "unknown".into(),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json(response).await;
        assert!(body.is_object(), "{body}");
        assert!(!body.as_object().unwrap().is_empty());
    }

    /// Catches: dropped keep/remove decisions and no-op clear handlers leaving a dialog unanswered.
    #[cfg(unix)]
    #[tokio::test]
    async fn orphan_decisions_and_clear_are_visible_to_other_clients() {
        let temp = tempfile::tempdir_in(tuic_test_support::non_temporary_fixture_root()).unwrap();
        let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let repo = super::warm_tests::setup_repo(temp.path());
        let orphan = temp.path().join("orphan");
        crate::git_cli::git_cmd(&repo)
            .args(["worktree", "add", "--detach", orphan.to_str().unwrap()])
            .run()
            .unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let repo_path = repo.to_string_lossy().into_owned();
        for (decision, expected) in [("keep", false), ("remove", true)] {
            crate::worktree::begin_orphan_cleanup_internal(
                &state,
                &repo_path,
                vec![orphan.to_string_lossy().into_owned()],
            )
            .unwrap();
            let response = answer_orphan_cleanup_http(
                State(state.clone()),
                Json(AnswerOrphanCleanupRequest {
                    repo_path: repo_path.clone(),
                    decision: decision.into(),
                }),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(json(response).await, serde_json::json!({"ok": true}));
            let pending = pending_orphan_cleanup_http(
                State(state.clone()),
                Query(OptionalRepoQuery {
                    repo_path: Some(repo_path.clone()),
                }),
            )
            .await;
            assert_eq!(json(pending).await, serde_json::json!(expected));
            let response = clear_orphan_cleanup_http(
                State(state.clone()),
                Json(ClearOrphanCleanupRequest {
                    repo_path: repo_path.clone(),
                    kept: false,
                }),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(json(response).await, serde_json::Value::Null);
            let pending = pending_orphan_cleanup_http(
                State(state.clone()),
                Query(OptionalRepoQuery {
                    repo_path: Some(repo_path.clone()),
                }),
            )
            .await;
            assert_eq!(json(pending).await, serde_json::Value::Null);
        }
    }
}
