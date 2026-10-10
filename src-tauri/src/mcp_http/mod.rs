pub(crate) mod acp_mcp;
mod acp_routes;
mod agent_routes;
pub(crate) mod auth;
mod claude_routes;
mod config_routes;
#[cfg(feature = "dictation")]
mod dictation_routes;
mod ego_routes;
mod fs_routes;
mod git_routes;
mod github_routes;
mod guards;
mod log_routes;
pub(crate) mod mcp_transport;
mod mcp_transport_ancillary;
mod mcp_transport_catalogue;
mod mcp_transport_peer;
mod mcp_transport_session_agent;

mod plugin_docs;
mod plugin_routes;
mod remote_mcp_sessions;
pub(crate) mod remote_peer;
mod remote_session_proxy;
mod request_boundary;
#[cfg(test)]
mod secret_critic1435_tests;
pub(crate) mod session;
mod session_placement;
pub(crate) mod sse_routes;
pub(crate) mod static_files;
#[cfg(feature = "desktop")]
mod system_routes;
mod telegram_routes;
pub(crate) mod types;
mod watcher_routes;
mod worktree_routes;
mod ws_compression;

use crate::AppState;
#[cfg(test)]
use axum::http::header::CONTENT_TYPE;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{
    Json, Router,
    extract::{ConnectInfo, Extension, Path as AxumPath, Query, State},
};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{DefaultPredicate, Predicate, SizeAbove};
#[cfg(unix)]
use tuic_ipc::named_socket_path;

/// Maximum terminal dimension (rows or cols). Prevents resource abuse from
/// absurdly large allocations while still allowing generous sizes.
const MAX_TERMINAL_DIMENSION: u16 = 500;

/// Validate terminal dimensions (rows/cols) are within sane bounds.
fn validate_terminal_size(rows: u16, cols: u16) -> Result<(), String> {
    if rows == 0 || rows > MAX_TERMINAL_DIMENSION {
        return Err(format!(
            "rows must be between 1 and {MAX_TERMINAL_DIMENSION}, got {rows}"
        ));
    }
    if cols == 0 || cols > MAX_TERMINAL_DIMENSION {
        return Err(format!(
            "cols must be between 1 and {MAX_TERMINAL_DIMENSION}, got {cols}"
        ));
    }
    Ok(())
}

/// Core path validation logic shared by HTTP and MCP handlers.
/// Rejects empty paths, null bytes, relative traversals, and non-absolute paths.
///
/// This is a syntactic filter, not a canonical one: it does not resolve
/// symlinks or `.` segments, and the `..` check is a substring match (so
/// paths containing `..` in legitimate filenames are also rejected). Callers
/// that must enforce a containment boundary (e.g. "within a registered
/// repo") are responsible for that check themselves — see
/// `fs_routes::is_within_repo_roots`.
fn validate_path_string(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("path is required".to_string());
    }
    if path.contains("..") || path.contains('\0') {
        return Err("Path traversal is not allowed".to_string());
    }
    if !crate::fs::is_absolute_on_any_platform(path) {
        return Err("Path must be absolute".to_string());
    }
    Ok(())
}

/// Validate a repo path for HTTP handlers, returning a 400 error response on failure.
fn validate_repo_path(path: &str) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    validate_path_string(path).map_err(|msg| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        )
    })
}

/// Wrap an `Ok(T)` / `Err(String)` into a JSON HTTP response.
/// Ok → 200 with JSON body, Err → 500 with `{"error": msg}`.
fn json_result<T: serde::Serialize>(result: Result<T, String>) -> Response {
    match result {
        Ok(val) => (StatusCode::OK, Json(serde_json::json!(val))).into_response(),
        Err(e) => err_500(&e),
    }
}

/// Shorthand for a 500 Internal Server Error with `{"error": msg}` body.
fn err_500(msg: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": msg})),
    )
        .into_response()
}

/// Wrap an `Ok(T)` / `Err(String)` result from a call to an upstream API (e.g.
/// GitHub) into a JSON HTTP response. Ok → 200 with JSON body, Err → 502 Bad
/// Gateway with `{"error": msg}` — distinct from [`json_result`]'s 500 because
/// the failure originates upstream, not in our own server.
pub(crate) fn upstream_json_result<T: serde::Serialize>(result: Result<T, String>) -> Response {
    match result {
        Ok(val) => (StatusCode::OK, Json(serde_json::json!(val))).into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

/// Default IPC endpoint path for local MCP bridge connections (Unix domain socket).
#[cfg(unix)]
pub(crate) fn socket_path() -> std::path::PathBuf {
    tuic_ipc::socket_path(
        crate::app_instance::current_app_instance(),
        &crate::config::config_dir(),
        &std::env::temp_dir(),
    )
}

/// Resolve which socket path this instance should bind to.
/// If `mcp.sock` is already held by another live process, returns `mcp-{pid}.sock`.
#[cfg(unix)]
fn resolve_socket_path() -> std::path::PathBuf {
    let primary = socket_path();
    if primary.exists() {
        // Try connecting — if it succeeds, another instance is alive on this socket.
        if std::os::unix::net::UnixStream::connect(&primary).is_ok() {
            let alt = crate::app_instance::current_app_instance()
                .named_id()
                .map(|id| {
                    let base = named_socket_path(id, &std::env::temp_dir());
                    base.with_file_name(format!(
                        "{}-{}.sock",
                        base.file_stem().unwrap_or_default().to_string_lossy(),
                        std::process::id()
                    ))
                })
                .unwrap_or_else(|| {
                    crate::config::config_dir().join(format!("mcp-{}.sock", std::process::id()))
                });
            tracing::info!(
                source = "mcp_http",
                primary = %primary.display(),
                alt = %alt.display(),
                "Primary socket held by another instance, using alternative"
            );
            return alt;
        }
        // Socket file exists but nobody is listening — stale, will be cleaned up by bind.
    }
    // Also clean up any stale mcp-*.sock files left by dead processes.
    cleanup_stale_sockets();
    primary
}

/// Remove `mcp-{pid}.sock` files whose owning PID is no longer alive.
#[cfg(unix)]
fn cleanup_stale_sockets() {
    let config_dir = crate::config::config_dir();
    let Ok(entries) = std::fs::read_dir(&config_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        // Match pattern: mcp-{digits}.sock
        if let Some(rest) = name_str.strip_prefix("mcp-")
            && let Some(pid_str) = rest.strip_suffix(".sock")
            && let Ok(pid) = pid_str.parse::<u32>()
        {
            // SAFETY: signal 0 does not deliver a signal — it is a POSIX existence
            // check. pid_t is i32; valid PIDs are always non-negative, so u32→i32 is safe.
            let alive = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
            if !alive {
                let path = entry.path();
                tracing::info!(source = "mcp_http", path = %path.display(), pid, "Removing stale socket");
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

/// Named pipe name for Windows IPC (without the \\.\pipe\ prefix for display).
#[cfg(windows)]
const PIPE_NAME: &str = tuic_ipc::PIPE_NAME;

/// axum::serve::Listener implementation for Windows named pipes.
/// Uses the tokio reconnect pattern: pre-creates the next pipe instance before
/// spawning the handler for the current connection, avoiding a listen gap.
#[cfg(windows)]
struct NamedPipeListener {
    server: tokio::net::windows::named_pipe::NamedPipeServer,
}

#[cfg(windows)]
impl NamedPipeListener {
    fn new() -> std::io::Result<Self> {
        use tokio::net::windows::named_pipe::ServerOptions;
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(PIPE_NAME)?;
        Ok(Self { server })
    }
}

#[cfg(windows)]
impl axum::serve::Listener for NamedPipeListener {
    type Io = tokio::net::windows::named_pipe::NamedPipeServer;
    type Addr = String;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        use tokio::net::windows::named_pipe::ServerOptions;
        loop {
            match self.server.connect().await {
                Ok(()) => {
                    let connected = std::mem::replace(
                        &mut self.server,
                        match ServerOptions::new()
                            .reject_remote_clients(true)
                            .create(PIPE_NAME)
                        {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::error!(
                                    source = "mcp_http",
                                    "Failed to create next pipe instance: {e}"
                                );
                                // Sleep briefly then retry — the pipe name might be transiently busy
                                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                continue;
                            }
                        },
                    );
                    return (connected, PIPE_NAME.to_string());
                }
                Err(e) => {
                    tracing::error!(source = "mcp_http", "Named pipe accept error: {e}");
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    fn local_addr(&self) -> tokio::io::Result<Self::Addr> {
        Ok(PIPE_NAME.to_string())
    }
}

/// Return the plugin development guide as JSON.
async fn plugin_dev_guide_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({"content": plugin_docs::PLUGIN_DOCS}))
}

/// GET /mcp/upstreams — load upstream MCP server config.
async fn load_mcp_upstreams_http() -> impl IntoResponse {
    Json(crate::mcp_upstream_config::load_mcp_upstreams())
}

/// PUT /mcp/upstreams — apply a base-to-desired upstream delta (validates, hot-reloads).
async fn save_mcp_upstreams_http(
    State(state): State<Arc<AppState>>,
    Json(request): Json<crate::mcp_upstream_config::UpstreamMcpSaveRequest>,
) -> Response {
    match crate::mcp_upstream_config::save_mcp_upstreams_inner(request.base, request.config, &state)
        .await
    {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e)
            if e.starts_with("Invalid upstream config")
                || e.starts_with("Duplicate upstream id") =>
        {
            (StatusCode::BAD_REQUEST, e).into_response()
        }
        Err(e) => err_500(&e),
    }
}

/// POST /mcp/upstreams/reconnect — reconnect a single upstream by name.
async fn reconnect_mcp_upstream_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let name = match body.get("name").and_then(|v| v.as_str()) {
        Some(n) => n.to_string(),
        None => return (StatusCode::BAD_REQUEST, "missing 'name'").into_response(),
    };
    let config: crate::mcp_upstream_config::UpstreamMcpConfig =
        crate::config::load_json_config(crate::mcp_upstream_config::UPSTREAMS_FILE);
    let self_port = state.config.read().services.server.port;
    let server = match config.servers.into_iter().find(|s| s.name == name) {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                format!("Upstream '{name}' not found"),
            )
                .into_response();
        }
    };
    let registry = &state.mcp.upstream_registry;
    registry.emit_status_change(&name, "connecting");
    if let Err(e) = registry.disconnect_upstream(&name) {
        tracing::warn!(source = "mcp_http", upstream = %name, error = %e, "Failed to disconnect upstream before reconnect");
    }
    match registry.connect_upstream(server, Some(self_port)).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => err_500(&e),
    }
}

/// POST /mcp/upstreams/credential — save an upstream credential to OS keyring.
async fn save_mcp_upstream_credential_http(Json(body): Json<serde_json::Value>) -> Response {
    let name = match body.get("name").and_then(|v| v.as_str()) {
        Some(n) => n.to_string(),
        None => return (StatusCode::BAD_REQUEST, "missing 'name'").into_response(),
    };
    let token = match body.get("token").and_then(|v| v.as_str()) {
        Some(t) => t.to_string(),
        None => return (StatusCode::BAD_REQUEST, "missing 'token'").into_response(),
    };
    match crate::mcp_upstream_credentials::save_mcp_upstream_credential(name, token) {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => err_500(&e),
    }
}

/// DELETE /mcp/upstreams/credential — delete an upstream credential from OS keyring.
async fn delete_mcp_upstream_credential_http(Json(body): Json<serde_json::Value>) -> Response {
    let name = match body.get("name").and_then(|v| v.as_str()) {
        Some(n) => n.to_string(),
        None => return (StatusCode::BAD_REQUEST, "missing 'name'").into_response(),
    };
    match crate::mcp_upstream_credentials::delete_mcp_upstream_credential(name) {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => err_500(&e),
    }
}

/// GET /mcp/upstream-status — returns status + metrics for all upstream MCP servers.
async fn upstream_status_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(state.mcp.upstream_registry.status_snapshot())
}

async fn post_progress_report(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<guards::Authenticated>>,
    Query(q): Query<types::PathQuery>,
    State(state): State<Arc<AppState>>,
    Json(input): Json<crate::progress::ProgressReportInput>,
) -> Response {
    if let Err(resp) = guards::require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    // A local HTTP caller is not an agent: nothing to attribute, no per-agent
    // override to apply. The same shape the desktop IPC command sends.
    json_result(mcp_transport::report_progress(
        &state,
        Some(&q.path),
        input,
        None,
        None,
        None,
        None,
    ))
}

#[derive(serde::Deserialize)]
struct SubmitAgentReplyRequest {
    input: String,
    timeout_ms: Option<u64>,
}

/// Browser counterpart of the managed session `submit` action. Both transports
/// use the same validation, atomic PTY write and bounded receipt path.
async fn submit_agent_reply(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<guards::Authenticated>>,
    AxumPath(session_id): AxumPath<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<SubmitAgentReplyRequest>,
) -> Response {
    if let Err(resp) = guards::require_local_or_auth(&addr, auth.is_some()) {
        return resp.into_response();
    }
    if body.input.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "submitted": false,
                "reason": "empty_input",
                "error": "Reply text must not be empty"
            })),
        )
            .into_response();
    }
    let result = mcp_transport::handle_session_submit(
        &state,
        &serde_json::json!({"session_id": session_id, "input": body.input, "timeout_ms": body.timeout_ms}),
        true,
    )
    .await;
    let status = if result["reason"] == "session_not_found" {
        StatusCode::NOT_FOUND
    } else if result.get("error").is_some() {
        StatusCode::BAD_REQUEST
    } else if result["submitted"] == false {
        StatusCode::CONFLICT
    } else {
        StatusCode::OK
    };
    (status, Json(result)).into_response()
}

/// Returns the rejection response, or `None` when the caller may proceed.
///
/// `Option` rather than `Result`: an axum `Response` is a large value, so a
/// `Result<(), Response>` makes every success path carry the rejection's size.
/// The guard has no success payload either, so the `Ok(())` was never read.
fn progress_auth(addr: &SocketAddr, authenticated: bool) -> Option<Response> {
    guards::require_local_or_auth(addr, authenticated)
        .err()
        .map(IntoResponse::into_response)
}

async fn post_progress_list(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<guards::Authenticated>>,
    Query(q): Query<types::PathQuery>,
    Json(input): Json<crate::progress::ProgressListInput>,
) -> Response {
    if let Some(r) = progress_auth(&addr, auth.is_some()) {
        return r;
    }
    json_result(crate::progress::progress_list(&q.path, input))
}

async fn get_progress_projects(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<guards::Authenticated>>,
) -> Response {
    if let Some(response) = progress_auth(&addr, auth.is_some()) {
        return response;
    }
    json_result(crate::progress::progress_projects())
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoryActionRequest {
    action: crate::stories::StoryAction,
    session_id: Option<String>,
}

async fn post_story_action(
    State(state): State<Arc<AppState>>,
    caller: Option<Extension<ConnectInfo<SocketAddr>>>,
    auth: Option<Extension<guards::Authenticated>>,
    user_auth: Option<Extension<guards::UserAuthenticated>>,
    Query(q): Query<types::PathQuery>,
    Json(input): Json<StoryActionRequest>,
) -> Response {
    let addr = caller.map_or(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        |Extension(ConnectInfo(addr))| addr,
    );
    if let Some(r) = progress_auth(&addr, auth.is_some()) {
        return r;
    }
    let result = tokio::task::spawn_blocking(move || {
        crate::stories::story_action_for_http_authenticated(
            &state,
            &q.path,
            input.action,
            input.session_id.as_deref(),
            user_auth.is_some(),
        )
    })
    .await
    .map_err(|error| format!("story task failed: {error}"))
    .and_then(|result| result);
    json_result(result)
}

async fn get_story_capabilities() -> Json<bool> {
    Json(true)
}

async fn post_workflow_definition_action(
    caller: Option<Extension<ConnectInfo<SocketAddr>>>,
    auth: Option<Extension<guards::Authenticated>>,
    Query(q): Query<types::PathQuery>,
    Json(action): Json<crate::workflows::WorkflowAction>,
) -> Response {
    let addr = caller.map_or(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        |Extension(ConnectInfo(addr))| addr,
    );
    if let Some(response) = progress_auth(&addr, auth.is_some()) {
        return response;
    }
    let result =
        tokio::task::spawn_blocking(move || crate::workflows::definition_action(&q.path, action))
            .await
            .map_err(|error| format!("workflow definition task failed: {error}"))
            .and_then(|result| result);
    json_result(result)
}
async fn post_workflow_run_action(
    State(state): State<Arc<AppState>>,
    caller: Option<Extension<ConnectInfo<SocketAddr>>>,
    auth: Option<Extension<guards::Authenticated>>,
    Query(q): Query<types::PathQuery>,
    Json(action): Json<crate::workflows::RunAction>,
) -> Response {
    let addr = caller.map_or(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        |Extension(ConnectInfo(addr))| addr,
    );
    if let Some(response) = progress_auth(&addr, auth.is_some()) {
        return response;
    }
    let result = tokio::task::spawn_blocking(move || {
        crate::workflows::run_action_with_events(&state, &q.path, action)
    })
    .await
    .map_err(|error| format!("workflow run task failed: {error}"))
    .and_then(|result| result);
    json_result(result)
}
async fn post_progress_delete(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<guards::Authenticated>>,
    Query(q): Query<types::PathQuery>,
    Json(input): Json<crate::progress::ProgressDeleteInput>,
) -> Response {
    if let Some(r) = progress_auth(&addr, auth.is_some()) {
        return r;
    }
    json_result(crate::progress::progress_delete(&q.path, input))
}
async fn post_progress_viewed(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<guards::Authenticated>>,
    Query(q): Query<types::ProgressViewedQuery>,
) -> Response {
    if let Some(r) = progress_auth(&addr, auth.is_some()) {
        return r;
    }
    json_result(crate::progress::progress_mark_viewed(
        &q.path,
        q.pty_id.as_deref(),
    ))
}

async fn post_progress_flow(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<guards::Authenticated>>,
    Query(q): Query<types::PathQuery>,
    Json(input): Json<crate::progress::ProgressFlowInput>,
) -> Response {
    if let Some(r) = progress_auth(&addr, auth.is_some()) {
        return r;
    }
    json_result(crate::progress::progress_flow_blocking(state, q.path, input).await)
}
async fn post_progress_flow_detail(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<guards::Authenticated>>,
    Json(input): Json<crate::progress::ProgressFlowDetailInput>,
) -> Response {
    if let Some(r) = progress_auth(&addr, auth.is_some()) {
        return r;
    }
    json_result(crate::progress::progress_flow_detail_blocking(state, input).await)
}

/// Serve plugin data files over HTTP.
/// Reuses the same sandboxed read logic as the Tauri `read_plugin_data` command.
async fn plugin_data_http(AxumPath((plugin_id, path)): AxumPath<(String, String)>) -> Response {
    match crate::plugins::read_plugin_data(plugin_id, path) {
        Ok(Some(content)) => {
            let content_type = if content.starts_with('{') || content.starts_with('[') {
                "application/json"
            } else {
                "text/plain"
            };
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, content_type)],
                content,
            )
                .into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

#[derive(serde::Deserialize)]
struct PluginDataWriteBody {
    content: String,
}

/// Persist plugin data over HTTP (browser/PWA parity for the credential-consent flow,
/// which calls `write_plugin_data`). Reuses the same sandboxed write logic as the Tauri
/// command. (`delete_plugin_data` has no frontend caller, so no route is added for it.)
async fn plugin_data_write_http(
    AxumPath((plugin_id, path)): AxumPath<(String, String)>,
    Json(body): Json<PluginDataWriteBody>,
) -> Response {
    match crate::plugins::write_plugin_data(plugin_id, path, body.content) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

/// Return the VAPID public key so the frontend can call PushManager.subscribe().
/// No auth required — the public key is not secret.
async fn push_vapid_key(State(state): State<Arc<AppState>>) -> Response {
    let public_key = {
        let config = state.config.read();
        if config.services.push.vapid_public_key.is_empty() {
            return (
                StatusCode::NOT_FOUND,
                "VAPID keys not generated — restart the app",
            )
                .into_response();
        }
        config.services.push.vapid_public_key.clone()
    };
    Json(serde_json::json!({ "publicKey": public_key })).into_response()
}

/// Register a push subscription from a browser.
/// No loopback restriction — push subscriptions come from remote mobile devices.
/// The endpoint URL is validated against a known push service allowlist.
async fn push_subscribe(
    State(state): State<Arc<AppState>>,
    Json(sub): Json<crate::push::PushSubscription>,
) -> Response {
    if let Err(e) = crate::push::validate_push_endpoint(&sub.endpoint) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    state.push_store.upsert(sub);
    // Auto-enable push on first subscription. Through the shared config lock: the old
    // mutate-then-snapshot-then-save left a window where another writer's save carried
    // this flag, or this save carried the other writer's half-applied change. Blocking
    // pool because the critical section writes to disk.
    let already_enabled = state.config.read().services.push.enabled;
    if !already_enabled {
        let state = state.clone();
        let saved = tokio::task::spawn_blocking(move || {
            crate::config::commit_config_change(&state, |current| {
                let mut next = current.clone();
                next.services.push.enabled = true;
                Ok(next)
            })
        })
        .await;
        match saved {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                tracing::error!(source = "push", "Failed to persist push_enabled=true: {e}")
            }
            Err(e) => tracing::error!(source = "push", "push_enabled save task failed: {e}"),
        }
    }
    StatusCode::CREATED.into_response()
}

/// Unregister a push subscription.
async fn push_unsubscribe(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> StatusCode {
    if let Some(endpoint) = body.get("endpoint").and_then(|v| v.as_str())
        && state.push_store.remove(endpoint)
    {
        return StatusCode::OK;
    }
    StatusCode::NOT_FOUND
}

/// Send a test push notification to all registered subscribers.
/// Useful for debugging from console: `curl -X POST http://localhost:PORT/api/push/test`
async fn push_test(
    State(state): State<Arc<AppState>>,
    body: Option<Json<serde_json::Value>>,
) -> Response {
    let subs = state.push_store.list();
    if subs.is_empty() {
        return (StatusCode::NOT_FOUND, "No push subscriptions registered").into_response();
    }
    let config = state.config.read().clone();
    if !config.services.push.enabled {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "sent": 0,
                "error": "Push notifications are disabled"
            })),
        )
            .into_response();
    }
    if config.services.push.vapid_private_key.is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "sent": 0,
                "error": "VAPID private key is unavailable"
            })),
        )
            .into_response();
    }
    let title = body
        .as_ref()
        .and_then(|b| b.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("TUICommander Test");
    let msg = body
        .as_ref()
        .and_then(|b| b.get("body"))
        .and_then(|v| v.as_str())
        .unwrap_or("Test push notification");
    let result =
        crate::push::send_push_batch(subs, &config, &state.http_client, title, msg, "/mobile")
            .await;
    for endpoint in &result.stale_endpoints {
        state.push_store.remove(endpoint);
    }
    Json(serde_json::json!({
        "sent": result.sent,
        "failed": result.failed,
        "stale_removed": result.stale_endpoints.len()
    }))
    .into_response()
}

/// Middleware that injects a synthetic `ConnectInfo<SocketAddr>` for IPC
/// connections (Unix socket / named pipe) which lack a TCP peer address.
/// Always uses 127.0.0.1:0 since IPC is inherently local.
async fn inject_localhost_connect_info(
    mut req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::extract::connect_info::ConnectInfo;
    req.extensions_mut()
        .insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))));
    next.run(req).await
}

/// Top-level path segments owned by the HTTP API.
///
/// The SPA catch-all serves index.html for anything no route matched, which is
/// right for a deep link like `/settings` and wrong for `/repo/typo`: the caller
/// gets 200 + HTML, `rpc()` reads that as success and then fails parsing it as a
/// command result, so every unregistered route hides in plain sight. A path
/// under one of these prefixes 404s with a JSON error instead.
///
/// Kept honest by `api_prefixes_cover_every_registered_route`, which reads the
/// route literals out of this file — a new family added here without a matching
/// entry fails that test instead of quietly serving HTML again.
#[cfg(any(feature = "desktop", test))]
const API_PREFIXES: &[&str] = &[
    "acp",
    "agent",
    "agents",
    "ai",
    "api",
    "attachments",
    "audio",
    "auth",
    "circleci",
    "claude",
    "codex",
    "config",
    "debug",
    "diagnostics",
    "design-mode",
    "dictation",
    "ego",
    "events",
    "exec",
    "fs",
    "generators",
    "github",
    "grok",
    "health",
    "logs",
    "mcp",
    "metrics",
    "plugins",
    "process",
    "progress",
    "prompt",
    "registry",
    "remote",
    "repo",
    "secrets",
    "sessions",
    "stats",
    "stories",
    "system",
    "terminal",
    "tunnels",
    "watchers",
    "workflows",
    "worktrees",
];

/// Catch-all for every path no route matched.
///
/// Under an API prefix that means a missing route, so answer 404 with a JSON
/// error; anything else is an SPA deep link and gets the frontend shell.
///
/// Registered with `Router::fallback`, not `.route("/{*path}", get(..))`, so it
/// answers on EVERY method. As a GET-only route it replied 405 to a PATCH/POST
/// on a path that does not exist at all — a lie, and one that made
/// `command_table_paths_all_hit_a_registered_route` unable to fail, since its
/// PATCH probe read that 405 as "route present". A fallback fires only when no
/// route matched the path; a real route with the wrong method still returns its
/// own 405, which is precisely the distinction the probe needs.
#[cfg(feature = "desktop")]
async fn spa_or_api_404(uri: axum::http::Uri) -> Response {
    // Compare against the path without its leading slash, as `{*path}` did.
    let path = AxumPath(uri.path().trim_start_matches('/').to_string());
    let head = path.0.split('/').next().unwrap_or("");
    if API_PREFIXES.contains(&head) {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("no such endpoint: /{}", path.0),
            })),
        )
            .into_response();
    }
    static_files::serve_static(path).await
}

/// Build the router (exposed for testing).
/// When `remote_auth` is true, applies Basic Auth middleware (requires ConnectInfo).
/// When `mcp_enabled` is false, excludes MCP Streamable HTTP route (/mcp).
/// Sub-router for SSH tunnel management routes.
fn tunnel_routes() -> Router<Arc<AppState>> {
    use crate::tunnels::commands;
    Router::new()
        .route(
            "/profiles",
            get(commands::list_tunnel_profiles).post(commands::save_tunnel_profile),
        )
        .route("/profiles/{id}", delete(commands::delete_tunnel_profile))
        .route("/start/{id}", post(commands::start_tunnel))
        .route("/stop/{id}", post(commands::stop_tunnel))
        .route("/active", get(commands::list_active_tunnels))
        .route("/status/{id}", get(commands::get_tunnel_status))
        .route("/audit/{id}", get(commands::get_tunnel_audit))
        .route("/ssh-hosts", get(commands::list_ssh_config_hosts))
        .route(
            "/ssh-hosts/discovered",
            get(commands::list_discovered_ssh_hosts_http),
        )
        .route(
            "/ssh-hosts/probe",
            post(commands::probe_discovered_host_http),
        )
        .route(
            "/ssh-hosts/status",
            get(commands::probe_ssh_config_hosts_http),
        )
        .route("/agent-keys", get(commands::list_agent_keys))
}

/// Routes shared by BOTH `build_router` (desktop/loopback) and
/// `build_remote_router` (tuic-remote daemon) — identical method + handler in
/// both. Returned WITHOUT `.with_state`/layers so each caller merges it before
/// applying its own state and middleware.
///
/// Router-specific routes stay in their own builder: desktop-only surfaces and
/// the per-router `/fs/read-editor*` handler down-scope (SECURITY).
fn shared_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mcp/peer", get(remote_peer::endpoint))
        // Version (authenticated)
        .route("/api/version", get(session::app_version))
        // Shared on purpose: this is how a remote client escapes the header-only
        // Basic Auth it cannot put on a WebSocket upgrade. Registered on
        // `build_router` alone it would be 404 on the very daemon that needs it.
        .route(
            "/api/auth/session-token",
            get(config_routes::get_session_token),
        )
        // The mobile app's login form posts here. It is the one route a device
        // without a session may call: `basic_auth_middleware` lets it through
        // and the handler runs the same admission as the Basic fallback.
        .route("/auth/login", post(auth::login_handler))
        // Agent configs. Shared: a remote repo's agents run with the remote
        // machine's `agents.json`, which the frontend reads by connection
        // (`remoteIO` in agentConfigs.ts). Only in `build_router`, the daemon
        // answered 404 and those repos ran with no agent configs.
        .route(
            "/config/agents",
            get(config_routes::get_agents_config).put(config_routes::put_agents_config),
        )
        // Progress. Shared, not desktop-only: the store is a SQLite file in the
        // app config directory and every handler calls `crate::progress::*`,
        // which needs no WebView and no Tauri. These lived in `build_router`
        // only, so a remote/PWA client reaching a `tuic-remote` daemon got 404
        // on the whole feature — not an auth failure, no route at all. Each
        // handler still guards with `require_local_or_auth`: loopback passes on
        // the address, a remote caller passes because `basic_auth_middleware`
        // inserts `Authenticated` before the handler runs.
        .route("/progress/report", post(post_progress_report))
        .route("/progress/list", post(post_progress_list))
        .route("/progress/projects", get(get_progress_projects))
        .route("/progress/delete", post(post_progress_delete))
        .route("/progress/viewed", post(post_progress_viewed))
        .route("/progress/flow", post(post_progress_flow))
        .route("/progress/flow/detail", post(post_progress_flow_detail))
        .route("/stories/action", post(post_story_action))
        .route("/stories/capabilities", get(get_story_capabilities))
        .route(
            "/workflows/definition/action",
            post(post_workflow_definition_action),
        )
        .route("/workflows/run/action", post(post_workflow_run_action))
        // Session lifecycle
        .route(
            "/sessions",
            get(session::list_sessions).post(session::create_session),
        )
        .route("/sessions/{id}/write", post(session::write_to_session))
        .route("/sessions/{id}/submit", post(submit_agent_reply))
        // The N-ary sibling: one round trip, but the per-input bookkeeping still
        // runs once per part. Concatenating into `/write` is NOT equivalent —
        // `apply_input_bookkeeping` reads the whole payload as one keystroke.
        .route(
            "/sessions/{id}/write-parts",
            post(session::write_parts_to_session),
        )
        .route("/attachments/upload", post(crate::attachments::upload_http))
        .route(
            "/sessions/{id}/queue",
            get(session::list_queued_commands)
                .post(session::enqueue_command)
                .delete(session::clear_queued_commands),
        )
        .route(
            "/sessions/{id}/queue/{command_id}",
            delete(session::remove_queued_command),
        )
        .route("/sessions/{id}/name", put(session::set_session_name))
        .route("/sessions/{id}/resize", post(session::resize_session))
        .route("/sessions/{id}/output", get(session::get_output))
        .route("/sessions/{id}/raw-ring", get(session::get_raw_ring))
        .route("/sessions/{id}/pause", post(session::pause_session))
        .route("/sessions/{id}/resume", post(session::resume_session))
        .route("/sessions/{id}/kitty-flags", get(session::get_kitty_flags))
        .route(
            "/sessions/{id}/foreground",
            get(session::get_foreground_process),
        )
        .route("/sessions/{id}/shell-state", get(session::get_shell_state))
        .route(
            "/sessions/{id}/shell-family",
            get(session::get_session_shell_family),
        )
        .route("/sessions/{id}/last-prompt", get(session::get_last_prompt))
        .route(
            "/sessions/{id}/prompt-receipt",
            get(session::get_prompt_receipt),
        )
        .route(
            "/sessions/{id}/input-buffer",
            get(session::get_input_buffer_content),
        )
        .route(
            "/sessions/{id}/leaf-pid",
            get(session::get_session_leaf_pid),
        )
        .route(
            "/sessions/{id}/has-foreground",
            get(session::has_foreground_process),
        )
        .route("/sessions/{id}/visible", post(session::set_session_visible))
        .route("/sessions/{id}", delete(session::close_session))
        // WebSocket streaming
        .route("/sessions/{id}/stream", get(session::ws_stream))
        // Terminal theme, so OSC 10/11/12 colour queries can be answered.
        .route(
            "/terminal/theme-colors",
            post(session::terminal_theme_colors),
        )
        // Terminal grid commands
        .route(
            "/sessions/{id}/terminal/scroll",
            post(session::terminal_scroll),
        )
        .route(
            "/sessions/{id}/terminal/scroll-to",
            post(session::terminal_scroll_to),
        )
        .route(
            "/sessions/{id}/terminal/scroll-to-offset",
            post(session::terminal_scroll_to_offset),
        )
        .route(
            "/sessions/{id}/terminal/scroll-info",
            get(session::terminal_scroll_info),
        )
        .route(
            "/sessions/{id}/terminal/search",
            post(session::terminal_search),
        )
        .route(
            "/sessions/{id}/terminal/search-buffer",
            post(session::terminal_search_buffer),
        )
        .route(
            "/sessions/{id}/terminal/row-text",
            get(session::terminal_get_row_text),
        )
        .route(
            "/sessions/{id}/terminal/lines",
            get(session::terminal_get_lines),
        )
        .route("/sessions/{id}/chat-view", get(session::chat_view))
        .route(
            "/sessions/{id}/terminal/styled-rows",
            get(session::terminal_styled_rows),
        )
        .route(
            "/sessions/{id}/terminal/cursor-line",
            get(session::terminal_get_cursor_line),
        )
        .route(
            "/sessions/{id}/terminal/hyperlink",
            get(session::terminal_hyperlink_at),
        )
        .route(
            "/sessions/{id}/terminal/hyperlink-span",
            get(session::terminal_hyperlink_span),
        )
        .route(
            "/sessions/{id}/terminal/selection-text",
            get(session::terminal_get_selection_text),
        )
        .route(
            "/sessions/{id}/terminal/logical-line",
            get(session::terminal_get_logical_line),
        )
        .route(
            "/sessions/{id}/terminal/request-frame",
            post(session::terminal_request_frame),
        )
        // Agent sessions
        .route("/sessions/agent", post(agent_routes::spawn_agent_session))
        .route(
            "/sessions/worktree",
            post(session::create_session_with_worktree),
        )
        // Orchestrator stats
        .route("/stats", get(session::get_stats))
        .route("/metrics", get(session::get_metrics))
        .route("/process/stats", get(session::get_process_stats))
        .route("/process/monitor", get(session::process_monitor_panel))
        // Git operations
        .route("/repo/info", get(git_routes::repo_info))
        .route("/repo/remote-url", get(git_routes::remote_url))
        .route("/repo/diff", get(git_routes::repo_diff))
        .route("/repo/diff-stats", get(git_routes::repo_diff_stats))
        .route("/repo/files", get(git_routes::repo_changed_files))
        .route("/repo/branches", get(git_routes::repo_branches))
        .route(
            "/repo/branches/merged",
            get(git_routes::repo_merged_branches),
        )
        .route("/repo/summary", get(git_routes::repo_summary))
        .route("/repo/structure", get(git_routes::repo_structure))
        .route(
            "/repo/diff-stats/batch",
            get(git_routes::repo_diff_stats_batch),
        )
        // Watchers
        .route(
            "/watchers/repo",
            post(watcher_routes::start_repo_watcher_http)
                .delete(watcher_routes::stop_repo_watcher_http),
        )
        .route(
            "/watchers/dir",
            post(watcher_routes::start_dir_watcher_http)
                .delete(watcher_routes::stop_dir_watcher_http),
        )
        // Shared, not remote-only: `set_hot_repos` is a COMMAND_TABLE entry, so
        // a browser/PWA client of the DESKTOP app calls it too and used to get a
        // 404 — the repo watcher then never learned which repos were hot. Found
        // by `command_table_paths_all_hit_a_registered_route`.
        .route(
            "/watchers/hot-repos",
            put(watcher_routes::set_hot_repos_http),
        )
        // Logs
        .route(
            "/logs",
            get(log_routes::get_logs)
                .post(log_routes::push_log)
                .delete(log_routes::clear_logs),
        )
        // Diagnostics
        .route(
            "/diagnostics",
            get(log_routes::diagnostics_get).post(log_routes::diagnostics_set),
        )
        .route(
            "/diagnostics/markers",
            get(log_routes::marker_compliance_get),
        )
        .route(
            "/diagnostics/capture",
            get(log_routes::capture_get).post(log_routes::capture_set),
        )
        .route("/diagnostics/memory", get(log_routes::memory_report_get))
        // Worktrees
        .route(
            "/worktrees",
            get(worktree_routes::list_worktrees_http).post(worktree_routes::create_worktree_http),
        )
        .route(
            "/worktrees/dir",
            get(worktree_routes::get_worktrees_dir_http),
        )
        .route(
            "/worktrees/paths",
            get(worktree_routes::get_worktree_paths_http),
        )
        .route(
            "/worktrees/generate-name",
            post(worktree_routes::generate_worktree_name_http),
        )
        .route(
            "/worktrees/finalize",
            post(worktree_routes::finalize_merged_worktree_http),
        )
        .route(
            "/worktrees/lifecycle",
            get(worktree_routes::workspace_lifecycle_http),
        )
        .route(
            "/worktrees/run-script",
            post(worktree_routes::run_setup_script_http),
        )
        .route(
            "/worktrees/{workspace_id}",
            delete(worktree_routes::remove_worktree_http),
        )
        // File operations
        .route("/repo/file", get(git_routes::read_file_http))
        .route("/repo/file-diff", get(git_routes::get_file_diff_http))
        .route(
            "/repo/markdown-files",
            get(git_routes::list_markdown_files_http),
        )
        // Branch operations
        .route(
            "/repo/local-branches",
            get(worktree_routes::list_local_branches_http),
        )
        .route(
            "/repo/checkout-remote",
            post(worktree_routes::checkout_remote_branch_http),
        )
        .route(
            "/repo/orphan-worktrees",
            get(worktree_routes::detect_orphan_worktrees_http),
        )
        .route(
            "/repo/orphan-cleanup-assessment",
            get(worktree_routes::assess_orphan_cleanup_http),
        )
        .route(
            "/repo/orphan-cleanup/begin",
            post(worktree_routes::begin_orphan_cleanup_http),
        )
        .route(
            "/repo/orphan-cleanup/pending",
            get(worktree_routes::pending_orphan_cleanup_http),
        )
        .route(
            "/repo/orphan-cleanup/answer",
            post(worktree_routes::answer_orphan_cleanup_http),
        )
        .route(
            "/repo/orphan-cleanup/clear",
            post(worktree_routes::clear_orphan_cleanup_http),
        )
        .route(
            "/repo/remove-orphan",
            post(worktree_routes::remove_orphan_worktree_http),
        )
        .route("/repo/branch/rename", post(git_routes::rename_branch_http))
        .route("/repo/initials", get(git_routes::get_initials_http))
        .route(
            "/repo/is-main-branch",
            get(git_routes::check_is_main_branch_http),
        )
        // Agents
        .route(
            "/agents/verify-session",
            post(agent_routes::verify_agent_session_http),
        )
        .route("/agents", get(agent_routes::detect_agents))
        .route(
            "/agents/detect",
            get(agent_routes::detect_agent_binary_http),
        )
        .route(
            "/agents/launch-args",
            post(agent_routes::prepare_agent_launch_args_http),
        )
        .route(
            "/agents/ides",
            get(agent_routes::detect_installed_ides_http),
        )
        .route(
            "/agents/detect-all",
            post(agent_routes::detect_all_agent_binaries_http),
        )
        .route("/agents/open-in-app", post(agent_routes::open_in_app_http))
        // File system
        .route("/fs/list", get(fs_routes::list_directory_http))
        .route("/fs/search", get(fs_routes::search_files_http))
        .route("/fs/search-content", get(fs_routes::search_content_http))
        .route(
            "/fs/search-content-all",
            get(fs_routes::search_content_all_http),
        )
        .route("/fs/read", get(fs_routes::fs_read_file_http))
        .route("/fs/markdown-image", get(fs_routes::markdown_image_http))
        .route("/fs/read-external", get(fs_routes::read_external_file_http))
        .route("/fs/write", post(fs_routes::write_file_http))
        .route(
            "/fs/write-if-unchanged",
            post(fs_routes::write_file_if_unchanged_http),
        )
        .route("/fs/mkdir", post(fs_routes::create_directory_http))
        .route("/fs/delete", post(fs_routes::delete_path_http))
        .route("/fs/rename", post(fs_routes::rename_path_http))
        .route("/fs/copy", post(fs_routes::copy_path_http))
        .route("/fs/gitignore", post(fs_routes::add_to_gitignore_http))
        .route(
            "/fs/resolve-terminal-path",
            get(fs_routes::resolve_terminal_path_http),
        )
        .route(
            "/fs/resolve-terminal-paths",
            post(fs_routes::resolve_terminal_paths_http),
        )
        .route(
            "/fs/resolve-markdown-link",
            post(fs_routes::resolve_markdown_link_http),
        )
        .route("/fs/stat", get(fs_routes::stat_path_http))
        .route("/fs/warm-index", post(fs_routes::warm_content_index_http))
        .route(
            "/fs/write-external",
            post(fs_routes::write_external_file_http),
        )
        .route("/fs/copy-abs", post(fs_routes::copy_path_abs_http))
        .route("/fs/move-abs", post(fs_routes::move_path_abs_http))
        .route("/fs/transfer", post(fs_routes::fs_transfer_paths_http))
        .route("/fs/upload-copy", post(fs_routes::upload_copy_http))
        // Claude Usage dashboard
        .route("/claude/usage", get(claude_routes::claude_usage_api))
        .route(
            "/claude/timeline",
            get(claude_routes::claude_usage_timeline),
        )
        .route(
            "/claude/session-stats",
            get(claude_routes::claude_session_stats),
        )
        .route("/claude/projects", get(claude_routes::claude_project_list))
        // Codex usage (same ticker, different agent)
        .route("/codex/usage", get(claude_routes::codex_usage_api))
        .route("/codex/stats", get(claude_routes::codex_usage_stats))
        // Grok usage (provider-owned ACP billing extension)
        .route("/grok/usage", get(claude_routes::grok_usage_api))
        // Recent commits / git panel
        .route(
            "/repo/recent-commits",
            get(git_routes::get_recent_commits_http),
        )
        .route("/repo/panel-context", get(git_routes::git_panel_context))
        .route("/repo/run-git", post(git_routes::run_git_command_http))
        .route(
            "/repo/working-tree-status",
            get(git_routes::working_tree_status),
        )
        .route("/repo/stage", post(git_routes::stage_files_http))
        .route("/repo/unstage", post(git_routes::unstage_files_http))
        .route("/repo/discard", post(git_routes::discard_files_http))
        .route(
            "/repo/apply-reverse-patch",
            post(git_routes::apply_reverse_patch_http),
        )
        .route("/repo/commit", post(git_routes::git_commit_http))
        .route("/repo/commit-log", get(git_routes::commit_log_http))
        .route("/repo/stash", get(git_routes::stash_list_http))
        .route("/repo/stash/apply", post(git_routes::stash_apply_http))
        .route("/repo/stash/pop", post(git_routes::stash_pop_http))
        .route("/repo/stash/drop", post(git_routes::stash_drop_http))
        .route("/repo/stash/show", get(git_routes::stash_show_http))
        .route("/repo/file-history", get(git_routes::file_history_http))
        .route("/repo/file-blame", get(git_routes::file_blame_http))
        // Git panel (story 064)
        .route(
            "/repo/gutter-changes",
            get(git_routes::get_gutter_changes_http),
        )
        .route(
            "/repo/branches-detail",
            get(git_routes::get_branches_detail_http),
        )
        .route(
            "/repo/recent-branches",
            get(git_routes::get_recent_branches_http),
        )
        .route("/repo/branch-base", get(git_routes::get_branch_base_http))
        .route(
            "/repo/worktree-dirty",
            get(git_routes::check_worktree_dirty_http),
        )
        .route(
            "/repo/base-ref-options",
            get(git_routes::list_base_ref_options_http),
        )
        .route(
            "/repo/clone-branch-name",
            post(git_routes::generate_clone_branch_name_http),
        )
        .route("/repo/commit-graph", get(git_routes::get_commit_graph_http))
        .route("/repo/create-branch", post(git_routes::create_branch_http))
        .route("/repo/delete-branch", post(git_routes::delete_branch_http))
        .route(
            "/repo/delete-local-branch",
            post(git_routes::delete_local_branch_http),
        )
        .route(
            "/repo/update-from-base",
            post(git_routes::update_from_base_http),
        )
        .route(
            "/repo/switch-branch",
            post(worktree_routes::switch_branch_http),
        )
        .route(
            "/repo/merge-archive-worktree",
            post(worktree_routes::merge_and_archive_worktree_http),
        )
        // System info
        .route("/system/local-ips", get(git_routes::get_local_ips_http))
        .route("/system/local-ip", get(git_routes::get_local_ip_http))
        .route(
            "/system/home-directory",
            get(fs_routes::home_directory_http),
        )
        // Server-Sent Events
        .route("/events", get(sse_routes::sse_events))
        .route("/events/types", post(sse_routes::sse_update_types))
        // Answer a pending MCP confirmation (the browser/PWA half of the desktop
        // `mcp_confirm_response` command).
        .route("/mcp/confirm-response", post(mcp_confirm_response_http))
        // The tab's verdict on a `session action=suspend` request (browser/PWA half
        // of the desktop `session_suspend_response` command).
        .route("/mcp/suspend-response", post(session_suspend_response_http))
        // ACP (ego). Shared, not desktop-only: driving ego from a phone is the
        // whole point of the client, and the binary it may launch comes from
        // this host's configuration rather than from any request.
        .nest("/acp", acp_routes::acp_routes())
        // ego's own command line, for the configuration ACP does not carry:
        // which model a run defaults to, and whether a provider has a
        // credential. Every route here starts a process, so every route here
        // takes the spawn guard — unlike `/acp`, where only the two routes that
        // launch ego do.
        .nest("/ego", ego_routes::ego_routes())
}

/// Body of `POST /mcp/confirm-response`.
#[derive(serde::Deserialize)]
struct McpConfirmResponseBody {
    request_id: String,
    confirmed: bool,
}

async fn mcp_confirm_response_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<McpConfirmResponseBody>,
) -> Json<serde_json::Value> {
    resolve_mcp_confirm(&state, &body.request_id, body.confirmed);
    Json(serde_json::json!({ "ok": true }))
}

/// Body of `POST /mcp/suspend-response`.
#[derive(serde::Deserialize)]
struct SessionSuspendResponseBody {
    request_id: String,
    ok: bool,
    reason: Option<String>,
}

async fn session_suspend_response_http(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SessionSuspendResponseBody>,
) -> Json<serde_json::Value> {
    mcp_transport::resolve_session_suspend(&state, &body.request_id, body.ok, body.reason);
    Json(serde_json::json!({ "ok": true }))
}

/// Resolve a pending MCP confirmation and tell every client to dismiss it.
///
/// Shared by the Tauri command and the HTTP route so the two transports cannot
/// drift. Unknown or already-answered ids are ignored: every client races to
/// answer the same request, and only one of them can win.
pub(crate) fn resolve_mcp_confirm(state: &Arc<AppState>, request_id: &str, confirmed: bool) {
    let Some((_, sender)) = state.confirm_responses.remove(request_id) else {
        return;
    };
    let _ = sender.send(confirmed);
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::McpConfirmResolved {
            request_id: request_id.to_string(),
            confirmed,
        });
    #[cfg(feature = "desktop")]
    if let Some(ref app) = *state.app_handle.read() {
        use tauri::Emitter;
        let _ = app.emit(
            "mcp-confirm-resolved",
            serde_json::json!({ "request_id": request_id, "confirmed": confirmed }),
        );
    }
}

/// Wall-clock bound on producing a response. A handler that wedges holds its
/// connection forever without this; 301 s is above the slowest legitimate
/// request (including a cold worktree with warm build artifacts) and far below "never".
///
/// **Must stay strictly greater than every in-handler deadline this layer
/// wraps**, or this outer bound fires first, drops the inner future, and the
/// caller gets a bare 408 instead of the handler's own documented response —
/// `mcp_transport::CONFIRM_TIMEOUT` (300 s) is the tightest of those, so 301 s
/// is a deliberate 1 s margin over it, not a rounding choice (#760-c29f). The
/// local MCP bridge in turn waits 305 s for a confirm or a workspace op,
/// a 4 s margin over THIS layer — `request_timeout_beats_every_in_handler_deadline`
/// pins the ordering.
pub(crate) const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(301);

/// Largest request body any route will buffer.
///
/// axum already applies a 2 MB `DefaultBodyLimit` to `Json`/`String`/`Bytes`,
/// so this is not a new restriction — it makes an invisible framework default
/// into an asserted one. A future axum release cannot loosen it silently, and
/// `server_limits_reject_a_body_over_the_cap` fails if anyone raises it without
/// meaning to.
pub(crate) const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

/// The client image cap is shared with the browser draft. Base64 expands each
/// three bytes to four; allow JSON framing and text alongside the image.
fn acp_prompt_body_limit() -> usize {
    let image_bytes: usize =
        serde_json::from_str(include_str!("../../../src/shared/acp-image-limit.json"))
            .expect("valid shared ACP image limit");
    image_bytes.div_ceil(3) * 4 + 64 * 1024
}

/// Another buffered route allowed a larger body than [`MAX_BODY_BYTES`]: importing a
/// voice file, which travels whole as base64 in JSON so the payload is the same
/// over IPC and HTTP. The cap is what the largest accepted voice
/// (`MAX_USER_VOICE_BYTES`, 64 MB) encodes to, plus room for the JSON around
/// it; the handler refuses anything longer before decoding it. A layer on the
/// route overrides the router-wide `DefaultBodyLimit`, and only for this path —
/// `only_the_voice_import_route_accepts_a_large_body` pins both halves.
/// The remote binary upload reads the raw Body as a stream, so its separate
/// 512 MiB limit is enforced while copying chunks rather than by this layer.
#[cfg(feature = "dictation")]
pub(crate) const SPEECH_VOICE_IMPORT_BODY_BYTES: usize =
    crate::dictation::speech::assets::MAX_USER_VOICE_BYTES.div_ceil(3) * 4 + 64 * 1024;

/// Apply the server's two resource bounds to an assembled router.
///
/// `timeout` is a parameter rather than a read of `REQUEST_TIMEOUT` because the
/// deadline IS the subject here: the test needs a bound it can exceed in
/// milliseconds, and a `cfg(test)` constant would leave production and test
/// exercising different code (AGENTS.md, "Which timing assertions are
/// load-bearing").
///
/// Both layers are safe over SSE and WebSocket. The timeout races its sleep
/// only against the future that produces the `Response`; once
/// headers are returned the timeout is dropped and the body streams
/// unwatched. `Sse` and `WebSocketUpgrade` both return immediately, so neither
/// `/events` nor a PTY socket can be cut off mid-stream.
pub(crate) fn with_server_limits(routes: Router, timeout: std::time::Duration) -> Router {
    routes
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| async move {
                // Upload bodies carry their own idle, size and total receive budgets.
                // The ordinary response deadline is too short for large transfers.
                if request.uri().path() == "/fs/upload-copy" {
                    return next.run(request).await;
                }
                match tokio::time::timeout(timeout, next.run(request)).await {
                    Ok(response) => response,
                    Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
                }
            },
        ))
}

pub fn build_router(state: Arc<AppState>, remote_auth: bool, mcp_enabled: bool) -> Router {
    // false selects the trusted local IPC transport; every TCP listener uses true.
    let boundary = request_boundary::RequestBoundary::new(state.clone());
    let cors = boundary.cors();

    let mut routes = Router::new()
        // Routes common to the remote daemon router live in shared_routes().
        .merge(shared_routes())
        // Health
        .route("/health", get(session::health))
        // Git/GitHub (desktop-only)
        .route("/repo/github", get(github_routes::repo_github_status))
        .route("/repo/prs", get(github_routes::repo_pr_statuses))
        .route("/repo/ci", get(github_routes::repo_ci_checks))
        .route(
            "/repo/pr-review-threads",
            get(github_routes::repo_pr_review_threads),
        )
        .route("/repo/pr-diff", get(github_routes::repo_pr_diff))
        .route("/repo/merged-prs", get(github_routes::repo_merged_prs))
        .route(
            "/repo/changelog",
            get(github_routes::repo_generate_changelog),
        )
        .route("/repo/pr-review", post(github_routes::repo_pr_review))
        .route(
            "/repo/improvement-scan",
            post(github_routes::repo_improvement_scan),
        )
        .route(
            "/repo/create-issue-from-proposal",
            post(github_routes::repo_create_issue_from_proposal),
        )
        .route(
            "/repo/conflict-assist",
            post(github_routes::repo_conflict_assist),
        )
        .route("/repo/approve-pr", post(github_routes::repo_approve_pr))
        .route(
            "/repo/update-pr-branch",
            post(github_routes::repo_update_pr_branch),
        )
        .route("/repo/close-pr", post(github_routes::repo_close_pr))
        .route("/repo/create-pr", post(github_routes::repo_create_pr))
        .route("/repo/create-issue", post(github_routes::repo_create_issue))
        .route(
            "/repo/post-pr-review",
            post(github_routes::repo_post_pr_review),
        )
        .route("/repo/prs/batch", post(github_routes::repo_all_pr_statuses))
        .route("/repo/issues", get(github_routes::repo_issues))
        .route("/repo/issue-detail", get(github_routes::repo_issue_detail))
        .route("/repo/issues/close", post(github_routes::repo_close_issue))
        .route(
            "/repo/issues/reopen",
            post(github_routes::repo_reopen_issue),
        )
        // GitHub poller
        .route(
            "/repo/github-poller/start",
            post(github_routes::poller_start),
        )
        .route("/repo/github-poller/stop", post(github_routes::poller_stop))
        .route(
            "/repo/github-poller/visibility",
            post(github_routes::poller_set_visibility),
        )
        .route(
            "/repo/github-poller/poll-repo",
            post(github_routes::poller_poll_repo),
        )
        .route(
            "/repo/github-poller/update-paths",
            post(github_routes::poller_update_paths),
        )
        .route(
            "/repo/github-poller/set-issue-filter",
            post(github_routes::poller_set_issue_filter),
        )
        .route(
            "/repo/github-poller/api-debug",
            get(github_routes::api_debug_get).post(github_routes::api_debug_set),
        )
        // GitHub auth (device-code flow) + misc — browser/PWA via loopback
        .route(
            "/github/viewer-login",
            get(github_routes::github_viewer_login),
        )
        .route("/repo/ci-failure-logs", get(github_routes::ci_failure_logs))
        .route(
            "/circleci/token",
            get(github_routes::circleci_token_status)
                .post(github_routes::circleci_set_token)
                .delete(github_routes::circleci_delete_token),
        )
        .route(
            "/github/pr-hide-drafts",
            post(github_routes::github_set_hide_drafts),
        )
        .route(
            "/github/auth/start",
            post(github_routes::github_start_login),
        )
        .route("/github/auth/poll", post(github_routes::github_poll_login))
        .route(
            "/github/accounts/poll",
            post(github_routes::github_poll_add_account),
        )
        .route("/github/auth/logout", post(github_routes::github_logout))
        .route(
            "/github/auth/disconnect",
            post(github_routes::github_disconnect),
        )
        .route(
            "/github/auth/status",
            get(github_routes::github_auth_status),
        )
        .route(
            "/github/diagnostics",
            get(github_routes::github_diagnostics),
        )
        // Multi-account repo bindings (IPC/HTTP parity). Accounts + repo resolution
        // are gated to desktop below — their impls are #[cfg(feature = "desktop")].
        .route(
            "/github/bindings",
            get(github_routes::github_list_bindings).post(github_routes::github_bind_repo),
        )
        .route(
            "/github/bindings/remove",
            post(github_routes::github_unbind_repo),
        )
        // Config
        .route(
            "/config",
            get(config_routes::get_config).put(config_routes::put_config),
        )
        .route(
            "/config/hash-password",
            post(config_routes::hash_password_http),
        )
        .route(
            "/api/auth/rotate-token",
            post(config_routes::rotate_session_token),
        )
        .route(
            "/config/telegram",
            get(telegram_routes::get).put(telegram_routes::put),
        )
        .route(
            "/config/notifications",
            get(config_routes::get_notification_config).put(config_routes::put_notification_config),
        )
        .route(
            "/config/ui-prefs",
            get(config_routes::get_ui_prefs).put(config_routes::put_ui_prefs),
        )
        .route(
            "/config/defaults",
            get(config_routes::get_config_defaults_http),
        )
        .route(
            "/config/repo-settings",
            get(config_routes::get_repo_settings).put(config_routes::put_repo_settings),
        )
        .route(
            "/config/repo-settings/has-custom",
            get(config_routes::check_has_custom_settings_http),
        )
        .route(
            "/config/repo-defaults",
            get(config_routes::get_repo_defaults).put(config_routes::put_repo_defaults),
        )
        .route(
            "/config/repositories",
            get(config_routes::get_repositories).put(config_routes::put_repositories),
        )
        .route(
            "/config/repositories/stale-temp",
            get(config_routes::get_stale_temp_repository_candidates)
                .post(config_routes::post_repair_stale_temp_repositories),
        )
        .route(
            "/config/pane-layout",
            get(config_routes::get_pane_layout).put(config_routes::put_pane_layout),
        )
        .route("/config/clear-caches", post(config_routes::clear_caches))
        .route(
            "/config/clear-repo-caches",
            post(config_routes::clear_repo_caches),
        )
        .route(
            "/config/repo-local-config",
            get(config_routes::get_repo_local_config)
                .post(config_routes::save_repo_local_config_http),
        )
        // Story 066: config / themes / notes / misc stateless parity (loopback)
        .route(
            "/config/branch-label",
            post(config_routes::set_branch_label_http),
        )
        .route(
            "/config/note-image",
            post(config_routes::save_note_image_http),
        )
        .route(
            "/config/note-assets/delete",
            post(config_routes::delete_note_assets_http),
        )
        .route(
            "/config/note-assets/delete-batch",
            post(config_routes::delete_note_assets_batch_http),
        )
        .route("/config/themes", get(config_routes::list_themes_http))
        .route(
            "/config/project-mcp-upstreams",
            post(config_routes::set_project_mcp_upstreams_http),
        )
        .route(
            "/exec/shell-script",
            post(config_routes::execute_shell_script_http),
        )
        .route(
            "/audio/output-devices",
            get(config_routes::list_audio_output_devices_http),
        )
        .route(
            "/agent/discover-session",
            post(config_routes::discover_agent_session_http),
        )
        .route(
            "/agent/claude-project-dir",
            post(config_routes::claude_project_dir_http),
        )
        .route(
            "/agent/open-in-custom",
            post(config_routes::open_in_custom_http),
        )
        .route(
            "/generators/generate",
            post(config_routes::generate_value_http),
        )
        .route(
            "/registry/plugins",
            get(config_routes::fetch_plugin_registry_http),
        )
        .route(
            "/config/prompt-library",
            get(config_routes::get_prompt_library).put(config_routes::put_prompt_library),
        )
        .route(
            "/config/activity",
            get(config_routes::get_activity).put(config_routes::put_activity),
        )
        .route(
            "/config/keybindings",
            get(config_routes::get_keybindings).put(config_routes::put_keybindings),
        )
        .route(
            "/config/agents/{agent}/hook-instrumentation",
            get(config_routes::get_agent_hook_state)
                .put(config_routes::put_agent_hook_instrumentation),
        )
        .route(
            "/config/agents/{agent}/native-status-signals",
            get(config_routes::get_agent_native_status_signals)
                .put(config_routes::put_agent_native_status_signals),
        )
        .route(
            "/config/remote-connections",
            get(config_routes::get_remote_connections).put(config_routes::put_remote_connection),
        )
        .route(
            "/config/remote-connections/{id}",
            delete(config_routes::delete_remote_connection),
        )
        .route(
            "/config/remote-connections/{id}/password",
            put(config_routes::put_remote_connection_password)
                .get(config_routes::get_remote_connection_password_exists),
        )
        .route(
            "/config/remote-connections/{id}/token",
            post(config_routes::post_remote_connection_token),
        )
        .route(
            "/config/remote-connections/status",
            get(config_routes::get_remote_connection_statuses),
        )
        .route(
            "/config/remote-connections/{id}/update",
            get(config_routes::get_remote_update_preview).post(config_routes::post_remote_update),
        )
        .route(
            "/config/remote-connections/{id}/connect",
            post(config_routes::post_remote_connection_connect)
                .delete(config_routes::delete_remote_connection_connect),
        )
        .route(
            "/config/remote-connections/{id}/install",
            post(config_routes::post_remote_connection_install)
                .delete(config_routes::delete_remote_connection_install),
        )
        // Debug: execute JS in the main WebView (loopback-only, enforced in handler).
        // Local router only — never the remote router (this is an RCE surface).
        .route("/debug/invoke_js", post(log_routes::invoke_js_http))
        .route("/secrets/forms/{nonce}", get(crate::secrets::form_http))
        .route("/secrets/forms/submit", post(crate::secrets::submit_http))
        // Debug: reload the main WebView natively (loopback-only, enforced in
        // handler). Local router only — the remote client reloads its own tab.
        .route(
            "/debug/reload_webview",
            post(log_routes::reload_webview_http),
        )
        // Branch operations (desktop-only)
        .route(
            "/repo/merge-pr",
            post(worktree_routes::merge_pr_via_github_http),
        )
        // Prompt processing
        .route("/prompt/process", post(agent_routes::process_prompt_http))
        .route(
            "/prompt/extract-variables",
            post(agent_routes::extract_prompt_variables_http),
        )
        .route(
            "/prompt/resolve-variables",
            post(agent_routes::resolve_context_variables_http),
        )
        .route(
            "/prompt/resolve-prompt-variables",
            post(agent_routes::resolve_prompt_variables_http),
        )
        .route(
            "/prompt/execute-headless",
            post(agent_routes::execute_headless_prompt_http),
        )
        // File browser — desktop gets the large (250 MB) editor read cap; the
        // remote router down-scopes these two paths to the standard-cap handlers.
        .route("/fs/read-editor", get(fs_routes::read_editor_file_http))
        .route(
            "/fs/read-editor-external",
            get(fs_routes::read_editor_file_external_http),
        )
        // Notes
        .route(
            "/config/notes",
            get(config_routes::get_notes).put(config_routes::put_notes),
        )
        // Plugins
        .route("/plugins/list", get(git_routes::list_user_plugins_http))
        // MCP status + instructions
        .route("/mcp/status", get(config_routes::get_mcp_status_http))
        .route("/mcp/upstream-status", get(upstream_status_handler))
        .route(
            "/mcp/upstreams",
            get(load_mcp_upstreams_http).put(save_mcp_upstreams_http),
        )
        .route(
            "/mcp/upstreams/reconnect",
            post(reconnect_mcp_upstream_http),
        )
        .route(
            "/mcp/upstreams/credential",
            post(save_mcp_upstream_credential_http).delete(delete_mcp_upstream_credential_http),
        )
        .route(
            "/mcp/instructions",
            get(mcp_transport::mcp_instructions_http),
        )
        // Plugin docs (for MCP bridge)
        .route("/plugins/docs", get(plugin_dev_guide_handler))
        // Plugin data (for external HTTP clients)
        .route(
            "/api/plugins/{plugin_id}/data/{*path}",
            get(plugin_data_http).post(plugin_data_write_http),
        )
        // Plugin RPC commands (story 071)
        .route(
            "/api/plugins/{plugin_id}/fs/read",
            get(plugin_routes::plugin_fs_read),
        )
        .route(
            "/api/plugins/{plugin_id}/fs/read-batch",
            post(plugin_routes::plugin_fs_read_batch),
        )
        .route(
            "/api/plugins/{plugin_id}/fs/read-base64",
            get(plugin_routes::plugin_fs_read_base64),
        )
        .route(
            "/api/plugins/{plugin_id}/fs/tail",
            get(plugin_routes::plugin_fs_tail),
        )
        .route(
            "/api/plugins/{plugin_id}/fs/list",
            get(plugin_routes::plugin_fs_list),
        )
        .route(
            "/api/plugins/{plugin_id}/fs/write",
            post(plugin_routes::plugin_fs_write),
        )
        .route(
            "/api/plugins/{plugin_id}/fs/write-base64",
            post(plugin_routes::plugin_fs_write_base64),
        )
        .route(
            "/api/plugins/{plugin_id}/fs/rename",
            post(plugin_routes::plugin_fs_rename),
        )
        .route(
            "/api/plugins/{plugin_id}/build-artifacts/scan",
            post(plugin_routes::plugin_scan_build_artifacts),
        )
        .route(
            "/api/plugins/{plugin_id}/build-artifacts/delete",
            post(plugin_routes::plugin_delete_build_artifact),
        )
        .route(
            "/api/plugins/{plugin_id}/build-artifacts/trim",
            post(plugin_routes::plugin_trim_build_artifact),
        )
        .route(
            "/api/plugins/{plugin_id}/exec",
            post(plugin_routes::plugin_exec),
        )
        .route(
            "/api/plugins/{plugin_id}/http",
            post(plugin_routes::plugin_http_fetch),
        )
        .route(
            "/api/plugins/{plugin_id}/pty/output",
            get(plugin_routes::plugin_pty_output),
        )
        .route(
            "/api/plugins/{plugin_id}/register",
            post(plugin_routes::plugin_register),
        )
        .route(
            "/api/plugins/{plugin_id}/unregister",
            post(plugin_routes::plugin_unregister),
        )
        .route(
            "/api/plugins/output-watchers",
            post(plugin_routes::plugin_set_output_watchers),
        )
        .route(
            "/api/plugins/{plugin_id}/readme",
            get(plugin_routes::plugin_readme),
        )
        // Push notification API
        .route("/api/push/vapid-key", get(push_vapid_key))
        .route(
            "/api/push/subscribe",
            post(push_subscribe).delete(push_unsubscribe),
        )
        .route("/api/push/test", post(push_test))
        // SSH tunnel management
        .nest("/tunnels", tunnel_routes());

    #[cfg(feature = "desktop")]
    {
        routes = routes
            .route(
                "/design-mode/start",
                post(crate::design_mode::commands::start),
            )
            .route(
                "/design-mode/stop",
                post(crate::design_mode::commands::stop),
            )
            .route("/design-mode", get(crate::design_mode::commands::statuses));
    }

    // MCP Streamable HTTP transport — only when MCP is enabled
    if mcp_enabled {
        routes = routes.route(
            "/mcp",
            post(mcp_transport::mcp_post)
                .get(mcp_transport::mcp_get)
                .delete(mcp_transport::mcp_delete),
        );
    }

    // Multi-account accounts + repo resolution — desktop-only: add/remove/resolve go
    // through the keychain-backed #[cfg(feature = "desktop")] impls in github_account.rs.
    // The remote daemon never serves GitHub routes (build_remote_router omits them), so
    // gate these out of the always-compiled build_router so the lib builds for
    // tuic-remote (#094-ec55 remote-target fix).
    #[cfg(feature = "desktop")]
    let routes = routes
        .route(
            "/github/accounts",
            get(github_routes::github_list_accounts).post(github_routes::github_add_account),
        )
        .route(
            "/github/accounts/remove",
            post(github_routes::github_remove_account),
        )
        .route(
            "/github/resolve-repo",
            get(github_routes::github_resolve_repo),
        )
        .route(
            "/github/resolve-repos",
            post(github_routes::github_resolve_repos),
        );

    // Dictation — desktop-only: `crate::dictation` owns the audio capture and
    // the whisper model, both gated on the opt-in `dictation` feature.
    #[cfg(feature = "dictation")]
    let routes = routes
        .route(
            "/dictation/status",
            get(dictation_routes::get_dictation_status_http),
        )
        .route(
            "/dictation/models",
            get(dictation_routes::get_model_info_http),
        )
        .route(
            "/dictation/models/download",
            post(dictation_routes::download_whisper_model_http),
        )
        .route(
            "/dictation/models/delete",
            post(dictation_routes::delete_whisper_model_http),
        )
        .route(
            "/dictation/speech/assets",
            get(dictation_routes::get_speech_assets_http),
        )
        .route(
            "/dictation/speech/assets/download",
            post(dictation_routes::download_speech_asset_http),
        )
        .route(
            "/dictation/speech/assets/cancel",
            post(dictation_routes::cancel_speech_download_http),
        )
        .route(
            "/dictation/speech/assets/delete",
            post(dictation_routes::delete_speech_asset_http),
        )
        .route(
            "/dictation/speech/voices",
            get(dictation_routes::get_speech_voices_http),
        )
        .route(
            "/dictation/speech/edge-voices",
            get(dictation_routes::get_edge_voices_http),
        )
        .route(
            "/dictation/speech/voices/import",
            post(dictation_routes::import_speech_voice_http).layer(
                axum::extract::DefaultBodyLimit::max(SPEECH_VOICE_IMPORT_BODY_BYTES),
            ),
        )
        .route(
            "/dictation/speech/voices/delete",
            post(dictation_routes::delete_speech_voice_http),
        )
        .route(
            "/dictation/speech/voices/preview",
            post(dictation_routes::preview_speech_voice_http),
        )
        .route(
            "/dictation/speech/speak",
            post(dictation_routes::speak_http),
        )
        .route(
            "/dictation/speech/stop",
            post(dictation_routes::stop_speech_http),
        )
        .route(
            "/dictation/speech/pause",
            post(dictation_routes::pause_speech_http),
        )
        .route(
            "/dictation/speech/resume",
            post(dictation_routes::resume_speech_http),
        )
        .route(
            "/dictation/speech/status",
            get(dictation_routes::get_speech_status_http),
        )
        .route(
            "/dictation/start",
            post(dictation_routes::start_dictation_http),
        )
        .route(
            "/dictation/stop",
            post(dictation_routes::stop_dictation_http),
        )
        .route(
            "/dictation/corrections",
            get(dictation_routes::get_correction_map_http)
                .put(dictation_routes::set_correction_map_http),
        )
        .route(
            "/dictation/devices",
            get(dictation_routes::list_audio_devices_http),
        )
        .route(
            "/dictation/inject",
            post(dictation_routes::inject_text_http),
        )
        .route(
            "/dictation/config",
            get(dictation_routes::get_dictation_config_http)
                .put(dictation_routes::set_dictation_config_http),
        )
        .route(
            "/dictation/hands-free",
            get(dictation_routes::get_hands_free_status_http),
        )
        .route(
            "/dictation/hands-free/default-notice",
            get(dictation_routes::get_hands_free_default_notice_http),
        )
        .route(
            "/dictation/hands-free/arm",
            post(dictation_routes::arm_hands_free_http),
        )
        .route(
            "/dictation/hands-free/disarm",
            post(dictation_routes::disarm_hands_free_http),
        )
        // A WS rather than a route pair, because this one carries audio in
        // both directions for as long as the client wants a conversation —
        // the one dictation surface that is a stream and not a call.
        .route(
            "/dictation/hands-free/audio",
            get(dictation_routes::hands_free_audio_ws),
        );

    // OS integration — desktop-only: the relay client, the audio output and the
    // updater all live behind the `desktop` feature.
    #[cfg(feature = "desktop")]
    let routes = routes
        .route(
            "/system/relay-status",
            get(system_routes::relay_status_http),
        )
        .route(
            "/system/notification-sound",
            post(system_routes::play_notification_sound_http),
        )
        .route(
            "/system/check-update",
            get(system_routes::check_update_channel_http),
        );

    // Static files — SPA frontend (desktop only; not embedded in the remote binary)
    #[cfg(feature = "desktop")]
    let routes = routes
        .route("/", get(static_files::serve_index))
        .fallback(spa_or_api_404);

    let routes = routes
        .with_state(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::workflow_actor_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            remote_session_proxy::proxy_http,
        ))
        .layer(cors)
        // DefaultPredicate auto-excludes SSE (text/event-stream) and WebSocket upgrades.
        // Do NOT replace with a bare SizeAbove — it would break streaming endpoints.
        .layer(
            CompressionLayer::new().compress_when(DefaultPredicate::new().and(SizeAbove::new(860))),
        );

    let routes = with_server_limits(routes, REQUEST_TIMEOUT);

    if remote_auth {
        routes
            .layer(axum::middleware::from_fn_with_state(
                state,
                auth::basic_auth_middleware,
            ))
            .layer(axum::middleware::from_fn_with_state(
                boundary,
                request_boundary::check,
            ))
    } else {
        routes
    }
}

/// Build a minimal router for the `tuic-remote` binary.
///
/// Includes all per-repo routes needed for remote terminal management:
/// sessions (CRUD + stream), terminal grid, git, worktrees, file system,
/// agents, watchers, events (SSE), and repo info.
///
/// Excludes desktop-only routes: config management, MCP bridge, plugins,
/// push notifications, prompt processing, AI chat, GitHub poller, and
/// static file serving (no frontend embedded in the remote binary).
///
/// Auth and compression layers are applied identically to `build_router()`.
#[allow(dead_code)] // Used by tuic-remote binary (not(desktop) build)
pub fn build_remote_router(state: Arc<AppState>) -> Router {
    let boundary = request_boundary::RequestBoundary::new(state.clone());
    let cors = boundary.cors();

    let public_routes = Router::new()
        .route("/health", get(session::health))
        .layer(cors.clone())
        .with_state(state.clone());

    let routes = Router::new()
        // Routes common to the desktop/loopback router live in shared_routes().
        .merge(shared_routes())
        .route("/remote/update", post(crate::remote_update::upload))
        // SECURITY: remote clients get the standard (10 MB) cap, NOT the large
        // editor cap. The 250 MB editor read is a desktop-local feature; serving
        // it over a (possibly metered/slow) remote link risks OOM/latency since
        // the whole file is read into a String→JSON with no streaming. Route the
        // editor paths through the standard-cap handlers remotely.
        .route("/fs/read-editor", get(fs_routes::fs_read_file_http))
        .route(
            "/fs/read-editor-external",
            get(fs_routes::read_external_file_http),
        )
        // SSH tunnel management
        .nest("/tunnels", tunnel_routes())
        .with_state(state.clone())
        .layer(cors)
        .layer(
            CompressionLayer::new().compress_when(DefaultPredicate::new().and(SizeAbove::new(860))),
        );

    let authed = with_server_limits(routes, REQUEST_TIMEOUT).layer(
        axum::middleware::from_fn_with_state(state, auth::basic_auth_middleware),
    );

    public_routes
        .merge(authed)
        .layer(axum::middleware::from_fn_with_state(
            boundary,
            request_boundary::check,
        ))
}

/// Rebind the TCP listener after a config write changed remote-access settings.
///
/// The IPC `save_config` has always done this; the HTTP and MCP config writers
/// did not, so a save over those transports left the running process serving a
/// state the disk no longer agreed with. Routed through one helper so the
/// transports cannot drift again. No-op in the headless binary, which has no
/// desktop restart plumbing.
pub(crate) fn restart_after_server_settings_change(state: &Arc<AppState>, reason: &'static str) {
    #[cfg(feature = "desktop")]
    crate::restart_server(state, reason);
    #[cfg(not(feature = "desktop"))]
    let _ = (state, reason);
}

/// Start the HTTP API server.
///
/// **Unix socket** (macOS/Linux): always starts at `<config_dir>/mcp.sock`.
/// No auth, MCP always enabled. Used by the local MCP bridge.
///
/// **TCP listener** (optional): when `remote_enabled` is true, binds to
/// `0.0.0.0:{remote_access_port}` with Basic Auth.
///
/// Both listeners share a single shutdown signal so `save_config` can restart
/// the server cleanly.
/// Drop the peer identities that a reaped MCP protocol session was carrying,
/// and report `(removed, retained)`.
///
/// Split out of the reaper loop so the eviction rule can be tested without a
/// one-hour timer. Callers hold `PEER_IDENTITY_BIND_LOCK`. The rule itself is
/// [`AppState::peer_identity_is_reapable`](crate::state::AppState::peer_identity_is_reapable):
/// the transport is gone, but an identity someone can still reach — or still
/// name as a parent — must outlive it.
fn evict_peers_for_reaped_mcp_session_locked(
    state: &AppState,
    mcp_sid: &str,
) -> (Vec<String>, Vec<String>) {
    // Reaping the protocol metadata must retire its other per-session state
    // under the same lock as bridge registration and DELETE. A surviving
    // sibling inherits delivery ownership; an addressable peer with no sibling
    // keeps its identity but no route to the expired transport.
    state.session_maps.messaging_channels.remove(mcp_sid);
    if let Some((_, tuic)) = state.mcp.to_session.remove(mcp_sid) {
        let survivors = if let Some(mut reverse) = state.mcp.session_to_mcp.get_mut(&tuic) {
            reverse.retain(|sid| sid != mcp_sid);
            reverse.clone()
        } else {
            Vec::new()
        };
        if let Some(next_owner) = survivors.first() {
            if let Some(mut peer) = state.peer_agents.get_mut(&tuic)
                && peer.mcp_session_id == mcp_sid
            {
                peer.mcp_session_id = next_owner.clone();
            }
        } else {
            state.mcp.session_to_mcp.remove(&tuic);
        }
    }
    let (removed, retained): (Vec<String>, Vec<String>) = state
        .peer_agents
        .iter()
        .filter(|entry| entry.value().mcp_session_id == *mcp_sid)
        .map(|entry| entry.key().clone())
        .partition(|tuic| state.peer_identity_is_reapable(tuic));
    for tuic in &removed {
        crate::mcp_http::remote_peer::unregister_peer(state, tuic);
        state.orchestrator_peers.remove(tuic);
        state.active_agent_waiters.remove(tuic);
        let _ = state
            .event_bus
            .send(crate::state::AppEvent::PeerUnregistered {
                tuic_session: tuic.clone(),
            });
    }
    (removed, retained)
}

#[cfg(test)]
fn evict_peers_for_reaped_mcp_session(state: &AppState, sid: &str) -> (Vec<String>, Vec<String>) {
    let _bind_guard = mcp_transport::PEER_IDENTITY_BIND_LOCK.lock();
    evict_peers_for_reaped_mcp_session_locked(state, sid)
}

fn reap_selected_mcp_session(
    state: &AppState,
    sid: &str,
    ttl: std::time::Duration,
) -> Option<(Vec<String>, Vec<String>)> {
    // Refresh of a missing session also takes this lock. A refresh that found
    // existing metadata holds its DashMap entry while stamping last_activity,
    // so remove_if either observes the new timestamp or wins first; in the
    // latter case refresh recreates the session after route cleanup finishes.
    let _bind_guard = mcp_transport::PEER_IDENTITY_BIND_LOCK.lock();
    state
        .mcp
        .sessions
        .remove_if(sid, |_, meta| meta.last_activity.elapsed() >= ttl)?;
    // Clean up peer agents whose MCP session was reaped. An identity that
    // is still addressable outlives the transport that carried it.
    Some(evict_peers_for_reaped_mcp_session_locked(state, sid))
}

/// Spawn the once-a-minute maintenance sweep: reap idle MCP protocol sessions
/// and the peer identities they carried, expired auth rate-limit entries and
/// expired task handles.
///
/// Not a desktop convenience. A `tuic-remote` daemon authenticates every TCP
/// request, so its rate-limit map grows with every scanner that finds the port,
/// and it now holds MCP sessions of its own — without this sweep both grow for
/// as long as the process lives (#793-23a5).
pub(crate) fn spawn_maintenance_sweep(state: &Arc<AppState>) {
    let reaper_state = state.clone();
    tokio::spawn(async move {
        const MCP_SESSION_TTL: std::time::Duration = std::time::Duration::from_secs(3600);
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let now = std::time::Instant::now();
            let selected: Vec<String> = reaper_state
                .mcp
                .sessions
                .iter()
                .filter(|e| now.duration_since(e.value().last_activity) >= MCP_SESSION_TTL)
                .map(|e| e.key().clone())
                .collect();
            let mut any_reaped = false;
            for sid in &selected {
                let Some((_removed, retained)) =
                    reap_selected_mcp_session(&reaper_state, sid, MCP_SESSION_TTL)
                else {
                    continue;
                };
                any_reaped = true;
                tracing::warn!("MCP session reaped (idle ≥1h): {sid}");
                if !retained.is_empty() {
                    tracing::info!(
                        "MCP session {sid} reaped, {} peer identity/identities kept addressable: {}",
                        retained.len(),
                        retained.join(", ")
                    );
                }
            }
            // Evict orphaned inboxes for peers that no longer exist
            if any_reaped {
                let known_tuic: std::collections::HashSet<String> = reaper_state
                    .peer_agents
                    .iter()
                    .map(|e| e.key().clone())
                    .collect();
                reaper_state
                    .agent_inbox
                    .retain(|tuic, _| known_tuic.contains(tuic));
            }

            // Sweep expired auth rate-limit entries so the map can't grow
            // unbounded for IPs that fail once and never return (scanners,
            // IPv6 rotation). Window is read fresh each pass so runtime
            // config changes take effect on the next sweep.
            let rl_window = reaper_state
                .config
                .read()
                .services
                .auth
                .auth_rate_limit_window_secs;
            let evicted =
                auth::sweep_expired_rate_limits(&reaper_state.auth_rate_limits, rl_window);
            if evicted > 0 {
                tracing::debug!(
                    source = "auth",
                    evicted,
                    "Swept expired auth rate-limit entries"
                );
            }

            // Drop tasks past their TTL on the same pass — a task handle
            // outlives its protocol session, so it needs its own sweep, but
            // not its own timer.
            let reaped_tasks = reaper_state.tasks.reap_expired();
            if reaped_tasks > 0 {
                tracing::debug!(
                    source = "tasks",
                    reaped = reaped_tasks,
                    "Reaped expired tasks"
                );
            }
        }
    });
}

/// Spawn the local IPC listener: a Unix domain socket on unix, a named pipe on
/// Windows. Always on, never authenticated — the OS user is the boundary.
///
/// This is the only way `tuic-bridge` reaches the process. An agent running on
/// this machine spawns the bridge as a stdio child and the bridge speaks HTTP
/// over this socket, so a process without it hosts agents that have no
/// `tuicommander` MCP server at all — which is what `tuic-remote` did before
/// #793-23a5. It serves the loopback router, not the remote one: the caller is
/// a local child process, so it gets the same surface a desktop agent gets.
pub(crate) async fn spawn_ipc_listener(state: &Arc<AppState>, mcp_enabled: bool) {
    // --- Unix socket listener (always on, no auth) ---
    #[cfg(unix)]
    {
        let sock = resolve_socket_path();

        if let Some(parent) = sock.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            tracing::warn!(source = "mcp_http", path = %parent.display(), "Failed to create socket parent dir: {e}");
        }

        // Bind the socket. Remove stale file first (left by a crashed previous run).
        // resolve_socket_path() already verified the primary socket is not live,
        // so remove_file here only cleans up stale/dead sockets.
        const MAX_BIND_ATTEMPTS: u8 = 3;
        async fn bind_unix_socket(
            sock: &std::path::Path,
        ) -> Result<tokio::net::UnixListener, std::io::Error> {
            let mut last_err = std::io::Error::other("no bind attempts");
            for attempt in 0..MAX_BIND_ATTEMPTS {
                let _ = std::fs::remove_file(sock);
                match tokio::net::UnixListener::bind(sock) {
                    Ok(uds) => return Ok(uds),
                    Err(e) => {
                        tracing::warn!(source = "mcp_http", attempt, path = %sock.display(), "Unix socket bind failed: {e}");
                        last_err = e;
                        if attempt + 1 < MAX_BIND_ATTEMPTS {
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    }
                }
            }
            Err(last_err)
        }

        match bind_unix_socket(&sock).await {
            Err(e) => {
                tracing::error!(source = "mcp_http", path = %sock.display(), "Failed to bind Unix socket after retries: {e}");
            }
            Ok(initial_uds) => {
                tracing::info!(source = "mcp_http", path = %sock.display(), "Unix socket listening");
                *state.bound_socket_path.write() = sock.clone();
                // Watchdog task: if axum::serve() returns unexpectedly, rebind
                // and restart. No shutdown signal — this task runs until the
                // process exits.
                let watchdog_state = state.clone();
                tokio::spawn(async move {
                    let mut uds = initial_uds;
                    loop {
                        let app = build_router(watchdog_state.clone(), false, mcp_enabled);
                        let app =
                            app.layer(axum::middleware::from_fn(inject_localhost_connect_info));
                        match axum::serve(uds, app.into_make_service()).await {
                            Err(e) => tracing::error!(
                                source = "mcp_http",
                                "Unix socket server error: {e}"
                            ),
                            Ok(()) => tracing::warn!(
                                source = "mcp_http",
                                "Unix socket server exited cleanly (unexpected)"
                            ),
                        }
                        // Unexpected exit — rebind and restart.
                        tracing::warn!(source = "mcp_http", path = %sock.display(), "Unix socket server stopped unexpectedly, restarting…");
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        match bind_unix_socket(&sock).await {
                            Ok(new_uds) => {
                                tracing::info!(source = "mcp_http", path = %sock.display(), "Unix socket rebound successfully");
                                uds = new_uds;
                            }
                            Err(e) => {
                                tracing::error!(source = "mcp_http", path = %sock.display(), "Unix socket rebind failed permanently ({e}) — MCP bridge will be unavailable");
                                break;
                            }
                        }
                    }
                });
            }
        }
    }

    // --- Windows named pipe listener (always on, no auth) ---
    #[cfg(windows)]
    {
        match NamedPipeListener::new() {
            Ok(pipe) => {
                tracing::info!(
                    source = "mcp_http",
                    pipe = PIPE_NAME,
                    "Named pipe listening"
                );
                let app = build_router(state.clone(), false, mcp_enabled);
                let app = app.layer(axum::middleware::from_fn(inject_localhost_connect_info));
                tokio::spawn(async move {
                    match axum::serve(pipe, app.into_make_service()).await {
                        Err(e) => {
                            tracing::error!(source = "mcp_http", "Named pipe server error: {e}")
                        }
                        Ok(()) => tracing::warn!(
                            source = "mcp_http",
                            "Named pipe server exited cleanly (unexpected)"
                        ),
                    }
                });
            }
            Err(e) => {
                tracing::error!(
                    source = "mcp_http",
                    pipe = PIPE_NAME,
                    "Failed to create named pipe: {e}"
                );
            }
        }
    }
}

/// Start IPC + TCP listeners. Returns `true` if TCP bound successfully (or
/// wasn't requested). Returns `false` only when `remote_enabled` is true and
/// TCP bind failed on all port attempts.
pub async fn start_server(
    state: Arc<AppState>,
    mcp_enabled: bool,
    remote_enabled: bool,
    tls_config: Option<axum_server::tls_rustls::RustlsConfig>,
) -> bool {
    let config = state.config.read().clone();

    // Register shutdown channel so save_config can restart server. Only the
    // TCP listener + TLS renewal task listen to this signal: IPC listeners
    // (Unix socket / named pipe), the MCP session reaper, and the upstream
    // health checker persist across restarts and ignore shutdown.
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    *state.server_shutdown.lock() = Some(shutdown_tx);

    // One-time IPC + background-task initialisation. Guards against re-spawn
    // on `restart_server`, which would leak a new reaper/health-checker each
    // time and (before this fix) tear down the Unix socket for ~200ms — long
    // enough to trip the MCP bridge's 3-failure/9s health threshold and
    // disconnect Claude Code on every `save_config` / Tailscale state change.
    let first_start = !state
        .ipc_started
        .swap(true, std::sync::atomic::Ordering::AcqRel);

    tracing::info!(
        source = "mcp_http",
        first_start,
        mcp_enabled,
        remote_enabled,
        "HTTP server lifecycle starting"
    );

    if first_start {
        crate::pty::spawn_process_snapshot_refresher(Arc::clone(&state));

        acp_mcp::install(&state);
        // Reap idle MCP sessions, expired rate limits and expired tasks every 60s.
        spawn_maintenance_sweep(&state);

        // Spawn upstream health checker: pings Ready upstreams every 60s
        crate::mcp_proxy::registry::UpstreamRegistry::spawn_health_checker(Arc::clone(
            &state.mcp.upstream_registry,
        ));

        // Spawn standby checker: SIGSTOP idle+unfocused sessions after timeout
        #[cfg(unix)]
        crate::pty::spawn_standby_checker(Arc::clone(&state));

        spawn_ipc_listener(&state, mcp_enabled).await;
    }

    // --- TCP listener (only for remote access with auth) ---
    // Supports dual-protocol (HTTP+HTTPS on same port) when TLS cert is available.
    let tcp_handle = if remote_enabled {
        let base_port = std::env::var("TUIC_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(config.services.server.port);
        let host = if config.services.server.ipv6_enabled {
            "[::]"
        } else {
            "0.0.0.0"
        };
        const MAX_PORT_ATTEMPTS: u16 = 3;
        // Boot-race resilience: on a `make dev` restart the outgoing process
        // (debug builds skip the single-instance lock) may still hold the port
        // for a moment, so the fresh boot's bind loses the race. Without a retry
        // the server would run Unix-socket-only until a settings toggle triggers
        // restart_server — the "starts without :9876, comes alive when I touch
        // settings" symptom. Retry the full port sweep with backoff so the boot
        // waits for the port to free. A genuine 2nd live instance still binds
        // base_port+1 on the first sweep, so it pays no delay.
        const BIND_RETRY_ROUNDS: u32 = 6;
        const BIND_RETRY_BACKOFF_MS: u64 = 500;

        let mut listener_result: Option<std::net::TcpListener> = None;
        // Port 0 = OS-assigned, no retry needed
        let attempts = if base_port == 0 { 1 } else { MAX_PORT_ATTEMPTS };
        'bind: for round in 0..BIND_RETRY_ROUNDS {
            for attempt in 0..attempts {
                let port = base_port + attempt;
                let bind_addr = format!("{host}:{port}");
                match std::net::TcpListener::bind(&bind_addr) {
                    Ok(listener) => {
                        listener.set_nonblocking(true).ok();
                        if attempt > 0 {
                            tracing::info!(
                                source = "mcp_http",
                                "Port {base_port} busy, using {port}"
                            );
                        }
                        listener_result = Some(listener);
                        break 'bind;
                    }
                    Err(_) if attempt + 1 < attempts => {
                        tracing::warn!(
                            source = "mcp_http",
                            "Port {port} busy, trying {}",
                            port + 1
                        );
                    }
                    Err(e) => {
                        // Whole sweep failed this round. Retry after a backoff to
                        // ride out a restart race, unless rounds are exhausted.
                        if round + 1 < BIND_RETRY_ROUNDS {
                            tracing::warn!(
                                source = "mcp_http",
                                "Ports {base_port}–{port} busy (round {}/{BIND_RETRY_ROUNDS}); retrying in {BIND_RETRY_BACKOFF_MS}ms",
                                round + 1
                            );
                        } else {
                            tracing::error!(
                                source = "mcp_http",
                                "Failed to bind TCP on ports {base_port}–{port} after {BIND_RETRY_ROUNDS} rounds: {e}"
                            );
                        }
                    }
                }
            }
            // OS-assigned port can't fail meaningfully; don't sleep past the last round.
            if base_port == 0 || round + 1 >= BIND_RETRY_ROUNDS {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(BIND_RETRY_BACKOFF_MS)).await;
        }

        if let Some(listener) = listener_result {
            let addr = listener
                .local_addr()
                .unwrap_or_else(|_| std::net::SocketAddr::from(([0, 0, 0, 0], 0)));

            let app = build_router(state.clone(), true, mcp_enabled);
            let svc = app.into_make_service_with_connect_info::<std::net::SocketAddr>();

            if let Some(tls) = tls_config.clone() {
                tracing::info!(source = "mcp_http", %addr, "TCP listening with dual-protocol HTTP+HTTPS");
                Some(tokio::spawn(async move {
                    use axum_server_dual_protocol::ServerExt;
                    // axum-server 0.8 moved the non-blocking-mode switch into
                    // `from_tcp`, so building the server is now fallible and its
                    // failure is reported separately from a serve failure: one
                    // means the listener never started, the other that it stopped.
                    let server =
                        match axum_server_dual_protocol::from_tcp_dual_protocol(listener, tls) {
                            Ok(server) => server,
                            Err(e) => {
                                tracing::error!(
                                    source = "mcp_http",
                                    "TCP/TLS listener setup failed: {e}"
                                );
                                return;
                            }
                        };
                    if let Err(e) = server.set_upgrade(false).serve(svc).await {
                        tracing::error!(source = "mcp_http", "TCP/TLS server error: {e}");
                    }
                }))
            } else {
                tracing::info!(source = "mcp_http", %addr, "TCP listening (HTTP only, remote access enabled)");
                Some(tokio::spawn(async move {
                    // Fallible since axum-server 0.8 — see the dual-protocol arm.
                    let server = match axum_server::from_tcp(listener) {
                        Ok(server) => server,
                        Err(e) => {
                            tracing::error!(source = "mcp_http", "TCP listener setup failed: {e}");
                            return;
                        }
                    };
                    if let Err(e) = server.serve(svc).await {
                        tracing::error!(source = "mcp_http", "TCP server error: {e}");
                    }
                }))
            }
        } else {
            None
        }
    } else {
        None
    };

    let tcp_bound = !remote_enabled || tcp_handle.is_some();

    // Spawn TLS cert renewal task (checks every 24h, renews if < 30 days to expiry)
    let renewal_handle = if let Some(ref tls) = tls_config {
        let ts_state = state.tailscale_state.read().clone();
        if let crate::tailscale::TailscaleState::Running {
            fqdn,
            https_enabled: true,
        } = ts_state
        {
            let tls_clone = tls.clone();
            Some(tokio::spawn(async move {
                crate::tailscale::cert_renewal_loop(fqdn, tls_clone).await;
            }))
        } else {
            None
        }
    } else {
        None
    };

    // Wait for shutdown signal
    let _ = shutdown_rx.await;

    tracing::info!(
        source = "mcp_http",
        remote_enabled,
        "TCP server lifecycle stopping; local MCP IPC remains active"
    );

    // Abort only TCP-bound listeners — IPC listeners, reaper, and health
    // checker persist across restarts. Socket-file cleanup is deferred to the
    // next process start's `bind_unix_socket`, which already removes stale
    // files before binding.
    if let Some(h) = tcp_handle {
        h.abort();
    }
    if let Some(h) = renewal_handle {
        h.abort();
    }

    tcp_bound
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MAX_CONCURRENT_SESSIONS;
    use axum::body::Body;
    use axum::extract::connect_info::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// Build a POST request with ConnectInfo from the given address.
    fn mcp_post_from(
        url: &str,
        body: &serde_json::Value,
        addr: std::net::SocketAddr,
    ) -> Request<Body> {
        let mut req = Request::post(url)
            .header(header::HOST, "127.0.0.1:9876")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(body).expect("serialize JSON body"),
            ))
            .expect("build POST request");
        req.extensions_mut().insert(ConnectInfo(addr));
        req
    }

    /// Build a POST request with ConnectInfo set to localhost (the common case).
    fn mcp_post(url: &str, body: &serde_json::Value) -> Request<Body> {
        mcp_post_from(url, body, std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
    }

    fn get_localhost(url: &str) -> Request<Body> {
        let mut req = Request::get(url)
            .header(header::HOST, "127.0.0.1:9876")
            .body(Body::empty())
            .expect("build GET request");
        req.extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))));
        req
    }

    /// Build a PUT request with ConnectInfo from the given address.
    fn put_from(url: &str, body: &serde_json::Value, addr: std::net::SocketAddr) -> Request<Body> {
        let mut req = Request::put(url)
            .header(header::HOST, "127.0.0.1:9876")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(body).expect("serialize JSON body"),
            ))
            .expect("build PUT request");
        req.extensions_mut().insert(ConnectInfo(addr));
        req
    }

    fn config_save_body(config: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"base": config, "config": config})
    }

    pub(super) fn test_state() -> Arc<AppState> {
        // Was a hand-copied 116-field `AppState` literal, kept in sync with
        // `AppState::new` by hand and sharing one `test-tuic-data` dir across
        // every test — the SQLITE_BUSY collision `make_test_app_state`
        // documents and avoids (#678-9a75).
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        // Override default disabled_native_tools so all 8 tools are visible in tests
        state.config.write().disabled_native_tools = Vec::new();
        mcp_transport::rebuild_tool_search_index(&state);
        state
    }

    #[tokio::test]
    async fn push_test_does_not_claim_delivery_when_push_is_disabled() {
        let state = test_state();
        state.push_store.upsert(crate::push::PushSubscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/example".to_string(),
            keys: crate::push::PushSubscriptionKeys {
                p256dh: "unused".to_string(),
                auth: "unused".to_string(),
            },
            created_at: chrono::Utc::now(),
        });
        state.config.write().services.push.enabled = false;

        let response = build_router(state, false, true)
            .oneshot(mcp_post("/api/push/test", &serde_json::json!({})))
            .await
            .expect("push test response");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("push test body");
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["sent"], 0);
        assert_eq!(body["error"], "Push notifications are disabled");
    }

    #[tokio::test]
    async fn push_test_does_not_count_an_invalid_subscription_as_sent() {
        let state = test_state();
        state.push_store.upsert(crate::push::PushSubscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/example".to_string(),
            keys: crate::push::PushSubscriptionKeys {
                p256dh: "invalid".to_string(),
                auth: "invalid".to_string(),
            },
            created_at: chrono::Utc::now(),
        });
        let (private, public) = crate::push::generate_vapid_keys().unwrap();
        {
            let mut config = state.config.write();
            config.services.push.enabled = true;
            config.services.push.vapid_private_key = private;
            config.services.push.vapid_public_key = public;
        }

        let response = build_router(state, false, true)
            .oneshot(mcp_post("/api/push/test", &serde_json::json!({})))
            .await
            .expect("push test response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("push test body");
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["sent"], 0);
        assert_eq!(body["failed"], 1);
    }

    #[tokio::test]
    async fn mobile_reply_to_a_closed_session_returns_a_structured_rejection() {
        let response = build_router(test_state(), false, true)
            .oneshot(mcp_post(
                "/sessions/closed-session/submit",
                &serde_json::json!({ "input": "yes" }),
            ))
            .await
            .expect("reply response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("reply body");
        let body: serde_json::Value = serde_json::from_slice(&body).expect("JSON rejection");
        assert_eq!(body["submitted"], false);
        assert_eq!(body["reason"], "session_not_found");
    }

    #[tokio::test]
    async fn mobile_reply_rejects_an_unauthenticated_remote_caller() {
        let remote = std::net::SocketAddr::from(([203, 0, 113, 1], 4444));
        let response = build_router(test_state(), false, true)
            .oneshot(mcp_post_from(
                "/sessions/closed-session/submit",
                &serde_json::json!({ "input": "yes" }),
                remote,
            ))
            .await
            .expect("remote reply response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn mobile_reply_rejects_empty_input_without_a_submission() {
        let response = build_router(test_state(), false, true)
            .oneshot(mcp_post(
                "/sessions/closed-session/submit",
                &serde_json::json!({ "input": "" }),
            ))
            .await
            .expect("empty reply response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("empty reply body");
        let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result["submitted"], false);
    }

    #[tokio::test]
    async fn test_health() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(json["build"]["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(json["build"]["target"], env!("TUIC_TARGET_TRIPLE"));
        assert_eq!(json["build"]["sha256"].as_str().unwrap().len(), 64);
    }

    #[tokio::test]
    async fn remote_update_rejects_a_missing_session_token() {
        let state = test_state();
        *state.session_token.write() = "update-secret".to_string();
        state.config.write().services.auth.lan_auth_bypass = false;
        let app = build_remote_router(state);
        let mut request = Request::post("/remote/update")
            .header(header::HOST, "127.0.0.1:9876")
            .body(Body::from("not a binary"))
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from((
                [203, 0, 113, 5],
                5555,
            ))));
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn remote_update_requires_the_pty_token_even_with_valid_basic_auth() {
        use base64::Engine;
        let state = test_state();
        *state.session_token.write() = "update-secret".to_string();
        {
            let mut config = state.config.write();
            config.services.auth.lan_auth_bypass = false;
            config.services.auth.username = "boss".to_string();
            config.services.auth.password_hash = bcrypt::hash("known-password", 4).unwrap();
        }
        let credentials = base64::engine::general_purpose::STANDARD.encode("boss:known-password");
        let mut request = Request::post("/remote/update")
            .header(header::HOST, "127.0.0.1:9876")
            .header(
                axum::http::header::AUTHORIZATION,
                format!("Basic {credentials}"),
            )
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from((
                [203, 0, 113, 5],
                5555,
            ))));
        let response = build_remote_router(state).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    // In-process executable replacement is unsupported on Windows (HTTP 501).
    #[cfg(unix)]
    #[tokio::test]
    async fn remote_update_rejects_invalid_metadata_before_writing() {
        let state = test_state();
        *state.session_token.write() = "update-secret".to_string();
        state.config.write().services.auth.lan_auth_bypass = false;
        let app = build_remote_router(state);
        for (target, sha256, sessions, content_length, expected) in [
            (
                "wrong-target",
                "a".repeat(64),
                "0",
                "1",
                StatusCode::BAD_REQUEST,
            ),
            (
                env!("TUIC_TARGET_TRIPLE"),
                "not-a-digest".to_string(),
                "0",
                "1",
                StatusCode::BAD_REQUEST,
            ),
            (
                env!("TUIC_TARGET_TRIPLE"),
                format!("{}g", "a".repeat(63)),
                "0",
                "1",
                StatusCode::BAD_REQUEST,
            ),
            (
                env!("TUIC_TARGET_TRIPLE"),
                "a".repeat(63),
                "0",
                "1",
                StatusCode::BAD_REQUEST,
            ),
            (
                env!("TUIC_TARGET_TRIPLE"),
                "a".repeat(65),
                "0",
                "1",
                StatusCode::BAD_REQUEST,
            ),
            (
                env!("TUIC_TARGET_TRIPLE"),
                "a".repeat(64),
                "1",
                "1",
                StatusCode::CONFLICT,
            ),
            (
                env!("TUIC_TARGET_TRIPLE"),
                "a".repeat(64),
                "0",
                "536870913",
                StatusCode::PAYLOAD_TOO_LARGE,
            ),
        ] {
            let mut request = Request::post("/remote/update")
                .header(header::COOKIE, "tui-session=update-secret")
                .header(header::HOST, "127.0.0.1:9876")
                .header("x-tuic-target", target)
                .header("x-tuic-sha256", sha256)
                .header("x-tuic-confirmed-sessions", sessions)
                .header("content-length", content_length)
                .body(Body::from("x"))
                .unwrap();
            request
                .extensions_mut()
                .insert(ConnectInfo(std::net::SocketAddr::from((
                    [203, 0, 113, 5],
                    5555,
                ))));
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), expected);
        }
    }

    // In-process executable replacement is unsupported on Windows (HTTP 501).
    #[cfg(unix)]
    #[tokio::test]
    async fn remote_update_preserves_the_old_executable_on_bad_digest_then_promotes() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("tuic-remote");
        std::fs::write(&executable, b"old executable").unwrap();
        let mut state = crate::state::tests_support::make_test_app_state();
        *state.session_token.write() = "update-secret".to_string();
        state.config.write().services.auth.lan_auth_bypass = false;
        state.remote_update = Some(crate::remote_update::RemoteUpdateState {
            executable: executable.clone(),
            restart: Arc::new(tokio::sync::Notify::new()),
            in_progress: tokio::sync::Mutex::new(()),
            installed: std::sync::atomic::AtomicBool::new(false),
        });
        let app = build_remote_router(Arc::new(state));
        let bytes = b"replacement executable";
        // Independent fixture digest, computed outside the implementation.
        let good_hash = "74faa3811f5e551111ed370650ae6d6acf14f8f7141bc5c4f653eb52bf57bf16";
        for (hash, expected, contents) in [
            (
                "0".repeat(64),
                StatusCode::BAD_REQUEST,
                b"old executable".as_slice(),
            ),
            (
                good_hash.to_string(),
                StatusCode::ACCEPTED,
                bytes.as_slice(),
            ),
        ] {
            let mut request = Request::post("/remote/update")
                .header(header::COOKIE, "tui-session=update-secret")
                .header(header::HOST, "127.0.0.1:9876")
                .header("x-tuic-target", env!("TUIC_TARGET_TRIPLE"))
                .header("x-tuic-sha256", hash)
                .header("x-tuic-confirmed-sessions", "0")
                .body(Body::from(bytes.to_vec()))
                .unwrap();
            request
                .extensions_mut()
                .insert(ConnectInfo(std::net::SocketAddr::from((
                    [203, 0, 113, 5],
                    5555,
                ))));
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), expected);
            assert_eq!(std::fs::read(&executable).unwrap(), contents);
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        }
        let mut duplicate = Request::post("/remote/update")
            .header(header::COOKIE, "tui-session=update-secret")
            .header(header::HOST, "127.0.0.1:9876")
            .header("x-tuic-target", env!("TUIC_TARGET_TRIPLE"))
            .header("x-tuic-sha256", good_hash)
            .header("x-tuic-confirmed-sessions", "0")
            .body(Body::from(bytes.to_vec()))
            .unwrap();
        duplicate
            .extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from((
                [203, 0, 113, 5],
                5555,
            ))));
        assert_eq!(
            app.oneshot(duplicate).await.unwrap().status(),
            StatusCode::CONFLICT
        );
    }

    // Catches: overlapping valid uploads both promote a binary or leave staging files behind.
    // In-process executable replacement is unsupported on Windows (HTTP 501).
    #[cfg(unix)]
    #[tokio::test]
    async fn concurrent_remote_updates_reject_the_second_upload_without_leaking_staging() {
        use futures_util::StreamExt;

        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("tuic-remote");
        std::fs::write(&executable, b"old executable").unwrap();
        let mut state = crate::state::tests_support::make_test_app_state();
        *state.session_token.write() = "update-secret".to_string();
        state.config.write().services.auth.lan_auth_bypass = false;
        state.remote_update = Some(crate::remote_update::RemoteUpdateState {
            executable: executable.clone(),
            restart: Arc::new(tokio::sync::Notify::new()),
            in_progress: tokio::sync::Mutex::new(()),
            installed: std::sync::atomic::AtomicBool::new(false),
        });
        let app = build_remote_router(Arc::new(state));

        let (first_chunk_sent, first_chunk_received) = tokio::sync::oneshot::channel();
        let (release_first, first_released) = tokio::sync::oneshot::channel();
        let first_body = futures_util::stream::once(async move {
            let _ = first_chunk_sent.send(());
            Ok::<_, std::io::Error>(axum::body::Bytes::from_static(b"first "))
        })
        .chain(futures_util::stream::once(async move {
            first_released.await.unwrap();
            Ok::<_, std::io::Error>(axum::body::Bytes::from_static(b"replacement"))
        }));
        let mut first = Request::post("/remote/update")
            .header(header::COOKIE, "tui-session=update-secret")
            .header(header::HOST, "127.0.0.1:9876")
            .header("x-tuic-target", env!("TUIC_TARGET_TRIPLE"))
            .header(
                "x-tuic-sha256",
                "07c360a6be1a9a97d1dfd58d0066dda36be2c80f70a229446401b5c69273af2c",
            )
            .header("x-tuic-confirmed-sessions", "0")
            .body(Body::from_stream(first_body))
            .unwrap();
        first
            .extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from((
                [203, 0, 113, 5],
                5555,
            ))));
        let first_task = tokio::spawn(app.clone().oneshot(first));
        first_chunk_received.await.unwrap();

        let mut second = Request::post("/remote/update")
            .header(header::COOKIE, "tui-session=update-secret")
            .header(header::HOST, "127.0.0.1:9876")
            .header("x-tuic-target", env!("TUIC_TARGET_TRIPLE"))
            .header(
                "x-tuic-sha256",
                "a21e1650d755eeec3aa7b80224c1513cd4b0bae02776a4b9931432b52b42d385",
            )
            .header("x-tuic-confirmed-sessions", "0")
            .body(Body::from("second replacement"))
            .unwrap();
        second
            .extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from((
                [203, 0, 113, 5],
                5556,
            ))));
        let response = app.oneshot(second).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(std::fs::read(&executable).unwrap(), b"old executable");

        release_first.send(()).unwrap();
        assert_eq!(
            first_task.await.unwrap().unwrap().status(),
            StatusCode::ACCEPTED
        );
        assert_eq!(std::fs::read(&executable).unwrap(), b"first replacement");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    // In-process executable replacement is unsupported on Windows (HTTP 501).
    #[cfg(unix)]
    #[tokio::test]
    async fn two_local_remote_routers_isolate_update_and_restart_signal() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first-tuic-remote");
        let second = directory.path().join("second-tuic-remote");
        std::fs::write(&first, b"first old binary").unwrap();
        std::fs::write(&second, b"second old binary").unwrap();
        let mut first_state = crate::state::tests_support::make_test_app_state();
        let mut second_state = crate::state::tests_support::make_test_app_state();
        let restarted = Arc::new(tokio::sync::Notify::new());
        let untouched = Arc::new(tokio::sync::Notify::new());
        for (state, executable, signal, token) in [
            (&mut first_state, &first, &restarted, "first-secret"),
            (&mut second_state, &second, &untouched, "second-secret"),
        ] {
            *state.session_token.write() = token.to_string();
            state.config.write().services.auth.lan_auth_bypass = false;
            state.remote_update = Some(crate::remote_update::RemoteUpdateState {
                executable: executable.to_path_buf(),
                restart: signal.clone(),
                in_progress: tokio::sync::Mutex::new(()),
                installed: std::sync::atomic::AtomicBool::new(false),
            });
        }
        let first_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let first_url = format!("http://{}", first_listener.local_addr().unwrap());
        let second_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let second_url = format!("http://{}", second_listener.local_addr().unwrap());
        let first_server = tokio::spawn(async move {
            axum::serve(
                first_listener,
                build_remote_router(Arc::new(first_state))
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
        });
        let second_server = tokio::spawn(async move {
            axum::serve(
                second_listener,
                build_remote_router(Arc::new(second_state))
                    .into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
        });
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{first_url}/remote/update"))
            .header("cookie", "tui-session=first-secret")
            .header("x-tuic-target", env!("TUIC_TARGET_TRIPLE"))
            .header(
                "x-tuic-sha256",
                "74faa3811f5e551111ed370650ae6d6acf14f8f7141bc5c4f653eb52bf57bf16",
            )
            .header("x-tuic-confirmed-sessions", "0")
            .body("replacement executable")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
        tokio::time::timeout(std::time::Duration::from_secs(3), restarted.notified())
            .await
            .expect("first daemon signalled restart");
        assert_eq!(std::fs::read(&first).unwrap(), b"replacement executable");
        assert_eq!(std::fs::read(&second).unwrap(), b"second old binary");
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(250), untouched.notified())
                .await
                .is_err()
        );
        let second_health: serde_json::Value = client
            .get(format!("{second_url}/health"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(second_health["ok"], true);
        first_server.abort();
        second_server.abort();
    }

    #[tokio::test]
    async fn remote_lifetime_health_reports_survive_secs() {
        let mut state = crate::state::tests_support::make_test_app_state();
        state.remote_survive_secs = Some(1_800);
        let app = build_router(Arc::new(state), false, true);
        let response = app
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["survive_secs"], 1_800);
    }

    #[tokio::test]
    async fn verify_agent_session_http_uses_the_requested_claude_profile() {
        let profile = tempfile::tempdir().unwrap();
        let other_profile = tempfile::tempdir().unwrap();
        let session_id = "af467730-5e79-49d9-8a17-ebd94c99f262";
        let project_dir = profile.path().join("projects/-work-project");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join(format!("{session_id}.jsonl")), "{}").unwrap();

        let app = build_router(test_state(), false, true);
        for (profile_root, requested_id, expected) in [
            (profile.path(), session_id, true),
            (other_profile.path(), session_id, false),
            (profile.path(), "not-a-session-id", false),
        ] {
            let body = serde_json::json!({
                "agentType": "claude",
                "sessionId": requested_id,
                "cwd": "/work/project",
                "agentPid": null,
                "envOverrides": { "CLAUDE_CONFIG_DIR": profile_root.to_str().unwrap() },
            });
            let response = app
                .clone()
                .oneshot(mcp_post("/agents/verify-session", &body))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let found: bool = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                found, expected,
                "profile={profile_root:?}, session={requested_id}"
            );
        }
    }

    /// The node-tree call map was removed on 2026-09-23; the Progress Flow
    /// view replaced it. Its three routes must not come back as a stray page,
    /// and nothing under them may read a transcript.
    #[tokio::test]
    async fn the_removed_agent_map_routes_are_not_served() {
        for path in [
            "/agents/map",
            "/agents/map/data",
            "/agents/map/prompt?session=x&agent=y",
        ] {
            let app = build_router(test_state(), false, true);
            let resp = app
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }

    /// Every `/progress/*` route, with a body its extractors accept.
    ///
    /// The bodies must be VALID. An extractor runs before the handler, so a
    /// malformed one answers 422 without ever reaching the auth check — and a
    /// mutant that removes the auth check answers 422 too. The test would then
    /// pass against both and prove nothing.
    fn progress_routes() -> Vec<(&'static str, &'static str, serde_json::Value)> {
        vec![
            (
                "POST",
                "/progress/report",
                serde_json::json!({"type": "done", "text": "routing only"}),
            ),
            (
                "POST",
                "/progress/list",
                serde_json::json!({"blockedOnly": false}),
            ),
            ("POST", "/progress/delete", serde_json::json!({"ids": []})),
            ("POST", "/progress/viewed", serde_json::Value::Null),
            ("POST", "/progress/flow", serde_json::json!({})),
            (
                "POST",
                "/progress/flow/detail",
                serde_json::json!({"ptyId": "p", "agentId": "a", "part": "prompt"}),
            ),
        ]
    }

    fn progress_request(
        method: &str,
        path: &str,
        body: &serde_json::Value,
        addr: std::net::SocketAddr,
    ) -> Request<Body> {
        let url = format!("{path}?path=/nonexistent-project-for-routing-only");
        let builder = Request::builder().method(method).uri(url);
        let mut req = if body.is_null() {
            builder.body(Body::empty()).expect("build request")
        } else {
            builder
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("build request with body")
        };
        req.extensions_mut().insert(ConnectInfo(addr));
        req
    }

    #[tokio::test]
    async fn story_action_route_enforces_auth_and_returns_a_created_plan() {
        let config = tempfile::tempdir().expect("config directory");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project = tempfile::tempdir().expect("project directory");
        let path = format!("/stories/action?path={}", project.path().display());
        let body = serde_json::json!({
            "action": {"action": "create_plan", "title": "Route plan", "source": "plan.md"}
        });
        let remote = std::net::SocketAddr::from(([203, 0, 113, 1], 4444));
        let denied = build_router(test_state(), false, true)
            .oneshot(mcp_post_from(&path, &body, remote))
            .await
            .expect("remote response");
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);

        let local = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
        let accepted = build_router(test_state(), false, true)
            .oneshot(mcp_post_from(&path, &body, local))
            .await
            .expect("local response");
        assert_eq!(accepted.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(accepted.into_body(), usize::MAX)
            .await
            .expect("read story response");
        let reply: serde_json::Value = serde_json::from_slice(&bytes).expect("story JSON");
        assert_eq!(reply["type"], "plan", "unexpected response {reply}");
        assert_eq!(reply["value"]["title"], "Route plan");
    }

    #[tokio::test]
    async fn markdown_image_route_serves_nested_images_and_rejects_escapes_and_unauthenticated_peers()
     {
        let parent = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let root = parent.path().join("repo");
        std::fs::create_dir_all(root.join("docs/images")).unwrap();
        std::fs::write(root.join("docs/images/chart.png"), [137, 80, 78, 71]).unwrap();
        std::fs::write(parent.path().join("outside.png"), [1, 2, 3]).unwrap();
        std::fs::write(root.join("docs/images/not-image.txt"), b"private").unwrap();
        std::fs::File::create(root.join("docs/images/huge.png"))
            .unwrap()
            .set_len(10 * 1024 * 1024 + 1)
            .unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            parent.path().join("outside.png"),
            root.join("docs/images/link.png"),
        )
        .unwrap();

        let uri = |file: &str, token: Option<&str>| {
            let mut params = url::form_urlencoded::Serializer::new(String::new());
            params
                .append_pair("repoPath", root.to_str().unwrap())
                .append_pair("file", file);
            if let Some(token) = token {
                params.append_pair("token", token);
            }
            format!("/fs/markdown-image?{}", params.finish())
        };
        let request = |path: String, addr: std::net::SocketAddr| {
            let mut req = Request::get(path)
                .header(header::HOST, "127.0.0.1:9876")
                .body(Body::empty())
                .unwrap();
            req.extensions_mut().insert(ConnectInfo(addr));
            req
        };
        let remote = std::net::SocketAddr::from(([203, 0, 113, 1], 4444));
        let local = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
        let state = test_state();
        let token = state.session_token.read().clone();
        let app = build_router(state, true, true);
        let denied = app
            .clone()
            .oneshot(request(uri("docs/images/chart.png", None), remote))
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

        // Loopback HTTP must authenticate in desktop and headless builds alike.
        let denied_local = app
            .clone()
            .oneshot(request(uri("docs/images/chart.png", None), local))
            .await
            .unwrap();
        assert_eq!(denied_local.status(), StatusCode::UNAUTHORIZED);

        let accepted_remote = app
            .clone()
            .oneshot(request(uri("docs/images/chart.png", Some(&token)), remote))
            .await
            .unwrap();
        assert_eq!(accepted_remote.status(), StatusCode::OK);

        let accepted = app
            .clone()
            .oneshot(request(uri("docs/images/chart.png", Some(&token)), local))
            .await
            .unwrap();
        assert_eq!(accepted.status(), StatusCode::OK);
        assert_eq!(accepted.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(
            accepted.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        let bytes = axum::body::to_bytes(accepted.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(bytes.as_ref(), &[137, 80, 78, 71]);
        for (path, expected) in [
            ("docs/images/../../../outside.png", StatusCode::FORBIDDEN),
            (
                "docs/images/not-image.txt",
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ),
            ("docs/images/missing.png", StatusCode::NOT_FOUND),
        ] {
            let response = app
                .clone()
                .oneshot(request(uri(path, Some(&token)), local))
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "wrong response for {path}");
        }
        let huge = app
            .clone()
            .oneshot(request(uri("docs/images/huge.png", Some(&token)), local))
            .await
            .unwrap();
        assert_eq!(huge.status(), StatusCode::PAYLOAD_TOO_LARGE);
        #[cfg(unix)]
        {
            let response = app
                .oneshot(request(uri("docs/images/link.png", Some(&token)), local))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
    }

    #[tokio::test]
    async fn loopback_story_action_without_session_is_local_api_not_human() {
        use crate::stories::{
            NewPlan, NewStory, StoryCommand, StoryOrigin, StoryStatus, StoryStore,
            StoryTransitionActor,
        };
        let config = tempfile::tempdir().expect("config");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project = tempfile::tempdir().expect("project");
        let store = StoryStore::open().expect("stories");
        let plan = store
            .create_plan(NewPlan {
                project: project
                    .path()
                    .canonicalize()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
                title: "Plan".into(),
                source: "plan.md".into(),
            })
            .unwrap();
        let story = store
            .create_story(NewStory {
                plan_id: plan.id.clone(),
                title: "Story".into(),
                criteria: vec!["Done".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .unwrap();
        let path = format!("/stories/action?path={}", project.path().display());
        let local = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
        let state = test_state();
        let token = state.session_token.read().clone();
        let app = build_router(state, false, true);
        let response = app.clone().oneshot(mcp_post_from(&path, &serde_json::json!({
            "action": { "action": "transition", "story_id": story.id, "expected_revision": story.revision, "command": "start_manual" }
        }), local)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let started = store.get_story(&story.id).unwrap();
        assert_eq!(
            store.transition_history(&story.id).unwrap()[0].actor,
            StoryTransitionActor::LocalApi
        );
        let checked = store
            .transition(&story.id, started.revision, StoryCommand::CheckCriterion(0))
            .unwrap();
        let review = store
            .transition(&story.id, checked.revision, StoryCommand::SubmitReview)
            .unwrap();
        let response = app.clone().oneshot(mcp_post_from(&path, &serde_json::json!({
            "action": { "action": "transition", "story_id": story.id, "expected_revision": review.revision, "command": "approve" }
        }), local)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            store.get_story(&story.id).unwrap().status,
            StoryStatus::Done
        );
        assert_eq!(
            store
                .transition_history(&story.id)
                .unwrap()
                .last()
                .unwrap()
                .actor,
            StoryTransitionActor::LocalApi
        );
        let other = store
            .create_story(NewStory {
                plan_id: plan.id,
                title: "Operator decision".into(),
                criteria: vec!["Done".into()],
                priority: 1,
                origin: StoryOrigin::Native,
                file_scope: vec![],
            })
            .unwrap();
        let body = serde_json::json!({"action": {
            "action": "transition", "story_id": other.id,
            "expected_revision": other.revision, "command": "block"
        }});
        let response = app
            .clone()
            .oneshot(mcp_post_from(&path, &body, local))
            .await
            .unwrap();
        // Catches: administrative local actions being refused or attributed to Human.
        assert_eq!(response.status(), StatusCode::OK);
        let blocked = store.get_story(&other.id).unwrap();
        assert_eq!(blocked.status, StoryStatus::Blocked);
        assert_eq!(
            store.transition_history(&other.id).unwrap()[0].actor,
            StoryTransitionActor::LocalApi
        );
        let body = serde_json::json!({"action": {
            "action": "transition", "story_id": other.id,
            "expected_revision": blocked.revision, "command": "unblock"
        }});
        let response = app
            .oneshot(mcp_post_from(
                &format!("{path}&token={token}"),
                &body,
                local,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            store.get_story(&other.id).unwrap().status,
            StoryStatus::Ready
        );
        let history = store.transition_history(&other.id).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[1].actor, StoryTransitionActor::Human);
    }

    #[tokio::test]
    async fn workflow_actor_metadata_does_not_block_local_actions() {
        // Catches: local requests failing actor authorization instead of normal missing-record validation.
        let config = tempfile::tempdir().expect("config");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project = tempfile::tempdir().expect("project");
        let state = test_state();
        // The operator API needs the daemon owner, independently of caller metadata.
        crate::workflows::WorkflowRuntime::spawn(&state);
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            while state.workflow_runtime.require_owner().is_err() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("daemon startup must acquire the run owner lock");
        let token = state.session_token.read().clone();
        let app = build_router(state, false, true);
        let local = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
        for (route, body) in [
            (
                "/workflows/definition/action",
                serde_json::json!({
                    "action": "publish", "id": "missing", "expected_revision": 1
                }),
            ),
            (
                "/workflows/run/action",
                serde_json::json!({
                    "action": "command", "run_id": "missing", "command_id": "decision",
                    "expected_sequence": 1, "command": {"action": "resume"}
                }),
            ),
        ] {
            let path = format!("{route}?path={}", project.path().display());
            let response = app
                .clone()
                .oneshot(mcp_post_from(&path, &body, local))
                .await
                .unwrap();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            assert!(String::from_utf8_lossy(&bytes).contains("not found"));
            let path = format!("{path}&token={token}");
            let response = app
                .clone()
                .oneshot(mcp_post_from(&path, &body, local))
                .await
                .unwrap();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            assert!(String::from_utf8_lossy(&bytes).contains("not found"));
        }
    }

    /// `edd69ea7` moved the Progress routes into `shared_routes()` so a
    /// `tuic-remote` daemon serves them, and asserted in prose that "auth is
    /// unchanged". Nothing tested it: `progress_auth` had no test anywhere, and
    /// mutating it to `None` — always authorised — survived the whole suite.
    #[tokio::test]
    async fn every_progress_route_rejects_a_remote_unauthenticated_caller() {
        // TEST-NET-3, so it can never be mistaken for loopback.
        let remote = std::net::SocketAddr::from(([203, 0, 113, 1], 4444));
        for (method, path, body) in progress_routes() {
            let app = build_router(test_state(), false, true);
            let resp = app
                .oneshot(progress_request(method, path, &body, remote))
                .await
                .expect("router responds");
            assert_eq!(
                resp.status(),
                StatusCode::FORBIDDEN,
                "{method} {path} must reject an unauthenticated non-loopback caller"
            );
        }
    }

    /// Kills the ten "replace the handler with `Default::default()`" mutants,
    /// which the route-parity gate cannot: that gate PATCH-probes for route
    /// EXISTENCE, so a route wired to a stub returning an empty 200 passes it.
    ///
    /// An empty body is the whole signal — `Response::default()` is 200 with no
    /// bytes, while a handler that ran returns either its payload or
    /// `{"error": ...}` from `err_500`. The project path does not exist on
    /// purpose: which of the two it returns is the store's business, and
    /// asserting it here would duplicate the store's own tests.
    #[tokio::test]
    async fn every_progress_route_runs_its_handler_for_a_loopback_caller() {
        let local = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
        for (method, path, body) in progress_routes() {
            let app = build_router(test_state(), false, true);
            let resp = app
                .oneshot(progress_request(method, path, &body, local))
                .await
                .expect("router responds");
            let status = resp.status();
            assert_ne!(
                status,
                StatusCode::FORBIDDEN,
                "{method} {path} must not reject a loopback caller"
            );
            let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .expect("read body");
            assert!(
                !bytes.is_empty(),
                "{method} {path} answered {status} with an empty body — the handler did not run"
            );
            serde_json::from_slice::<serde_json::Value>(&bytes)
                .unwrap_or_else(|e| panic!("{method} {path} answered {status}, not JSON: {e}"));
        }
    }

    #[tokio::test]
    async fn progress_http_returns_the_shared_durable_receipt_contract() {
        let config = tempfile::tempdir().unwrap();
        // The journal is one file in the config directory, so an un-overridden
        // run of this test would append to Boss's real history.
        let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project = tempfile::tempdir().unwrap();
        let state = test_state();
        let app = build_router(state.clone(), false, true);
        let body = serde_json::json!({
            "type": "done",
            "text": "HTTP transport is equivalent.",
            "step": "Progress"
        });
        let mut url = url::Url::parse("http://localhost/progress/report").unwrap();
        url.query_pairs_mut()
            .append_pair("path", &project.path().to_string_lossy());
        let uri = &url[url::Position::BeforePath..];
        let mut request = Request::post(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))));

        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let receipt: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(receipt["id"].is_i64());
        assert!(
            receipt.get("entry").is_none() && receipt.get("text").is_none(),
            "a receipt carries the identity and nothing else"
        );

        let owner = project.path().canonicalize().unwrap();
        let stored = crate::progress::ProgressStore::open()
            .unwrap()
            .list(&owner.to_string_lossy(), &Default::default())
            .unwrap();
        assert_eq!(stored.entries.len(), 1);
        assert_eq!(stored.entries[0].id, receipt["id"].as_i64().unwrap());
        assert_eq!(stored.entries[0].text, "HTTP transport is equivalent.");
    }

    /// A phone has no desktop active repository. The journal itself must name
    /// projects with entries, newest activity first, so an empty mobile tab
    /// cannot mistake "no selected project" for "no progress".
    #[tokio::test]
    async fn mobile_progress_lists_projects_with_recent_entries_first() {
        let config = tempfile::tempdir().unwrap();
        let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let store = crate::progress::ProgressStore::open().unwrap();
        for (project, text) in [("/older", "first done"), ("/newer", "later blocked")] {
            store
                .record(
                    project,
                    &crate::progress::NewProgressEntry {
                        kind: if project == "/older" {
                            crate::progress::ProgressKind::Done
                        } else {
                            crate::progress::ProgressKind::Blocked
                        },
                        text: text.into(),
                        step: None,
                        agent_name: None,
                    },
                )
                .unwrap();
        }
        let response = build_router(test_state(), false, true)
            .oneshot(get_localhost("/progress/projects"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let projects: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(projects, serde_json::json!(["/newer", "/older"]));
    }

    #[tokio::test]
    async fn shared_routes_surface_is_locked_and_desktop_only_excluded() {
        // Drift guard (#094-ec55): shared_routes() is the single source both build_router
        // and build_remote_router merge, so a shared route present in only one router is
        // impossible by construction. This test pins the shared SURFACE so (a) a shared
        // route silently dropped from shared_routes() is caught, and (b) a desktop-only
        // route accidentally added to shared_routes() — which would re-expose it on the
        // remote daemon — is caught. We probe with PATCH — a method NO shared route uses —
        // so a registered path returns 405 (method-not-allowed, router level) while an
        // unregistered path returns 404. This reflects ROUTE EXISTENCE only, never a
        // handler's own 404 (e.g. get_output 404s for a missing session, which would
        // false-fail a GET probe). Path params are filled with a placeholder segment.
        let must_exist = [
            "/api/version",
            // The remote client's only way onto a WebSocket: it trades Basic
            // Auth for the session token here, so the remote router must carry
            // it. `build_router` alone is exactly the Progress mistake below.
            "/api/auth/session-token",
            // The mobile login form's POST: the PWA of a `tuic-remote` host
            // needs it exactly as the desktop one does.
            "/auth/login",
            "/sessions",
            "/sessions/x/write",
            "/sessions/x/submit",
            "/sessions/x/output",
            "/sessions/x/terminal/scroll",
            "/sessions/x/terminal/lines",
            "/sessions/x/chat-view",
            "/sessions/agent",
            "/sessions/worktree",
            "/stats",
            "/metrics",
            "/process/stats",
            "/repo/info",
            "/repo/diff",
            "/repo/files",
            "/repo/branches",
            "/repo/commit",
            "/repo/stash",
            "/repo/gutter-changes",
            "/repo/create-branch",
            "/watchers/repo",
            "/watchers/dir",
            "/watchers/hot-repos",
            "/logs",
            "/diagnostics",
            "/worktrees",
            "/worktrees/x",
            "/agents",
            "/agents/detect",
            "/fs/list",
            "/fs/read",
            "/fs/markdown-image",
            "/fs/write",
            "/fs/write-if-unchanged",
            "/fs/stat",
            "/claude/usage",
            "/claude/projects",
            "/codex/usage",
            "/codex/stats",
            "/grok/usage",
            "/terminal/theme-colors",
            "/system/local-ip",
            "/system/home-directory",
            "/acp/workspace",
            "/acp/connections",
            "/acp/connections/x",
            "/acp/connections/x/reconnect",
            "/acp/connections/x/stream",
            "/acp/connections/x/sessions",
            "/acp/connections/x/sessions/y",
            "/acp/connections/x/sessions/y/prompt",
            "/acp/connections/x/sessions/y/config",
            "/acp/connections/x/sessions/y/pause",
            "/acp/connections/x/sessions/y/compact",
            "/acp/connections/x/interactions",
            "/acp/connections/x/permissions/y/response",
            "/acp/connections/x/elicitations/y/response",
            // Progress was in NEITHER list, which is how it shipped registered
            // on `build_router` alone: the guard only catches a path it names.
            // A feature absent from both lists is not "undecided", it is
            // unguarded — so pin every route, not a representative one.
            "/progress/report",
            "/progress/list",
            "/progress/projects",
            "/progress/delete",
            "/progress/viewed",
            "/progress/flow",
            "/progress/flow/detail",
            // The frontend reads a remote machine's agents.json (827c4bab);
            // absent here, tuic-remote answered 404 and remote repos ran with
            // no agent configs.
            "/config/agents",
        ];
        // Desktop-only or router-specific — MUST NOT be in shared_routes():
        // /health (public_routes only), /fs/read-editor (router-specific
        // handler down-scope), and every desktop-only family.
        let must_not_exist = [
            "/health",
            "/fs/read-editor",
            "/github/accounts",
            "/github/resolve-repo",
            "/repo/github",
            "/repo/prs",
            "/dictation/status",
            "/config",
            "/config/themes",
            "/mcp/status",
            "/plugins/list",
            "/api/push/test",
            "/prompt/process",
            "/debug/invoke_js",
            "/debug/reload_webview",
            "/exec/shell-script",
        ];
        let state = test_state();
        let app = shared_routes().with_state(state);
        for p in must_exist {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PATCH")
                        .uri(p)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_ne!(
                resp.status(),
                StatusCode::NOT_FOUND,
                "shared route missing from shared_routes(): {p}"
            );
        }
        for p in must_not_exist {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PATCH")
                        .uri(p)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::NOT_FOUND,
                "desktop-only/router-specific path leaked into shared_routes(): {p}"
            );
        }
    }

    /// Catches: the shared route exists but answers with a client-side or fixed root path.
    #[tokio::test]
    async fn home_directory_route_reports_serving_hosts_home() {
        let app = build_router(test_state(), false, true);
        let response = app
            .oneshot(
                Request::get("/system/home-directory")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let reported: String = serde_json::from_slice(&bytes).unwrap();
        let host_home = dirs::home_dir().expect("test host has a home directory");
        assert_eq!(reported, host_home.to_string_lossy());
    }

    /// Half two of the COMMAND_TABLE → router gate (story 643).
    ///
    /// The parity tests in `src/__tests__/transport.test.ts` assert the TABLE;
    /// nothing asserted that the paths it produces exist on the ROUTER, so a
    /// COMMAND_TABLE entry pointing at an unregistered path shipped green.
    ///
    /// The table is TypeScript and the router is Rust, so the two halves are
    /// bridged by `command_table_paths.txt`, which the Vitest half regenerates
    /// by EXECUTING every mapper. Reading that file here (rather than
    /// regex-scanning `transport.ts`, whose mappers use template literals,
    /// ternaries and query builders) is what lets this test actually fail:
    /// a new table entry makes the Vitest snapshot stale, and once regenerated
    /// its path lands here and must resolve.
    ///
    /// `INTENTIONALLY_UNMAPPED` commands never reach the file — they are not
    /// COMMAND_TABLE entries at all, which the Vitest half asserts explicitly.
    ///
    /// We probe `build_router`, not `shared_routes()`: COMMAND_TABLE is the
    /// desktop frontend's mapping and includes desktop-only families (`/config`,
    /// `/plugins`, `/github`, `/dictation`) that only `build_router`
    /// registers, so probing the shared subset alone would false-fail on every
    /// one of them. The remote router's narrower surface is already pinned by
    /// `shared_routes_surface_is_locked_and_desktop_only_excluded`.
    ///
    /// PATCH is the probe method because no route uses it, so a registered path
    /// answers 405 at the router level without ever running a handler — a GET
    /// probe would execute real handlers (`/repo/ci` shells out to `gh`,
    /// `/system/check-update` hits the network). An unregistered path falls
    /// through to the catch-all, which answers 404 under an API prefix and
    /// index.html otherwise; both are failures here, hence the HTML check.
    ///
    /// Limitation, stated rather than hidden: this proves the PATH is
    /// registered, not that it accepts the method the table declares. Probing
    /// the declared method would run the handler, which is what the technique
    /// exists to avoid.
    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn command_table_paths_all_hit_a_registered_route() {
        const PATHS: &str = include_str!("command_table_paths.txt");
        let paths: Vec<&str> = PATHS
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with('/'))
            .collect();
        // A truncated or emptied file would make every assertion below vacuous.
        assert!(
            paths.len() > 200,
            "command_table_paths.txt yielded only {} paths — regenerate it with \
             `pnpm vitest run src/__tests__/transport.test.ts -u`",
            paths.len()
        );

        let state = test_state();
        let app = build_router(state, false, true);
        let mut unrouted: Vec<String> = Vec::new();
        for path in paths {
            if !cfg!(feature = "dictation") && path.starts_with("/dictation/") {
                continue;
            }
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PATCH")
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let html = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|ct| ct.contains("text/html"));
            if resp.status() == StatusCode::NOT_FOUND || html {
                unrouted.push(format!("{path} -> {}", resp.status()));
            }
        }
        assert!(
            unrouted.is_empty(),
            "COMMAND_TABLE paths with no registered route: {unrouted:#?}\n\
             Every entry needs an axum route (AGENTS.md → IPC/HTTP Parity)."
        );
    }

    #[cfg(not(feature = "desktop"))]
    #[tokio::test]
    async fn desktop_only_command_table_paths_are_refused_by_headless_router() {
        let state = test_state();
        let app = build_router(state, false, true);
        for path in [
            "/dictation/status",
            "/github/accounts",
            "/system/check-update",
        ] {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PATCH")
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{path} should 404");
        }
    }

    #[tokio::test]
    async fn test_list_sessions_empty() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(Request::get("/sessions").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json, serde_json::json!([]));
    }

    /// A session running on a connected remote machine is one row of this list,
    /// tagged with the connection that owns it — the one field that tells it
    /// apart from a local row (#791-055e). The Tauri `list_active_sessions`
    /// answers from the same builder.
    #[tokio::test]
    async fn list_sessions_carries_a_mirrored_remote_session_and_names_its_machine() {
        let state = test_state();
        crate::remote_mirror::store_seed_for_test(
            &state,
            "vps",
            vec![crate::mcp_http::types::SessionInfo {
                session_id: "vps-sess".into(),
                display_name: Some("claude on the vps".into()),
                ..Default::default()
            }],
        );
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(Request::get("/sessions").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.as_array().expect("an array").len(), 1);
        assert_eq!(json[0]["session_id"], "vps-sess");
        assert_eq!(json[0]["connection_id"], "vps");
        assert_eq!(json[0]["display_name"], "claude on the vps");
    }

    /// A mirror row is a real session on its owner daemon, even though it has
    /// no local PTY. The phone's same-origin output request must reach that
    /// daemon, using the owner's credential rather than the caller's token.
    #[tokio::test]
    async fn mirrored_session_output_reaches_its_owner_without_forwarding_the_phone_token() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let remote = Router::new().route(
            "/sessions/remote-phone/output",
            get(|Query(query): Query<std::collections::HashMap<String, String>>| async move {
                if query.get("token").map(String::as_str) != Some("owner-secret") {
                    return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"bad token"})));
                }
                (
                    StatusCode::OK,
                    Json(serde_json::json!({"lines":[{"spans":[{"text":"remote line"}]}],"total_lines":1})),
                )
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, remote).await.unwrap() });
        let state = test_state();
        crate::remote_mirror::store_seed_for_test(
            &state,
            "owner",
            vec![types::SessionInfo {
                session_id: "remote-phone".into(),
                ..Default::default()
            }],
        );
        state.remote.force_connected_for_test(
            "owner",
            &format!("http://{addr}"),
            Some("owner-secret"),
        );
        let response = build_router(state, false, true)
            .oneshot(get_localhost(
                "/sessions/remote-phone/output?format=log&token=phone-secret",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let output: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(output["lines"][0]["spans"][0]["text"], "remote line");
        server.abort();
    }

    /// The kill button must terminate the mirrored session on its owner once.
    /// A local 404 leaves the remote process alive, which is Boss's symptom.
    #[tokio::test]
    async fn mirrored_session_close_reaches_owner_once() {
        let closes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::clone(&closes);
        let remote = Router::new().route(
            "/sessions/remote-phone",
            delete(move || {
                let seen = Arc::clone(&seen);
                async move {
                    seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Json(serde_json::json!({"ok":true}))
                }
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, remote).await.unwrap() });
        let state = test_state();
        crate::remote_mirror::store_seed_for_test(
            &state,
            "owner",
            vec![types::SessionInfo {
                session_id: "remote-phone".into(),
                ..Default::default()
            }],
        );
        state.remote.force_connected_for_test(
            "owner",
            &format!("http://{addr}"),
            Some("owner-secret"),
        );
        let response = build_router(state, false, true)
            .oneshot(
                Request::delete("/sessions/remote-phone")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(closes.load(std::sync::atomic::Ordering::SeqCst), 1);
        server.abort();
    }

    #[tokio::test]
    async fn disconnected_mirrored_session_reports_unavailable_instead_of_local_not_found() {
        let state = test_state();
        crate::remote_mirror::store_seed_for_test(
            &state,
            "owner",
            vec![types::SessionInfo {
                session_id: "remote-phone".into(),
                ..Default::default()
            }],
        );
        let response = build_router(state, false, true)
            .oneshot(get_localhost("/sessions/remote-phone/output?format=log"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn mirrored_session_websocket_streams_owner_log_frames() {
        use axum::extract::ws::{Message, WebSocketUpgrade};
        use futures_util::{SinkExt, StreamExt};

        let owner_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let owner_addr = owner_listener.local_addr().unwrap();
        let (input_sender, input_receiver) = tokio::sync::oneshot::channel();
        let input_sender = Arc::new(std::sync::Mutex::new(Some(input_sender)));
        let owner = Router::new().route(
            "/sessions/remote-phone/stream",
            get(move |Query(query): Query<std::collections::HashMap<String, String>>, ws: WebSocketUpgrade| {
                let input_sender = Arc::clone(&input_sender);
                async move {
                    if query.get("token").map(String::as_str) != Some("owner-secret") {
                        return StatusCode::UNAUTHORIZED.into_response();
                    }
                    ws.on_upgrade(move |mut socket| async move {
                        let _ = socket.send(Message::Text("{\"type\":\"log\",\"lines\":[{\"spans\":[{\"text\":\"live remote line\"}]}],\"total_lines\":1}".into())).await;
                        if let Some(Ok(Message::Text(input))) = socket.recv().await
                            && let Some(sender) = input_sender.lock().unwrap().take()
                        {
                            let _ = sender.send(input.to_string());
                        }
                    }).into_response()
                }
            }),
        );
        let owner_task =
            tokio::spawn(async move { axum::serve(owner_listener, owner).await.unwrap() });
        let state = test_state();
        crate::remote_mirror::store_seed_for_test(
            &state,
            "owner",
            vec![types::SessionInfo {
                session_id: "remote-phone".into(),
                ..Default::default()
            }],
        );
        state.remote.force_connected_for_test(
            "owner",
            &format!("http://{owner_addr}"),
            Some("owner-secret"),
        );
        let local_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_addr = local_listener.local_addr().unwrap();
        let local = tokio::spawn(async move {
            axum::serve(
                local_listener,
                build_router(state, false, true)
                    .into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap()
        });
        let (mut stream, _) = tokio_tungstenite::connect_async(format!(
            "ws://{local_addr}/sessions/remote-phone/stream?format=log"
        ))
        .await
        .expect("mirrored session must accept the phone WebSocket");
        let frame = tokio::time::timeout(std::time::Duration::from_secs(10), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(frame.into_text().unwrap().contains("live remote line"));
        stream
            .send(tokio_tungstenite::tungstenite::Message::Text(
                "phone input".into(),
            ))
            .await
            .unwrap();
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(10), input_receiver)
                .await
                .unwrap()
                .unwrap(),
            "phone input"
        );
        let _ = stream.close(None).await;
        local.abort();
        owner_task.abort();
    }

    #[tokio::test]
    async fn test_stats_no_sessions() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(Request::get("/stats").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["active_sessions"], 0);
        assert_eq!(json["max_sessions"], MAX_CONCURRENT_SESSIONS);
    }

    #[tokio::test]
    async fn test_metrics_initial() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["total_spawned"], 0);
        assert_eq!(json["active_sessions"], 0);
    }

    #[tokio::test]
    async fn test_session_not_found_404() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(
                Request::get("/sessions/nonexistent/output")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_config_roundtrip() {
        let state = test_state();

        // GET config
        let app = build_router(state.clone(), false, true);
        let resp = app.oneshot(get_localhost("/config")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // Should have default font_family
        assert!(config["font_family"].as_str().is_some());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn upstream_save_reports_same_id_concurrent_add_without_mutating_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());
        let current = crate::mcp_upstream_config::UpstreamMcpConfig {
            servers: vec![crate::mcp_upstream_config::UpstreamMcpServer {
                id: "shared-id".to_string(),
                name: "existing".to_string(),
                transport: crate::mcp_upstream_config::UpstreamTransport::Http {
                    url: "https://existing.example.com/mcp".to_string(),
                },
                enabled: false,
                timeout_secs: 30,
                tool_filter: None,
                auth: None,
            }],
        };
        crate::config::ConfigFile::<crate::mcp_upstream_config::UpstreamMcpConfig>::new(
            crate::mcp_upstream_config::UPSTREAMS_FILE,
        )
        .save(&current)
        .unwrap();

        let body = serde_json::json!({
            "base": { "servers": [] },
            "config": {
                "servers": [{
                    "id": "shared-id",
                    "name": "requested",
                    "transport": {
                        "type": "http",
                        "url": "https://requested.example.com/mcp"
                    },
                    "enabled": false,
                    "timeout_secs": 30
                }]
            }
        });
        let app = build_router(test_state(), false, true);
        let response = app
            .oneshot(put_from(
                "/mcp/upstreams",
                &body,
                std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            error["error"]
                .as_str()
                .is_some_and(|message| message.contains("was added concurrently"))
        );
        assert_eq!(crate::mcp_upstream_config::load_mcp_upstreams(), current);
    }

    // Catches: omitting the maintenance task leaves expired protocol sessions and orphan inboxes live.
    #[tokio::test(start_paused = true)]
    async fn maintenance_task_reaps_expired_sessions_and_orphan_inboxes() {
        let state = test_state();
        let now = std::time::Instant::now();
        for (sid, last_activity) in [
            ("expired", now - std::time::Duration::from_secs(7200)),
            ("fresh", now),
        ] {
            state.mcp.sessions.insert(
                sid.into(),
                crate::state::McpSessionMeta {
                    prompt_instructions: None,
                    last_activity,
                    is_claude_code: false,
                    requires_meta_tools: false,
                    has_sse_stream: false,
                    sse_generation: 0,
                    repo_path: None,
                },
            );
        }
        state
            .agent_inbox
            .insert("orphan".into(), std::collections::VecDeque::new());
        spawn_maintenance_sweep(&state);
        // Yield first so the sweep arms its own timer; no wall-clock startup deadline.
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_secs(61)).await;
        assert!(!state.mcp.sessions.contains_key("expired"));
        assert!(state.mcp.sessions.contains_key("fresh"));
        assert!(!state.agent_inbox.contains_key("orphan"));
    }

    // Catches: workflow route handlers return an empty default response instead of backend errors.
    #[tokio::test]
    async fn workflow_action_routes_preserve_project_validation_errors() {
        let app = build_router(test_state(), false, true);
        for (path, body) in [
            (
                "/workflows/definition/action?path=relative",
                r#"{"action":"list_drafts"}"#,
            ),
            (
                "/workflows/run/action?path=relative",
                r#"{"action":"get","run_id":"missing"}"#,
            ),
        ] {
            let mut req = Request::builder()
                .method("POST")
                .uri(path)
                .header(header::HOST, "127.0.0.1:9876")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap();
            req.extensions_mut()
                .insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))));
            let response = app.clone().oneshot(req).await.unwrap();
            assert_eq!(
                response.status(),
                StatusCode::INTERNAL_SERVER_ERROR,
                "{path}"
            );
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(body["error"], "project must be an absolute path");
        }
    }

    // Catches: removing the tunnel subrouter turns an implemented route into HTTP 404.
    #[tokio::test]
    async fn tunnel_subrouter_keeps_session_listing_available() {
        let mut req = Request::builder()
            .uri("/tunnels/active")
            .header(header::HOST, "127.0.0.1:9876")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))));
        let response = build_router(test_state(), false, true)
            .oneshot(req)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            serde_json::json!([])
        );
    }

    // Catches: arithmetic mutations under-size the documented 10 MiB base64 image plus JSON allowance.
    #[test]
    fn acp_prompt_cap_preserves_the_documented_image_and_framing_allowance() {
        assert_eq!(acp_prompt_body_limit(), 14_046_552);
    }

    /// The SSH host and agent-key listings disclose machine names and key
    /// fingerprints. Catches: `tunnel_routes()` being merged outside the
    /// Basic Auth layer so a public address reads them without credentials.
    #[tokio::test]
    async fn ssh_host_disclosure_routes_require_auth_from_a_public_address() {
        let remote = std::net::SocketAddr::from(([203, 0, 113, 1], 4444));
        let routers = [
            ("build_router", build_router(test_state(), true, true)),
            ("build_remote_router", build_remote_router(test_state())),
        ];
        for (name, app) in routers {
            for (method, path) in [
                ("GET", "/tunnels/ssh-hosts"),
                ("GET", "/tunnels/ssh-hosts/discovered"),
                ("GET", "/tunnels/ssh-hosts/status"),
                ("POST", "/tunnels/ssh-hosts/probe"),
                ("GET", "/tunnels/agent-keys"),
            ] {
                let mut req = Request::builder()
                    .header(header::HOST, "127.0.0.1:9876")
                    .method(method)
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap();
                req.extensions_mut().insert(ConnectInfo(remote));
                let response = app.clone().oneshot(req).await.unwrap();
                assert_eq!(
                    response.status(),
                    StatusCode::UNAUTHORIZED,
                    "{name} {path} must not answer an unauthenticated public address"
                );
            }
        }
    }

    /// A remote client cannot put an `Authorization` header on a WebSocket
    /// upgrade, so it trades Basic Auth for the session token once and then
    /// uses `?token=`. The trade must itself be authenticated: a public address
    /// with no credentials gets 401, never the token.
    #[tokio::test]
    async fn session_token_route_trades_basic_auth_for_the_token() {
        use axum::http::header;
        use base64::Engine;

        let state = test_state();
        let hash = bcrypt::hash("hunter2", 4).unwrap();
        {
            let mut cfg = state.config.write();
            cfg.services.auth.username = "boss".to_string();
            cfg.services.auth.password_hash = hash;
        }
        *state.session_token.write() = "the-live-token".to_string();

        let remote =
            || axum::extract::ConnectInfo(std::net::SocketAddr::from(([203, 0, 113, 5], 51234)));
        let app = build_router(state, true, true);

        let unauthenticated = app
            .clone()
            .oneshot(
                Request::get("/api/auth/session-token")
                    .header(header::HOST, "127.0.0.1:9876")
                    .extension(remote())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

        let credentials =
            base64::engine::general_purpose::STANDARD.encode(format!("boss:{}", "hunter2"));
        let authenticated = app
            .oneshot(
                Request::get("/api/auth/session-token")
                    .header(header::HOST, "127.0.0.1:9876")
                    .header(header::AUTHORIZATION, format!("Basic {credentials}"))
                    .extension(remote())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authenticated.status(), StatusCode::OK);
        let body = axum::body::to_bytes(authenticated.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            json.pointer("/token").and_then(serde_json::Value::as_str),
            Some("the-live-token")
        );
    }

    #[tokio::test]
    async fn test_config_strips_password_hash() {
        let state = test_state();
        {
            let mut cfg = state.config.write();
            cfg.services.auth.password_hash = "password-hash".to_string();
            cfg.services.auth.session_token = "session-secret".to_string();
            cfg.services.auth.session_token_exists = true;
            cfg.services.relay.token = "relay-secret".to_string();
            cfg.services.relay.token_exists = Some(true);
            cfg.services.push.vapid_private_key = "vapid-secret".to_string();
            cfg.services.push.vapid_private_key_exists = true;
        }
        let app = build_router(state, false, true);
        let resp = app.oneshot(get_localhost("/config")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            config.pointer("/services/auth/password_hash").is_none(),
            "Password hash should be stripped from HTTP response"
        );
        assert!(
            config.pointer("/services/auth/session_token").is_none(),
            "Session token should be stripped from HTTP response"
        );
        assert_eq!(
            config.pointer("/services/auth/session_token_exists"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(
            config.pointer("/services/relay/token").is_none(),
            "Relay token should be stripped from HTTP response"
        );
        assert_eq!(
            config.pointer("/services/relay/token_exists"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(
            config.pointer("/services/push/vapid_private_key").is_none(),
            "VAPID private key should be stripped from HTTP response"
        );
        assert_eq!(
            config.pointer("/services/push/vapid_private_key_exists"),
            Some(&serde_json::Value::Bool(true))
        );
    }

    #[tokio::test]
    async fn test_config_save_rejects_non_loopback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let remote_addr = std::net::SocketAddr::from(([192, 168, 1, 100], 12345));
        let body = config_save_body(
            serde_json::to_value(crate::config::AppConfig::default())
                .expect("serialize default AppConfig"),
        );
        let resp = app
            .oneshot(put_from("/config", &body, remote_addr))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "Config save from non-loopback address should be rejected"
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn stale_agents_http_saves_preserve_independent_changes() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let state = test_state();
        let app = build_router(state, false, true);
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
        let base = serde_json::json!({"agents": {}});
        let first =
            serde_json::json!({"base": base, "config": {"agents": {}, "headless_agent": "claude"}});
        let second = serde_json::json!({"base": base, "config": {"agents": {"codex": {"auto_retry_on_error": true}}}});

        let first_response = app
            .clone()
            .oneshot(put_from("/config/agents", &first, addr))
            .await
            .unwrap();
        assert_eq!(first_response.status(), StatusCode::OK);
        let second_response = app
            .oneshot(put_from("/config/agents", &second, addr))
            .await
            .unwrap();
        assert_eq!(second_response.status(), StatusCode::OK);

        let saved = crate::config::load_agents_config();
        assert_eq!(saved.headless_agent.as_deref(), Some("claude"));
        assert!(saved.agents.get("codex").unwrap().auto_retry_on_error);
    }

    #[tokio::test]
    async fn stale_remote_connection_http_saves_preserve_independent_fields() {
        let state = test_state();
        let app = build_router(state.clone(), false, true);
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
        let original = crate::remote_connection::RemoteConnection::new_direct(
            "original",
            "https://host.example",
            "alice",
        );
        let base = serde_json::to_value(&original).unwrap();
        let created = app
            .clone()
            .oneshot(put_from(
                "/config/remote-connections",
                &serde_json::json!({"base": null, "connection": base}),
                addr,
            ))
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::OK);

        let mut auto_update = base.clone();
        auto_update["auto_update"] = serde_json::json!(true);
        let mut renamed = base.clone();
        renamed["name"] = serde_json::json!("renamed");
        for desired in [auto_update, renamed] {
            let response = app
                .clone()
                .oneshot(put_from(
                    "/config/remote-connections",
                    &serde_json::json!({"base": base, "connection": desired}),
                    addr,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
        let saved = crate::remote_connection::RemoteConnectionStore::load(&state.data_dir).unwrap();
        let result = saved
            .iter()
            .find(|connection| connection.id == original.id)
            .unwrap();
        assert_eq!(result.name, "renamed");
        assert!(result.auto_update);
    }

    #[tokio::test]
    async fn test_notification_config_rejects_non_loopback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let remote_addr = std::net::SocketAddr::from(([192, 168, 1, 100], 12345));
        let body = config_save_body(
            serde_json::json!({"sound_enabled": false, "flash_enabled": false, "defer_secs": 10}),
        );
        let resp = app
            .oneshot(put_from("/config/notifications", &body, remote_addr))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "Notification config save from non-loopback should be rejected"
        );
    }

    #[tokio::test]
    async fn test_ui_prefs_rejects_non_loopback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let remote_addr = std::net::SocketAddr::from(([10, 0, 0, 1], 9999));
        let body = config_save_body(serde_json::json!({}));
        let resp = app
            .oneshot(put_from("/config/ui-prefs", &body, remote_addr))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "UI prefs save from non-loopback should be rejected"
        );
    }

    #[tokio::test]
    async fn test_repo_settings_rejects_non_loopback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let remote_addr = std::net::SocketAddr::from(([172, 16, 0, 5], 4000));
        let body = config_save_body(serde_json::json!({}));
        let resp = app
            .oneshot(put_from("/config/repo-settings", &body, remote_addr))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "Repo settings save from non-loopback should be rejected"
        );
    }

    #[tokio::test]
    async fn test_repositories_rejects_non_loopback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let remote_addr = std::net::SocketAddr::from(([192, 168, 1, 50], 8080));
        let body = serde_json::json!({});
        let resp = app
            .oneshot(put_from("/config/repositories", &body, remote_addr))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "Repositories save from non-loopback should be rejected"
        );
    }

    #[tokio::test]
    async fn test_prompt_library_rejects_non_loopback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let remote_addr = std::net::SocketAddr::from(([10, 10, 10, 1], 3000));
        let body = config_save_body(serde_json::json!({"prompts": []}));
        let resp = app
            .oneshot(put_from("/config/prompt-library", &body, remote_addr))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "Prompt library save from non-loopback should be rejected"
        );
    }

    #[tokio::test]
    async fn test_notes_rejects_non_loopback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let remote_addr = std::net::SocketAddr::from(([192, 168, 0, 1], 5000));
        let body = config_save_body(serde_json::json!({}));
        let resp = app
            .oneshot(put_from("/config/notes", &body, remote_addr))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "Notes save from non-loopback should be rejected"
        );
    }

    #[tokio::test]
    async fn test_read_external_rejects_path_outside_repos() {
        let state = test_state();
        let app = build_router(state, false, true);
        // No repos registered in test_state → any path should be rejected
        let resp = app
            .oneshot(
                Request::get("/fs/read-external?path=/etc/passwd")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "read-external should reject paths outside registered repos"
        );
    }

    // --- Path validation tests ---

    #[test]
    fn test_validate_repo_path_rejects_empty() {
        assert!(validate_repo_path("").is_err());
    }

    #[test]
    fn test_validate_repo_path_rejects_traversal() {
        assert!(validate_repo_path("/home/../etc/passwd").is_err());
        assert!(validate_repo_path("../../secret").is_err());
    }

    #[test]
    fn test_validate_repo_path_rejects_relative() {
        assert!(validate_repo_path("relative/path").is_err());
        assert!(validate_repo_path("C:relative/path").is_err());
    }

    #[test]
    fn test_validate_repo_path_accepts_absolute_unix() {
        assert!(validate_repo_path("/Users/test/repos/my-project").is_ok());
    }

    #[test]
    fn test_validate_repo_path_accepts_absolute_windows() {
        assert!(validate_repo_path("C:\\Users\\test\\repos").is_ok());
        assert!(validate_repo_path("C:/Users/test/repos").is_ok());
        assert!(validate_repo_path("c:/Users/test/repos").is_ok());
        assert!(validate_repo_path("\\\\server\\share").is_ok());
    }

    // --- Terminal size validation tests ---

    #[test]
    fn test_validate_terminal_size_rejects_zero_rows() {
        assert!(validate_terminal_size(0, 80).is_err());
    }

    #[test]
    fn test_validate_terminal_size_rejects_zero_cols() {
        assert!(validate_terminal_size(24, 0).is_err());
    }

    #[test]
    fn test_validate_terminal_size_rejects_oversized_rows() {
        assert!(validate_terminal_size(501, 80).is_err());
    }

    #[test]
    fn test_validate_terminal_size_rejects_oversized_cols() {
        assert!(validate_terminal_size(24, 501).is_err());
    }

    #[test]
    fn test_validate_terminal_size_accepts_valid() {
        assert!(validate_terminal_size(24, 80).is_ok());
        assert!(validate_terminal_size(1, 1).is_ok());
        assert!(validate_terminal_size(500, 500).is_ok());
    }

    // --- MCP repo path validation tests ---

    #[test]
    fn test_mcp_repo_path_rejects_empty() {
        assert!(mcp_transport::test_validate_mcp_repo_path("").is_err());
    }

    #[test]
    fn test_mcp_repo_path_rejects_traversal() {
        assert!(mcp_transport::test_validate_mcp_repo_path("/home/../etc/passwd").is_err());
    }

    #[test]
    fn test_mcp_repo_path_rejects_relative() {
        assert!(mcp_transport::test_validate_mcp_repo_path("relative/path").is_err());
    }

    #[test]
    fn test_mcp_repo_path_accepts_absolute() {
        assert!(mcp_transport::test_validate_mcp_repo_path("/Users/test/repo").is_ok());
    }

    #[tokio::test]
    async fn test_detect_agents() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(Request::get("/agents").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let agents = json.as_array().unwrap();
        let names: Vec<&str> = agents.iter().map(|a| a["name"].as_str().unwrap()).collect();
        // The route must report every agent TUIC can launch — an omission makes an installed
        // agent invisible to an orchestrator, which is how grok and gemini went missing.
        assert_eq!(names, crate::agent::KNOWN_AGENT_BINARIES.to_vec());
        assert!(names.contains(&"grok"));
    }

    #[tokio::test]
    async fn test_close_nonexistent_session() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(
                Request::delete("/sessions/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_pause_nonexistent_session() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(
                Request::post("/sessions/nonexistent/pause")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_resume_nonexistent_session() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(
                Request::post("/sessions/nonexistent/resume")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_mcp_initialize() {
        let state = test_state();
        let app = build_router(state, false, true);
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1.0" }
            }
        });
        let resp = app.oneshot(mcp_post("/mcp", &body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // Verify Mcp-Session-Id header is present
        let session_id = resp.headers().get("mcp-session-id");
        assert!(
            session_id.is_some(),
            "Initialize should return Mcp-Session-Id header"
        );
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        // Echoes back what the client asked for, not a fixed server version.
        assert_eq!(json["result"]["protocolVersion"], "2025-03-26");
        assert!(json["result"]["serverInfo"]["name"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_mcp_initialize_accepts_missing_and_present_protocol_header() {
        let cases = [(None, "2025-03-26"), (Some("2025-11-25"), "2025-11-25")];
        for (header_value, offered_version) in cases {
            let state = test_state();
            let app = build_router(state, false, true);
            let body = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": offered_version,
                    "capabilities": {},
                    "clientInfo": { "name": "probe-client", "version": "1.0.0" }
                }
            });
            let mut request = mcp_post("/mcp", &body);
            if let Some(value) = header_value {
                request.headers_mut().insert(
                    "MCP-Protocol-Version",
                    value.parse().expect("valid protocol header"),
                );
            }

            let response = app.oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK, "header={header_value:?}");
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(
                json["result"]["protocolVersion"], offered_version,
                "header={header_value:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_mcp_initialize_stores_claude_code_identity() {
        let state = test_state();
        let app = build_router(state.clone(), false, true);
        // Initialize with Claude Code client name
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26", "capabilities": {},
                "clientInfo": { "name": "claude-code", "version": "2.0" }
            }
        });
        let resp = app.oneshot(mcp_post("/mcp", &body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let sid = resp
            .headers()
            .get("mcp-session-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let meta = state
            .mcp
            .sessions
            .get(&sid)
            .expect("session should be stored");
        assert!(meta.is_claude_code, "Claude Code client should be detected");
    }

    #[tokio::test]
    async fn test_mcp_initialize_non_cc_client_not_flagged() {
        let state = test_state();
        let app = build_router(state.clone(), false, true);
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26", "capabilities": {},
                "clientInfo": { "name": "cursor", "version": "1.0" }
            }
        });
        let resp = app.oneshot(mcp_post("/mcp", &body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let sid = resp
            .headers()
            .get("mcp-session-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let meta = state
            .mcp
            .sessions
            .get(&sid)
            .expect("session should be stored");
        assert!(!meta.is_claude_code, "Non-CC client should not be flagged");
    }

    #[tokio::test]
    async fn test_grok_initialize_gets_session_local_meta_tool_surface() {
        let state = test_state();
        assert!(!state.config.read().collapse_tools);
        let app = build_router(state.clone(), false, true);
        let init_body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26", "capabilities": {},
                "clientInfo": {
                    "name": "grok-shell-tuicommander",
                    "version": "1.0"
                }
            }
        });
        let init_response = app.oneshot(mcp_post("/mcp", &init_body)).await.unwrap();
        assert_eq!(init_response.status(), StatusCode::OK);
        let sid = init_response
            .headers()
            .get("mcp-session-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let init_bytes = axum::body::to_bytes(init_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let init_json: serde_json::Value = serde_json::from_slice(&init_bytes).unwrap();
        assert!(
            init_json["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("search_tools")
        );
        assert!(
            state
                .mcp
                .sessions
                .get(&sid)
                .is_some_and(|meta| meta.requires_meta_tools)
        );

        let list_body = serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}
        });
        let list_response = build_router(state.clone(), false, true)
            .oneshot(mcp_post_with_session("/mcp", &list_body, &sid))
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);
        let list_bytes = axum::body::to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let list_json: serde_json::Value = serde_json::from_slice(&list_bytes).unwrap();
        let names: Vec<&str> = list_json["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        // `progress` survives the collapse on purpose — a worker must be able to
        // report even to a client that cannot hold the full tool surface. See
        // mcp_transport::tests::merged_tools_collapse_true_keeps_progress_directly_available.
        assert_eq!(
            names,
            vec!["search_tools", "get_tool_schema", "call_tool", "progress"]
        );
        assert!(!state.config.read().collapse_tools);
    }

    #[tokio::test]
    async fn test_mcp_initialize_with_roots_populates_repo_path() {
        let state = test_state();
        let app = build_router(state.clone(), false, true);
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26", "capabilities": {},
                "clientInfo": { "name": "claude-code", "version": "2.0" },
                "roots": [{ "uri": "file:///home/user/project" }]
            }
        });
        let resp = app.oneshot(mcp_post("/mcp", &body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let sid = resp
            .headers()
            .get("mcp-session-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let meta = state
            .mcp
            .sessions
            .get(&sid)
            .expect("session should be stored");
        assert_eq!(
            meta.repo_path.as_deref(),
            Some("/home/user/project"),
            "repo_path should be extracted from roots[0].uri"
        );
    }

    #[tokio::test]
    async fn test_mcp_initialize_without_roots_leaves_repo_path_none() {
        let state = test_state();
        let app = build_router(state.clone(), false, true);
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26", "capabilities": {},
                "clientInfo": { "name": "test", "version": "1.0" }
            }
        });
        let resp = app.oneshot(mcp_post("/mcp", &body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let sid = resp
            .headers()
            .get("mcp-session-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let meta = state
            .mcp
            .sessions
            .get(&sid)
            .expect("session should be stored");
        assert_eq!(
            meta.repo_path, None,
            "repo_path should be None when initialize has no roots"
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_mcp_tools_list_filtered_by_repo_mcp_upstreams() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());

        // Write repo-settings with mcp_upstreams allowlist for /test/repo
        let repo_settings = serde_json::json!({
            "repos": {
                "/test/repo": {
                    "path": "/test/repo",
                    "mcp_upstreams": ["allowed-server"]
                }
            }
        });
        std::fs::write(
            tmp.path().join("repo-settings.json"),
            serde_json::to_string_pretty(&repo_settings).unwrap(),
        )
        .unwrap();

        let state = test_state();

        // Register two upstream servers with tools
        state
            .mcp
            .upstream_registry
            .inject_ready_upstream("allowed-server", &["my_tool"]);
        state
            .mcp
            .upstream_registry
            .inject_ready_upstream("blocked-server", &["my_tool"]);

        // Initialize session with roots pointing to /test/repo
        let app = build_router(state.clone(), false, true);
        let init_body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26", "capabilities": {},
                "clientInfo": { "name": "test", "version": "1.0" },
                "roots": [{ "uri": "file:///test/repo" }]
            }
        });
        let resp = app.oneshot(mcp_post("/mcp", &init_body)).await.unwrap();
        let sid = resp
            .headers()
            .get("mcp-session-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        // Call tools/list with the session
        let app2 = build_router(state.clone(), false, true);
        let list_body = serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}
        });
        let resp = app2
            .oneshot(mcp_post_with_session("/mcp", &list_body, &sid))
            .await
            .unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tools = json["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();

        // allowed-server__my_tool should be present
        assert!(
            names.contains(&"allowed-server__my_tool"),
            "allowed upstream tool missing: {names:?}"
        );
        // blocked-server__my_tool should NOT be present
        assert!(
            !names.contains(&"blocked-server__my_tool"),
            "blocked upstream tool should be filtered: {names:?}"
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_mcp_tool_call_rejects_project_disabled_upstream() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());

        // Write repo-settings with mcp_upstreams allowlist excluding "blocked-server"
        let repo_settings = serde_json::json!({
            "repos": {
                "/test/repo": {
                    "path": "/test/repo",
                    "mcp_upstreams": ["other-server"]
                }
            }
        });
        std::fs::write(
            tmp.path().join("repo-settings.json"),
            serde_json::to_string_pretty(&repo_settings).unwrap(),
        )
        .unwrap();

        let state = test_state();
        state
            .mcp
            .upstream_registry
            .inject_ready_upstream("blocked-server", &["some_tool"]);

        // Initialize session with roots pointing to /test/repo
        let app = build_router(state.clone(), false, true);
        let init_body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26", "capabilities": {},
                "clientInfo": { "name": "test", "version": "1.0" },
                "roots": [{ "uri": "file:///test/repo" }]
            }
        });
        let resp = app.oneshot(mcp_post("/mcp", &init_body)).await.unwrap();
        let sid = resp
            .headers()
            .get("mcp-session-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        // Try to call a tool on the blocked upstream
        let app2 = build_router(state.clone(), false, true);
        let call_body = serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {
                "name": "blocked-server__some_tool",
                "arguments": {}
            }
        });
        let resp = app2
            .oneshot(mcp_post_with_session("/mcp", &call_body, &sid))
            .await
            .unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let text = json["result"]["content"][0]["text"].as_str().unwrap_or("");
        assert!(
            text.contains("not enabled"),
            "expected 'not enabled' error, got: {text}"
        );
        assert!(
            json["result"]["isError"].as_bool().unwrap_or(false),
            "should be an error response"
        );
    }

    #[tokio::test]
    async fn test_mcp_instructions_include_worktree_hint_for_cc() {
        let state = test_state();
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26", "capabilities": {},
                "clientInfo": { "name": "claude-code", "version": "2.0" }
            }
        });
        let app = build_router(state, false, true);
        let resp = app.oneshot(mcp_post("/mcp", &body)).await.unwrap();
        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        let instructions = json["result"]["instructions"].as_str().unwrap();
        assert!(
            instructions.contains("cc_agent_hint"),
            "CC instructions should mention cc_agent_hint: {instructions}"
        );
        assert!(
            instructions.contains("absolute paths"),
            "CC instructions should mention absolute paths"
        );
    }

    #[tokio::test]
    async fn test_mcp_get_without_session_returns_401() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(Request::get("/mcp").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_mcp_delete_session() {
        let state = test_state();
        let now = std::time::Instant::now();
        state.mcp.sessions.insert(
            "test-sid".to_string(),
            crate::state::McpSessionMeta {
                prompt_instructions: None,
                last_activity: now,
                is_claude_code: false,
                requires_meta_tools: false,
                has_sse_stream: false,
                sse_generation: 0,
                repo_path: None,
            },
        );
        let app = build_router(state.clone(), false, true);
        let mut req = Request::delete("/mcp")
            .header("mcp-session-id", "test-sid")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            state.mcp.sessions.get("test-sid").is_none(),
            "Session should be removed after DELETE"
        );
    }

    // Catches: tools/list re-advertises retired native actions or input aliases,
    // making clients send requests that the native dispatcher rejects.
    #[tokio::test]
    async fn test_mcp_tools_list() {
        let state = test_state();
        let app = build_router(state, false, true);
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });
        let resp = app.oneshot(mcp_post("/mcp", &body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tools = json["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"session"));
        assert!(names.contains(&"agent"));
        assert!(names.contains(&"repo"));
        assert!(names.contains(&"ui"));
        assert!(names.contains(&"plugin_dev_guide"));
        assert!(names.contains(&"config"));
        assert!(names.contains(&"debug"));
        // One family only: the ai_terminal_* tools were deleted in 789-f6ed.
        assert!(
            !names.iter().any(|n| n.starts_with("ai_terminal_")),
            "a second tool family is registered again: {names:?}"
        );
        assert_eq!(tools.len(), names.len());
        for (tool, retired_actions, retired_parameters) in [
            (
                "agent",
                &["detect", "stats", "metrics"][..],
                &["project"][..],
            ),
            ("session", &["process_stats"][..], &[][..]),
            (
                "repo",
                &["prs", "issues", "close_issue", "reopen_issue", "ci_logs"][..],
                &["workspace_id"][..],
            ),
        ] {
            let definition = tools.iter().find(|entry| entry["name"] == tool).unwrap();
            let properties = &definition["inputSchema"]["properties"];
            let actions = properties["action"]["description"]
                .as_str()
                .unwrap()
                .strip_prefix("One of: ")
                .unwrap();
            for action in retired_actions {
                assert!(
                    !actions.split(", ").any(|listed| listed == *action),
                    "tools/list advertises retired {tool} action {action}"
                );
            }
            for parameter in retired_parameters {
                assert!(
                    properties.get(*parameter).is_none(),
                    "tools/list advertises retired {tool} parameter {parameter}"
                );
            }
        }
    }

    #[tokio::test]
    async fn mcp_regression_ping_is_lightweight_and_refreshes_session() {
        let state = test_state();
        state.mcp.sessions.insert(
            "ping-session".to_string(),
            crate::state::McpSessionMeta {
                prompt_instructions: None,
                last_activity: std::time::Instant::now() - std::time::Duration::from_secs(60),
                is_claude_code: true,
                requires_meta_tools: false,
                has_sse_stream: false,
                sse_generation: 0,
                repo_path: None,
            },
        );
        let app = build_router(state.clone(), false, true);
        let body = serde_json::json!({"jsonrpc": "2.0", "id": 9, "method": "ping"});

        let resp = app
            .oneshot(mcp_post_with_session("/mcp", &body, "ping-session"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["result"], serde_json::json!({}));
        assert!(
            state
                .mcp
                .sessions
                .get("ping-session")
                .unwrap()
                .last_activity
                .elapsed()
                < std::time::Duration::from_secs(1)
        );
    }

    /// Catches: Settings inventories only enabled tools, loses newly registered
    /// tools, or offers switches which MCP discovery ignores.
    #[tokio::test]
    async fn native_settings_catalog_keeps_disabled_tools_and_disables_every_registry_tool() {
        let state = test_state();
        let definitions = mcp_transport::test_mcp_tool_definitions();
        let app = build_router(state.clone(), false, true);
        for tool in definitions.as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            {
                let mut cfg = state.config.write();
                cfg.disabled_native_tools = vec![name.to_owned()];
                cfg.collapse_tools = true;
                cfg.progress_tracking = false;
            }
            let response = app
                .clone()
                .oneshot(get_localhost("/mcp/status"))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let status: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let catalog = status["native_tools"].as_array().unwrap();
            assert_eq!(catalog.len(), definitions.as_array().unwrap().len());
            for (entry, definition) in catalog.iter().zip(definitions.as_array().unwrap()) {
                assert_eq!(entry["name"], definition["name"]);
                assert_eq!(entry["description"], definition["description"]);
                // Catches: HTTP status retaining description prefixes while Settings
                // switches to dedicated, concise native registry summaries.
                let summary = entry["summary"].as_str().unwrap();
                assert!(!summary.trim().is_empty());
                assert!(summary.chars().count() <= 70);
                assert!(
                    !definition["description"]
                        .as_str()
                        .unwrap()
                        .starts_with(summary)
                );
                if entry["name"] == "secret" {
                    assert_eq!(summary, "Request sensitive values from the user securely.");
                }
            }
            // Collapse/progress gates must not shrink the Settings inventory.
            {
                let mut cfg = state.config.write();
                cfg.collapse_tools = false;
                cfg.progress_tracking = true;
            }
            let response = app
                .clone()
                .oneshot(mcp_post(
                    "/mcp",
                    &serde_json::json!({
                        "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}
                    }),
                ))
                .await
                .unwrap();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let listing: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let listed = listing["result"]["tools"].as_array().unwrap();
            assert_eq!(listed.len(), catalog.len() - 1, "disabled {name}");
            assert!(
                listed.iter().all(|entry| entry["name"] != name),
                "disabled {name} remains discoverable"
            );
        }
    }

    /// Catches: a Settings switch hides critical tools but direct or collapsed
    /// calls still execute them (the stdio bridge forwards to this same route).
    #[tokio::test]
    async fn disabled_critical_native_tools_cannot_bypass_settings_through_dispatch() {
        let state = test_state();
        state.config.write().disabled_native_tools =
            vec!["session".into(), "agent".into(), "progress".into()];
        let app = build_router(state.clone(), false, true);
        for collapse in [false, true] {
            state.config.write().collapse_tools = collapse;
            for (name, arguments) in [
                ("session", serde_json::json!({"action": "list"})),
                ("agent", serde_json::json!({"action": "list_peers"})),
                (
                    "progress",
                    serde_json::json!({"type": "done", "message": "must not run"}),
                ),
            ] {
                for meta in [false, true] {
                    let params = if meta {
                        serde_json::json!({"name": "call_tool", "arguments": {
                            "tool_name": name, "arguments": arguments
                        }})
                    } else {
                        serde_json::json!({"name": name, "arguments": arguments})
                    };
                    let response = app
                        .clone()
                        .oneshot(mcp_post(
                            "/mcp",
                            &serde_json::json!({
                                "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": params
                            }),
                        ))
                        .await
                        .unwrap();
                    assert_eq!(response.status(), StatusCode::OK);
                    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                        .await
                        .unwrap();
                    let reply: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(
                        reply["result"]["isError"], true,
                        "{name} collapse={collapse} meta={meta}: {reply}"
                    );
                    let text = reply["result"]["content"][0]["text"].as_str().unwrap();
                    assert!(
                        text.contains(&format!("Tool '{name}' is disabled by configuration")),
                        "{reply}"
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn test_mcp_tools_list_respects_disabled_native_tools() {
        let state = test_state();
        state.config.write().disabled_native_tools = vec!["debug".to_string()];
        let app = build_router(state, false, true);
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });
        let resp = app.oneshot(mcp_post("/mcp", &body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tools = json["result"]["tools"].as_array().unwrap();
        let total = mcp_transport::test_mcp_tool_definitions()
            .as_array()
            .unwrap()
            .len();
        // total native − 1 disabled
        assert_eq!(tools.len(), total - 1);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(
            !names.contains(&"debug"),
            "disabled tool must not appear in tools/list"
        );
        assert!(names.contains(&"session"));
    }

    /// The count this used to assert (`>= 13`) was sized when a second tool
    /// family shared the list, and it asserted nothing about which tools are
    /// there. Names are the durable fact; the exact ordered list is pinned by
    /// `native_tool_definitions_are_the_one_surviving_family`.
    #[test]
    fn test_mcp_tool_definitions_carry_every_core_tool() {
        let tools = mcp_transport::test_mcp_tool_definitions();
        let names: Vec<&str> = tools
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        for core in [
            "session",
            "agent",
            "task",
            "repo",
            "progress",
            "ui",
            "config",
            "debug",
            "plugin_dev_guide",
        ] {
            assert!(
                names.contains(&core),
                "core tool {core} is missing: {names:?}"
            );
        }
    }

    #[test]
    fn test_translate_special_key() {
        assert_eq!(
            mcp_transport::test_translate_special_key("enter"),
            Some("\r")
        );
        assert_eq!(
            mcp_transport::test_translate_special_key("ctrl+c"),
            Some("\x03")
        );
        assert_eq!(mcp_transport::test_translate_special_key("tab"), Some("\t"));
        assert_eq!(
            mcp_transport::test_translate_special_key("up"),
            Some("\x1b[A")
        );
        assert_eq!(mcp_transport::test_translate_special_key("unknown"), None);
    }

    // --- MCP tool call tests ---
    // Test tool calls through the SSE transport (tools/call via /messages endpoint)

    /// Build a POST request to /mcp with an mcp-session-id header.
    fn mcp_post_with_session(
        url: &str,
        body: &serde_json::Value,
        session_id: &str,
    ) -> Request<Body> {
        let mut req = Request::post(url)
            .header("content-type", "application/json")
            .header("mcp-session-id", session_id)
            .body(Body::from(
                serde_json::to_string(body).expect("serialize JSON body"),
            ))
            .expect("build POST request");
        req.extensions_mut()
            .insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))));
        req
    }

    /// Initialize an MCP session and return the session ID.
    async fn mcp_initialize(state: &Arc<AppState>) -> String {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        let app = build_router(state.clone(), false, true);
        let resp = app.oneshot(mcp_post("/mcp", &body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        resp.headers()
            .get("mcp-session-id")
            .expect("initialize must return mcp-session-id")
            .to_str()
            .unwrap()
            .to_string()
    }

    /// Helper: send a tools/call MCP request via POST /mcp and return the parsed result content.
    /// Automatically initializes a session first to pass session validation.
    /// Initialize MCP session with a specific client name (for CC detection tests).
    async fn mcp_initialize_as(state: &Arc<AppState>, client_name: &str) -> String {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": client_name, "version": "1.0" }
            }
        });
        let app = build_router(state.clone(), false, true);
        let resp = app.oneshot(mcp_post("/mcp", &body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        resp.headers()
            .get("mcp-session-id")
            .expect("initialize must return mcp-session-id")
            .to_str()
            .unwrap()
            .to_string()
    }

    /// Send a tools/call MCP request with a specific session_id and return the parsed result.
    async fn call_mcp_tool_with_session(
        state: &Arc<AppState>,
        tool_name: &str,
        args: serde_json::Value,
        session_id: &str,
    ) -> serde_json::Value {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 99,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": args,
            }
        });
        let app = build_router(state.clone(), false, true);
        let resp = app
            .oneshot(mcp_post_with_session("/mcp", &body, session_id))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 99);

        let text = json["result"]["content"][0]["text"].as_str().unwrap();
        serde_json::from_str(text).unwrap_or_else(|_| serde_json::json!(text))
    }

    async fn call_mcp_tool(
        state: &Arc<AppState>,
        tool_name: &str,
        args: serde_json::Value,
    ) -> serde_json::Value {
        let session_id = mcp_initialize(state).await;
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 99,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": args,
            }
        });
        let app = build_router(state.clone(), false, true);
        let resp = app
            .oneshot(mcp_post_with_session("/mcp", &body, &session_id))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 99);

        // Parse the text content back to JSON
        let text = json["result"]["content"][0]["text"].as_str().unwrap();
        serde_json::from_str(text).unwrap_or_else(|_| serde_json::json!(text))
    }

    // --- Session meta-command tests ---

    #[tokio::test]
    async fn test_session_list_empty() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({"action": "list"})).await;
        let sessions = result.as_array().unwrap();
        assert!(sessions.is_empty(), "Expected empty sessions list");
    }

    #[tokio::test]
    async fn test_session_input_missing_session_id() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({"action": "input"})).await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("requires 'session_id'")
        );
    }

    #[tokio::test]
    async fn test_session_input_nonexistent_session() {
        let state = test_state();
        let result = call_mcp_tool(
            &state,
            "session",
            serde_json::json!({
                "action": "input",
                "session_id": "nonexistent",
                "input": "hello"
            }),
        )
        .await;
        assert_eq!(result["error"], "Session not found");
    }

    #[tokio::test]
    async fn test_session_input_no_input_or_key() {
        let state = test_state();
        let result = call_mcp_tool(
            &state,
            "session",
            serde_json::json!({
                "action": "input",
                "session_id": "some-id"
            }),
        )
        .await;
        assert!(result["error"].as_str().unwrap().contains("'input'"));
    }

    #[tokio::test]
    async fn test_session_input_unknown_special_key() {
        let state = test_state();
        let result = call_mcp_tool(
            &state,
            "session",
            serde_json::json!({
                "action": "input",
                "session_id": "some-id",
                "special_key": "nonexistent_key"
            }),
        )
        .await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("Unknown special key")
        );
    }

    #[tokio::test]
    async fn test_session_output_missing_session_id() {
        let state = test_state();
        let result =
            call_mcp_tool(&state, "session", serde_json::json!({"action": "output"})).await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("requires 'session_id'")
        );
    }

    #[tokio::test]
    async fn test_session_output_nonexistent_session() {
        let state = test_state();
        let result = call_mcp_tool(
            &state,
            "session",
            serde_json::json!({
                "action": "output",
                "session_id": "nonexistent"
            }),
        )
        .await;
        assert_eq!(result["error"], "Session not found");
    }

    #[tokio::test]
    async fn test_session_resize_missing_session_id() {
        let state = test_state();
        let result =
            call_mcp_tool(&state, "session", serde_json::json!({"action": "resize"})).await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("requires 'session_id'")
        );
    }

    #[tokio::test]
    async fn test_session_resize_nonexistent_session() {
        let state = test_state();
        let result = call_mcp_tool(
            &state,
            "session",
            serde_json::json!({
                "action": "resize",
                "session_id": "nonexistent",
                "rows": 40,
                "cols": 120
            }),
        )
        .await;
        assert_eq!(result["error"], "Session not found");
    }

    #[tokio::test]
    async fn test_session_close_missing_session_id() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({"action": "close"})).await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("requires 'session_id'")
        );
    }

    #[tokio::test]
    async fn test_session_close_nonexistent_is_idempotent() {
        // close is idempotent: returns ok even for unknown sessions (tombstone design)
        let state = test_state();
        let result = call_mcp_tool(
            &state,
            "session",
            serde_json::json!({
                "action": "close",
                "session_id": "nonexistent"
            }),
        )
        .await;
        assert_eq!(result["ok"], true);
    }

    #[tokio::test]
    async fn test_session_pause_missing_session_id() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({"action": "pause"})).await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("requires 'session_id'")
        );
    }

    #[tokio::test]
    async fn test_session_pause_nonexistent() {
        let state = test_state();
        let result = call_mcp_tool(
            &state,
            "session",
            serde_json::json!({
                "action": "pause",
                "session_id": "nonexistent"
            }),
        )
        .await;
        assert_eq!(result["error"], "Session not found");
    }

    #[tokio::test]
    async fn test_session_resume_missing_session_id() {
        let state = test_state();
        let result =
            call_mcp_tool(&state, "session", serde_json::json!({"action": "resume"})).await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("requires 'session_id'")
        );
    }

    #[tokio::test]
    async fn test_session_resume_nonexistent() {
        let state = test_state();
        let result = call_mcp_tool(
            &state,
            "session",
            serde_json::json!({
                "action": "resume",
                "session_id": "nonexistent"
            }),
        )
        .await;
        assert_eq!(result["error"], "Session not found");
    }

    #[tokio::test]
    async fn test_agent_spawn_missing_prompt() {
        let state = test_state();
        let result = call_mcp_tool(&state, "agent", serde_json::json!({"action": "spawn"})).await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("requires 'prompt'")
        );
    }

    #[tokio::test]
    async fn test_agent_spawn_unknown_binary() {
        let state = test_state();
        let result = call_mcp_tool(
            &state,
            "agent",
            serde_json::json!({
                "action": "spawn",
                "prompt": "test task",
                "agent_type": "nonexistent-agent"
            }),
        )
        .await;
        assert!(result["error"].as_str().unwrap().contains("not found"));
    }

    // --- Config meta-command tests ---

    #[tokio::test]
    async fn test_config_get() {
        let state = test_state();
        {
            let mut cfg = state.config.write();
            cfg.services.auth.password_hash = "password-hash".to_string();
            cfg.services.auth.session_token = "session-secret".to_string();
            cfg.services.auth.session_token_exists = true;
            cfg.services.relay.token = "relay-secret".to_string();
            cfg.services.relay.token_exists = Some(true);
            cfg.services.push.vapid_private_key = "vapid-secret".to_string();
            cfg.services.push.vapid_private_key_exists = true;
        }
        let result = call_mcp_tool(&state, "config", serde_json::json!({"action": "get"})).await;
        assert!(result["font_family"].as_str().is_some());
        assert!(
            result.pointer("/services/auth/password_hash").is_none(),
            "Password hash should be stripped from MCP tool response"
        );
        assert!(result.pointer("/services/auth/session_token").is_none());
        assert_eq!(
            result.pointer("/services/auth/session_token_exists"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(result.pointer("/services/relay/token").is_none());
        assert_eq!(
            result.pointer("/services/relay/token_exists"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(result.pointer("/services/push/vapid_private_key").is_none());
        assert_eq!(
            result.pointer("/services/push/vapid_private_key_exists"),
            Some(&serde_json::Value::Bool(true))
        );
    }

    #[tokio::test]
    async fn test_config_save_requires_base_and_config() {
        let state = test_state();
        let result = call_mcp_tool(&state, "config", serde_json::json!({"action": "save"})).await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("Invalid config save request")
        );
    }

    #[tokio::test]
    async fn test_config_save_invalid_config() {
        let state = test_state();
        let result = call_mcp_tool(
            &state,
            "config",
            serde_json::json!({
                "action": "save",
                "config": "not an object"
            }),
        )
        .await;
        assert!(result["error"].as_str().unwrap().contains("Invalid config"));
    }

    // --- Git meta-command tests ---

    #[tokio::test]
    async fn test_repo_worktree_list_missing_path() {
        let state = test_state();
        let result = call_mcp_tool(
            &state,
            "repo",
            serde_json::json!({"action": "worktree_list"}),
        )
        .await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("requires 'path'")
        );
    }

    #[tokio::test]
    async fn test_repo_worktree_remove_requires_branch() {
        let repo = create_temp_git_repo();
        let state = test_state();
        let result = call_mcp_tool(
            &state,
            "repo",
            serde_json::json!({
                "action": "worktree_remove",
                "path": repo.path().to_str().unwrap()
            }),
        )
        .await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("requires 'branch'")
        );
    }

    // Catches: worktree_remove demanding `branch`, so a detached checkout could
    // not be removed through MCP at all; and the path route skipping the orphan
    // safety verdict (untracked files must survive).
    #[tokio::test]
    async fn test_repo_worktree_remove_by_path_removes_clean_detached_and_refuses_untracked() {
        let repo = create_temp_git_repo();
        let state = test_state();
        let add = |name: &str| {
            let linked = repo.path().join(name);
            crate::git_cli::git_cmd(repo.path())
                .args([
                    "worktree",
                    "add",
                    "--detach",
                    linked.to_str().unwrap(),
                    "HEAD",
                ])
                .run()
                .unwrap();
            linked
        };
        let clean = add("clean");
        let dirty = add("dirty");
        std::fs::write(dirty.join("untracked.txt"), "keep me").unwrap();

        let removed = call_mcp_tool(
            &state,
            "repo",
            serde_json::json!({
                "action": "worktree_remove",
                "path": repo.path().to_str().unwrap(),
                "worktree_path": clean.to_str().unwrap()
            }),
        )
        .await;
        assert_eq!(removed["ok"], true, "{removed}");
        assert!(!clean.exists());

        let refused = call_mcp_tool(
            &state,
            "repo",
            serde_json::json!({
                "action": "worktree_remove",
                "path": repo.path().to_str().unwrap(),
                "worktree_path": dirty.to_str().unwrap()
            }),
        )
        .await;
        assert!(
            refused["error"].as_str().unwrap().contains("untracked"),
            "{refused}"
        );
        assert!(dirty.join("untracked.txt").exists());
    }

    // Catches: worktree_remove defaulting `force` to true (or ignoring its absence), which
    // would discard the uncommitted work of a branch worktree without confirmation.
    #[tokio::test]
    async fn test_repo_worktree_remove_without_force_refuses_a_dirty_branch_worktree() {
        let repo = create_temp_git_repo();
        let linked = repo.path().join("dirty-wt");
        crate::git_cli::git_cmd(repo.path())
            .args([
                "worktree",
                "add",
                "-b",
                "dirty-branch",
                linked.to_str().unwrap(),
                "HEAD",
            ])
            .run()
            .unwrap();
        std::fs::write(linked.join("work.txt"), "uncommitted").unwrap();

        let result = call_mcp_tool(
            &test_state(),
            "repo",
            serde_json::json!({
                "action": "worktree_remove",
                "path": repo.path().to_str().unwrap(),
                "branch": "dirty-branch"
            }),
        )
        .await;

        assert!(result["error"].is_string(), "{result}");
        assert!(linked.join("work.txt").exists());
    }

    #[tokio::test]
    async fn test_repo_worktree_remove_rejects_renamed_workspace_id() {
        let repo = create_temp_git_repo();
        let state = test_state();
        let result = call_mcp_tool(
            &state,
            "repo",
            serde_json::json!({
                "action": "worktree_remove",
                "path": repo.path().to_str().unwrap(),
                "workspace_id": "feat-x"
            }),
        )
        .await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("workspace_id was renamed to branch")
        );
    }

    // --- Action routing error tests ---

    #[tokio::test]
    async fn test_session_missing_action() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({})).await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("Missing 'action'")
        );
        assert!(result["error"].as_str().unwrap().contains("list, create"));
    }

    #[tokio::test]
    async fn test_session_unknown_action() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({"action": "bogus"})).await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("Unknown action 'bogus'")
        );
        assert!(result["error"].as_str().unwrap().contains("session"));
    }

    #[tokio::test]
    async fn test_repo_missing_action() {
        let state = test_state();
        let result = call_mcp_tool(&state, "repo", serde_json::json!({})).await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("Missing 'action'")
        );
    }

    #[tokio::test]
    async fn test_repo_unknown_action() {
        let state = test_state();
        let result = call_mcp_tool(&state, "repo", serde_json::json!({"action": "commit"})).await;
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("Unknown action 'commit'")
        );
    }

    /// Helper: create a temporary git repo with an initial commit for worktree tests.
    fn create_temp_git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(path)
            .output()
            .unwrap();
        std::fs::write(path.join("README.md"), "test").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(path)
            .output()
            .unwrap();
        dir
    }

    #[tokio::test]
    async fn orphan_cleanup_assessment_rejects_untracked_files() {
        let repo = create_temp_git_repo();
        let linked = repo.path().join("linked");
        crate::git_cli::git_cmd(repo.path())
            .args([
                "worktree",
                "add",
                "--detach",
                linked.to_str().unwrap(),
                "HEAD",
            ])
            .run()
            .unwrap();
        std::fs::write(linked.join("untracked.txt"), "keep me").unwrap();

        let response = build_router(test_state(), false, true)
            .oneshot(get_localhost(&format!(
                "/repo/orphan-cleanup-assessment?repoPath={}",
                repo.path().display()
            )))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let rows: serde_json::Value = serde_json::from_slice(&body).unwrap();
        #[cfg(unix)]
        let expected = format!("{}/linked", repo.path().canonicalize().unwrap().display());
        #[cfg(windows)]
        let expected = format!(
            "{}/linked",
            repo.path().display().to_string().replace('\\', "/")
        );
        assert_eq!(rows[0]["path"], expected);
        assert_eq!(
            std::path::Path::new(rows[0]["path"].as_str().unwrap())
                .canonicalize()
                .unwrap(),
            linked.canonicalize().unwrap()
        );
        assert_eq!(rows[0]["safe"], false);
        assert!(rows[0]["reason"].as_str().unwrap().contains("untracked"));
        assert!(linked.join("untracked.txt").exists());
    }

    #[tokio::test]
    async fn orphan_cleanup_assessment_rejects_commit_reachable_only_from_tag() {
        let repo = create_temp_git_repo();
        let linked = repo.path().join("linked");
        crate::git_cli::git_cmd(repo.path())
            .args([
                "worktree",
                "add",
                "-b",
                "only-tag",
                linked.to_str().unwrap(),
                "HEAD",
            ])
            .run()
            .unwrap();
        std::fs::write(linked.join("change.txt"), "unique commit").unwrap();
        crate::git_cli::git_cmd(&linked)
            .args(["add", "change.txt"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(&linked)
            .args(["commit", "-m", "unique"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(&linked)
            .args(["tag", "-a", "durable-tag", "-m", "preserve unique commit"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(&linked)
            .args(["checkout", "--detach", "HEAD"])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(repo.path())
            .args(["branch", "-D", "only-tag"])
            .run()
            .unwrap();

        let response = build_router(test_state(), false, true)
            .oneshot(get_localhost(&format!(
                "/repo/orphan-cleanup-assessment?repoPath={}",
                repo.path().display()
            )))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let rows: serde_json::Value = serde_json::from_slice(&body).unwrap();
        #[cfg(unix)]
        let expected = format!("{}/linked", repo.path().canonicalize().unwrap().display());
        #[cfg(windows)]
        let expected = format!(
            "{}/linked",
            repo.path().display().to_string().replace('\\', "/")
        );
        assert_eq!(rows[0]["path"], expected);
        assert_eq!(
            std::path::Path::new(rows[0]["path"].as_str().unwrap())
                .canonicalize()
                .unwrap(),
            linked.canonicalize().unwrap()
        );
        assert_eq!(rows[0]["safe"], false);
        assert!(rows[0]["reason"].as_str().unwrap().contains("branch"));

        let state = test_state();
        let pending = build_router(state.clone(), false, true)
            .oneshot(mcp_post(
                "/repo/orphan-cleanup/begin",
                &serde_json::json!({
                    "repoPath": repo.path().display().to_string(),
                    "paths": [linked.display().to_string()]
                }),
            ))
            .await
            .unwrap();
        assert_eq!(pending.status(), StatusCode::OK);
        let override_result = call_mcp_tool(
            &state,
            "repo",
            serde_json::json!({
                "action": "orphan_cleanup_answer",
                "path": repo.path().display().to_string(),
                "decision": "remove"
            }),
        )
        .await;
        assert!(
            override_result["error"]
                .as_str()
                .is_some_and(|error| error.contains("branch")),
            "{override_result}"
        );

        let removal = build_router(test_state(), false, true)
            .oneshot(mcp_post(
                "/repo/remove-orphan",
                &serde_json::json!({
                    "repoPath": repo.path().display().to_string(),
                    "worktreePath": linked.display().to_string(),
                    "safeOnly": true
                }),
            ))
            .await
            .unwrap();
        assert_eq!(removal.status(), StatusCode::BAD_REQUEST);
        assert!(linked.exists(), "tag-only orphan must survive safe removal");
    }

    /// A clean detached checkout with a live session registered in it.
    #[cfg(unix)]
    fn orphan_with_live_session() -> (tempfile::TempDir, std::path::PathBuf, Arc<AppState>) {
        let repo = create_temp_git_repo();
        let linked = repo.path().join("linked");
        crate::git_cli::git_cmd(repo.path())
            .args([
                "worktree",
                "add",
                "--detach",
                linked.to_str().unwrap(),
                "HEAD",
            ])
            .run()
            .unwrap();
        let state = test_state();
        crate::state::tests_support::insert_dummy_session(&state, "agent-1");
        crate::state::tests_support::set_session_cwd(&state, "agent-1", &linked.to_string_lossy());
        state
            .session_maps
            .sessions
            .get("agent-1")
            .unwrap()
            .lock()
            .display_name = Some("Claude: refactor".to_string());
        (repo, linked, state)
    }

    // Catches: a clean detached checkout reported safe (and auto-removed) while
    // an agent session is still working inside it.
    #[tokio::test]
    #[cfg(unix)]
    async fn orphan_cleanup_assessment_names_live_sessions_and_is_not_safe() {
        let (repo, linked, state) = orphan_with_live_session();

        let response = build_router(state, false, true)
            .oneshot(get_localhost(&format!(
                "/repo/orphan-cleanup-assessment?repoPath={}",
                repo.path().display()
            )))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let rows: serde_json::Value = serde_json::from_slice(&body).unwrap();
        #[cfg(unix)]
        let expected = format!("{}/linked", repo.path().canonicalize().unwrap().display());
        #[cfg(windows)]
        let expected = format!(
            "{}/linked",
            repo.path().display().to_string().replace('\\', "/")
        );
        assert_eq!(rows[0]["path"], expected);
        assert_eq!(
            std::path::Path::new(rows[0]["path"].as_str().unwrap())
                .canonicalize()
                .unwrap(),
            linked.canonicalize().unwrap()
        );
        assert_eq!(rows[0]["safe"], false);
        assert!(
            rows[0]["reason"]
                .as_str()
                .unwrap()
                .contains("Claude: refactor")
        );
        assert_eq!(rows[0]["live_sessions"][0]["session_id"], "agent-1");
    }

    // Catches: the agent/MCP "remove" answer and the safe-only HTTP removal
    // skipping the session registry that the assessment consults.
    #[tokio::test]
    #[cfg(unix)]
    async fn orphan_cleanup_answer_and_safe_removal_refuse_a_live_session() {
        let (repo, linked, state) = orphan_with_live_session();
        let pending = build_router(state.clone(), false, true)
            .oneshot(mcp_post(
                "/repo/orphan-cleanup/begin",
                &serde_json::json!({
                    "repoPath": repo.path().display().to_string(),
                    "paths": [linked.display().to_string()]
                }),
            ))
            .await
            .unwrap();
        assert_eq!(pending.status(), StatusCode::OK);

        let answer = call_mcp_tool(
            &state,
            "repo",
            serde_json::json!({
                "action": "orphan_cleanup_answer",
                "path": repo.path().display().to_string(),
                "decision": "remove"
            }),
        )
        .await;
        assert!(
            answer["error"]
                .as_str()
                .is_some_and(|error| error.contains("Claude") || error.contains("live session")),
            "{answer}"
        );

        let removal = build_router(state, false, true)
            .oneshot(mcp_post(
                "/repo/remove-orphan",
                &serde_json::json!({
                    "repoPath": repo.path().display().to_string(),
                    "worktreePath": linked.display().to_string(),
                    "safeOnly": true
                }),
            ))
            .await
            .unwrap();
        assert_eq!(removal.status(), StatusCode::BAD_REQUEST);
        assert!(
            linked.exists(),
            "a checkout with a live session must survive"
        );
    }

    // Catches: a confirmed (safeOnly=false) removal ignoring a session that
    // started after the user reviewed the dialog, or refusing the sessions the
    // user did see.
    #[tokio::test]
    #[cfg(unix)]
    async fn confirmed_orphan_removal_refuses_only_sessions_the_user_did_not_see() {
        let (repo, linked, state) = orphan_with_live_session();
        let remove = |sessions: serde_json::Value| {
            mcp_post(
                "/repo/remove-orphan",
                &serde_json::json!({
                    "repoPath": repo.path().display().to_string(),
                    "worktreePath": linked.display().to_string(),
                    "safeOnly": false,
                    "confirmedSessions": sessions,
                }),
            )
        };

        let unreviewed = build_router(state.clone(), false, true)
            .oneshot(remove(serde_json::json!([])))
            .await
            .unwrap();
        assert_eq!(unreviewed.status(), StatusCode::BAD_REQUEST);
        assert!(linked.exists());

        crate::state::tests_support::insert_dummy_session(&state, "agent-late");
        crate::state::tests_support::set_session_cwd(
            &state,
            "agent-late",
            &linked.to_string_lossy(),
        );
        let late = build_router(state.clone(), false, true)
            .oneshot(remove(serde_json::json!(["agent-1"])))
            .await
            .unwrap();
        assert_eq!(late.status(), StatusCode::BAD_REQUEST);
        assert!(linked.exists());

        let reviewed = build_router(state, false, true)
            .oneshot(remove(serde_json::json!(["agent-1", "agent-late"])))
            .await
            .unwrap();
        assert_eq!(reviewed.status(), StatusCode::OK);
        assert!(!linked.exists());
    }

    // Catches: a client that predates `confirmedSessions` (field absent) being treated as
    // having reviewed every session, so its removal closes a live checkout.
    #[tokio::test]
    #[cfg(unix)]
    async fn confirmed_orphan_removal_without_the_session_field_refuses_a_live_checkout() {
        let (repo, linked, state) = orphan_with_live_session();
        let response = build_router(state, false, true)
            .oneshot(mcp_post(
                "/repo/remove-orphan",
                &serde_json::json!({
                    "repoPath": repo.path().display().to_string(),
                    "worktreePath": linked.display().to_string(),
                    "safeOnly": false,
                }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(linked.exists());
    }

    // Catches: an id list that names sessions which do not (or no longer) live in the checkout
    // blocking a removal that has no live session at all.
    #[tokio::test]
    async fn confirmed_orphan_removal_ignores_stale_confirmed_ids() {
        let repo = create_temp_git_repo();
        let linked = repo.path().join("linked");
        crate::git_cli::git_cmd(repo.path())
            .args([
                "worktree",
                "add",
                "--detach",
                linked.to_str().unwrap(),
                "HEAD",
            ])
            .run()
            .unwrap();
        let response = build_router(test_state(), false, true)
            .oneshot(mcp_post(
                "/repo/remove-orphan",
                &serde_json::json!({
                    "repoPath": repo.path().display().to_string(),
                    "worktreePath": linked.display().to_string(),
                    "safeOnly": false,
                    "confirmedSessions": ["gone-1"],
                }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(!linked.exists());
    }

    // Catches: a path outside the repo's worktree list being refused as a server error (500)
    // instead of 400 on the confirmed (safeOnly=false) path.
    #[tokio::test]
    #[cfg(unix)]
    async fn confirmed_orphan_removal_of_an_unregistered_path_is_a_400() {
        let (repo, _linked, state) = orphan_with_live_session();
        let stranger = tempfile::tempdir().unwrap();
        let response = build_router(state, false, true)
            .oneshot(mcp_post(
                "/repo/remove-orphan",
                &serde_json::json!({
                    "repoPath": repo.path().display().to_string(),
                    "worktreePath": stranger.path().display().to_string(),
                    "safeOnly": false,
                    "confirmedSessions": [],
                }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(stranger.path().exists());
    }

    #[tokio::test]
    async fn orphan_cleanup_answer_accepts_clean_branch_reachable_worktree() {
        let repo = create_temp_git_repo();
        let linked = repo.path().join("linked");
        crate::git_cli::git_cmd(repo.path())
            .args([
                "worktree",
                "add",
                "--detach",
                linked.to_str().unwrap(),
                "HEAD",
            ])
            .run()
            .unwrap();
        let state = test_state();
        let pending = build_router(state.clone(), false, true)
            .oneshot(mcp_post(
                "/repo/orphan-cleanup/begin",
                &serde_json::json!({
                    "repoPath": repo.path().display().to_string(),
                    "paths": [linked.display().to_string()]
                }),
            ))
            .await
            .unwrap();
        assert_eq!(pending.status(), StatusCode::OK);

        let answer = call_mcp_tool(
            &state,
            "repo",
            serde_json::json!({
                "action": "orphan_cleanup_answer",
                "path": repo.path().display().to_string(),
                "decision": "remove"
            }),
        )
        .await;
        assert_eq!(answer["ok"], true, "{answer}");
        let read = build_router(state, false, true)
            .oneshot(get_localhost(&format!(
                "/repo/orphan-cleanup/pending?repoPath={}",
                repo.path().display()
            )))
            .await
            .unwrap();
        assert_eq!(read.status(), StatusCode::OK);
        let body = axum::body::to_bytes(read.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            true
        );
        assert!(
            linked.exists(),
            "answering only closes the dialog; UI decides removal"
        );
    }

    fn critic_detached_checkout(repo: &std::path::Path, name: &str) -> std::path::PathBuf {
        let linked = repo.join(name);
        crate::git_cli::git_cmd(repo)
            .args([
                "worktree",
                "add",
                "--detach",
                linked.to_str().unwrap(),
                "HEAD",
            ])
            .run()
            .unwrap();
        linked
    }

    async fn critic_remove_by_path(
        state: &Arc<AppState>,
        repo: &std::path::Path,
        worktree_path: &str,
    ) -> serde_json::Value {
        call_mcp_tool(
            state,
            "repo",
            serde_json::json!({
                "action": "worktree_remove",
                "path": repo.to_str().unwrap(),
                "worktree_path": worktree_path
            }),
        )
        .await
    }

    // Catches (critic-1367): the path route reaching `git worktree remove` for the main
    // checkout when it is the detached one (rebase/bisect/`checkout --detach` leave it
    // so), naming it from the repo itself or from a linked worktree as `path`.
    #[tokio::test]
    async fn worktree_remove_by_path_never_removes_the_main_checkout_even_when_detached() {
        let repo = create_temp_git_repo();
        let state = test_state();
        crate::git_cli::git_cmd(repo.path())
            .args(["checkout", "--detach"])
            .run()
            .unwrap();
        // The linked checkout lives inside the main one; hide it from `git status` so
        // the main checkout is clean and only the removal guard can stop the call.
        std::fs::write(repo.path().join(".git/info/exclude"), "caller/\n").unwrap();
        let caller = critic_detached_checkout(repo.path(), "caller");
        let main = repo.path().to_str().unwrap();

        let by_itself = critic_remove_by_path(&state, repo.path(), main).await;
        let by_a_linked_caller = critic_remove_by_path(&state, &caller, main).await;

        assert!(by_itself["error"].is_string(), "{by_itself}");
        assert!(
            by_a_linked_caller["error"].is_string(),
            "{by_a_linked_caller}"
        );
        assert!(repo.path().join("README.md").exists());
        assert!(repo.path().join(".git").exists());
        assert!(caller.exists());
    }

    // Catches (critic-1367): the path route skipping a check the sweep applies, so a
    // checkout with staged work, a commit no branch holds, or a branch attached to it
    // is removed through MCP.
    #[tokio::test]
    async fn worktree_remove_by_path_refuses_staged_work_unreachable_commits_and_branch_checkouts()
    {
        let repo = create_temp_git_repo();
        let state = test_state();
        let staged = critic_detached_checkout(repo.path(), "staged");
        std::fs::write(staged.join("new.txt"), "x").unwrap();
        crate::git_cli::git_cmd(&staged)
            .args(["add", "new.txt"])
            .run()
            .unwrap();
        let unreachable = critic_detached_checkout(repo.path(), "unreachable");
        std::fs::write(unreachable.join("only-here.txt"), "x").unwrap();
        crate::git_cli::git_cmd(&unreachable)
            .args(["add", "."])
            .run()
            .unwrap();
        crate::git_cli::git_cmd(&unreachable)
            .args(["commit", "-m", "only here"])
            .run()
            .unwrap();
        let attached = repo.path().join("attached");
        crate::git_cli::git_cmd(repo.path())
            .args([
                "worktree",
                "add",
                "-b",
                "attached",
                attached.to_str().unwrap(),
                "HEAD",
            ])
            .run()
            .unwrap();

        for (dir, needle) in [
            (&staged, "uncommitted"),
            (&unreachable, "unreachable"),
            (&attached, "not detached"),
        ] {
            let result = critic_remove_by_path(&state, repo.path(), dir.to_str().unwrap()).await;
            assert!(
                result["error"].as_str().is_some_and(|e| e.contains(needle)),
                "{}: {result}",
                dir.display()
            );
            assert!(dir.exists(), "{} must survive", dir.display());
        }
    }

    // Catches (critic-1367): `worktree_path` accepted without being matched against the
    // repo's own worktree list (a stranger directory, a relative path, an empty string,
    // a `..` hop out of a real checkout), which would delete anything the caller names.
    #[tokio::test]
    async fn worktree_remove_by_path_refuses_anything_that_is_not_a_registered_worktree() {
        let repo = create_temp_git_repo();
        let state = test_state();
        let linked = critic_detached_checkout(repo.path(), "linked");
        std::fs::create_dir(linked.join("sub")).unwrap();
        let stranger = tempfile::tempdir().unwrap();
        std::fs::write(stranger.path().join("precious.txt"), "x").unwrap();
        let candidates = [
            stranger.path().display().to_string(),
            "linked".to_string(),
            String::new(),
            format!("{}/..", linked.display()),
            linked.join("sub").display().to_string(),
        ];

        for candidate in candidates {
            let result = critic_remove_by_path(&state, repo.path(), &candidate).await;
            assert!(result["error"].is_string(), "{candidate:?}: {result}");
        }

        assert!(stranger.path().join("precious.txt").exists());
        assert!(linked.join("sub").exists());
        assert!(repo.path().join("README.md").exists());
    }

    // Catches (critic-1367): the path route bypassing the session registry, so an agent
    // working in the checkout loses its directory to another agent's cleanup.
    #[tokio::test]
    #[cfg(unix)]
    async fn worktree_remove_by_path_refuses_a_checkout_with_a_live_session() {
        let (repo, linked, state) = orphan_with_live_session();

        let result = critic_remove_by_path(&state, repo.path(), linked.to_str().unwrap()).await;

        assert!(
            result["error"]
                .as_str()
                .is_some_and(|e| e.contains("live session")),
            "{result}"
        );
        assert!(linked.exists());
    }

    // Catches (critic-1367): a checkout whose directory vanished being refused with
    // "needs force confirmation" on the path route, so the stale registration that
    // keeps the orphan listed can never be dropped through MCP.
    #[tokio::test]
    async fn worktree_remove_by_path_drops_the_registration_of_a_vanished_checkout() {
        let repo = create_temp_git_repo();
        let state = test_state();
        let linked = critic_detached_checkout(repo.path(), "vanished");
        std::fs::remove_dir_all(&linked).unwrap();

        let result = critic_remove_by_path(&state, repo.path(), linked.to_str().unwrap()).await;

        assert_eq!(result["ok"], true, "{result}");
        let listed = crate::git_cli::git_cmd(repo.path())
            .args(["worktree", "list", "--porcelain"])
            .run()
            .unwrap()
            .stdout;
        assert!(!listed.contains(linked.to_str().unwrap()), "{listed}");
    }

    #[tokio::test]
    async fn test_repo_worktree_create_cc_agent_hint_present_for_claude_code() {
        let repo = create_temp_git_repo();
        let repo_path = repo.path().to_str().unwrap();
        let state = test_state();
        let sid = mcp_initialize_as(&state, "claude-code").await;
        let result = call_mcp_tool_with_session(
            &state,
            "repo",
            serde_json::json!({
                "action": "worktree_create",
                "path": repo_path,
                "branch": "test-cc-hint"
            }),
            &sid,
        )
        .await;
        assert!(
            result.get("worktree_path").is_some(),
            "should have worktree_path: {result}"
        );
        let hint = &result["cc_agent_hint"];
        assert!(
            hint.is_object(),
            "CC client should receive cc_agent_hint: {result}"
        );
        assert!(
            hint["worktree_path"].as_str().is_some(),
            "hint should have worktree_path"
        );
        assert!(
            hint["suggested_prompt"]
                .as_str()
                .unwrap()
                .contains("absolute paths"),
            "hint prompt should mention absolute paths"
        );
        // Cleanup
        let wt_path = result["worktree_path"].as_str().unwrap();
        let _ = std::process::Command::new("git")
            .args(["worktree", "remove", "--force", wt_path])
            .current_dir(repo_path)
            .output();
    }

    #[tokio::test]
    async fn test_repo_worktree_create_no_cc_agent_hint_for_other_clients() {
        let repo = create_temp_git_repo();
        let repo_path = repo.path().to_str().unwrap();
        let state = test_state();
        let sid = mcp_initialize_as(&state, "cursor").await;
        let result = call_mcp_tool_with_session(
            &state,
            "repo",
            serde_json::json!({
                "action": "worktree_create",
                "path": repo_path,
                "branch": "test-no-hint"
            }),
            &sid,
        )
        .await;
        assert!(
            result.get("worktree_path").is_some(),
            "should have worktree_path: {result}"
        );
        assert!(
            result.get("cc_agent_hint").is_none(),
            "Non-CC client should NOT receive cc_agent_hint: {result}"
        );
        // Cleanup
        let wt_path = result["worktree_path"].as_str().unwrap();
        let _ = std::process::Command::new("git")
            .args(["worktree", "remove", "--force", wt_path])
            .current_dir(repo_path)
            .output();
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let state = test_state();
        let result = call_mcp_tool(&state, "nonexistent_tool", serde_json::json!({})).await;
        assert!(result["error"].as_str().unwrap().contains("Unknown tool"));
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("story, progress, ui")
        );
        assert!(result["error"].as_str().unwrap().contains("search_tools"));
    }

    // --- isError flag tests ---

    #[tokio::test]
    async fn test_tool_call_is_error_flag() {
        let state = test_state();
        let session_id = mcp_initialize(&state).await;
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 50,
            "method": "tools/call",
            "params": {
                "name": "nonexistent_tool",
                "arguments": {}
            }
        });
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(mcp_post_with_session("/mcp", &body, &session_id))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(
            json["result"]["isError"], true,
            "Error responses should set isError=true"
        );
    }

    #[tokio::test]
    async fn test_tool_call_success_flag() {
        let state = test_state();
        let session_id = mcp_initialize(&state).await;
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 51,
            "method": "tools/call",
            "params": {
                "name": "session",
                "arguments": {"action": "list"}
            }
        });
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(mcp_post_with_session("/mcp", &body, &session_id))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(
            json["result"]["isError"], false,
            "Success responses should set isError=false"
        );
    }

    #[tokio::test]
    async fn test_mcp_unknown_method() {
        let state = test_state();
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 60,
            "method": "resources/list",
            "params": {}
        });
        let app = build_router(state, false, true);
        let resp = app.oneshot(mcp_post("/mcp", &body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(
            json["error"]["code"], -32601,
            "Unknown method should return -32601"
        );
        assert!(
            json["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Method not found")
        );
    }

    /// Was `test_tools_call_requires_session`, which pinned the blanket -32600 on a
    /// missing `mcp-session-id`. That rejection was the single blocker to stateless
    /// operation and is gone (#0f44); identity is now resolved per call. The test is
    /// inverted rather than dropped, so the route stays covered end-to-end.
    #[tokio::test]
    async fn test_tools_call_no_longer_requires_a_session_header() {
        let state = test_state();
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 70,
            "method": "tools/call",
            "params": {
                "name": "session",
                "arguments": {"action": "list"}
            }
        });
        let app = build_router(state, false, true);
        // No mcp-session-id header — `session action=list` needs no identity.
        let resp = app.oneshot(mcp_post("/mcp", &body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert!(
            json.get("error").is_none(),
            "a headerless call must not be refused: {json}"
        );
        assert!(
            json["result"]["content"][0]["text"].is_string(),
            "the tool must actually have run: {json}"
        );
    }

    // --- Auth validation tests ---
    // Tests for the pure validate_basic_auth function (no server needed).

    fn test_hash(password: &str) -> String {
        bcrypt::hash(password, 4).unwrap() // cost=4 for fast tests
    }

    fn basic_header(username: &str, password: &str) -> String {
        use base64::Engine;
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        format!("Basic {encoded}")
    }

    #[test]
    fn test_auth_valid_credentials() {
        let hash = test_hash("secret123");
        let header = basic_header("admin", "secret123");
        assert!(matches!(
            auth::validate_basic_auth(Some(&header), "admin", &hash),
            auth::AuthResult::Ok
        ));
    }

    #[test]
    fn test_auth_wrong_password() {
        let hash = test_hash("secret123");
        let header = basic_header("admin", "wrongpass");
        assert!(matches!(
            auth::validate_basic_auth(Some(&header), "admin", &hash),
            auth::AuthResult::Invalid
        ));
    }

    #[test]
    fn test_auth_wrong_username() {
        let hash = test_hash("secret123");
        let header = basic_header("hacker", "secret123");
        assert!(matches!(
            auth::validate_basic_auth(Some(&header), "admin", &hash),
            auth::AuthResult::Invalid
        ));
    }

    #[test]
    fn test_auth_missing_header() {
        let hash = test_hash("secret123");
        assert!(matches!(
            auth::validate_basic_auth(None, "admin", &hash),
            auth::AuthResult::MissingHeader
        ));
    }

    #[test]
    fn test_auth_not_configured() {
        assert!(matches!(
            auth::validate_basic_auth(Some("Basic dGVzdDp0ZXN0"), "", ""),
            auth::AuthResult::NotConfigured
        ));
    }

    #[test]
    fn test_auth_invalid_scheme() {
        let hash = test_hash("secret123");
        assert!(matches!(
            auth::validate_basic_auth(Some("Bearer token123"), "admin", &hash),
            auth::AuthResult::Invalid
        ));
    }

    #[test]
    fn test_auth_invalid_base64() {
        let hash = test_hash("secret123");
        assert!(matches!(
            auth::validate_basic_auth(Some("Basic !!!invalid!!!"), "admin", &hash),
            auth::AuthResult::Invalid
        ));
    }

    #[test]
    fn test_auth_no_colon_separator() {
        let hash = test_hash("secret123");
        // base64 of "nocolon" (no colon separator)
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, "nocolon");
        let header = format!("Basic {encoded}");
        assert!(matches!(
            auth::validate_basic_auth(Some(&header), "admin", &hash),
            auth::AuthResult::Invalid
        ));
    }

    // --- Static file serving tests ---

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_serve_index_html() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("text/html"), "Expected text/html, got {ct}");
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(
            text.contains("<!DOCTYPE html>") || text.contains("<html"),
            "index.html should contain HTML"
        );
    }

    // The embedded dist only exists in the desktop build; without this gate
    // `cargo check --no-default-features --tests` cannot compile at all, which
    // hides every other cfg regression in test code.
    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_serve_static_js() {
        // Find an actual JS file in the embedded dist
        let js_file = static_files::FRONTEND_DIST
            .find("assets/*.js")
            .expect("glob should work")
            .next();
        if let Some(entry) = js_file {
            let path = format!("/{}", entry.path().display());
            let state = test_state();
            let app = build_router(state, false, true);
            let resp = app
                .oneshot(Request::get(&path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
            let ct = resp
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap();
            assert!(
                ct.contains("javascript"),
                "Expected javascript MIME, got {ct}"
            );
        }
    }

    // The embedded dist only exists in the desktop build; without this gate
    // `cargo check --no-default-features --tests` cannot compile at all, which
    // hides every other cfg regression in test code.
    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_serve_static_font() {
        let font_file = static_files::FRONTEND_DIST
            .find("fonts/*.woff2")
            .expect("glob should work")
            .next();
        if let Some(entry) = font_file {
            let path = format!("/{}", entry.path().display());
            let state = test_state();
            let app = build_router(state, false, true);
            let resp = app
                .oneshot(Request::get(&path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
            let ct = resp
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap();
            assert!(
                ct.contains("font") || ct.contains("woff2") || ct.contains("octet-stream"),
                "Expected font MIME, got {ct}"
            );
        }
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn test_spa_fallback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(
                Request::get("/some/unknown/spa/route")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            ct.contains("text/html"),
            "SPA fallback should return HTML, got {ct}"
        );
    }

    #[cfg(not(feature = "desktop"))]
    #[tokio::test]
    async fn headless_router_refuses_embedded_frontend_and_spa_paths() {
        let state = test_state();
        let app = build_router(state, false, true);
        for path in ["/", "/some/unknown/spa/route", "/settings"] {
            let resp = app
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{path} should 404");
        }
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn unknown_api_path_404s_while_spa_deep_links_still_load() {
        // A path under an API prefix that matched no route is a missing route,
        // not an SPA deep link. Serving index.html with 200 makes `rpc()` see a
        // success and then choke parsing HTML as a command result, which hides
        // every unregistered route (audit §2 #1).
        let state = test_state();
        let app = build_router(state, false, true);
        for p in [
            "/sessions/abc/no-such-endpoint",
            "/repo/no-such-endpoint",
            "/system/no-such-endpoint",
            "/fs/no-such-endpoint",
            "/worktrees/a/b/c",
            "/api/no-such-endpoint",
            "/attachments/no-such-endpoint",
            "/dictation/no-such-endpoint",
        ] {
            let resp = app
                .clone()
                .oneshot(Request::get(p).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{p} should 404");
            let ct = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            assert!(
                ct.contains("application/json"),
                "{p} should answer JSON, got {ct}"
            );
            let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap();
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert!(
                json["error"].is_string(),
                "{p} should carry an error message, got {json}"
            );
        }
        // Deep links outside the API surface keep booting the SPA shell.
        for p in ["/settings", "/some/unknown/spa/route", "/mobile/session/x"] {
            let resp = app
                .clone()
                .oneshot(Request::get(p).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK, "{p} should serve the SPA");
            let ct = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            assert!(ct.contains("text/html"), "{p} should be HTML, got {ct}");
        }
    }

    /// Every `COMMAND_TABLE` path in this story's scope must resolve to a route.
    ///
    /// Probing is deliberately side-effect free: a POST/PUT carries syntactically
    /// invalid JSON, so the extractor rejects it before the handler runs and
    /// `/worktrees/run-script` never runs a shell nor the sound route a tone.
    ///
    /// The assertion is "neither 404 nor 405" because a missing route takes both
    /// shapes here. A GET falls through to the SPA catch-all, which answers 404
    /// for an API prefix; any other method hits that same GET-only catch-all and
    /// answers 405 — and where a same-prefix route with another method absorbs the
    /// path (`/worktrees/{workspace_id}` is DELETE) the 405 comes from there instead.
    /// Asserting only `!= 404` would therefore pass on every missing POST route.
    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn every_dictation_and_os_integration_path_has_a_route() {
        // (method, path) mirrors src/transport.ts COMMAND_TABLE.
        const PROBES: &[(&str, &str)] = &[
            ("GET", "/dictation/status"),
            ("GET", "/dictation/models"),
            ("POST", "/dictation/models/download"),
            ("POST", "/dictation/models/delete"),
            ("POST", "/dictation/start"),
            ("POST", "/dictation/stop"),
            ("GET", "/dictation/corrections"),
            ("PUT", "/dictation/corrections"),
            ("GET", "/dictation/devices"),
            ("POST", "/dictation/inject"),
            ("GET", "/dictation/config"),
            ("PUT", "/dictation/config"),
            ("GET", "/dictation/speech/voices?language=it"),
            ("POST", "/dictation/speech/voices/import"),
            ("POST", "/dictation/speech/voices/delete"),
            ("POST", "/agents/open-in-app"),
            ("POST", "/agents/detect-all"),
            ("POST", "/system/notification-sound"),
            ("GET", "/system/relay-status"),
            ("GET", "/system/check-update"),
            ("GET", "/sessions/probe-session/shell-family"),
            ("POST", "/worktrees/run-script"),
        ];

        let state = test_state();
        let app = build_router(state, false, true);
        for (method, path) in PROBES {
            if !cfg!(feature = "dictation") && path.starts_with("/dictation/") {
                continue;
            }
            let req = if *method == "GET" {
                Request::get(*path).body(Body::empty()).unwrap()
            } else {
                Request::builder()
                    .method(*method)
                    .uri(*path)
                    .header("content-type", "application/json")
                    .body(Body::from("["))
                    .unwrap()
            };
            let status = app.clone().oneshot(req).await.unwrap().status();
            assert!(
                status != StatusCode::NOT_FOUND && status != StatusCode::METHOD_NOT_ALLOWED,
                "{method} {path} resolves to no route (got {status})"
            );
        }
    }

    /// Catches: a device route answering 200 with an error body (a plain `Json(..)`
    /// instead of `json_result`) when the enumeration fails, which the client
    /// would parse as a device list. The injected failure panics inside the real
    /// blocking task, independent of enumeration speed; no mock layer.
    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn the_device_routes_answer_500_when_the_enumeration_fails() {
        let _failure = crate::audio_enumeration::fail_enumeration_for_test();
        let app = build_router(test_state(), false, true);
        for path in ["/dictation/devices", "/audio/output-devices"] {
            if !cfg!(feature = "dictation") && path.starts_with("/dictation/") {
                continue;
            }
            let response = app
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::INTERNAL_SERVER_ERROR,
                "GET {path}"
            );
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert!(
                json["error"].as_str().is_some_and(|e| e.contains("failed")),
                "GET {path}: {json}"
            );
        }
    }

    /// Drift guard for `API_PREFIXES`: the catch-all decides 404-vs-index.html
    /// from that list, so a route family nobody adds to it silently goes back to
    /// answering HTML for its own typos. Derive the truth from the registered
    /// routes instead of trusting the list: every `.route(...)`/`.nest(...)` path
    /// literal in this file — every route this server serves is declared here —
    /// must have its first segment covered.
    #[test]
    fn api_prefixes_cover_every_registered_route() {
        const SRC: &str = include_str!("mod.rs");
        let prod = SRC
            .split("#[cfg(test)]\nmod tests {")
            .next()
            .expect("source has a production half");
        // `tunnel_routes()` is merged with `.nest("/tunnels", ...)`, so its own
        // literals are relative to that prefix — skip its body, the nest covers it.
        let (head, rest) = prod
            .split_once("fn tunnel_routes()")
            .expect("tunnel_routes() is declared here");
        let (_body, tail) = rest.split_once("\n}\n").expect("tunnel_routes() body ends");

        fn route_call_end(line: &str) -> Option<usize> {
            let a = line.find(".route(").map(|i| i + ".route(".len());
            let b = line.find(".nest(").map(|i| i + ".nest(".len());
            match (a, b) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            }
        }
        fn quoted_path(s: &str) -> Option<&str> {
            let start = s.find('"')? + 1;
            let end = start + s[start..].find('"')?;
            let lit = &s[start..end];
            lit.starts_with('/').then_some(lit)
        }

        let mut registered: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for source in [head, tail] {
            let mut awaiting = false;
            for line in source.lines() {
                let mut rest = line;
                if let Some(i) = route_call_end(line) {
                    rest = &line[i..];
                    awaiting = true;
                }
                if awaiting && let Some(path) = quoted_path(rest) {
                    awaiting = false;
                    let seg = path.trim_start_matches('/').split('/').next().unwrap_or("");
                    // "" is `/` itself; `{*path}` is the catch-all being guarded.
                    if !seg.is_empty() && !seg.starts_with('{') {
                        registered.insert(seg);
                    }
                }
            }
        }

        // A scanner that silently matches nothing would pass every assertion below.
        assert!(
            registered.len() > 20,
            "route scanner found only {registered:?} — it stopped matching the source"
        );
        let missing: Vec<&str> = registered
            .iter()
            .copied()
            .filter(|seg| !API_PREFIXES.contains(seg))
            .collect();
        assert!(
            missing.is_empty(),
            "route prefixes missing from API_PREFIXES: {missing:?} — \
             unregistered paths under them would serve index.html with 200"
        );
    }

    #[tokio::test]
    async fn test_cors_rejects_unknown_origin() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(
                Request::get("/health")
                    .header("Origin", "http://evil.example.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // Unknown origin should not get CORS allow header
        let cors = resp.headers().get("access-control-allow-origin");
        assert!(
            cors.is_none(),
            "Unknown origin should not be allowed, got: {:?}",
            cors
        );
    }

    #[tokio::test]
    async fn test_cors_allows_localhost() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(
                Request::get("/health")
                    .header("Origin", "http://localhost")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let cors = resp.headers().get("access-control-allow-origin");
        assert!(cors.is_some(), "Localhost origin should be allowed");
        assert_eq!(cors.unwrap().to_str().unwrap(), "http://localhost");
    }

    #[tokio::test]
    async fn test_cors_allows_tauri() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(
                Request::get("/health")
                    .header("Origin", "tauri://localhost")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let cors = resp.headers().get("access-control-allow-origin");
        assert!(cors.is_some(), "Tauri origin should be allowed");
        assert_eq!(cors.unwrap().to_str().unwrap(), "tauri://localhost");
    }

    #[tokio::test]
    async fn test_api_routes_still_work_with_static_fallback() {
        // Verify that API routes take precedence over the static catch-all
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(Request::get("/sessions").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json, serde_json::json!([]));
    }

    // --- WebSocket tests ---

    #[tokio::test]
    async fn test_ws_stream_route_exists() {
        // Verify the WS stream route exists and responds to non-WS requests
        // (axum returns 426 Upgrade Required for non-WebSocket GET)
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(
                Request::get("/sessions/some-id/stream")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Non-WS request to a WS route returns 400 Bad Request
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_ws_clients_cleanup() {
        // Verify the ws_clients DashMap operations work correctly
        let state = test_state();
        let (tx1, _rx1) = crate::state::new_ws_client_channel();
        let (tx2, _rx2) = crate::state::new_ws_client_channel();

        // Add clients
        state
            .ws_clients
            .entry("session-1".to_string())
            .or_default()
            .push(tx1);
        state
            .ws_clients
            .entry("session-1".to_string())
            .or_default()
            .push(tx2);
        assert_eq!(state.ws_clients.get("session-1").unwrap().len(), 2);

        // Remove session cleans up all clients
        state.ws_clients.remove("session-1");
        assert!(state.ws_clients.get("session-1").is_none());
    }

    #[test]
    fn test_ws_clients_retain_disconnected() {
        // Verify that retain removes closed channels
        let state = test_state();
        let (tx1, _rx1) = crate::state::new_ws_client_channel();
        let (tx2, rx2) = crate::state::new_ws_client_channel();

        state
            .ws_clients
            .entry("sess".to_string())
            .or_default()
            .push(tx1);
        state
            .ws_clients
            .entry("sess".to_string())
            .or_default()
            .push(tx2);

        // Drop rx2 so tx2 is closed
        drop(rx2);

        // Through the production fan-out, not a hand-rolled copy of it: the
        // retain lives in one place now and this is what exercises it.
        crate::state::broadcast_to_ws_clients(&state.ws_clients, "sess", "test");

        // Only tx1 should remain (its rx1 is still alive)
        assert_eq!(state.ws_clients.get("sess").unwrap().len(), 1);
    }

    #[test]
    fn test_ws_clients_idle_session_leak() {
        // Simulate mobile reconnect churn on an idle PTY session:
        // N clients connect and disconnect without any PTY output arriving.
        // Without the fix, dead senders accumulate indefinitely.
        let state = test_state();
        let session_id = "idle-session".to_string();

        // Simulate 10 connect/disconnect cycles (mobile reconnects)
        for _ in 0..10 {
            let (tx, rx) = crate::state::new_ws_client_channel();
            state
                .ws_clients
                .entry(session_id.clone())
                .or_default()
                .push(tx);
            // Client disconnects — rx is dropped, making tx a dead sender
            drop(rx);
        }

        // Without cleanup, all 10 dead senders remain
        assert_eq!(state.ws_clients.get(&session_id).unwrap().len(), 10);

        // Now run the cleanup that should happen on WS close
        // (purge_dead_ws_clients is the function we'll add)
        crate::state::purge_dead_ws_clients(&state.ws_clients, &session_id);

        // After cleanup, all dead senders should be removed
        // Vec may remain as empty entry, or be removed entirely
        let remaining = state
            .ws_clients
            .get(&session_id)
            .map(|c| c.len())
            .unwrap_or(0);
        assert_eq!(remaining, 0, "dead senders should be purged on WS close");
    }

    #[test]
    fn test_ws_clients_purge_preserves_live_senders() {
        // Purge should only remove dead senders, keeping live ones
        let state = test_state();
        let session_id = "mixed-session".to_string();

        let (tx_live, _rx_live) = crate::state::new_ws_client_channel();
        let (tx_dead, rx_dead) = crate::state::new_ws_client_channel();

        state
            .ws_clients
            .entry(session_id.clone())
            .or_default()
            .push(tx_live);
        state
            .ws_clients
            .entry(session_id.clone())
            .or_default()
            .push(tx_dead);

        // Kill one sender
        drop(rx_dead);

        crate::state::purge_dead_ws_clients(&state.ws_clients, &session_id);

        // Only the live sender should remain
        assert_eq!(state.ws_clients.get(&session_id).unwrap().len(), 1);
    }

    // --- WS client fan-out (604-cb45 F13) ---
    //
    // A browser that connects once leaves a Vec behind. While that Vec exists
    // the PTY reader copies every chunk into it before discovering there is
    // nobody to hand the copy to, for the whole life of the session.

    #[test]
    fn test_ws_clients_purge_drops_the_empty_entry() {
        let state = test_state();
        let session_id = "gone-session".to_string();

        let (tx, rx) = crate::state::new_ws_client_channel();
        state
            .ws_clients
            .entry(session_id.clone())
            .or_default()
            .push(tx);
        drop(rx);

        crate::state::purge_dead_ws_clients(&state.ws_clients, &session_id);

        assert!(
            state.ws_clients.get(&session_id).is_none(),
            "the last client leaving must take the map entry with it, so the \
             reader's lookup misses instead of finding an empty Vec"
        );
    }

    #[test]
    fn test_ws_broadcast_reaps_the_entry_when_every_client_is_gone() {
        let state = test_state();
        let session_id = "dead-session".to_string();

        let (tx, rx) = crate::state::new_ws_client_channel();
        state
            .ws_clients
            .entry(session_id.clone())
            .or_default()
            .push(tx);
        drop(rx);

        crate::state::broadcast_to_ws_clients(&state.ws_clients, &session_id, "output");

        assert!(
            state.ws_clients.get(&session_id).is_none(),
            "a send that finds every client dead must reap the entry, not leave \
             an empty Vec for the next chunk to copy into"
        );
    }

    #[test]
    fn test_ws_broadcast_delivers_to_a_live_client() {
        let state = test_state();
        let session_id = "live-session".to_string();

        let (tx, mut rx) = crate::state::new_ws_client_channel();
        state
            .ws_clients
            .entry(session_id.clone())
            .or_default()
            .push(tx);

        crate::state::broadcast_to_ws_clients(&state.ws_clients, &session_id, "hello");

        assert_eq!(rx.try_recv().ok(), Some("hello".to_string()));
        assert_eq!(state.ws_clients.get(&session_id).unwrap().len(), 1);
    }

    #[test]
    fn test_ws_broadcast_is_a_no_op_without_clients() {
        let state = test_state();

        crate::state::broadcast_to_ws_clients(&state.ws_clients, "never-connected", "output");

        assert!(state.ws_clients.get("never-connected").is_none());
    }

    // --- Raw-output WS backpressure (604-cb45 F15) ---

    #[test]
    fn test_ws_broadcast_drops_a_client_that_stopped_draining() {
        // An unbounded queue makes a stalled browser a memory leak on the PTY
        // reader: the producer cannot block (it holds the ring lock), so the
        // only honest options are drop-the-client or grow forever.
        let state = test_state();
        let session_id = "stalled-session".to_string();

        let (tx, _rx) = crate::state::new_ws_client_channel();
        state
            .ws_clients
            .entry(session_id.clone())
            .or_default()
            .push(tx);

        for _ in 0..(crate::state::WS_CLIENT_QUEUE_CAPACITY + 8) {
            crate::state::broadcast_to_ws_clients(&state.ws_clients, &session_id, "chunk");
        }

        assert!(
            state.ws_clients.get(&session_id).is_none(),
            "a client that never drains must be dropped, not queued without limit"
        );
    }

    #[test]
    fn test_ws_broadcast_keeps_a_client_that_drains() {
        let state = test_state();
        let session_id = "healthy-session".to_string();

        let (tx, mut rx) = crate::state::new_ws_client_channel();
        state
            .ws_clients
            .entry(session_id.clone())
            .or_default()
            .push(tx);

        for _ in 0..(crate::state::WS_CLIENT_QUEUE_CAPACITY * 3) {
            crate::state::broadcast_to_ws_clients(&state.ws_clients, &session_id, "chunk");
            assert!(
                rx.try_recv().is_ok(),
                "the drained client must keep receiving"
            );
        }

        assert_eq!(state.ws_clients.get(&session_id).unwrap().len(), 1);
    }

    // --- MCP proxy wiring tests ---

    /// tools/list returns only native tools when no upstream is connected.
    #[tokio::test]
    async fn test_tools_list_no_upstream_returns_native_only() {
        let state = test_state();
        let app = build_router(state, false, true);
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/list", "params": {}
        });
        let resp = app.oneshot(mcp_post("/mcp", &body)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let tools = json["result"]["tools"].as_array().unwrap();
        // No upstream → all native tools only
        let expected = mcp_transport::test_mcp_tool_definitions()
            .as_array()
            .unwrap()
            .len();
        assert_eq!(tools.len(), expected);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"session"));
        assert!(names.contains(&"agent"));
        assert!(names.contains(&"repo"));
        assert!(names.contains(&"ui"));
        assert!(names.contains(&"plugin_dev_guide"));
        assert!(names.contains(&"config"));
        assert!(names.contains(&"debug"));
    }

    /// tools/call with upstream-prefixed name returns error (no upstream registered).
    #[tokio::test]
    async fn test_tools_call_upstream_prefix_returns_error_when_no_upstream() {
        let state = test_state();
        // Inject a session so the session_valid check passes
        let now = std::time::Instant::now();
        state.mcp.sessions.insert(
            "test-sid-proxy".to_string(),
            crate::state::McpSessionMeta {
                prompt_instructions: None,
                last_activity: now,
                is_claude_code: false,
                requires_meta_tools: false,
                has_sse_stream: false,
                sse_generation: 0,
                repo_path: None,
            },
        );

        let app = build_router(state, false, true);
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": { "name": "myupstream__do_thing", "arguments": {} }
        });
        let resp = app
            .oneshot(mcp_post_with_session("/mcp", &body, "test-sid-proxy"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // Should be an error response with isError:true
        assert_eq!(
            json["result"]["isError"], true,
            "Expected isError:true, got: {json}"
        );
    }

    /// tools/call with a native tool name still works after wiring.
    #[tokio::test]
    async fn test_native_tool_call_still_works_after_wiring() {
        let state = test_state();
        let now = std::time::Instant::now();
        state.mcp.sessions.insert(
            "test-sid-native".to_string(),
            crate::state::McpSessionMeta {
                prompt_instructions: None,
                last_activity: now,
                is_claude_code: false,
                requires_meta_tools: false,
                has_sse_stream: false,
                sse_generation: 0,
                repo_path: None,
            },
        );
        let app = build_router(state, false, true);
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "tools/call",
            "params": { "name": "session", "arguments": { "action": "list" } }
        });
        let resp = app
            .oneshot(mcp_post_with_session("/mcp", &body, "test-sid-native"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // Native tool — should succeed (empty session list, no error)
        assert_eq!(
            json["result"]["isError"], false,
            "Native tool should not error: {json}"
        );
    }

    /// Changing disabled_native_tools via put_config fires mcp_tools_changed.
    /// Uses the process-global CONFIG_DIR_OVERRIDE — must run serially.
    #[tokio::test]
    #[serial_test::serial]
    async fn test_config_change_fires_tools_changed() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());

        let state = test_state();
        let mut rx = state.mcp.tools_changed.subscribe();

        // Save config with a disabled tool
        let base = state.config.read().clone();
        let mut config = base.clone();
        config.disabled_native_tools = vec!["session".to_string()];
        let app = build_router(state.clone(), false, true);
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
        let resp = app
            .oneshot(put_from(
                "/config",
                &serde_json::json!({"base": base, "config": config}),
                addr,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Should have received a tools_changed signal
        let result = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await;
        assert!(
            result.is_ok(),
            "expected tools_changed signal after config change"
        );
    }

    /// Changing collapse_tools via put_config fires mcp_tools_changed.
    /// Uses the process-global CONFIG_DIR_OVERRIDE — must run serially.
    #[tokio::test]
    #[serial_test::serial]
    async fn test_collapse_tools_toggle_fires_tools_changed() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());

        let state = test_state();
        let mut rx = state.mcp.tools_changed.subscribe();

        // Enable collapse_tools
        let base = state.config.read().clone();
        let mut config = base.clone();
        config.collapse_tools = true;
        let app = build_router(state.clone(), false, true);
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
        let resp = app
            .oneshot(put_from(
                "/config",
                &serde_json::json!({"base": base, "config": config}),
                addr,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let result = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await;
        assert!(
            result.is_ok(),
            "expected tools_changed signal when collapse_tools toggled"
        );
    }

    /// GET /mcp with valid session returns SSE stream that emits tools/list_changed
    /// when mcp_tools_changed is signaled.
    #[tokio::test]
    async fn test_mcp_sse_tools_changed() {
        let state = test_state();

        // Start a real TCP server so we can use reqwest streaming
        let app = build_router(state.clone(), false, true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });

        let client = reqwest::Client::new();

        // Establish MCP session via initialize
        let init_body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "initialize",
            "params": { "protocolVersion": "2025-03-26", "capabilities": {},
                        "clientInfo": { "name": "test", "version": "0.1" } }
        });
        let resp = client
            .post(format!("http://{addr}/mcp"))
            .json(&init_body)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let session_id = resp
            .headers()
            .get("mcp-session-id")
            .expect("initialize should return session id")
            .to_str()
            .unwrap()
            .to_string();

        // GET /mcp with session header → SSE stream
        let resp = client
            .get(format!("http://{addr}/mcp"))
            .header("mcp-session-id", &session_id)
            .header("Accept", "text/event-stream")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "GET /mcp should return 200 for SSE");
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            ct.contains("text/event-stream"),
            "expected SSE content-type, got: {ct}"
        );

        // Signal tools changed after a small delay so SSE stream is ready
        let tools_tx = state.mcp.tools_changed.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let _ = tools_tx.send(());
        });

        // Read SSE chunks with timeout
        use futures_util::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut collected = String::new();
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while let Some(Ok(chunk)) = stream.next().await {
                collected.push_str(&String::from_utf8_lossy(&chunk));
                if collected.contains("tools/list_changed") {
                    return true;
                }
            }
            false
        })
        .await;

        assert!(
            result.unwrap_or(false),
            "expected tools/list_changed notification in SSE stream, got: {collected}"
        );
    }

    #[cfg(unix)]
    fn short_socket_test_dir() -> tempfile::TempDir {
        let dir = tempfile::Builder::new()
            .prefix("s")
            .tempdir_in(crate::test_support::short_socket_test_temp_root())
            .expect("create repository-local socket test dir");
        assert!(
            dir.path().join("mcp-4294967295.sock").as_os_str().len() < 104,
            "socket test path exceeds macOS SUN_LEN: {}",
            dir.path().display()
        );
        dir
    }

    /// Unix socket listener: binds, serves health check, cleans up socket file on drop.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_unix_socket_serves_health() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let state = test_state();
        let tmp_dir = short_socket_test_dir();
        let sock_path = tmp_dir.path().join("s");

        // Bind Unix socket and serve the router (no auth, MCP enabled)
        let app = build_router(state.clone(), false, true);
        let uds = tokio::net::UnixListener::bind(&sock_path).unwrap();
        assert!(sock_path.exists(), "socket file should exist after bind");

        let server = tokio::spawn(async move {
            axum::serve(uds, app.into_make_service()).await.unwrap();
        });

        // Connect via UnixStream, send raw HTTP GET /health with Connection: close
        let mut stream = tokio::net::UnixStream::connect(&sock_path).await.unwrap();
        stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();

        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf);
        assert!(
            response.contains("200 OK"),
            "expected 200 OK, got: {response}"
        );
        assert!(
            response.contains("\"ok\":true"),
            "expected JSON health body, got: {response}"
        );

        server.abort();
        let _ = std::fs::remove_file(&sock_path);
    }

    /// Regression test: aborting the first server task must NOT remove the socket file
    /// already rebound by the second instance. The fix is removing SocketGuard from the
    /// serve task — abort no longer triggers any on-drop cleanup of the socket file.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_unix_socket_rebind_no_race() {
        let tmp_dir = short_socket_test_dir();
        let sock_path = tmp_dir.path().join("s");

        // First instance: bind and spawn server
        let _ = std::fs::remove_file(&sock_path);
        let uds1 = tokio::net::UnixListener::bind(&sock_path).unwrap();
        let state = test_state();
        let app1 = build_router(state.clone(), false, true)
            .layer(axum::middleware::from_fn(inject_localhost_connect_info));
        let server1 = tokio::spawn(async move {
            let _ = axum::serve(uds1, app1.into_make_service()).await;
        });

        // Abort first instance (simulates server restart)
        server1.abort();
        let _ = server1.await; // wait for abort to complete

        // Second instance: rebind (as done in bind_unix_socket)
        let _ = std::fs::remove_file(&sock_path);
        let uds2 = tokio::net::UnixListener::bind(&sock_path).unwrap();
        let app2 = build_router(state.clone(), false, true)
            .layer(axum::middleware::from_fn(inject_localhost_connect_info));
        let server2 = tokio::spawn(async move {
            let _ = axum::serve(uds2, app2.into_make_service()).await;
        });

        // Give tokio a moment to run any lingering drop callbacks
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Socket file must still exist — the first instance's abort must not have removed it
        assert!(
            sock_path.exists(),
            "socket file removed by first instance abort — race condition present"
        );

        server2.abort();
        let _ = std::fs::remove_file(&sock_path);
    }

    /// When a live socket exists, resolve_socket_path-style logic should pick an alternative.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_multi_instance_socket_coexistence() {
        let tmp_dir = short_socket_test_dir();
        let primary = tmp_dir.path().join("mcp.sock");

        // First instance: bind primary socket and start serving
        let _ = std::fs::remove_file(&primary);
        let uds1 = tokio::net::UnixListener::bind(&primary).unwrap();
        let state = test_state();
        let app1 = build_router(state.clone(), false, true)
            .layer(axum::middleware::from_fn(inject_localhost_connect_info));
        let server1 = tokio::spawn(async move {
            let _ = axum::serve(uds1, app1.into_make_service()).await;
        });
        // Give server a moment to start accepting
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Verify primary socket is live (can connect)
        assert!(
            tokio::net::UnixStream::connect(&primary).await.is_ok(),
            "primary socket should accept connections"
        );

        // Second instance: primary is live, so it should NOT be able to bind primary.
        // Simulate resolve_socket_path logic: check if primary is live, use alt if so.
        let primary_live = std::os::unix::net::UnixStream::connect(&primary).is_ok();
        assert!(primary_live, "primary should be detected as live");

        let alt = tmp_dir
            .path()
            .join(format!("mcp-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&alt);
        let uds2 = tokio::net::UnixListener::bind(&alt).unwrap();
        let app2 = build_router(state.clone(), false, true)
            .layer(axum::middleware::from_fn(inject_localhost_connect_info));
        let server2 = tokio::spawn(async move {
            let _ = axum::serve(uds2, app2.into_make_service()).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Both sockets should be live simultaneously
        assert!(
            tokio::net::UnixStream::connect(&primary).await.is_ok(),
            "primary still alive"
        );
        assert!(
            tokio::net::UnixStream::connect(&alt).await.is_ok(),
            "alternative also alive"
        );

        // Cleanup
        server1.abort();
        server2.abort();
        let _ = std::fs::remove_file(&primary);
        let _ = std::fs::remove_file(&alt);
    }

    #[cfg(unix)]
    #[test]
    fn named_instance_socket_path_stays_within_macos_limit() {
        let path = named_socket_path(
            "validate-763-20260913-with-a-long-but-valid-instance-name",
            std::path::Path::new("/var/folders/ab/cdefghijklmnop/T"),
        );
        assert!(
            path.as_os_str().len() < 104,
            "named instance socket path must fit macOS SUN_LEN: {}",
            path.display()
        );
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("tuic-mcp-")
        );
    }

    /// Stale mcp-{pid}.sock files (dead PID) should be cleaned up.
    #[cfg(unix)]
    #[test]
    fn test_cleanup_stale_sockets() {
        let tmp_dir = short_socket_test_dir();

        // Create a socket file for a PID that definitely doesn't exist (PID 1 is launchd, skip it)
        let dead_pid = 99999;
        let stale = tmp_dir.path().join(format!("mcp-{dead_pid}.sock"));
        std::fs::write(&stale, "").unwrap();
        assert!(stale.exists());

        // Create a socket file for our own PID (alive)
        let alive = tmp_dir
            .path()
            .join(format!("mcp-{}.sock", std::process::id()));
        std::fs::write(&alive, "").unwrap();

        // Run cleanup logic inline (can't call cleanup_stale_sockets directly as it uses config_dir)
        for entry in std::fs::read_dir(tmp_dir.path()).unwrap().flatten() {
            let name = entry.file_name();
            let Some(name_str) = name.to_str() else {
                continue;
            };
            if let Some(rest) = name_str.strip_prefix("mcp-")
                && let Some(pid_str) = rest.strip_suffix(".sock")
                && let Ok(pid) = pid_str.parse::<u32>()
            {
                let alive_check = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
                if !alive_check {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }

        // Dead PID socket should be removed
        assert!(
            !stale.exists(),
            "stale socket for dead PID should be cleaned up"
        );
        // Our own PID socket should remain
        assert!(alive.exists(), "socket for live PID should not be removed");

        // Cleanup
        let _ = std::fs::remove_file(&alive);
    }

    // ---- VtLogBuffer HTTP integration tests ----

    #[tokio::test]
    async fn test_get_output_format_log_returns_lines() {
        use crate::state::{VT_LOG_BUFFER_CAPACITY, VtLogBuffer};

        let state = test_state();
        let sid = "test-log-session";

        // Pre-populate a VtLogBuffer with some lines
        let mut vt_log = VtLogBuffer::new(24, 80, VT_LOG_BUFFER_CAPACITY);
        for i in 0..30 {
            vt_log.process(format!("log-line-{i}\r\n").as_bytes());
        }
        let expected_total = vt_log.total_lines();
        assert!(expected_total > 0, "should have captured some lines");
        state
            .grid
            .vt_log_buffers
            .insert(sid.to_string(), parking_lot::Mutex::new(vt_log));

        let app = build_router(state, false, true);
        let resp = app
            .oneshot(
                Request::get(format!("/sessions/{sid}/output?format=log"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let lines = json["lines"].as_array().expect("lines should be an array");
        let total = json["total_lines"]
            .as_u64()
            .expect("total_lines should be u64");
        assert_eq!(total, expected_total as u64);
        assert_eq!(lines.len(), expected_total);
        // Lines are LogLine objects: {spans: [{text: "log-line-N", ...}]}
        let has_expected = lines.iter().any(|l| {
            l["spans"]
                .as_array()
                .and_then(|spans| spans.first())
                .and_then(|s| s["text"].as_str())
                .map(|t| t.starts_with("log-line-"))
                .unwrap_or(false)
        });
        assert!(
            has_expected,
            "should contain log-line-N entries, got: {json}"
        );
    }

    #[tokio::test]
    async fn test_get_output_format_log_with_limit() {
        use crate::state::{VT_LOG_BUFFER_CAPACITY, VtLogBuffer};

        let state = test_state();
        let sid = "test-log-limit";

        let mut vt_log = VtLogBuffer::new(24, 80, VT_LOG_BUFFER_CAPACITY);
        for i in 0..30 {
            vt_log.process(format!("lim-{i}\r\n").as_bytes());
        }
        let total = vt_log.total_lines();
        state
            .grid
            .vt_log_buffers
            .insert(sid.to_string(), parking_lot::Mutex::new(vt_log));

        // Request only 3 lines
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(
                Request::get(format!("/sessions/{sid}/output?format=log&limit=3"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let lines = json["lines"].as_array().expect("lines should be an array");
        assert_eq!(lines.len(), 3, "limit=3 should return 3 lines");
        assert_eq!(json["total_lines"].as_u64().unwrap(), total as u64);
    }

    #[tokio::test]
    async fn test_get_output_format_text_does_not_duplicate_rows_after_growing_viewport() {
        use crate::state::{VT_LOG_BUFFER_CAPACITY, VtLogBuffer};

        let state = test_state();
        let sid = "test-text-resize-growth";
        let mut vt_log = VtLogBuffer::new(12, 80, VT_LOG_BUFFER_CAPACITY);
        for i in 0..50 {
            vt_log.process(format!("resize-line-{i:02}\r\n").as_bytes());
        }
        vt_log.resize(33, 80);
        let canonical_total = vt_log.grid_total_lines();
        state
            .grid
            .vt_log_buffers
            .insert(sid.to_string(), parking_lot::Mutex::new(vt_log));

        let app = build_router(state, false, true);
        let resp = app
            .oneshot(
                Request::get(format!("/sessions/{sid}/output?format=text&limit=1000"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let text = json["data"].as_str().expect("data should be text");

        for i in 0..50 {
            let marker = format!("resize-line-{i:02}");
            assert_eq!(
                text.lines().filter(|line| *line == marker).count(),
                1,
                "canonical text snapshot must contain {marker} exactly once"
            );
        }
        assert_eq!(json["total_written"], canonical_total);
    }

    #[tokio::test]
    async fn test_cache_control_index_html_no_cache() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        // Root serves HTML (or redirect); if 200, should have no-cache
        let status = resp.status();
        if status == StatusCode::OK {
            let cc = resp
                .headers()
                .get("cache-control")
                .map(|v| v.to_str().unwrap().to_string());
            assert_eq!(
                cc.as_deref(),
                Some("no-cache"),
                "HTML responses must have no-cache"
            );
        }
        // 3xx redirect is also valid (mobile redirect)
    }

    #[tokio::test]
    async fn test_get_output_format_log_unknown_session_404() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(
                Request::get("/sessions/nonexistent/output?format=log")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // --- Swarm protocol tests (Layer 0) ---

    #[tokio::test]
    async fn test_session_close_self_close_guard() {
        // An agent that registered as a peer should not be able to close its own session.
        // The close handler must resolve caller identity via mcp_session_id and reject.
        let state = test_state();
        let mcp_sid = mcp_initialize(&state).await;

        // Register a peer — this auto-populates mcp_to_session
        let pty_session_id = "550e8400-e29b-41d4-a716-446655440c01";
        call_mcp_tool_with_session(
            &state,
            "agent",
            serde_json::json!({
                "action": "register",
                "tuic_session": pty_session_id,
                "name": "orchestrator"
            }),
            &mcp_sid,
        )
        .await;

        // Try to close own session — should be rejected
        let result = call_mcp_tool_with_session(
            &state,
            "session",
            serde_json::json!({
                "action": "close",
                "session_id": pty_session_id
            }),
            &mcp_sid,
        )
        .await;
        assert!(
            result["error"].as_str().is_some(),
            "Self-close should return error"
        );
        assert!(
            result["error"].as_str().unwrap().contains("own session"),
            "Error should mention 'own session', got: {}",
            result["error"]
        );
    }

    #[tokio::test]
    async fn test_session_close_other_session_allowed() {
        // Closing someone else's session should succeed (idempotent)
        let state = test_state();
        let mcp_sid = mcp_initialize(&state).await;

        // Register caller — auto-populates mcp_to_session
        call_mcp_tool_with_session(
            &state,
            "agent",
            serde_json::json!({
                "action": "register",
                "tuic_session": "550e8400-e29b-41d4-a716-446655440c02",
                "name": "orchestrator"
            }),
            &mcp_sid,
        )
        .await;

        // Close a different session — should succeed
        let result = call_mcp_tool_with_session(
            &state,
            "session",
            serde_json::json!({
                "action": "close",
                "session_id": "other-agent-session"
            }),
            &mcp_sid,
        )
        .await;
        assert_eq!(result["ok"], true, "Closing other session should succeed");
    }

    #[tokio::test]
    async fn test_session_status_returns_metadata() {
        // session(action=status) should return shell_state metadata for a known session
        let state = test_state();
        let result = call_mcp_tool(
            &state,
            "session",
            serde_json::json!({
                "action": "status",
                "session_id": "nonexistent"
            }),
        )
        .await;
        // For now, status should exist as an action (not "Unknown action")
        assert!(
            result["error"].is_null()
                || !result["error"]
                    .as_str()
                    .unwrap_or("")
                    .contains("Unknown action"),
            "session(status) should be a valid action, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_tcp_health_smoke() {
        let state = test_state();
        let app = build_router(state, false, true)
            .into_make_service_with_connect_info::<std::net::SocketAddr>();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let resp = reqwest::Client::new()
            .get(format!("http://{addr}/health"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let json: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(json["ok"], true);

        server.abort();
    }

    fn register_reaper_peer(state: &AppState, tuic: &str, mcp_sid: &str) {
        state.peer_agents.insert(
            tuic.to_string(),
            crate::state::PeerAgent {
                tuic_session: tuic.to_string(),
                mcp_session_id: mcp_sid.to_string(),
                name: "peer".to_string(),
                project: None,
                registered_at: 0,
            },
        );
    }

    /// An agent thinking for over an hour makes no MCP request, so its protocol
    /// session is reaped while its PTY is still running. Deleting the identity
    /// there is what made a child's `agent action=send` report the parent as no
    /// longer registered — observed live on 2026-08-24.
    #[cfg(unix)]
    #[test]
    fn reaped_mcp_session_keeps_an_identity_that_still_owns_a_pty() {
        let state = crate::state::tests_support::make_test_app_state();
        crate::state::tests_support::insert_dummy_session(&state, "pty-owner");
        register_reaper_peer(&state, "pty-owner", "mcp-1");
        state
            .mcp
            .to_session
            .insert("mcp-1".to_string(), "pty-owner".to_string());
        state
            .mcp
            .session_to_mcp
            .insert("pty-owner".to_string(), vec!["mcp-1".to_string()]);

        let (removed, retained) = evict_peers_for_reaped_mcp_session(&state, "mcp-1");
        assert!(removed.is_empty(), "a live PTY is still addressable");
        assert_eq!(retained, vec!["pty-owner".to_string()]);
        assert!(state.peer_agents.contains_key("pty-owner"));
        assert!(state.mcp.to_session.is_empty());
        assert!(state.mcp.session_to_mcp.is_empty());
    }

    /// The unrecoverable case. A headerless orchestrator owns no PTY, so nothing
    /// re-creates its UUID: re-registering mints a fresh one and no child is told.
    /// Dropping it strands every handoff aimed at it, permanently.
    #[test]
    fn reaped_mcp_session_keeps_an_identity_a_live_child_calls_parent() {
        let state = crate::state::tests_support::make_test_app_state();
        register_reaper_peer(&state, "orchestrator", "mcp-2");
        state
            .session_maps
            .session_parent
            .insert("child-session".to_string(), "orchestrator".to_string());

        let (removed, retained) = evict_peers_for_reaped_mcp_session(&state, "mcp-2");
        assert!(removed.is_empty(), "a named parent is still addressable");
        assert_eq!(retained, vec!["orchestrator".to_string()]);
    }

    /// The retention is bounded: with no terminal and no child naming it, the
    /// identity is genuinely unreachable and the reaper must still free it.
    #[test]
    fn reaped_mcp_session_drops_an_unreachable_identity() {
        let state = crate::state::tests_support::make_test_app_state();
        register_reaper_peer(&state, "ghost", "mcp-3");
        state.orchestrator_peers.insert("ghost".to_string());
        // A different session's parent must not keep this one alive.
        state
            .session_maps
            .session_parent
            .insert("child-session".to_string(), "somebody-else".to_string());

        let (removed, retained) = evict_peers_for_reaped_mcp_session(&state, "mcp-3");
        assert_eq!(removed, vec!["ghost".to_string()]);
        assert!(retained.is_empty());
        assert!(!state.peer_agents.contains_key("ghost"));
        assert!(!state.orchestrator_peers.contains("ghost"));
    }

    /// Only the reaped session's peers are considered.
    #[test]
    fn reaping_one_mcp_session_leaves_another_session_peers_alone() {
        let state = crate::state::tests_support::make_test_app_state();
        register_reaper_peer(&state, "ghost", "mcp-4");
        register_reaper_peer(&state, "bystander", "mcp-5");

        let (removed, _) = evict_peers_for_reaped_mcp_session(&state, "mcp-4");
        assert_eq!(removed, vec!["ghost".to_string()]);
        assert!(state.peer_agents.contains_key("bystander"));
    }

    #[test]
    fn reaping_one_mcp_bridge_promotes_its_surviving_sibling() {
        let state = crate::state::tests_support::make_test_app_state();
        register_reaper_peer(&state, "shared", "mcp-old");
        state
            .mcp
            .to_session
            .insert("mcp-old".to_string(), "shared".to_string());
        state
            .mcp
            .to_session
            .insert("mcp-live".to_string(), "shared".to_string());
        state.mcp.session_to_mcp.insert(
            "shared".to_string(),
            vec!["mcp-old".to_string(), "mcp-live".to_string()],
        );
        let (old_channel, _) = tokio::sync::broadcast::channel(8);
        let (live_channel, _) = tokio::sync::broadcast::channel(8);
        state
            .session_maps
            .messaging_channels
            .insert("mcp-old".to_string(), old_channel);
        state
            .session_maps
            .messaging_channels
            .insert("mcp-live".to_string(), live_channel);

        let (removed, retained) = evict_peers_for_reaped_mcp_session(&state, "mcp-old");
        assert!(removed.is_empty());
        assert!(retained.is_empty());
        assert_eq!(
            state.peer_agents.get("shared").unwrap().mcp_session_id,
            "mcp-live"
        );
        assert_eq!(state.mcp.to_session.len(), 1);
        assert_eq!(
            state.mcp.session_to_mcp.get("shared").unwrap().as_slice(),
            ["mcp-live"]
        );
        assert_eq!(state.session_maps.messaging_channels.len(), 1);
        assert!(
            state
                .session_maps
                .messaging_channels
                .contains_key("mcp-live")
        );
    }

    /// Catches: the sweep deletes a session refreshed after its stale-ID scan.
    #[test]
    fn refresh_after_reaper_snapshot_preserves_session_and_route() {
        const TUIC: &str = "550e8400-e29b-41d4-a716-446655440000";
        const SID: &str = "mcp-refreshed";
        let state = crate::state::tests_support::make_test_app_state();
        let now = std::time::Instant::now();
        state.mcp.sessions.insert(
            SID.to_string(),
            crate::state::McpSessionMeta {
                prompt_instructions: None,
                last_activity: now - std::time::Duration::from_secs(7200),
                is_claude_code: true,
                requires_meta_tools: false,
                has_sse_stream: false,
                sse_generation: 0,
                repo_path: None,
            },
        );
        register_reaper_peer(&state, TUIC, SID);
        state
            .mcp
            .to_session
            .insert(SID.to_string(), TUIC.to_string());
        state
            .mcp
            .session_to_mcp
            .insert(TUIC.to_string(), vec![SID.to_string()]);
        let (channel, _) = tokio::sync::broadcast::channel(8);
        state
            .session_maps
            .messaging_channels
            .insert(SID.to_string(), channel);

        let selected: Vec<String> = state
            .mcp
            .sessions
            .iter()
            .filter(|entry| {
                now.duration_since(entry.value().last_activity)
                    >= std::time::Duration::from_secs(3600)
            })
            .map(|entry| entry.key().clone())
            .collect();
        assert_eq!(selected, vec![SID.to_string()]);
        mcp_transport::refresh_mcp_session(&state, SID, true, Some(TUIC));
        assert!(
            reap_selected_mcp_session(&state, &selected[0], std::time::Duration::from_secs(3600))
                .is_none(),
            "refreshed session must no longer meet the idle deadline"
        );

        assert!(
            state.mcp.sessions.contains_key(SID),
            "fresh protocol session was reaped"
        );
        assert_eq!(
            state.mcp.to_session.get(SID).map(|v| v.value().clone()),
            Some(TUIC.to_string())
        );
        assert_eq!(
            state.mcp.session_to_mcp.get(TUIC).unwrap().as_slice(),
            [SID]
        );
        assert!(state.session_maps.messaging_channels.contains_key(SID));
        assert_eq!(state.peer_agents.get(TUIC).unwrap().mcp_session_id, SID);
    }

    /// Catches: a fresh-session guard prevents the sweep from freeing truly
    /// expired routes, or removes the sibling that still serves the identity.
    #[test]
    fn expired_reaper_candidate_releases_only_its_own_session_state() {
        const TUIC: &str = "550e8400-e29b-41d4-a716-446655440001";
        let state = crate::state::tests_support::make_test_app_state();
        let now = std::time::Instant::now();
        for (sid, last_activity) in [
            ("mcp-expired", now - std::time::Duration::from_secs(7200)),
            ("mcp-sibling", now),
        ] {
            state.mcp.sessions.insert(
                sid.to_string(),
                crate::state::McpSessionMeta {
                    prompt_instructions: None,
                    last_activity,
                    is_claude_code: true,
                    requires_meta_tools: false,
                    has_sse_stream: false,
                    sse_generation: 0,
                    repo_path: None,
                },
            );
            state
                .mcp
                .to_session
                .insert(sid.to_string(), TUIC.to_string());
            let (channel, _) = tokio::sync::broadcast::channel(8);
            state
                .session_maps
                .messaging_channels
                .insert(sid.to_string(), channel);
        }
        state.mcp.session_to_mcp.insert(
            TUIC.to_string(),
            vec!["mcp-expired".to_string(), "mcp-sibling".to_string()],
        );
        register_reaper_peer(&state, TUIC, "mcp-expired");

        assert!(
            reap_selected_mcp_session(&state, "mcp-expired", std::time::Duration::from_secs(3600))
                .is_some()
        );
        assert!(!state.mcp.sessions.contains_key("mcp-expired"));
        assert!(!state.mcp.to_session.contains_key("mcp-expired"));
        assert!(
            !state
                .session_maps
                .messaging_channels
                .contains_key("mcp-expired")
        );
        assert!(state.mcp.sessions.contains_key("mcp-sibling"));
        assert_eq!(
            state.mcp.session_to_mcp.get(TUIC).unwrap().as_slice(),
            ["mcp-sibling"]
        );
        assert!(
            state
                .session_maps
                .messaging_channels
                .contains_key("mcp-sibling")
        );
        assert_eq!(
            state.peer_agents.get(TUIC).unwrap().mcp_session_id,
            "mcp-sibling"
        );
    }

    /// Catches: a request after a completed reap recreates metadata but
    /// leaves its addressable peer without a delivery route.
    #[test]
    fn refresh_after_completed_reap_restores_peer_route() {
        const TUIC: &str = "550e8400-e29b-41d4-a716-446655440003";
        const SID: &str = "mcp-reconnect-after-reap";
        let state = crate::state::tests_support::make_test_app_state();
        state.mcp.sessions.insert(
            SID.to_string(),
            crate::state::McpSessionMeta {
                prompt_instructions: None,
                last_activity: std::time::Instant::now() - std::time::Duration::from_secs(7200),
                is_claude_code: true,
                requires_meta_tools: false,
                has_sse_stream: false,
                sse_generation: 0,
                repo_path: None,
            },
        );
        register_reaper_peer(&state, TUIC, SID);
        state
            .mcp
            .to_session
            .insert(SID.to_string(), TUIC.to_string());
        state
            .mcp
            .session_to_mcp
            .insert(TUIC.to_string(), vec![SID.to_string()]);
        state
            .session_maps
            .session_parent
            .insert("live-child".to_string(), TUIC.to_string());

        assert!(
            reap_selected_mcp_session(&state, SID, std::time::Duration::from_secs(3600)).is_some()
        );
        assert!(!state.mcp.sessions.contains_key(SID));
        assert!(!state.mcp.to_session.contains_key(SID));
        assert!(state.peer_agents.contains_key(TUIC));

        mcp_transport::refresh_mcp_session(&state, SID, true, Some(TUIC));

        assert!(state.mcp.sessions.contains_key(SID));
        assert_eq!(state.mcp.to_session.get(SID).unwrap().value(), TUIC);
        assert_eq!(
            state.mcp.session_to_mcp.get(TUIC).unwrap().as_slice(),
            [SID]
        );
        assert_eq!(state.peer_agents.get(TUIC).unwrap().mcp_session_id, SID);
    }

    /// Catches: refresh re-creates metadata while the reaper is still dropping
    /// its routes, leaving the peer alive but unable to receive sibling mail.
    #[test]
    fn concurrent_refresh_and_reap_keep_peer_and_sibling_routes() {
        const TUIC: &str = "550e8400-e29b-41d4-a716-446655440002";
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let now = std::time::Instant::now();
        for (sid, last_activity) in [
            ("mcp-racing", now - std::time::Duration::from_secs(7200)),
            ("mcp-sibling", now),
        ] {
            state.mcp.sessions.insert(
                sid.to_string(),
                crate::state::McpSessionMeta {
                    prompt_instructions: None,
                    last_activity,
                    is_claude_code: true,
                    requires_meta_tools: false,
                    has_sse_stream: false,
                    sse_generation: 0,
                    repo_path: None,
                },
            );
            state
                .mcp
                .to_session
                .insert(sid.to_string(), TUIC.to_string());
        }
        state.mcp.session_to_mcp.insert(
            TUIC.to_string(),
            vec!["mcp-racing".to_string(), "mcp-sibling".to_string()],
        );
        register_reaper_peer(&state, TUIC, "mcp-racing");

        let gate = Arc::new(std::sync::Barrier::new(3));
        let refresh_state = state.clone();
        let refresh_gate = gate.clone();
        let refresh = std::thread::spawn(move || {
            refresh_gate.wait();
            mcp_transport::refresh_mcp_session(&refresh_state, "mcp-racing", true, Some(TUIC));
        });
        let reap_state = state.clone();
        let reap_gate = gate.clone();
        let reap = std::thread::spawn(move || {
            reap_gate.wait();
            reap_selected_mcp_session(
                &reap_state,
                "mcp-racing",
                std::time::Duration::from_secs(3600),
            );
        });
        gate.wait();
        refresh.join().unwrap();
        reap.join().unwrap();

        assert!(state.mcp.sessions.contains_key("mcp-racing"));
        assert_eq!(
            state.mcp.to_session.get("mcp-racing").unwrap().value(),
            TUIC
        );
        assert_eq!(
            state.mcp.to_session.get("mcp-sibling").unwrap().value(),
            TUIC
        );
        let routes = state.mcp.session_to_mcp.get(TUIC).unwrap();
        assert!(routes.contains(&"mcp-racing".to_string()));
        assert!(routes.contains(&"mcp-sibling".to_string()));
        assert!(state.peer_agents.contains_key(TUIC));
        assert!(routes.contains(&state.peer_agents.get(TUIC).unwrap().mcp_session_id));
    }

    /// Catches: reaping protocol metadata leaves routing entries and broadcast
    /// senders allocated for every fresh initialize in a reconnect storm.
    #[tokio::test]
    async fn reaping_fresh_mcp_sessions_releases_per_session_state() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let mut session_ids = Vec::new();
        for _ in 0..12 {
            let mut headers = axum::http::HeaderMap::new();
            headers.insert(
                mcp_transport::TUIC_SESSION_HEADER,
                "550e8400-e29b-41d4-a716-446655440000".parse().unwrap(),
            );
            let response = mcp_transport::mcp_post(
                State(state.clone()),
                ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))),
                headers,
                Json(serde_json::json!({
                    "jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": {"clientInfo": {"name": "tuic-bridge"}}
                })),
            )
            .await
            .into_response();
            let sid = response
                .headers()
                .get(mcp_transport::MCP_SESSION_HEADER)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            let (channel, _) = tokio::sync::broadcast::channel(8);
            state
                .session_maps
                .messaging_channels
                .insert(sid.clone(), channel);
            session_ids.push(sid);
        }
        assert_eq!(state.mcp.sessions.len(), session_ids.len());
        assert_eq!(state.mcp.to_session.len(), session_ids.len());
        assert_eq!(
            state.session_maps.messaging_channels.len(),
            session_ids.len()
        );

        for sid in &session_ids {
            state.mcp.sessions.remove(sid);
            evict_peers_for_reaped_mcp_session(&state, sid);
        }
        assert!(state.mcp.sessions.is_empty());
        assert!(state.mcp.to_session.is_empty(), "reaped routes retained");
        assert!(
            state.mcp.session_to_mcp.is_empty(),
            "reaped reverse routes retained"
        );
        assert!(
            state.session_maps.messaging_channels.is_empty(),
            "reaped broadcast senders retained"
        );
    }

    /// The three routes below stand in for the shapes the real router serves: a
    /// handler that wedges, a handler that buffers a body, and a handler that
    /// answers at once and then streams for a long time (SSE, a PTY socket).
    /// They are synthetic on purpose — the layers under test are applied by the
    /// same `with_server_limits` that `build_router` calls, so a test that
    /// passes here cannot be passing against a stack production does not run.
    fn limits_test_router(timeout: std::time::Duration) -> Router {
        use axum::response::sse::{Event, Sse};

        let routes = Router::new()
            .route(
                "/wedged",
                get(|| async {
                    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                    "unreachable"
                }),
            )
            .route(
                "/echo",
                post(|body: String| async move { body.len().to_string() }),
            )
            .route(
                "/stream",
                get(move || async move {
                    // Headers are returned now; items arrive long after the
                    // deadline. This is the SSE/WS shape.
                    let items = futures_util::stream::once(async {
                        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                        Ok::<_, std::convert::Infallible>(Event::default().data("late"))
                    });
                    Sse::new(items)
                }),
            );
        with_server_limits(routes, timeout)
    }

    /// A wedged handler must not hold its connection forever.
    #[tokio::test]
    async fn server_limits_time_out_a_wedged_handler() {
        let resp = limits_test_router(std::time::Duration::from_millis(50))
            .oneshot(Request::get("/wedged").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::REQUEST_TIMEOUT,
            "a handler that never returns must be cut off with 408, not held open"
        );
    }

    /// Both edges of the cap, because only the pair pins the constant.
    ///
    /// Honest limitation: this test passed before `with_server_limits` applied
    /// any layer, because axum's built-in `DefaultBodyLimit` is also 2 MB and
    /// already refuses the oversized body. `MAX_BODY_BYTES` is deliberately set
    /// equal to that default so making it explicit changes no behaviour. What
    /// the assertion buys is that the bound is now *stated*: a `DefaultBodyLimit
    /// ::disable()` added anywhere outside this layer, or an axum release that
    /// drifts its default upward, fails here instead of silently uncapping the
    /// server.
    #[tokio::test]
    async fn server_limits_reject_a_body_over_the_cap() {
        async fn post_bytes(n: usize) -> StatusCode {
            limits_test_router(std::time::Duration::from_secs(30))
                .oneshot(
                    Request::post("/echo")
                        .header(CONTENT_TYPE, "text/plain")
                        .body(Body::from(vec![b'x'; n]))
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
        }

        assert_eq!(
            post_bytes(MAX_BODY_BYTES + 1).await,
            StatusCode::PAYLOAD_TOO_LARGE,
            "a body over the cap must be refused, not buffered"
        );
        assert_eq!(
            post_bytes(MAX_BODY_BYTES - 1).await,
            StatusCode::OK,
            "the cap must not reject a body that fits under it"
        );
    }

    /// A voice file is imported whole, as base64 in JSON, so its one route has
    /// to take more than the 2 MB every other route is held to — and only it.
    #[cfg(feature = "dictation")]
    #[tokio::test]
    async fn only_the_voice_import_route_accepts_a_large_body() {
        async fn post_json(path: &str, body_bytes: usize) -> StatusCode {
            let payload = format!(
                r#"{{"language":"xx","name":"nonna","dataBase64":"{}"}}"#,
                "A".repeat(body_bytes)
            );
            build_router(test_state(), false, true)
                .oneshot(
                    Request::post(path)
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(payload))
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
        }

        let over_the_default = MAX_BODY_BYTES + 1024 * 1024;
        assert_ne!(
            post_json("/dictation/speech/voices/import", over_the_default).await,
            StatusCode::PAYLOAD_TOO_LARGE,
            "a voice file over 2 MB must reach the import handler"
        );
        assert_eq!(
            post_json("/dictation/speech/voices/delete", over_the_default).await,
            StatusCode::PAYLOAD_TOO_LARGE,
            "the larger limit must not leak to the route beside it"
        );
        assert_eq!(
            post_json(
                "/dictation/speech/voices/import",
                SPEECH_VOICE_IMPORT_BODY_BYTES + 1
            )
            .await,
            StatusCode::PAYLOAD_TOO_LARGE,
            "the import route is still capped"
        );
    }

    #[tokio::test]
    async fn acp_prompt_accepts_a_three_mib_image_without_raising_other_routes_cap() {
        let image_data = "A".repeat(4 * 1024 * 1024); // 3 MiB encoded as base64.
        let payload = format!(
            r#"{{"prompt":[{{"type":"image","mimeType":"image/jpeg","data":"{image_data}"}}]}}"#
        );
        async fn status(path: &str, payload: String) -> StatusCode {
            build_router(test_state(), false, true)
                .oneshot(
                    Request::post(path)
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(payload))
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
        }

        assert_ne!(
            status(
                "/acp/connections/00000000-0000-0000-0000-000000000001/sessions/y/prompt",
                payload.clone()
            )
            .await,
            StatusCode::PAYLOAD_TOO_LARGE,
            "the mobile image prompt must reach its handler"
        );
        assert_eq!(
            status(
                "/acp/connections/00000000-0000-0000-0000-000000000001/sessions",
                payload
            )
            .await,
            StatusCode::PAYLOAD_TOO_LARGE,
            "the larger prompt cap must not apply to another ACP route"
        );
        let oversized_image = "A".repeat(11 * 1024 * 1024 / 3 * 4);
        let oversized_prompt = format!(
            r#"{{"prompt":[{{"type":"image","mimeType":"image/jpeg","data":"{oversized_image}"}}]}}"#
        );
        assert_eq!(
            status(
                "/acp/connections/00000000-0000-0000-0000-000000000001/sessions/y/prompt",
                oversized_prompt
            )
            .await,
            StatusCode::PAYLOAD_TOO_LARGE,
            "an image above the 10 MiB draft cap must not be buffered"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn attachment_upload_streams_into_session_cwd_and_stays_out_of_git() {
        let repo = create_temp_git_repo();
        let state = test_state();
        crate::state::tests_support::insert_dummy_session(&state, "upload-session");
        crate::state::tests_support::set_session_cwd(
            &state,
            "upload-session",
            repo.path().to_str().unwrap(),
        );
        let route = build_router(state.clone(), false, true);
        let response = route
            .clone()
            .oneshot(
                Request::post("/attachments/upload?kind=pty&id=upload-session&name=notes.txt")
                    .header(CONTENT_TYPE, "application/octet-stream")
                    .body(Body::from("attachment bytes"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/json",
            "the upload endpoint must answer with attachment metadata, not the SPA fallback"
        );
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let reply: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let path = std::path::Path::new(reply["path"].as_str().unwrap());
        assert!(path.starts_with(repo.path().join(".tuic/attachments")));
        assert_eq!(std::fs::read(path).unwrap(), b"attachment bytes");
        let ignored = std::process::Command::new("git")
            .args(["check-ignore", "-q"])
            .arg(path)
            .current_dir(repo.path())
            .status()
            .unwrap();
        assert!(
            ignored.success(),
            "an uploaded file must stay out of commits"
        );

        let oversized = route
            .oneshot(
                Request::post("/attachments/upload?kind=pty&id=upload-session&name=too-big.bin")
                    .header(CONTENT_TYPE, "application/octet-stream")
                    .body(Body::from(vec![0u8; 26 * 1024 * 1024]))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(
            std::fs::read_dir(repo.path().join(".tuic/attachments"))
                .unwrap()
                .count(),
            1
        );
        let fresh = repo.path().join(".tuic/attachments/2-fresh.txt");
        std::fs::write(&fresh, b"keep me").unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(8 * 24 * 60 * 60);
        std::fs::File::open(path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();
        crate::pty::cleanup_session("upload-session", &state);
        assert!(
            !path.exists(),
            "closing the session removes attachments older than seven days"
        );
        assert_eq!(
            std::fs::read(&fresh).unwrap(),
            b"keep me",
            "recent attachments survive cleanup"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn attachment_upload_honors_a_configured_one_mib_cap() {
        let cwd = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let state = test_state();
        crate::state::tests_support::insert_dummy_session(&state, "limited-upload");
        crate::state::tests_support::set_session_cwd(
            &state,
            "limited-upload",
            cwd.path().to_str().unwrap(),
        );
        let mut config = serde_json::to_value(state.config.read().clone()).unwrap();
        config["attachment_max_bytes"] = serde_json::json!(1024 * 1024);
        *state.config.write() = serde_json::from_value(config).unwrap();

        let response = build_router(state, false, true)
            .oneshot(
                Request::post("/attachments/upload?kind=pty&id=limited-upload&name=photo.jpg")
                    .header(CONTENT_TYPE, "application/octet-stream")
                    .body(Body::from(vec![0u8; 2 * 1024 * 1024]))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn attachment_upload_excludes_nested_session_directory_from_git() {
        let repo = create_temp_git_repo();
        let nested = repo.path().join("project/subdir");
        std::fs::create_dir_all(&nested).unwrap();
        let state = test_state();
        crate::state::tests_support::insert_dummy_session(&state, "nested-upload");
        crate::state::tests_support::set_session_cwd(
            &state,
            "nested-upload",
            nested.to_str().unwrap(),
        );
        let response = build_router(state, false, true)
            .oneshot(
                Request::post("/attachments/upload?kind=pty&id=nested-upload&name=notes.txt")
                    .body(Body::from("nested bytes"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let reply: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let path = std::path::Path::new(reply["path"].as_str().unwrap());
        assert_eq!(std::fs::read(path).unwrap(), b"nested bytes");
        assert!(
            std::process::Command::new("git")
                .args(["check-ignore", "-q"])
                .arg(path)
                .current_dir(repo.path())
                .status()
                .unwrap()
                .success()
        );
    }

    #[tokio::test]
    async fn attachment_upload_recognizes_acp_targets_without_accepting_unknown_ids() {
        let response = build_router(test_state(), false, true)
            .oneshot(
                Request::post(format!(
                    "/attachments/upload?kind=acp&id={}&name=notes.txt",
                    crate::acp::AcpConnectionId::new()
                ))
                .body(Body::from("bytes"))
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    /// The failure this guards against is a timeout that looks correct on every
    /// request/response route and silently severs `/events` and every PTY
    /// WebSocket. `tower_http`'s timeout stops at the response, so a stream that
    /// answers immediately survives a deadline far shorter than its own life.
    /// If anyone swaps in a layer that wraps the response body, this fails.
    #[tokio::test]
    async fn server_limits_do_not_cut_off_a_long_lived_stream() {
        let resp = limits_test_router(std::time::Duration::from_millis(50))
            .oneshot(Request::get("/stream").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "SSE and WebSocket responses return headers at once; the deadline \
             must not apply to how long they then stream"
        );
    }

    /// Story 760, end-to-end through the real stack `build_router` assembles —
    /// not a direct call to `handle_confirm`. Before the fix, `REQUEST_TIMEOUT`
    /// and `mcp_transport::CONFIRM_TIMEOUT` were both 300s, so the outer
    /// `with_server_limits` layer could win the race, drop the confirm future,
    /// and hand the caller a bare 408 instead of the documented
    /// `{confirmed:false, reason:...}` body — and because the future was
    /// dropped, the handler's own cleanup (the `confirm_responses` entry, the
    /// `McpConfirmResolved` event) never ran either. `start_paused` fast-forwards
    /// both the 300s inner wait and the 301s outer one without real wall-clock
    /// delay, so this proves the ordering at production values rather than at
    /// scaled-down stand-ins.
    #[tokio::test(start_paused = true)]
    async fn confirm_left_unanswered_resolves_clean_through_the_real_server_stack() {
        let state = test_state();
        let mut bus = state.event_bus.subscribe();

        let result = call_mcp_tool(
            &state,
            "ui",
            serde_json::json!({"action": "confirm", "title": "Deploy?"}),
        )
        .await;

        // `call_mcp_tool` already asserts the HTTP status is 200; a 408 from the
        // outer layer would have failed inside it before we ever got here.
        assert_eq!(result["confirmed"], false);
        assert!(
            result["reason"]
                .as_str()
                .is_some_and(|r| r.contains("no answer")),
            "expected the handler's own timeout reason, not a bare 408 body: {result}"
        );
        assert!(
            state.confirm_responses.is_empty(),
            "the router-level path must still clean up the registry entry, the \
             same as calling handle_confirm directly"
        );
        let saw_resolved = std::iter::from_fn(|| bus.try_recv().ok()).any(|event| {
            matches!(
                event,
                crate::state::AppEvent::McpConfirmResolved {
                    confirmed: false,
                    ..
                }
            )
        });
        assert!(
            saw_resolved,
            "clients must still be told to dismiss the dialog even when the \
             outer layer, not the handler, would previously have cut the connection"
        );
    }

    /// Catches: registering agent config only on the desktop router makes
    /// remote hydration return 404 even when the daemon is authenticated.
    #[tokio::test]
    #[serial_test::serial]
    async fn remote_agent_config_round_trip_is_available_to_an_authenticated_client() {
        let dir = tempfile::tempdir().expect("isolated config directory");
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let state = test_state();
        *state.session_token.write() = "remote-agent-config-test".to_string();
        state.config.write().services.auth.lan_auth_bypass = false;
        let app = build_remote_router(state);
        let body = serde_json::json!({
            "base": {"agents": {}},
            "config": {
                "agents": {
                    "claude": {"run_configs": [{"name": "remote", "command": "claude", "args": [], "env": {}, "is_default": true}]}
                }
            }
        });
        let url = "/config/agents?token=remote-agent-config-test";
        let address = std::net::SocketAddr::from(([203, 0, 113, 5], 5555));

        let put = app
            .clone()
            .oneshot(put_from(url, &body, address))
            .await
            .unwrap();
        assert_eq!(put.status(), StatusCode::OK);

        let mut get = Request::get(url)
            .header(header::HOST, "127.0.0.1:9876")
            .body(Body::empty())
            .unwrap();
        get.extensions_mut().insert(ConnectInfo(address));
        let response = app.oneshot(get).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            saved["agents"]["claude"]["run_configs"][0]["name"],
            "remote"
        );
    }

    // Catches: /mcp/suspend-response missing from the router the app serves (the browser/PWA
    // half of session_suspend_response), so a web tab's verdict never reaches the MCP caller.
    #[tokio::test]
    async fn crit1358_suspend_response_route_delivers_the_verdict_and_ignores_unknown_ids() {
        let state = test_state();
        let (tx, rx) = tokio::sync::oneshot::channel();
        state.suspend_responses.insert("req-1".to_string(), tx);

        let unknown = build_router(state.clone(), false, true)
            .oneshot(mcp_post(
                "/mcp/suspend-response",
                &serde_json::json!({"request_id": "other", "ok": true, "reason": null}),
            ))
            .await
            .expect("unknown id response");
        assert_eq!(unknown.status(), StatusCode::OK);
        assert!(state.suspend_responses.contains_key("req-1"));

        let known = build_router(state.clone(), false, true)
            .oneshot(mcp_post(
                "/mcp/suspend-response",
                &serde_json::json!({"request_id": "req-1", "ok": false, "reason": "agent working"}),
            ))
            .await
            .expect("known id response");
        assert_eq!(known.status(), StatusCode::OK);
        assert_eq!(rx.await.unwrap(), Err("agent working".to_string()));
    }

    // Catches: a body without `reason` (the web client sends null, other clients may omit it)
    // being rejected, so a refusal never arrives and the caller waits 20 s.
    #[tokio::test]
    async fn crit1358_suspend_response_route_accepts_a_missing_reason() {
        let state = test_state();
        let (tx, rx) = tokio::sync::oneshot::channel();
        state.suspend_responses.insert("req-2".to_string(), tx);

        let response = build_router(state, false, true)
            .oneshot(mcp_post(
                "/mcp/suspend-response",
                &serde_json::json!({"request_id": "req-2", "ok": false}),
            ))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(rx.await.unwrap(), Err("refused".to_string()));
    }
}

#[cfg(test)]
mod workflow_authority_critic_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// Catches: accepted loopback token actions being refused or recorded as LocalApi instead of Human.
    #[tokio::test]
    async fn loopback_token_exchange_records_human_provenance_for_successful_action() {
        let config =
            tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("isolated config");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project =
            tempfile::tempdir_in(crate::test_support::test_temp_root()).expect("isolated project");
        let store = crate::stories::StoryStore::open().unwrap();
        let plan = store
            .create_plan(crate::stories::NewPlan {
                // Match story_action: the store expects the API's canonical identity.
                project: crate::progress::resolve_owning_project(Some(
                    project.path().to_str().unwrap(),
                ))
                .unwrap()
                .to_string_lossy()
                .into_owned(),
                title: "Operator plan".into(),
                source: "operator.md".into(),
            })
            .unwrap();
        let story = store
            .create_story(crate::stories::NewStory {
                plan_id: plan.id,
                title: "Operator decision".into(),
                criteria: vec!["Approved by the operator".into()],
                priority: 1,
                origin: crate::stories::StoryOrigin::Native,
                file_scope: vec![],
            })
            .unwrap();
        let app = build_router(super::tests::test_state(), false, true);
        let local = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
        let mut request = Request::get("/api/auth/session-token")
            .header(header::HOST, "127.0.0.1:9876")
            .body(Body::empty())
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(local));
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let token = value["token"].as_str().expect("returned token");
        for (index, (with_address, with_token, command)) in [
            (true, true, "block"),
            (false, false, "unblock"),
            (true, false, "block"),
            (false, true, "unblock"),
        ]
        .into_iter()
        .enumerate()
        {
            let current = store.get_story(&story.id).unwrap();
            let body = serde_json::json!({"action": {
                "action": "transition", "story_id": story.id,
                "expected_revision": current.revision, "command": command
            }});
            let mut path = format!("/stories/action?path={}", project.path().display());
            if with_token {
                path.push_str(&format!("&token={token}"));
            }
            let mut request = Request::post(path)
                .header(header::HOST, "127.0.0.1:9876")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            if with_address {
                request.extensions_mut().insert(ConnectInfo(local));
            }
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let expected_status = if command == "block" {
                crate::stories::StoryStatus::Blocked
            } else {
                crate::stories::StoryStatus::Ready
            };
            assert_eq!(store.get_story(&story.id).unwrap().status, expected_status);
            let history = store.transition_history(&story.id).unwrap();
            assert_eq!(history.len(), index + 1);
            assert_eq!(
                history[index].actor,
                if with_token {
                    crate::stories::StoryTransitionActor::Human
                } else {
                    crate::stories::StoryTransitionActor::LocalApi
                }
            );
        }
    }
}
