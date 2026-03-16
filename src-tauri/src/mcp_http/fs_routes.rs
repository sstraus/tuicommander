use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::types::*;
use super::validate_repo_path;

pub(super) async fn list_directory_http(Query(q): Query<FsDirQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) { return e.into_response(); }
    match crate::fs::list_directory_impl(q.repo_path, q.subdir.unwrap_or_default()) {
        Ok(entries) => (StatusCode::OK, Json(serde_json::json!(entries))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn search_files_http(Query(q): Query<FsSearchQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) { return e.into_response(); }
    match crate::fs::search_files_impl(q.repo_path, q.query, q.limit) {
        Ok(entries) => (StatusCode::OK, Json(serde_json::json!(entries))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn search_content_http(Query(q): Query<FsSearchContentQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) { return e.into_response(); }
    match crate::fs::search_content_impl(
        q.repo_path,
        q.query,
        q.case_sensitive.unwrap_or(false),
        q.use_regex.unwrap_or(false),
        q.whole_word.unwrap_or(false),
        q.limit,
    ) {
        Ok(result) => (StatusCode::OK, Json(serde_json::json!(result))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn fs_read_file_http(Query(q): Query<FsFileQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) { return e.into_response(); }
    match crate::fs::fs_read_file(q.repo_path, q.file) {
        Ok(content) => (StatusCode::OK, Json(serde_json::json!(content))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
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
    match crate::read_external_file(q.path) {
        Ok(content) => (StatusCode::OK, Json(serde_json::json!(content))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn write_file_http(Json(body): Json<FsWriteFileRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    match crate::fs::write_file(body.repo_path, body.file, body.content) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn create_directory_http(Json(body): Json<FsDirCreateRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    match crate::fs::create_directory(body.repo_path, body.dir) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn delete_path_http(Json(body): Json<FsPathRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    match crate::fs::delete_path(body.repo_path, body.path) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn rename_path_http(Json(body): Json<FsRenameRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    match crate::fs::rename_path(body.repo_path, body.from, body.to) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn copy_path_http(Json(body): Json<FsCopyRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    match crate::fs::copy_path(body.repo_path, body.from, body.to) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn add_to_gitignore_http(Json(body): Json<FsGitignoreRequest>) -> Response {
    if let Err(e) = validate_repo_path(&body.repo_path) { return e.into_response(); }
    match crate::fs::add_to_gitignore(body.repo_path, body.pattern) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
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
