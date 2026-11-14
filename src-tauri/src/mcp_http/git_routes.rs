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

pub(super) async fn repo_diff(Query(q): Query<DiffScopeQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    match crate::git::get_git_diff(path, q.scope).await {
        Ok(diff) => (StatusCode::OK, Json(serde_json::json!({"diff": diff}))).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn repo_diff_stats(Query(q): Query<DiffScopeQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    match crate::git::get_diff_stats(path, q.scope).await {
        Ok(stats) => Json(stats).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn repo_changed_files(Query(q): Query<DiffScopeQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    let path = q.path;
    json_result(crate::git::get_changed_files(path, q.scope).await)
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
        crate::git::rename_branch_with_label(&path, &old_name, &new_name)
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

#[derive(serde::Deserialize)]
pub(super) struct PluginListQuery {
    plugin_ids: Option<String>,
}

pub(super) async fn list_user_plugins_http(
    Query(query): Query<PluginListQuery>,
) -> impl axum::response::IntoResponse {
    let ids = match query
        .plugin_ids
        .map(|ids| serde_json::from_str::<Vec<String>>(&ids))
        .transpose()
    {
        Ok(ids) => ids,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": error.to_string()})),
            )
                .into_response();
        }
    };
    // The shared command owns the blocking-pool boundary for both transports.
    match crate::plugins::list_user_plugins(ids).await {
        Ok(plugins) => Json(serde_json::json!(plugins)).into_response(),
        Err(e) => err_500(&e),
    }
}

// --- GitPanel commands ---

pub(super) async fn run_git_command_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Json(body): Json<RunGitCommandRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }

    if let Err(error) = crate::git::validate_git_command_args(&body.args) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error})),
        )
            .into_response();
    }

    let path = body.path;
    let args = body.args;
    // Shared with the Tauri command so both transports carry the same deadline
    // and return the same shape: a failure — a killed network command included —
    // is a `GitCommandResult` with `success: false`, not an HTTP error.
    match tokio::task::spawn_blocking(move || {
        crate::git::run_git_command_blocking(&state, &path, &args)
    })
    .await
    {
        Ok(result) => Json(result).into_response(),
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

pub(super) async fn stash_facts_http(Query(q): Query<StashRefRequest>) -> Response {
    if let Err(e) = validate_repo_path(&q.path) {
        return e.into_response();
    }
    match crate::git::git_stash_facts(q.path, q.stash_ref).await {
        Ok(facts) => Json(facts).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn stash_check_drop_http(Json(body): Json<StashCheckDropRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }
    match crate::git::git_stash_check_drop(body.path, body.stash_ref, body.hash, body.verdict).await
    {
        Ok(outcome) => Json(outcome).into_response(),
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
// update_from_base / switch_branch / merge_and_archive_worktree are intentionally
// NOT mapped here. run_diff_triage used to be on that list; it went
// with the embedded AI engine (#784-0aec).

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
        workspace_id,
    } = q;
    match tokio::task::spawn_blocking(move || {
        crate::worktree::check_worktree_dirty(repo_path, workspace_id)
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
        workspace_id,
        keep_worktree,
    } = body;
    let res = tokio::task::spawn_blocking(move || {
        crate::worktree::guard_suspended_workspace(
            std::path::Path::new(&repo_path),
            &workspace_id,
        )?;
        crate::worktree::delete_local_branch_impl(
            &repo_path,
            &branch_name,
            &workspace_id,
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

pub(super) async fn update_from_base_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    Json(body): Json<GitUpdateFromBaseRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.path) {
        return e.into_response();
    }
    let GitUpdateFromBaseRequest {
        path,
        branch_name,
        strategy,
    } = body;
    let res = tokio::task::spawn_blocking(move || {
        crate::git::update_from_base_impl(&state, &path, &branch_name, strategy.as_deref())
    })
    .await;
    match res {
        // Plain-string result: serialized bare so `invoke<string>` receives it directly.
        Ok(r) => json_result(r),
        Err(e) => err_500(&format!("Task failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::ALLOWED_GIT_SUBCOMMANDS;
    use std::time::{Duration, Instant};

    /// A remote that completes the TCP handshake and then answers nothing, so a
    /// `git fetch` aimed at it blocks reading the ref advertisement. It
    /// reproduces the shape this endpoint has to survive — a host that is
    /// reachable but mute — without depending on a real network or on how a
    /// firewall treats an unroutable address.
    ///
    /// The accept loop parks for the life of the test binary; there is nothing
    /// to shut down because a mute remote has nothing to say.
    fn mute_git_remote() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("local_addr").port();
        std::thread::spawn(move || {
            let mut held = Vec::new();
            while let Ok((stream, _)) = listener.accept() {
                held.push(stream);
            }
        });
        port
    }

    /// A repo whose `origin` points at `port` and which has never been fetched.
    fn repo_pointing_at(port: u16) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .current_dir(dir.path())
                .args(args)
                .output()
                .expect("git");
        };
        run(&["init"]);
        run(&[
            "remote",
            "add",
            "origin",
            &format!("git://127.0.0.1:{port}/mute.git"),
        ]);
        dir
    }

    async fn json_body(response: Response) -> serde_json::Value {
        let (_, body) = response.into_parts();
        let bytes = axum::body::to_bytes(body, usize::MAX).await.expect("body");
        serde_json::from_slice(&bytes).expect("json body")
    }

    /// Catches the HTTP twin dropping the optional ID array, or malformed JSON
    /// silently widening a targeted request into full-store discovery.
    #[tokio::test]
    async fn plugin_list_http_preserves_empty_filter_and_rejects_invalid_json() {
        let empty = list_user_plugins_http(Query(PluginListQuery {
            plugin_ids: Some("[]".into()),
        }))
        .await
        .into_response();
        assert_eq!(empty.status(), StatusCode::OK);
        assert_eq!(json_body(empty).await, serde_json::json!([]));
        let invalid = list_user_plugins_http(Query(PluginListQuery {
            plugin_ids: Some("not-json".into()),
        }))
        .await
        .into_response();
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
        assert!(
            json_body(invalid).await["error"]
                .as_str()
                .is_some_and(|error| !error.is_empty())
        );
    }

    /// Pins the audit in `git::NETWORK_GIT_SUBCOMMANDS` in both directions.
    ///
    /// The second assertion alone would be near-tautological: it filters the
    /// allowlist *through* the network set and then compares against that same
    /// set, so adding `clone` to the allowlist and forgetting to classify it
    /// changes nothing — the filter simply drops it, and an unbounded network
    /// command becomes reachable over HTTP with the test still green. That is
    /// the exact regression this test exists to catch, so the allowlist is
    /// pinned verbatim first: any addition fails here and forces a decision.
    #[test]
    fn every_allowlisted_subcommand_that_reaches_a_remote_is_bounded() {
        assert_eq!(
            ALLOWED_GIT_SUBCOMMANDS,
            &[
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
            ],
            "a new allowlisted subcommand must be classified in \
             NETWORK_GIT_SUBCOMMANDS before it is added here — an unclassified \
             one that reaches a remote runs unbounded"
        );
        // And nothing may quietly leave the network set: these four must stay
        // bounded for as long as they are reachable over HTTP.
        let bounded: Vec<&str> = ALLOWED_GIT_SUBCOMMANDS
            .iter()
            .copied()
            .filter(|sub| crate::git::is_network_git_subcommand(&[(*sub).to_string()]))
            .collect();
        assert_eq!(bounded, vec!["fetch", "pull", "push", "remote"]);
    }

    /// Runs fixture git in `dir` and returns stdout; a failing setup step panics with stderr.
    fn git_in(dir: &std::path::Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
            .args(args)
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// A throwaway clone with a local bare `origin`, a pushed `main`, a local
    /// `feature` branch, and a committed file whose name starts with a dash.
    fn repo_with_local_remote() -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        let remote = root.path().join("origin.git");
        let work = root.path().join("work");
        git_in(root.path(), &["init", "--bare", "-b", "main", "origin.git"]);
        git_in(root.path(), &["init", "-b", "main", "work"]);
        std::fs::write(work.join("-dash.txt"), "one\n").unwrap();
        git_in(&work, &["add", "--", "-dash.txt"]);
        git_in(&work, &["commit", "-m", "first subject"]);
        git_in(
            &work,
            &["remote", "add", "origin", &remote.to_string_lossy()],
        );
        git_in(&work, &["push", "-u", "origin", "main"]);
        git_in(&work, &["branch", "feature"]);
        (root, work)
    }

    async fn run_git_http(
        state: &std::sync::Arc<crate::AppState>,
        dir: &std::path::Path,
        args: &[&str],
    ) -> (StatusCode, serde_json::Value) {
        let response = run_git_command_http(
            axum::extract::State(state.clone()),
            Json(RunGitCommandRequest {
                path: dir.to_string_lossy().into_owned(),
                args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            }),
        )
        .await;
        let status = response.status();
        (status, json_body(response).await)
    }

    // Catches: the owner cannot request `log --oneline` or `push --force-with-lease`
    // (or any other normal option) through the authenticated Git API.
    #[tokio::test]
    async fn authenticated_git_api_accepts_normal_flags() {
        let (_root, work) = repo_with_local_remote();
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());

        let (status, body) = run_git_http(&state, &work, &["log", "--oneline"]).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["success"], true, "{body}");
        let short = git_in(&work, &["rev-parse", "--short", "HEAD"]);
        assert_eq!(
            body["stdout"].as_str().unwrap().trim(),
            format!("{} first subject", short.trim())
        );

        // An option value in its own argv slot, and a dash-prefixed file after `--`.
        let (status, body) = run_git_http(
            &state,
            &work,
            &["log", "-n", "1", "--format=%s", "--", "-dash.txt"],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["success"], true, "{body}");
        assert_eq!(body["stdout"].as_str().unwrap().trim(), "first subject");

        // Rewrite the pushed tip, then a lease-protected force push must land it.
        git_in(&work, &["commit", "--amend", "-m", "amended subject"]);
        let local = git_in(&work, &["rev-parse", "HEAD"]);
        let (status, body) = run_git_http(
            &state,
            &work,
            &["push", "--force-with-lease", "origin", "main"],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["success"], true, "{body}");
        let remote = git_in(&work, &["ls-remote", "origin", "refs/heads/main"]);
        assert!(remote.starts_with(local.trim()), "{remote} vs {local}");

        // The IPC twin shares the policy: the same argv succeeds there too.
        let ipc = crate::git::run_git_command_blocking(
            &state,
            &work.to_string_lossy(),
            &["diff".to_owned(), "--stat".to_owned(), "HEAD".to_owned()],
        );
        assert!(ipc.success, "{}", ipc.stderr);
    }

    // Catches: the relaxed option policy letting an unauthenticated or cross-site
    // request through, or letting generic `branch` delete a branch past the
    // dedicated deletion owner (abbreviated long option, short cluster), while
    // non-deleting `branch` options still work.
    #[tokio::test]
    async fn git_api_cannot_bypass_auth_or_deletion_safety() {
        use tower::ServiceExt;
        let (_root, work) = repo_with_local_remote();
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        *state.session_token.write() = "git-token".into();
        let app = super::super::build_router(state.clone(), true, true);
        let feature_exists = || {
            std::process::Command::new("git")
                .current_dir(&work)
                .args(["rev-parse", "--verify", "--quiet", "refs/heads/feature"])
                .output()
                .expect("git")
                .status
                .success()
        };
        let body = serde_json::json!({
            "path": work.to_string_lossy(),
            "args": ["branch", "-D", "feature"],
        })
        .to_string();
        let request = |uri: &str, origin: Option<&str>| {
            let mut req = axum::http::Request::post(uri)
                .header(axum::http::header::HOST, "127.0.0.1:9876")
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .extension(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                    [192, 168, 1, 50],
                    40000,
                ))))
                .body(axum::body::Body::from(body.clone()))
                .unwrap();
            if let Some(origin) = origin {
                req.headers_mut()
                    .insert(axum::http::header::ORIGIN, origin.parse().unwrap());
            }
            req
        };
        let unauthenticated = app
            .clone()
            .oneshot(request("/repo/run-git", None))
            .await
            .unwrap();
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
        let cross_site = app
            .clone()
            .oneshot(request(
                "/repo/run-git?token=git-token",
                Some("https://evil.example"),
            ))
            .await
            .unwrap();
        assert_eq!(cross_site.status(), StatusCode::FORBIDDEN);
        assert!(feature_exists(), "a refused request must not delete");

        for args in [
            vec!["branch", "-D", "feature"],
            vec!["branch", "-d", "feature"],
            vec!["branch", "--delete", "feature"],
            vec!["branch", "--del", "feature"],
            vec!["branch", "-fd", "feature"],
            vec!["branch", "--force", "-Dq", "feature"],
        ] {
            let (status, body) = run_git_http(&state, &work, &args).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{args:?}: {body}");
            assert!(feature_exists(), "{args:?} deleted the branch");
            let ipc = crate::git::run_git_command_blocking(
                &state,
                &work.to_string_lossy(),
                &args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
            );
            assert!(!ipc.success, "{args:?} passed over IPC");
            assert!(feature_exists(), "{args:?} deleted the branch over IPC");
        }

        // Non-deleting options stay available; `--` ends option parsing, so a
        // dash-named argument after it is never read as a deletion flag.
        let (status, body) = run_git_http(&state, &work, &["branch", "-vv"]).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["success"], true, "{body}");
        let (status, body) = run_git_http(&state, &work, &["log", "--oneline", "--", "-d"]).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(feature_exists());
    }

    // Catches: a force rename or copy (`branch -M`, `-C`, `--move --force`)
    // overwriting an existing branch, which drops its unmerged commits past the
    // dedicated deletion owner exactly like a generic `branch -D` would.
    #[tokio::test]
    async fn git_api_cannot_overwrite_an_existing_branch() {
        let (_root, work) = repo_with_local_remote();
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        git_in(&work, &["checkout", "-q", "-b", "victim"]);
        git_in(
            &work,
            &["commit", "--allow-empty", "-m", "unique victim work"],
        );
        let unique = git_in(&work, &["rev-parse", "HEAD"]);
        git_in(&work, &["checkout", "-q", "main"]);
        for args in [
            vec!["branch", "-M", "feature", "victim"],
            vec!["branch", "-C", "feature", "victim"],
            vec!["branch", "--move", "--force", "feature", "victim"],
        ] {
            let (status, body) = run_git_http(&state, &work, &args).await;
            let victim = git_in(&work, &["rev-parse", "refs/heads/victim"]);
            assert_eq!(
                victim.trim(),
                unique.trim(),
                "{args:?} overwrote an unmerged branch: {status} {body}"
            );
        }
    }

    // Catches: a restrictive flag policy breaking GitPanel's local operations.
    #[tokio::test]
    async fn run_git_http_preserves_git_panel_operations() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        for args in [
            vec!["init", "-b", "main"],
            vec![
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "--allow-empty",
                "-m",
                "initial",
            ],
            vec!["branch", "feature"],
        ] {
            let output = std::process::Command::new("git")
                .current_dir(dir.path())
                .args(args)
                .output()
                .expect("git");
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        for args in [
            vec!["status", "--porcelain"],
            vec!["diff", "--name-status", "main...feature"],
            vec!["merge", "feature"],
            vec!["rebase", "feature"],
            vec!["stash", "push"],
            vec!["fetch", "--all"],
        ] {
            let response = run_git_command_http(
                axum::extract::State(state.clone()),
                Json(RunGitCommandRequest {
                    path: dir.path().to_string_lossy().into_owned(),
                    args: args.iter().map(|arg| (*arg).to_owned()).collect(),
                }),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK, "{args:?}");
            let body = json_body(response).await;
            assert_eq!(body["success"], true, "{args:?}: {body}");
        }
    }

    #[tokio::test]
    async fn fetch_over_http_gives_up_on_a_mute_remote() {
        // Setup gets its own budget, and deliberately not a deadline. Two git
        // subprocesses and a socket bind must not be able to fail this test on
        // time — a loaded machine that takes a while to reach the starting line
        // has not disproved anything about the deadline under test. It is only
        // measured so a failure below can name it instead of implying the
        // deadline misfired.
        let setup_started = Instant::now();
        let port = mute_git_remote();
        let dir = repo_pointing_at(port);
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        let body = RunGitCommandRequest {
            path: dir.path().to_string_lossy().to_string(),
            args: vec!["fetch".to_string(), "origin".to_string()],
        };
        let setup = setup_started.elapsed();

        // The bound must wrap the await, not follow it. Asserting on an elapsed
        // measured *after* the call can only fire when the call returned — the
        // one failure it named, "never came back", makes the assertion itself
        // unreachable and hands the test to nextest's anonymous 120s kill,
        // which names nothing. That is how this test once failed as a bare
        // TIMED OUT and accused the mechanism it exists to prove works.
        //
        // 35s is far above the 5s deadline under test and far below the outer
        // 120s, so setup keeps 85s of headroom it cannot spend on us.
        let response = tokio::time::timeout(
            crate::git_cli::FETCH_TIMEOUT + Duration::from_secs(30),
            run_git_command_http(axum::extract::State(state), Json(body)),
        )
        .await
        .unwrap_or_else(|_| {
            panic!(
                "run_git_command_http never returned: the FETCH_TIMEOUT inside it \
                 did not fire. Setup had already finished in {setup:?}, so this is \
                 the deadline, not a slow machine"
            )
        });

        let json = json_body(response).await;
        assert_eq!(
            json["success"], false,
            "a killed fetch must not report success: {json}"
        );
        // Names the deadline, so this cannot pass on a fetch that failed for an
        // unrelated reason and happened to be fast.
        assert!(
            json["stderr"]
                .as_str()
                .unwrap_or_default()
                .contains("timed out"),
            "expected the deadline named in stderr, got {json}"
        );
    }

    /// A repo where the index and the working tree disagree: `staged.txt` is a
    /// new two-line file in the index only, and the committed `tracked.txt` has
    /// one unstaged line edit. Each scope therefore names a different file and
    /// a different line count.
    fn repo_with_staged_and_unstaged_edits() -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("tempdir");
        let work = root.path().join("work");
        git_in(root.path(), &["init", "-b", "main", "work"]);
        std::fs::write(work.join("tracked.txt"), "one\n").unwrap();
        git_in(&work, &["add", "tracked.txt"]);
        git_in(&work, &["commit", "-m", "first"]);
        std::fs::write(work.join("staged.txt"), "alpha\nbeta\n").unwrap();
        git_in(&work, &["add", "staged.txt"]);
        std::fs::write(work.join("tracked.txt"), "two\n").unwrap();
        (root, work)
    }

    /// GETs `route` through the real router, so the query string goes through
    /// the same deserialization a browser or remote client hits.
    async fn get_repo_route(route: &str, repo: &std::path::Path, scope: &str) -> serde_json::Value {
        use axum::body::Body;
        use axum::extract::connect_info::ConnectInfo;
        use tower::ServiceExt;
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("path", &repo.to_string_lossy())
            .append_pair("scope", scope)
            .finish();
        let mut request = axum::http::Request::get(format!("{route}?{query}"))
            .header(axum::http::header::HOST, "127.0.0.1:9876")
            .body(Body::empty())
            .expect("build GET request");
        request
            .extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))));
        let response = super::super::build_router(super::super::tests::test_state(), false, true)
            .oneshot(request)
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::OK, "{route} scope={scope}");
        json_body(response).await
    }

    // Catches: the HTTP diff routes drop `scope` and always answer with the
    // unstaged diff, so browser and remote clients see the wrong changes.
    #[tokio::test]
    async fn diff_routes_honour_the_scope_query() {
        let (_root, repo) = repo_with_staged_and_unstaged_edits();

        let staged = get_repo_route("/repo/diff", &repo, "staged").await;
        let staged = staged["diff"].as_str().expect("diff string");
        assert!(staged.contains("+alpha"), "staged diff: {staged}");
        assert!(!staged.contains("+two"), "staged diff: {staged}");
        let unstaged = get_repo_route("/repo/diff", &repo, "").await;
        let unstaged = unstaged["diff"].as_str().expect("diff string");
        assert!(unstaged.contains("+two"), "unstaged diff: {unstaged}");
        assert!(!unstaged.contains("+alpha"), "unstaged diff: {unstaged}");

        assert_eq!(
            get_repo_route("/repo/diff-stats", &repo, "staged").await,
            serde_json::json!({"additions": 2, "deletions": 0})
        );
        assert_eq!(
            get_repo_route("/repo/diff-stats", &repo, "").await,
            serde_json::json!({"additions": 1, "deletions": 1})
        );

        let paths = |files: serde_json::Value| -> Vec<String> {
            files
                .as_array()
                .expect("file list")
                .iter()
                .map(|file| file["path"].as_str().expect("path").to_owned())
                .collect()
        };
        assert_eq!(
            paths(get_repo_route("/repo/files", &repo, "staged").await),
            ["staged.txt"]
        );
        assert_eq!(
            paths(get_repo_route("/repo/files", &repo, "").await),
            ["tracked.txt"]
        );
        // `head` is the third named scope: index and working tree together.
        let mut head = paths(get_repo_route("/repo/files", &repo, "head").await);
        head.sort();
        assert_eq!(head, ["staged.txt", "tracked.txt"]);
    }
}
