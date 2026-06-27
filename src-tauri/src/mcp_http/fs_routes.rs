use axum::Json;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use super::types::*;
use super::{err_500, json_result, validate_repo_path};

// `list_directory_http` intentionally omits `State` + `indexer_throttle`: the
// underlying `fs::list_directory_impl` is a single `read_dir` + sort, which
// completes in microseconds on non-pathological directories and does not walk
// recursively. The throttle exists to keep *long* blocking walks (search,
// content grep, BM25 indexing) off the Tokio executor.
pub(super) async fn list_directory_http(Query(q): Query<FsDirQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    json_result(crate::fs::list_directory_impl(
        q.repo_path,
        q.subdir.unwrap_or_default(),
    ))
}

pub(super) async fn search_files_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::state::AppState>>,
    Query(q): Query<FsSearchQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    // `search_files_impl` is a synchronous `WalkBuilder` traversal and can
    // block for hundreds of ms on large repos — move off the Tokio executor.
    let guard = state.indexer_throttle.begin_search();
    let result = tokio::task::spawn_blocking(move || {
        let _g = guard; // hold across the walk; dropped when closure returns
        crate::fs::search_files_impl(q.repo_path, q.query, q.limit)
    })
    .await
    .unwrap_or_else(|e| Err(format!("search task panicked: {e}")));
    json_result(result)
}

pub(super) async fn search_content_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::state::AppState>>,
    Query(q): Query<FsSearchContentQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    let use_regex = q.use_regex.unwrap_or(false);
    let whole_word = q.whole_word.unwrap_or(false);
    let case_sensitive = q.case_sensitive.unwrap_or(false);

    // Use BM25 index when available and applicable. Guard is held for the
    // duration of the in-memory query (fast, stays on the executor).
    let can_use_index = !use_regex && !whole_word && !q.query.is_empty();
    if can_use_index {
        let _guard = state.indexer_throttle.begin_search();
        let index_arc = crate::content_index::ensure_index(&state, &q.repo_path);
        let index = index_arc.read();
        if index.is_ready() {
            return json_result(crate::fs::search_via_index(
                &index,
                &q.query,
                case_sensitive,
                q.limit,
            ));
        }
    }

    // Fallback to full grep — blocks on WalkBuilder, offload to blocking pool.
    let guard = state.indexer_throttle.begin_search();
    let result = tokio::task::spawn_blocking(move || {
        let _g = guard;
        crate::fs::search_content_impl(
            q.repo_path,
            q.query,
            case_sensitive,
            use_regex,
            whole_word,
            q.limit,
        )
    })
    .await
    .unwrap_or_else(|e| Err(format!("search task panicked: {e}")));
    json_result(result)
}

/// BM25 semantic search across **all** indexed repos. Each match includes `repoPath`.
/// Only searches repos whose index is already ready — repos still building are skipped.
pub(super) async fn search_content_all_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::state::AppState>>,
    Query(q): Query<FsSearchContentAllQuery>,
) -> Response {
    let case_sensitive = q.case_sensitive.unwrap_or(false);
    let global_limit = q.limit.unwrap_or(100);

    let _guard = state.indexer_throttle.begin_search();
    let result = crate::fs::search_content_all_impl(
        &state.content_indices,
        &q.query,
        case_sensitive,
        global_limit,
    );

    json_result(Ok::<crate::fs::ContentSearchResult, String>(result))
}

pub(super) async fn fs_read_file_http(Query(q): Query<FsFileQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    json_result(crate::fs::fs_read_file(q.repo_path, q.file))
}

/// Repo file read for the code editor, at the larger `MAX_EDITOR_LARGE_FILE_SIZE` cap.
pub(super) async fn read_editor_file_http(Query(q): Query<FsFileQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) {
        return e.into_response();
    }
    json_result(crate::read_file_impl_with_limit(
        q.repo_path,
        q.file,
        crate::MAX_EDITOR_LARGE_FILE_SIZE,
    ))
}

/// Check if a path falls within any of the given repository roots.
/// Uses Path::starts_with for component-level matching (not string prefix).
fn is_within_repo_roots(path: &std::path::Path, roots: &[String]) -> bool {
    roots.iter().any(|root| path.starts_with(root))
}

/// Extract registered repository root paths from the opaque repos JSON.
fn registered_repo_roots() -> Vec<String> {
    crate::config::load_repositories()
        .get("repositories")
        .and_then(|r| r.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default()
}

pub(super) async fn read_external_file_http(Query(q): Query<FsExternalFileQuery>) -> Response {
    // Restrict to files within registered repos — prevents arbitrary file reads via HTTP
    let roots = registered_repo_roots();
    if !is_within_repo_roots(std::path::Path::new(&q.path), &roots) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Access denied: path must be within a registered repository"
            })),
        )
            .into_response();
    }
    json_result(crate::read_external_file(q.path))
}

/// External (absolute-path) file read for the code editor, at the larger
/// `MAX_EDITOR_LARGE_FILE_SIZE` cap. Same repo-root restriction as `read_external_file_http`.
pub(super) async fn read_editor_file_external_http(
    Query(q): Query<FsExternalFileQuery>,
) -> Response {
    let roots = registered_repo_roots();
    if !is_within_repo_roots(std::path::Path::new(&q.path), &roots) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Access denied: path must be within a registered repository"
            })),
        )
            .into_response();
    }
    json_result(crate::read_external_file_with_limit(
        &q.path,
        crate::MAX_EDITOR_LARGE_FILE_SIZE,
    ))
}

pub(super) async fn write_file_http(Json(body): Json<FsWriteFileRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    match crate::fs::write_file(body.repo_path, body.file, body.content) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn create_directory_http(Json(body): Json<FsDirCreateRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    match crate::fs::create_directory(body.repo_path, body.dir) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn delete_path_http(Json(body): Json<FsPathRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    match crate::fs::delete_path(body.repo_path, body.path) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn rename_path_http(Json(body): Json<FsRenameRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    match crate::fs::rename_path(body.repo_path, body.from, body.to) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn copy_path_http(Json(body): Json<FsCopyRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    match crate::fs::copy_path(body.repo_path, body.from, body.to) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn add_to_gitignore_http(Json(body): Json<FsGitignoreRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    match crate::fs::add_to_gitignore(body.repo_path, body.pattern) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

/// Resolve a terminal path candidate (handles `~`, relative, `:line:col`) to an
/// absolute canonical path. Metadata-only result (path + is_directory), so it is
/// intentionally NOT gated to repo roots — the terminal's cwd may sit anywhere.
pub(super) async fn resolve_terminal_path_http(
    Query(q): Query<FsResolveTerminalPathQuery>,
) -> Response {
    json_result(Ok::<Option<crate::fs::ResolvedFilePath>, String>(
        crate::fs::resolve_terminal_path(q.cwd, q.candidate),
    ))
}

/// Stat an absolute path. Metadata-only (exists/is_dir/size/mtime), no content,
/// so it is not repo-root gated — `stat_path_impl` already refuses TCC-protected dirs.
pub(super) async fn stat_path_http(Query(q): Query<FsExternalFileQuery>) -> Response {
    json_result(Ok::<crate::fs::PathStat, String>(
        crate::fs::stat_path_impl(q.path),
    ))
}

/// Warm the BM25 content index for a repo (fire-and-forget; build runs in the
/// background). Mirrors `search_content_http`'s use of `ensure_index`.
pub(super) async fn warm_content_index_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::state::AppState>>,
    Json(body): Json<FsWarmIndexRequest>,
) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) {
        return e.into_response();
    }
    crate::content_index::ensure_index(&state, &body.repo_path);
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

/// Write a file at an absolute path (editor saves for files outside any repo).
/// Gated to registered repo roots for the HTTP boundary, matching
/// `read_external_file_http` — the editor can only *open* repo-root files over
/// HTTP, so saves target the same set. `write_external_file` also confines to $HOME.
pub(super) async fn write_external_file_http(Json(body): Json<FsExternalWriteRequest>) -> Response {
    let roots = registered_repo_roots();
    if !is_within_repo_roots(std::path::Path::new(&body.path), &roots) {
        return access_denied();
    }
    match crate::write_external_file(body.path, body.content) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

/// Copy a single file by absolute paths (FileBrowser cross-repo copy). Both
/// endpoints gated to registered repo roots for the HTTP boundary.
pub(super) async fn copy_path_abs_http(Json(body): Json<FsAbsTransferRequest>) -> Response {
    if let Some(resp) = deny_unless_both_in_roots(&body.from, &body.to) {
        return resp;
    }
    match crate::fs::copy_path_abs(body.from, body.to) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

/// Move a single file by absolute paths (FileBrowser cross-repo cut). Both
/// endpoints gated to registered repo roots for the HTTP boundary.
pub(super) async fn move_path_abs_http(Json(body): Json<FsAbsTransferRequest>) -> Response {
    if let Some(resp) = deny_unless_both_in_roots(&body.from, &body.to) {
        return resp;
    }
    match crate::fs::move_path_abs(body.from, body.to) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

/// Bulk OS drag-drop transfer into a destination directory. Only the destination
/// is gated to repo roots: sources are frequently external (a file dragged from
/// the desktop), which is the whole point of drag-import.
pub(super) async fn fs_transfer_paths_http(Json(body): Json<FsTransferPathsRequest>) -> Response {
    let roots = registered_repo_roots();
    if !is_within_repo_roots(std::path::Path::new(&body.dest_dir), &roots) {
        return access_denied();
    }
    json_result(crate::fs::fs_transfer_paths(
        body.dest_dir,
        body.paths,
        body.mode,
        body.allow_recursive,
    ))
}

/// 403 response for an absolute path that escapes every registered repo root.
fn access_denied() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "error": "Access denied: path must be within a registered repository"
        })),
    )
        .into_response()
}

/// Deny unless BOTH absolute paths fall within a registered repo root.
fn deny_unless_both_in_roots(from: &str, to: &str) -> Option<Response> {
    let roots = registered_repo_roots();
    let ok = is_within_repo_roots(std::path::Path::new(from), &roots)
        && is_within_repo_roots(std::path::Path::new(to), &roots);
    (!ok).then(access_denied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn within_repo_roots_match() {
        let roots = vec![
            "/Users/dev/project-a".to_string(),
            "/Users/dev/project-b".to_string(),
        ];
        assert!(is_within_repo_roots(
            Path::new("/Users/dev/project-a/src/main.rs"),
            &roots
        ));
        assert!(is_within_repo_roots(
            Path::new("/Users/dev/project-b/README.md"),
            &roots
        ));
    }

    #[test]
    fn within_repo_roots_no_match() {
        let roots = vec!["/Users/dev/project-a".to_string()];
        assert!(!is_within_repo_roots(
            Path::new("/Users/dev/.ssh/id_rsa"),
            &roots
        ));
        assert!(!is_within_repo_roots(Path::new("/etc/passwd"), &roots));
    }

    #[test]
    fn within_repo_roots_no_prefix_trick() {
        // "/Users/dev/project-abc" should NOT match root "/Users/dev/project-a"
        // Path::starts_with checks components, not string prefix
        let roots = vec!["/Users/dev/project-a".to_string()];
        assert!(!is_within_repo_roots(
            Path::new("/Users/dev/project-abc/file.txt"),
            &roots
        ));
    }

    #[test]
    fn within_repo_roots_empty() {
        assert!(!is_within_repo_roots(Path::new("/any/path"), &[]));
    }
}
