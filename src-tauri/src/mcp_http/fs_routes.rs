use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::types::*;
use super::{err_500, json_result, validate_repo_path};

// `list_directory_http` intentionally omits `State` + `indexer_throttle`: the
// underlying `fs::list_directory_impl` is a single `read_dir` + sort, which
// completes in microseconds on non-pathological directories and does not walk
// recursively. The throttle exists to keep *long* blocking walks (search,
// content grep, BM25 indexing) off the Tokio executor.
pub(super) async fn list_directory_http(Query(q): Query<FsDirQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) { return e.into_response(); }
    json_result(crate::fs::list_directory_impl(q.repo_path, q.subdir.unwrap_or_default()))
}

pub(super) async fn search_files_http(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::state::AppState>>,
    Query(q): Query<FsSearchQuery>,
) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) { return e.into_response(); }
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
    if let Err(e) = validate_repo_path(&q.repo_path) { return e.into_response(); }
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
                &index, &q.query, case_sensitive, q.limit,
            ));
        }
    }

    // Fallback to full grep — blocks on WalkBuilder, offload to blocking pool.
    let guard = state.indexer_throttle.begin_search();
    let result = tokio::task::spawn_blocking(move || {
        let _g = guard;
        crate::fs::search_content_impl(
            q.repo_path, q.query, case_sensitive, use_regex, whole_word, q.limit,
        )
    })
    .await
    .unwrap_or_else(|e| Err(format!("search task panicked: {e}")));
    json_result(result)
}

pub(super) async fn fs_read_file_http(Query(q): Query<FsFileQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) { return e.into_response(); }
    json_result(crate::fs::fs_read_file(q.repo_path, q.file))
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
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({
            "error": "Access denied: path must be within a registered repository"
        }))).into_response();
    }
    json_result(crate::read_external_file(q.path))
}

pub(super) async fn write_file_http(Json(body): Json<FsWriteFileRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    match crate::fs::write_file(body.repo_path, body.file, body.content) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn create_directory_http(Json(body): Json<FsDirCreateRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    match crate::fs::create_directory(body.repo_path, body.dir) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn delete_path_http(Json(body): Json<FsPathRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    match crate::fs::delete_path(body.repo_path, body.path) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn rename_path_http(Json(body): Json<FsRenameRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    match crate::fs::rename_path(body.repo_path, body.from, body.to) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn copy_path_http(Json(body): Json<FsCopyRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    match crate::fs::copy_path(body.repo_path, body.from, body.to) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
}

pub(super) async fn add_to_gitignore_http(Json(body): Json<FsGitignoreRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    match crate::fs::add_to_gitignore(body.repo_path, body.pattern) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => err_500(&e),
    }
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
        assert!(is_within_repo_roots(Path::new("/Users/dev/project-a/src/main.rs"), &roots));
        assert!(is_within_repo_roots(Path::new("/Users/dev/project-b/README.md"), &roots));
    }

    #[test]
    fn within_repo_roots_no_match() {
        let roots = vec!["/Users/dev/project-a".to_string()];
        assert!(!is_within_repo_roots(Path::new("/Users/dev/.ssh/id_rsa"), &roots));
        assert!(!is_within_repo_roots(Path::new("/etc/passwd"), &roots));
    }

    #[test]
    fn within_repo_roots_no_prefix_trick() {
        // "/Users/dev/project-abc" should NOT match root "/Users/dev/project-a"
        // Path::starts_with checks components, not string prefix
        let roots = vec!["/Users/dev/project-a".to_string()];
        assert!(!is_within_repo_roots(Path::new("/Users/dev/project-abc/file.txt"), &roots));
    }

    #[test]
    fn within_repo_roots_empty() {
        assert!(!is_within_repo_roots(Path::new("/any/path"), &[]));
    }
}
