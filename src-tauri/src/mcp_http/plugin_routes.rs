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

#[derive(Deserialize)]
pub(super) struct FsReadBase64Query {
    pub path: String,
    #[serde(rename = "maxBytes")]
    pub max_bytes: Option<u64>,
}

pub(super) async fn plugin_fs_read(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Query(q): Query<FsReadQuery>,
) -> Response {
    json_result(crate::plugin_fs::plugin_read_file_impl(&state, q.path, plugin_id).await)
}

/// Batch read. The paths travel in the body because a query string cannot carry
/// a directory's worth of them.
#[derive(Deserialize)]
pub(super) struct FsReadBatchBody {
    pub paths: Vec<String>,
}

pub(super) async fn plugin_fs_read_batch(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Json(body): Json<FsReadBatchBody>,
) -> Response {
    json_result(crate::plugin_fs::plugin_read_files_impl(&state, body.paths, plugin_id).await)
}

pub(super) async fn plugin_fs_read_base64(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Query(q): Query<FsReadBase64Query>,
) -> Response {
    json_result(
        crate::plugin_fs::plugin_read_file_base64_impl(&state, q.path, q.max_bytes, plugin_id)
            .await,
    )
}

#[derive(Deserialize)]
pub(super) struct FsRangeQuery {
    pub path: String,
    pub offset: u64,
    pub length: u64,
}

pub(super) async fn plugin_fs_size(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Query(q): Query<FsReadQuery>,
) -> Response {
    json_result(crate::plugin_fs::plugin_file_size_impl(&state, q.path, plugin_id).await)
}
pub(super) async fn plugin_fs_range(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Query(q): Query<FsRangeQuery>,
) -> Response {
    json_result(
        crate::plugin_fs::plugin_read_file_range_impl(
            &state, q.path, q.offset, q.length, plugin_id,
        )
        .await,
    )
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

#[derive(Deserialize)]
pub(super) struct FsWriteBase64Body {
    pub path: String,
    pub content: String,
    #[serde(default, rename = "maxBytes")]
    pub max_bytes: Option<u64>,
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

pub(super) async fn plugin_fs_write_base64(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Json(body): Json<FsWriteBase64Body>,
) -> Response {
    json_result(
        crate::plugin_fs::plugin_write_file_base64_impl(
            &state,
            body.path,
            body.content,
            body.max_bytes,
            plugin_id,
        )
        .await,
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

#[derive(Deserialize)]
pub(super) struct ScanBody {
    #[serde(rename = "repoPaths")]
    pub repo_paths: Vec<String>,
    #[serde(default, rename = "forceRefresh")]
    pub force_refresh: bool,
}

pub(super) async fn plugin_scan_build_artifacts(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Json(body): Json<ScanBody>,
) -> Response {
    json_result(
        crate::plugin_fs::scan_build_artifacts_impl(
            &state,
            body.repo_paths,
            plugin_id,
            body.force_refresh,
        )
        .await,
    )
}

#[derive(Deserialize)]
pub(super) struct DeleteArtifactBody {
    pub path: String,
    #[serde(rename = "repoPaths")]
    pub repo_paths: Vec<String>,
}

pub(super) async fn plugin_delete_build_artifact(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Json(body): Json<DeleteArtifactBody>,
) -> Response {
    json_result(
        crate::plugin_fs::delete_build_artifact_impl(&state, body.path, body.repo_paths, plugin_id)
            .await,
    )
}

/// Same body as delete — the difference is how much of the dir is removed.
pub(super) async fn plugin_trim_build_artifact(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Json(body): Json<DeleteArtifactBody>,
) -> Response {
    json_result(
        crate::plugin_fs::trim_build_artifact_impl(&state, body.path, body.repo_paths, plugin_id)
            .await,
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
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: Option<u64>,
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
            body.timeout_ms,
        )
        .await,
    )
}

// ---------------------------------------------------------------------------
// HTTP fetch
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(super) struct HttpFetchBody {
    #[serde(rename = "requestId")]
    pub request_id: Option<String>,
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
    pub account: Option<String>,
}

pub(super) async fn plugin_http_fetch(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Json(body): Json<HttpFetchBody>,
) -> Response {
    json_result(
        crate::plugin_http::plugin_http_fetch_cancellable_impl(
            &state,
            body.url,
            body.method,
            body.headers,
            body.body,
            body.account,
            plugin_id,
            body.request_id,
        )
        .await,
    )
}

#[derive(Deserialize)]
pub(super) struct HttpCancelBody {
    #[serde(rename = "requestId")]
    pub request_id: String,
}

pub(super) async fn plugin_http_cancel(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Json(body): Json<HttpCancelBody>,
) -> Response {
    json_result(crate::plugin_http::plugin_http_cancel_impl(
        &state,
        plugin_id,
        body.request_id,
    ))
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
    json_result(
        crate::plugin_pty::plugin_read_session_output_impl(
            &state,
            q.session_id,
            q.max_lines,
            plugin_id,
        )
        .await,
    )
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

pub(super) async fn plugin_settings(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Json(action): Json<crate::settings::plugins::PluginAction>,
) -> Response {
    use axum::{http::StatusCode, response::IntoResponse};
    match crate::plugin_settings::plugin_settings_action_impl(&state, plugin_id, action).await {
        Ok(reply) => Json(reply).into_response(),
        Err(error) => {
            let status = if error.code == "not_found" {
                StatusCode::NOT_FOUND
            } else if error.code == "conflict" {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, Json(error)).into_response()
        }
    }
}

#[derive(Deserialize)]
pub(super) struct OutputWatchersBody {
    /// Identifies the frontend that owns this set — a desktop window and a
    /// browser tab keep independent watchers.
    pub client_id: String,
    /// Monotonic per-client counter that orders the mutations, so a sync that
    /// lost the race cannot install its older set.
    pub seq: u64,
    pub watchers: Vec<crate::output_watchers::WatcherSpec>,
}

pub(super) async fn plugin_set_output_watchers(
    State(state): State<Arc<AppState>>,
    Json(body): Json<OutputWatchersBody>,
) -> Response {
    let outcome = crate::plugins::set_plugin_output_watchers_impl(
        &state,
        &body.client_id,
        body.seq,
        &body.watchers,
    );
    json_result(Ok::<crate::output_watchers::SyncOutcome, String>(outcome))
}

// ---------------------------------------------------------------------------
// README path
// ---------------------------------------------------------------------------

pub(super) async fn plugin_readme(AxumPath(plugin_id): AxumPath<String>) -> Response {
    // Returns Option<String>: None → JSON null. json_result expects Result so wrap it.
    let path = crate::plugins::get_plugin_readme_path(plugin_id);
    json_result(Ok::<Option<String>, String>(path))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct GithubActionBody {
    account: String,
    reference: crate::work_items::model::WorkItemRef,
    request: crate::plugin_github::Request,
}

pub(super) async fn plugin_github_action(
    State(state): State<Arc<AppState>>,
    AxumPath(plugin_id): AxumPath<String>,
    Json(body): Json<GithubActionBody>,
) -> Response {
    json_result(
        crate::plugin_github::execute(
            &state,
            plugin_id,
            body.account,
            body.reference,
            body.request,
        )
        .await,
    )
}
