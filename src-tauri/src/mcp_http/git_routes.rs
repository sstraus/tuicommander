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
    let scope = q.scope;
    let untracked = q.untracked;
    match tokio::task::spawn_blocking(move || crate::git::get_file_diff(path, file, scope, untracked)).await {
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

pub(super) async fn repo_summary(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    match crate::git::get_repo_summary_impl(&state, q.path).await {
        Ok(summary) => (StatusCode::OK, Json(serde_json::json!(summary))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn repo_structure(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    match crate::git::get_repo_structure_impl(&state, q.path).await {
        Ok(structure) => (StatusCode::OK, Json(serde_json::json!(structure))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn repo_diff_stats_batch(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    match crate::git::get_repo_diff_stats_impl(&state, q.path).await {
        Ok(stats) => (StatusCode::OK, Json(serde_json::json!(stats))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn repo_merged_branches(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    // Check cache first (same pattern as Tauri command)
    if let Some(cached) = crate::AppState::get_cached(&state.git_cache.merged_branches, &path, crate::state::GIT_CACHE_TTL) {
        return (StatusCode::OK, Json(serde_json::json!(cached))).into_response();
    }
    let state_clone = state.clone();
    let path_clone = path.clone();
    match tokio::task::spawn_blocking(move || crate::git::get_merged_branches_impl(std::path::Path::new(&path_clone))).await {
        Ok(Ok(branches)) => {
            crate::AppState::set_cached(&state_clone.git_cache.merged_branches, path, branches.clone());
            (StatusCode::OK, Json(serde_json::json!(branches))).into_response()
        }
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn get_local_ip_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
) -> impl axum::response::IntoResponse {
    Json(crate::pick_preferred_ip(crate::get_local_ips_impl(&state)))
}

pub(super) async fn get_local_ips_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
) -> impl axum::response::IntoResponse {
    Json(crate::get_local_ips_impl(&state))
}

pub(super) async fn remote_url(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    match tokio::task::spawn_blocking(move || crate::git::get_remote_url(path)).await {
        Ok(Some(url)) => Json(serde_json::json!({"url": url})).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "No remote URL found"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn list_user_plugins_http() -> impl axum::response::IntoResponse {
    Json(serde_json::json!(crate::plugins::list_user_plugins()))
}

// --- GitPanel commands ---

pub(super) async fn git_panel_context(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path.clone();
    if let Some(cached) = crate::AppState::get_cached(&state.git_cache.git_panel_context, &path, crate::state::GIT_CACHE_TTL) {
        return Json(cached).into_response();
    }
    let state_clone = state.clone();
    let path_clone = path.clone();
    match tokio::task::spawn_blocking(move || crate::git::get_git_panel_context_impl(std::path::Path::new(&path_clone))).await {
        Ok(ctx) => {
            crate::AppState::set_cached(&state_clone.git_cache.git_panel_context, path, ctx.clone());
            Json(ctx).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn run_git_command_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Json(body): Json<RunGitCommandRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.path) { return e.into_response(); }
    let path = body.path;
    let args = body.args;
    let state_clone = state.clone();
    match tokio::task::spawn_blocking(move || {
        let repo_path = std::path::PathBuf::from(&path);
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let mut builder = crate::git_cli::git_cmd(&repo_path).args(&args_str);
        if let Some(ref askpass_path) = crate::git::ensure_askpass_script() {
            let askpass_str = askpass_path.to_string_lossy();
            builder = builder
                .env("SSH_ASKPASS", &askpass_str)
                .env("SSH_ASKPASS_REQUIRE", "prefer")
                .env("DISPLAY", ":0");
        }
        match builder.run_raw() {
            Ok(o) => {
                let success = o.status.success();
                let result = crate::git::GitCommandResult {
                    success,
                    stdout: String::from_utf8_lossy(&o.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&o.stderr).to_string(),
                    exit_code: o.status.code().unwrap_or(-1),
                };
                if success {
                    state_clone.invalidate_repo_caches(&path);
                }
                Ok(result)
            }
            Err(e) => Err(format!("git command failed: {e}")),
        }
    }).await {
        Ok(Ok(result)) => Json(result).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn working_tree_status(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    match tokio::task::spawn_blocking(move || {
        let repo_path = std::path::PathBuf::from(&path);
        let out = crate::git_cli::git_cmd(&repo_path)
            .args(["status", "--porcelain=v2", "--branch", "--show-stash"])
            .run()
            .map_err(|e| format!("git status failed: {e}"))?;
        let mut status = crate::git::parse_porcelain_v2(&out.stdout);
        crate::git::enrich_with_numstat(&repo_path, &mut status.staged, true);
        crate::git::enrich_with_numstat(&repo_path, &mut status.unstaged, false);
        Ok::<_, String>(status)
    }).await {
        Ok(Ok(status)) => Json(status).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn stage_files_http(Json(body): Json<StageFilesRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) { return e.into_response(); }
    let path = body.path;
    let files = body.files;
    match tokio::task::spawn_blocking(move || crate::git::git_stage_files(path, files)).await {
        Ok(Ok(())) => Json(serde_json::json!({"ok": true})).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn unstage_files_http(Json(body): Json<StageFilesRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) { return e.into_response(); }
    let path = body.path;
    let files = body.files;
    match tokio::task::spawn_blocking(move || crate::git::git_unstage_files(path, files)).await {
        Ok(Ok(())) => Json(serde_json::json!({"ok": true})).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn discard_files_http(Json(body): Json<StageFilesRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) { return e.into_response(); }
    let path = body.path;
    let files = body.files;
    match tokio::task::spawn_blocking(move || crate::git::git_discard_files(path, files)).await {
        Ok(Ok(())) => Json(serde_json::json!({"ok": true})).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn git_commit_http(Json(body): Json<CommitRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) { return e.into_response(); }
    let path = body.path;
    let message = body.message;
    let amend = body.amend;
    match tokio::task::spawn_blocking(move || crate::git::git_commit(path, message, amend)).await {
        Ok(Ok(hash)) => Json(serde_json::json!(hash)).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn commit_log_http(Query(q): Query<CommitLogQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    let count = q.count;
    let after = q.after;
    match crate::git::get_commit_log(path, count, after).await {
        Ok(entries) => Json(serde_json::json!(entries)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn stash_list_http(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    match tokio::task::spawn_blocking(move || crate::git::get_stash_list(path)).await {
        Ok(Ok(entries)) => Json(serde_json::json!(entries)).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn stash_apply_http(Json(body): Json<StashRefRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) { return e.into_response(); }
    let path = body.path;
    let stash_ref = body.stash_ref;
    match tokio::task::spawn_blocking(move || crate::git::git_stash_apply(path, stash_ref)).await {
        Ok(Ok(())) => Json(serde_json::json!({"ok": true})).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn stash_pop_http(Json(body): Json<StashRefRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) { return e.into_response(); }
    let path = body.path;
    let stash_ref = body.stash_ref;
    match tokio::task::spawn_blocking(move || crate::git::git_stash_pop(path, stash_ref)).await {
        Ok(Ok(())) => Json(serde_json::json!({"ok": true})).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn stash_drop_http(Json(body): Json<StashRefRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) { return e.into_response(); }
    let path = body.path;
    let stash_ref = body.stash_ref;
    match tokio::task::spawn_blocking(move || crate::git::git_stash_drop(path, stash_ref)).await {
        Ok(Ok(())) => Json(serde_json::json!({"ok": true})).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn stash_show_http(Query(q): Query<StashRefRequest>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    let stash_ref = q.stash_ref;
    match tokio::task::spawn_blocking(move || crate::git::git_stash_show(path, stash_ref)).await {
        Ok(Ok(diff)) => Json(serde_json::json!(diff)).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {e}")).into_response(),
    }
}

pub(super) async fn file_history_http(Query(q): Query<FilePathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    let file = q.file;
    let count = q.count;
    let after = q.after;
    match crate::git::get_file_history(path, file, count, after).await {
        Ok(entries) => Json(serde_json::json!(entries)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn file_blame_http(Query(q): Query<FileBlameQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) { return e.into_response(); }
    let path = q.path;
    let file = q.file;
    match crate::git::get_file_blame(path, file).await {
        Ok(lines) => Json(serde_json::json!(lines)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}
