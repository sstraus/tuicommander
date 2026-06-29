//! HTTP parity for plugin RPC commands — story 071.
//!
//! Routes are all under `/api/plugins/{plugin_id}/...` so the plugin ID
//! is always a path segment (matches the existing `/api/plugins/{plugin_id}/data/{*path}`
//! convention in mod.rs).

use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use axum::response::Response;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

use super::json_result;
use crate::AppState;

// ---------------------------------------------------------------------------
// Filesystem
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(super) struct FsReadQuery {
    pub path: String,
}

pub(super) async fn plugin_fs_read(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Query(q): Query<FsReadQuery>,
) -> Response {
    json_result(crate::plugin_fs::plugin_read_file_impl(&state, q.path, plugin_id).await)
}

#[derive(Deserialize)]
pub(super) struct FsTailQuery {
    pub path: String,
    #[serde(rename = "maxBytes")]
    pub max_bytes: u64,
}

pub(super) async fn plugin_fs_tail(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Query(q): Query<FsTailQuery>,
) -> Response {
    json_result(
        crate::plugin_fs::plugin_read_file_tail_impl(&state, q.path, q.max_bytes, plugin_id).await,
    )
}

#[derive(Deserialize)]
pub(super) struct FsListQuery {
    pub path: String,
    pub pattern: Option<String>,
    #[serde(rename = "sortBy")]
    pub sort_by: Option<String>,
}

pub(super) async fn plugin_fs_list(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Query(q): Query<FsListQuery>,
) -> Response {
    json_result(
        crate::plugin_fs::plugin_list_directory_impl(
            &state, q.path, q.pattern, q.sort_by, plugin_id,
        )
        .await,
    )
}

#[derive(Deserialize)]
pub(super) struct FsWriteBody {
    pub path: String,
    pub content: String,
}

pub(super) async fn plugin_fs_write(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Json(body): Json<FsWriteBody>,
) -> Response {
    json_result(
        crate::plugin_fs::plugin_write_file_impl(&state, body.path, body.content, plugin_id).await,
    )
}

#[derive(Deserialize)]
pub(super) struct FsRenameBody {
    pub from: String,
    pub to: String,
}

pub(super) async fn plugin_fs_rename(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Json(body): Json<FsRenameBody>,
) -> Response {
    json_result(
        crate::plugin_fs::plugin_rename_path_impl(&state, body.from, body.to, plugin_id).await,
    )
}

// ---------------------------------------------------------------------------
// CLI execution
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(super) struct ExecBody {
    pub binary: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}

pub(super) async fn plugin_exec(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Json(body): Json<ExecBody>,
) -> Response {
    json_result(
        crate::plugin_exec::plugin_exec_cli_impl(
            &state,
            body.binary,
            body.args,
            body.cwd,
            plugin_id,
        )
        .await,
    )
}

// ---------------------------------------------------------------------------
// HTTP fetch
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(super) struct HttpFetchBody {
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
    #[serde(rename = "allowedUrls")]
    pub allowed_urls: Vec<String>,
}

pub(super) async fn plugin_http_fetch(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Json(body): Json<HttpFetchBody>,
) -> Response {
    json_result(
        crate::plugin_http::plugin_http_fetch_impl(
            &state,
            body.url,
            body.method,
            body.headers,
            body.body,
            body.allowed_urls,
            plugin_id,
        )
        .await,
    )
}

// ---------------------------------------------------------------------------
// PTY read
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(super) struct PtyOutputQuery {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "maxLines")]
    pub max_lines: Option<usize>,
}

pub(super) async fn plugin_pty_output(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Query(q): Query<PtyOutputQuery>,
) -> Response {
    json_result(crate::plugin_pty::plugin_read_session_output_impl(
        &state,
        q.session_id,
        q.max_lines,
        plugin_id,
    ))
}

// ---------------------------------------------------------------------------
// Plugin registration
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(super) struct RegisterBody {
    pub capabilities: Vec<String>,
}

pub(super) async fn plugin_register(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Json(body): Json<RegisterBody>,
) -> Response {
    json_result(crate::plugins::register_loaded_plugin_impl(
        &state,
        plugin_id,
        body.capabilities,
    ))
}

pub(super) async fn plugin_unregister(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
) -> Response {
    crate::plugins::unregister_loaded_plugin_impl(&state, &plugin_id);
    json_result(Ok::<_, String>(()))
}

// ---------------------------------------------------------------------------
// README path
// ---------------------------------------------------------------------------

pub(super) async fn plugin_readme(AxumPath(plugin_id): AxumPath<String>) -> Response {
    // Returns Option<String>: None → JSON null. json_result expects Result so wrap it.
    let path = crate::plugins::get_plugin_readme_path(plugin_id);
    json_result(Ok::<Option<String>, String>(path))
}
