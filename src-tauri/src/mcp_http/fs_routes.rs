use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::types::*;
use super::validate_repo_path;

pub(super) async fn list_directory_http(Query(q): Query<FsDirQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) { return e.into_response(); }
    match crate::fs::list_directory(q.repo_path, q.subdir.unwrap_or_default()) {
        Ok(entries) => (StatusCode::OK, Json(serde_json::json!(entries))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

pub(super) async fn search_files_http(Query(q): Query<FsSearchQuery>) -> Response {
    if let Err(e) = validate_repo_path(&q.repo_path) { return e.into_response(); }
    match crate::fs::search_files(q.repo_path, q.query, q.limit) {
        Ok(entries) => (StatusCode::OK, Json(serde_json::json!(entries))).into_response(),
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

pub(super) async fn read_external_file_http(Query(q): Query<FsExternalFileQuery>) -> Response {
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
