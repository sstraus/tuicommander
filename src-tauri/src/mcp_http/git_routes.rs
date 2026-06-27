use axum::Json;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use super::types::*;
use super::{err_500, json_result, validate_repo_path};

pub(super) async fn repo_info(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    match tokio::task::spawn_blocking(move || crate::git::get_repo_info_cached(&state, &path)).await
    {
        Ok(info) => Json(info).into_response(),
        Err(e) => err_500(&format!("Task failed: {e}")),
    }
}

pub(super) async fn repo_diff(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    match crate::git::get_git_diff(path, None).await {
        Ok(diff) => (StatusCode::OK, Json(serde_json::json!({"diff": diff}))).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn repo_diff_stats(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    match crate::git::get_diff_stats(path, None).await {
        Ok(stats) => Json(stats).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn repo_changed_files(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    json_result(crate::git::get_changed_files(path, None).await)
}

pub(super) async fn repo_branches(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    json_result(crate::git::get_git_branches(path).await)
}

pub(super) async fn get_file_diff_http(Query(q): Query<FileQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    let file = q.file;
    let scope = q.scope;
    let untracked = q.untracked;
    json_result(crate::git::get_file_diff(path, file, scope, untracked).await)
}

pub(super) async fn list_markdown_files_http(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    match tokio::task::spawn_blocking(move || crate::list_markdown_files_impl(path)).await {
        Ok(Ok(files)) => (StatusCode::OK, Json(serde_json::json!(files))).into_response(),
        Ok(Err(e)) => err_500(&e),
        Err(e) => err_500(&format!("Task failed: {e}")),
    }
}

pub(super) async fn read_file_http(Query(q): Query<FileQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    let file = q.file;
    match tokio::task::spawn_blocking(move || crate::read_file_impl(path, file)).await {
        Ok(Ok(content)) => (StatusCode::OK, Json(serde_json::json!(content))).into_response(),
        Ok(Err(e)) => err_500(&e),
        Err(e) => err_500(&format!("Task failed: {e}")),
    }
}

pub(super) async fn rename_branch_http(Json(body): Json<RenameBranchRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }
    let path = body.path;
    let old_name = body.old_name;
    let new_name = body.new_name;
    match tokio::task::spawn_blocking(move || {
        crate::git::rename_branch_impl(&path, &old_name, &new_name)
    })
    .await
    {
        Ok(Ok(())) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Ok(Err(e)) => err_500(&e),
        Err(e) => err_500(&format!("Task failed: {e}")),
    }
}

pub(super) async fn get_initials_http(Query(q): Query<NameQuery>) -> impl IntoResponse {
    Json(crate::git::get_initials(q.name))
}

pub(super) async fn check_is_main_branch_http(Query(q): Query<BranchQuery>) -> impl IntoResponse {
    Json(crate::git::check_is_main_branch(q.branch))
}

pub(super) async fn get_recent_commits_http(Query(q): Query<RecentCommitsQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    let count = q.count;
    json_result(crate::git::get_recent_commits(path, count).await)
}

pub(super) async fn repo_summary(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    json_result(crate::git::get_repo_summary_impl(&state, q.path).await)
}

pub(super) async fn repo_structure(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    json_result(crate::git::get_repo_structure_impl(&state, q.path).await)
}

pub(super) async fn repo_diff_stats_batch(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    json_result(crate::git::get_repo_diff_stats_impl(&state, q.path).await)
}

pub(super) async fn repo_merged_branches(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    // Coalesced + cached load (same cache + TTL as the Tauri command).
    let cache = state.git_cache.merged_branches.clone();
    let p = path.clone();
    match tokio::task::spawn_blocking(move || {
        cache.try_get_with(p.clone(), || {
            crate::git::get_merged_branches_impl(std::path::Path::new(&p)).map(std::sync::Arc::new)
        })
    })
    .await
    {
        Ok(Ok(branches)) => (StatusCode::OK, Json(serde_json::json!(*branches))).into_response(),
        Ok(Err(e)) => err_500(&e.to_string()),
        Err(e) => err_500(&format!("Task failed: {e}")),
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
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    match crate::git::get_remote_url(path).await {
        Ok(Some(url)) => Json(serde_json::json!({"url": url})).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "No remote URL found"})),
        )
            .into_response(),
        Err(e) => err_500(&e),
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
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path.clone();
    let cache = state.git_cache.git_panel_context.clone();
    let p = path.clone();
    match tokio::task::spawn_blocking(move || {
        cache.get_with(p.clone(), || {
            std::sync::Arc::new(crate::git::get_git_panel_context_impl(
                std::path::Path::new(&p),
            ))
        })
    })
    .await
    {
        Ok(ctx) => Json(&*ctx).into_response(),
        Err(e) => err_500(&format!("Task failed: {e}")),
    }
}

/// Allowed git subcommands for the HTTP endpoint.
/// Only safe, non-destructive operations that the GitPanel needs.
const ALLOWED_GIT_SUBCOMMANDS: &[&str] = &[
    "fetch",
    "pull",
    "push",
    "stash",
    "log",
    "diff",
    "show",
    "branch",
    "tag",
    "merge",
    "rebase",
    "cherry-pick",
    "remote",
    "status",
    "rev-parse",
];

pub(super) async fn run_git_command_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Json(body): Json<RunGitCommandRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }

    // Validate subcommand against allowlist
    let subcommand = body.args.first().map(|s| s.as_str()).unwrap_or("");
    if !ALLOWED_GIT_SUBCOMMANDS.contains(&subcommand) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Git subcommand \"{subcommand}\" is not allowed via HTTP")
            })),
        )
            .into_response();
    }

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
    })
    .await
    {
        Ok(Ok(result)) => Json(result).into_response(),
        Ok(Err(e)) => err_500(&e),
        Err(e) => err_500(&format!("Task failed: {e}")),
    }
}

pub(super) async fn working_tree_status(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    match crate::git::get_working_tree_status(q.path).await {
        Ok(status) => Json(status).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn stage_files_http(Json(body): Json<StageFilesRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }
    let path = body.path;
    let files = body.files;
    match crate::git::git_stage_files(path, files).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn unstage_files_http(Json(body): Json<StageFilesRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }
    let path = body.path;
    let files = body.files;
    match crate::git::git_unstage_files(path, files).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn discard_files_http(Json(body): Json<StageFilesRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }
    let path = body.path;
    let files = body.files;
    match crate::git::git_discard_files(path, files).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn apply_reverse_patch_http(Json(body): Json<ReversePatchRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }
    let path = body.path;
    let patch = body.patch;
    let scope = body.scope;
    match crate::git::git_apply_reverse_patch(path, patch, scope).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn git_commit_http(Json(body): Json<CommitRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }
    let path = body.path;
    let message = body.message;
    let amend = body.amend;
    match crate::git::git_commit(path, message, amend).await {
        Ok(hash) => Json(serde_json::json!(hash)).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn commit_log_http(Query(q): Query<CommitLogQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    let count = q.count;
    let after = q.after;
    match crate::git::get_commit_log(path, count, after).await {
        Ok(entries) => Json(serde_json::json!(entries)).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn stash_list_http(Query(q): Query<PathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    match crate::git::get_stash_list(path).await {
        Ok(entries) => Json(serde_json::json!(entries)).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn stash_apply_http(Json(body): Json<StashRefRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }
    let path = body.path;
    let stash_ref = body.stash_ref;
    match crate::git::git_stash_apply(path, stash_ref).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn stash_pop_http(Json(body): Json<StashRefRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }
    let path = body.path;
    let stash_ref = body.stash_ref;
    match crate::git::git_stash_pop(path, stash_ref).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn stash_drop_http(Json(body): Json<StashRefRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }
    let path = body.path;
    let stash_ref = body.stash_ref;
    match crate::git::git_stash_drop(path, stash_ref).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn stash_show_http(Query(q): Query<StashRefRequest>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    let stash_ref = q.stash_ref;
    match crate::git::git_stash_show(path, stash_ref).await {
        Ok(diff) => Json(serde_json::json!(diff)).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn file_history_http(Query(q): Query<FilePathQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    let file = q.file;
    let count = q.count;
    let after = q.after;
    match crate::git::get_file_history(path, file, count, after).await {
        Ok(entries) => Json(serde_json::json!(entries)).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn file_blame_http(Query(q): Query<FileBlameQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    let file = q.file;
    match crate::git::get_file_blame(path, file).await {
        Ok(lines) => Json(serde_json::json!(lines)).into_response(),
        Err(e) => err_500(&e),
    }
}

// --- Git panel (story 064; browser/remote parity) ---
// Reads call the cfg_attr commands / *_impl fns directly; mutations call the
// non-gated *_impl + invalidate_repo_caches (mirroring the desktop wrappers).
// update_from_base / switch_branch / merge_and_archive_worktree / run_diff_triage
// are intentionally NOT mapped here (see todo.md).

pub(super) async fn get_gutter_changes_http(Query(q): Query<GitGutterQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    json_result(crate::git::get_gutter_changes(q.path, q.file, q.scope).await)
}

pub(super) async fn get_branches_detail_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Query(q): Query<PathQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    json_result(crate::git::branches_detail_cached(&state, q.path).await)
}

pub(super) async fn get_recent_branches_http(Query(q): Query<GitRecentBranchesQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    json_result(crate::git::get_recent_branches(q.path, q.limit).await)
}

pub(super) async fn get_branch_base_http(Query(q): Query<GitBranchBaseQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    // Option<String> -> 200 with JSON null on miss; the TS mapping passes null through.
    json_result(crate::git::get_branch_base(q.path, q.branch_name).await)
}

pub(super) async fn check_worktree_dirty_http(Query(q): Query<GitWorktreeDirtyQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let GitWorktreeDirtyQuery {
        repo_path,
        branch_name,
    } = q;
    match tokio::task::spawn_blocking(move || {
        crate::worktree::check_worktree_dirty(repo_path, branch_name)
    })
    .await
    {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("Task failed: {e}")),
    }
}

pub(super) async fn list_base_ref_options_http(Query(q): Query<GitRepoQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let repo_path = q.repo_path;
    match tokio::task::spawn_blocking(move || crate::worktree::list_base_ref_options(repo_path))
        .await
    {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("Task failed: {e}")),
    }
}

pub(super) async fn generate_clone_branch_name_http(
    Json(body): Json<GitCloneBranchNameRequest>,
) -> Response {
    json_result(Ok::<String, String>(
        crate::worktree::generate_clone_branch_name_cmd(body.source_branch, body.existing_names),
    ))
}

pub(super) async fn get_commit_graph_http(Query(q): Query<GitCommitGraphQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    json_result(crate::git_graph::get_commit_graph(q.path, q.count).await)
}

pub(super) async fn create_branch_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Json(body): Json<GitCreateBranchRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }
    let GitCreateBranchRequest {
        path,
        name,
        start_point,
        checkout,
    } = body;
    let res = tokio::task::spawn_blocking(move || {
        crate::git::create_branch_impl(&path, &name, start_point.as_deref(), checkout)?;
        state.invalidate_repo_caches(&path);
        Ok::<(), String>(())
    })
    .await;
    match res {
        Ok(Ok(())) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Ok(Err(e)) => err_500(&e),
        Err(e) => err_500(&format!("Task failed: {e}")),
    }
}

pub(super) async fn delete_branch_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Json(body): Json<GitDeleteBranchRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }
    let GitDeleteBranchRequest { path, name, force } = body;
    let res = tokio::task::spawn_blocking(move || {
        let r = crate::git::delete_branch_impl(&path, &name, force)?;
        state.invalidate_repo_caches(&path);
        Ok::<_, String>(r)
    })
    .await;
    match res {
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("Task failed: {e}")),
    }
}

pub(super) async fn delete_local_branch_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Json(body): Json<GitDeleteLocalBranchRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    let GitDeleteLocalBranchRequest {
        repo_path,
        branch_name,
        keep_worktree,
    } = body;
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::delete_local_branch_impl(
            &repo_path,
            &branch_name,
            keep_worktree.unwrap_or(false),
        )?;
        state.invalidate_repo_caches(&repo_path);
        Ok::<(), String>(())
    })
    .await;
    match res {
        Ok(Ok(())) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Ok(Err(e)) => err_500(&e),
        Err(e) => err_500(&format!("Task failed: {e}")),
    }
}
