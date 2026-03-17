mod agent_routes;
pub(crate) mod auth;
mod config_routes;
mod fs_routes;
mod git_routes;
mod log_routes;
mod github_routes;
mod mcp_transport;
mod plugin_docs;
mod session;
mod sse_routes;
mod static_files;
mod types;
mod watcher_routes;
mod worktree_routes;

use crate::AppState;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{header, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{extract::{Path as AxumPath, State}, Json, Router};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

/// Maximum terminal dimension (rows or cols). Prevents resource abuse from
/// absurdly large allocations while still allowing generous sizes.
const MAX_TERMINAL_DIMENSION: u16 = 500;

/// Validate terminal dimensions (rows/cols) are within sane bounds.
fn validate_terminal_size(rows: u16, cols: u16) -> Result<(), String> {
    if rows == 0 || rows > MAX_TERMINAL_DIMENSION {
        return Err(format!("rows must be between 1 and {MAX_TERMINAL_DIMENSION}, got {rows}"));
    }
    if cols == 0 || cols > MAX_TERMINAL_DIMENSION {
        return Err(format!("cols must be between 1 and {MAX_TERMINAL_DIMENSION}, got {cols}"));
    }
    Ok(())
}

/// Core path validation logic shared by HTTP and MCP handlers.
/// Rejects empty paths, null bytes, relative traversals, and non-absolute paths.
fn validate_path_string(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("path is required".to_string());
    }
    if path.contains("..") || path.contains('\0') {
        return Err("Path traversal is not allowed".to_string());
    }
    // Use Path::is_absolute() for the current platform, plus string matching
    // for cross-platform Windows paths (C:\... or \\...) that won't parse as
    // absolute on Unix.
    let is_abs = std::path::Path::new(path).is_absolute()
        || path.get(1..3) == Some(":\\")
        || path.starts_with("\\\\");
    if !is_abs {
        return Err("Path must be absolute".to_string());
    }
    Ok(())
}

/// Validate a repo path for HTTP handlers, returning a 400 error response on failure.
fn validate_repo_path(path: &str) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    validate_path_string(path)
        .map_err(|msg| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": msg}))))
}

/// IPC endpoint path for local MCP bridge connections.
/// Unix: `<config_dir>/mcp.sock` (Unix domain socket)
/// Windows: `\\.\pipe\tuicommander-mcp` (named pipe)
pub(crate) fn socket_path() -> std::path::PathBuf {
    #[cfg(unix)]
    { crate::config::config_dir().join("mcp.sock") }
    #[cfg(windows)]
    { std::path::PathBuf::from(r"\\.\pipe\tuicommander-mcp") }
}

/// Named pipe name for Windows IPC (without the \\.\pipe\ prefix for display).
#[cfg(windows)]
const PIPE_NAME: &str = r"\\.\pipe\tuicommander-mcp";

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
                                tracing::error!(source = "mcp_http", "Failed to create next pipe instance: {e}");
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

/// GET /mcp/upstream-status — returns status + metrics for all upstream MCP servers.
async fn upstream_status_handler(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    Json(state.mcp_upstream_registry.status_snapshot())
}

/// Serve plugin data files over HTTP.
/// Reuses the same sandboxed read logic as the Tauri `read_plugin_data` command.
async fn plugin_data_http(
    AxumPath((plugin_id, path)): AxumPath<(String, String)>,
) -> Response {
    match crate::plugins::read_plugin_data(plugin_id, path) {
        Ok(Some(content)) => {
            let content_type = if content.starts_with('{') || content.starts_with('[') {
                "application/json"
            } else {
                "text/plain"
            };
            (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], content).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

/// Middleware that injects a synthetic `ConnectInfo<SocketAddr>` for IPC
/// connections (Unix socket / named pipe) which lack a TCP peer address.
/// Always uses 127.0.0.1:0 since IPC is inherently local.
async fn inject_localhost_connect_info(
    mut req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::extract::connect_info::ConnectInfo;
    req.extensions_mut().insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))));
    next.run(req).await
}

/// Build the router (exposed for testing).
/// When `remote_auth` is true, applies Basic Auth middleware (requires ConnectInfo).
/// When `mcp_enabled` is false, excludes MCP Streamable HTTP route (/mcp).
pub fn build_router(state: Arc<AppState>, remote_auth: bool, mcp_enabled: bool) -> Router {
    // When remote access is enabled, allow any origin (Basic Auth secures the endpoint).
    // Otherwise, restrict to localhost and Tauri webview origins.
    let cors = if remote_auth {
        CorsLayer::new()
            .allow_origin(tower_http::cors::Any)
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
            .allow_headers([AUTHORIZATION, CONTENT_TYPE])
    } else {
        let allowed_origins = [
            "http://localhost".parse::<axum::http::HeaderValue>().unwrap(),
            "http://127.0.0.1".parse::<axum::http::HeaderValue>().unwrap(),
            "tauri://localhost".parse::<axum::http::HeaderValue>().unwrap(),
            "https://tauri.localhost".parse::<axum::http::HeaderValue>().unwrap(),
        ];
        CorsLayer::new()
            .allow_origin(allowed_origins.to_vec())
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
            .allow_headers([AUTHORIZATION, CONTENT_TYPE])
    };

    let mut routes = Router::new()
        // Health
        .route("/health", get(session::health))
        // Session lifecycle
        .route("/sessions", get(session::list_sessions).post(session::create_session))
        .route("/sessions/{id}/write", post(session::write_to_session))
        .route("/sessions/{id}/resize", post(session::resize_session))
        .route("/sessions/{id}/output", get(session::get_output))
        .route("/sessions/{id}/pause", post(session::pause_session))
        .route("/sessions/{id}/resume", post(session::resume_session))
        .route("/sessions/{id}/kitty-flags", get(session::get_kitty_flags))
        .route("/sessions/{id}/foreground", get(session::get_foreground_process))
        .route("/sessions/{id}", delete(session::close_session))
        // WebSocket streaming
        .route("/sessions/{id}/stream", get(session::ws_stream))
        // Agent sessions
        .route("/sessions/agent", post(agent_routes::spawn_agent_session))
        .route("/sessions/worktree", post(session::create_session_with_worktree))
        // Orchestrator
        .route("/stats", get(session::get_stats))
        .route("/metrics", get(session::get_metrics))
        // Git/GitHub
        .route("/repo/info", get(git_routes::repo_info))
        .route("/repo/remote-url", get(git_routes::remote_url))
        .route("/repo/diff", get(git_routes::repo_diff))
        .route("/repo/diff-stats", get(git_routes::repo_diff_stats))
        .route("/repo/files", get(git_routes::repo_changed_files))
        .route("/repo/github", get(github_routes::repo_github_status))
        .route("/repo/prs", get(github_routes::repo_pr_statuses))
        .route("/repo/branches", get(git_routes::repo_branches))
        .route("/repo/ci", get(github_routes::repo_ci_checks))
        .route("/repo/pr-diff", get(github_routes::repo_pr_diff))
        .route("/repo/approve-pr", post(github_routes::repo_approve_pr))
        .route("/repo/branches/merged", get(git_routes::repo_merged_branches))
        .route("/repo/summary", get(git_routes::repo_summary))
        .route("/repo/structure", get(git_routes::repo_structure))
        .route("/repo/diff-stats/batch", get(git_routes::repo_diff_stats_batch))
        .route("/repo/prs/batch", post(github_routes::repo_all_pr_statuses))
        // Watchers (for browser/mobile clients)
        .route("/watchers/head", post(watcher_routes::start_head_watcher_http).delete(watcher_routes::stop_head_watcher_http))
        .route("/watchers/repo", post(watcher_routes::start_repo_watcher_http).delete(watcher_routes::stop_repo_watcher_http))
        .route("/watchers/dir", post(watcher_routes::start_dir_watcher_http).delete(watcher_routes::stop_dir_watcher_http))
        // Config
        .route("/config", get(config_routes::get_config).put(config_routes::put_config))
        .route("/config/hash-password", post(config_routes::hash_password_http))
        .route("/config/notifications", get(config_routes::get_notification_config).put(config_routes::put_notification_config))
        .route("/config/ui-prefs", get(config_routes::get_ui_prefs).put(config_routes::put_ui_prefs))
        .route("/config/repo-settings", get(config_routes::get_repo_settings).put(config_routes::put_repo_settings))
        .route("/config/repo-settings/has-custom", get(config_routes::check_has_custom_settings_http))
        .route("/config/repo-defaults", get(config_routes::get_repo_defaults).put(config_routes::put_repo_defaults))
        .route("/config/repositories", get(config_routes::get_repositories).put(config_routes::put_repositories))
        .route("/config/prompt-library", get(config_routes::get_prompt_library).put(config_routes::put_prompt_library))
        // Logs
        .route("/logs", get(log_routes::get_logs).post(log_routes::push_log).delete(log_routes::clear_logs))
        // Worktrees
        .route("/worktrees", get(worktree_routes::list_worktrees_http).post(worktree_routes::create_worktree_http))
        .route("/worktrees/dir", get(worktree_routes::get_worktrees_dir_http))
        .route("/worktrees/paths", get(worktree_routes::get_worktree_paths_http))
        .route("/worktrees/generate-name", post(worktree_routes::generate_worktree_name_http))
        .route("/worktrees/finalize", post(worktree_routes::finalize_merged_worktree_http))
        .route("/worktrees/{branch}", delete(worktree_routes::remove_worktree_http))
        // File operations
        .route("/repo/file", get(git_routes::read_file_http))
        .route("/repo/file-diff", get(git_routes::get_file_diff_http))
        .route("/repo/markdown-files", get(git_routes::list_markdown_files_http))
        // Branch operations
        .route("/repo/local-branches", get(worktree_routes::list_local_branches_http))
        .route("/repo/checkout-remote", post(worktree_routes::checkout_remote_branch_http))
        .route("/repo/orphan-worktrees", get(worktree_routes::detect_orphan_worktrees_http))
        .route("/repo/remove-orphan", post(worktree_routes::remove_orphan_worktree_http))
        .route("/repo/merge-pr", post(worktree_routes::merge_pr_via_github_http))
        .route("/repo/branch/rename", post(git_routes::rename_branch_http))
        .route("/repo/initials", get(git_routes::get_initials_http))
        .route("/repo/is-main-branch", get(git_routes::check_is_main_branch_http))
        // Prompt processing
        .route("/prompt/process", post(agent_routes::process_prompt_http))
        .route("/prompt/extract-variables", post(agent_routes::extract_prompt_variables_http))
        // Agents
        .route("/agents", get(agent_routes::detect_agents))
        .route("/agents/detect", get(agent_routes::detect_agent_binary_http))
        .route("/agents/ides", get(agent_routes::detect_installed_ides_http))
        // File browser
        .route("/fs/list", get(fs_routes::list_directory_http))
        .route("/fs/search", get(fs_routes::search_files_http))
        .route("/fs/search-content", get(fs_routes::search_content_http))
        .route("/fs/read", get(fs_routes::fs_read_file_http))
        .route("/fs/read-external", get(fs_routes::read_external_file_http))
        .route("/fs/write", post(fs_routes::write_file_http))
        .route("/fs/mkdir", post(fs_routes::create_directory_http))
        .route("/fs/delete", post(fs_routes::delete_path_http))
        .route("/fs/rename", post(fs_routes::rename_path_http))
        .route("/fs/copy", post(fs_routes::copy_path_http))
        .route("/fs/gitignore", post(fs_routes::add_to_gitignore_http))
        // Notes
        .route("/config/notes", get(config_routes::get_notes).put(config_routes::put_notes))
        // Recent commits
        .route("/repo/recent-commits", get(git_routes::get_recent_commits_http))
        // GitPanel commands
        .route("/repo/panel-context", get(git_routes::git_panel_context))
        .route("/repo/run-git", post(git_routes::run_git_command_http))
        .route("/repo/working-tree-status", get(git_routes::working_tree_status))
        .route("/repo/stage", post(git_routes::stage_files_http))
        .route("/repo/unstage", post(git_routes::unstage_files_http))
        .route("/repo/discard", post(git_routes::discard_files_http))
        .route("/repo/commit", post(git_routes::git_commit_http))
        .route("/repo/commit-log", get(git_routes::commit_log_http))
        .route("/repo/stash", get(git_routes::stash_list_http))
        .route("/repo/stash/apply", post(git_routes::stash_apply_http))
        .route("/repo/stash/pop", post(git_routes::stash_pop_http))
        .route("/repo/stash/drop", post(git_routes::stash_drop_http))
        .route("/repo/stash/show", get(git_routes::stash_show_http))
        .route("/repo/file-history", get(git_routes::file_history_http))
        .route("/repo/file-blame", get(git_routes::file_blame_http))
        // System
        .route("/system/local-ips", get(git_routes::get_local_ips_http))
        .route("/system/local-ip", get(git_routes::get_local_ip_http))
        // Plugins
        .route("/plugins/list", get(git_routes::list_user_plugins_http))
        // Server-Sent Events (for browser/mobile clients)
        .route("/events", get(sse_routes::sse_events))
        // MCP status + instructions
        .route("/mcp/status", get(config_routes::get_mcp_status_http))
        .route("/mcp/upstream-status", get(upstream_status_handler))
        .route("/mcp/instructions", get(mcp_transport::mcp_instructions_http))
        // Plugin docs (for MCP bridge)
        .route("/plugins/docs", get(plugin_dev_guide_handler))
        // Plugin data (for external HTTP clients)
        .route("/api/plugins/{plugin_id}/data/{*path}", get(plugin_data_http));

    // MCP Streamable HTTP transport — only when MCP is enabled
    if mcp_enabled {
        routes = routes
            .route("/mcp", post(mcp_transport::mcp_post)
                          .get(mcp_transport::mcp_get)
                          .delete(mcp_transport::mcp_delete));
    }

    // Static files — SPA frontend
    let routes = routes
        .route("/", get(static_files::serve_index))
        .route("/{*path}", get(static_files::serve_static))
        .with_state(state.clone())
        .layer(cors);

    if remote_auth {
        routes.layer(axum::middleware::from_fn_with_state(state, auth::basic_auth_middleware))
    } else {
        routes
    }
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
pub async fn start_server(state: Arc<AppState>, mcp_enabled: bool, remote_enabled: bool) {
    let config = state.config.read().clone();

    // Register shutdown channel so save_config can restart server
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    *state.server_shutdown.lock() = Some(shutdown_tx);

    // Spawn MCP session reaper: evicts stale protocol sessions every 60s (1h TTL)
    let reaper_state = state.clone();
    let reaper_handle = tokio::spawn(async move {
        const MCP_SESSION_TTL: std::time::Duration = std::time::Duration::from_secs(3600);
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let now = std::time::Instant::now();
            reaper_state.mcp_sessions.retain(|_id, created_at| {
                now.duration_since(*created_at) < MCP_SESSION_TTL
            });
        }
    });

    // Spawn upstream health checker: pings Ready upstreams every 60s
    crate::mcp_proxy::registry::UpstreamRegistry::spawn_health_checker(
        Arc::clone(&state.mcp_upstream_registry),
    );

    // --- Unix socket listener (always on, no auth) ---
    #[cfg(unix)]
    let socket_handle = {
        let sock = socket_path();

        if let Some(parent) = sock.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            tracing::warn!(source = "mcp_http", path = %parent.display(), "Failed to create socket parent dir: {e}");
        }

        // Bind the socket. Remove any stale file first (left by a crashed previous run).
        // NOTE: No SocketGuard here — cleanup on server restart would race with the new
        // instance's bind. Explicit removal happens in the shutdown sequence below (line ~567).
        fn bind_unix_socket(sock: &std::path::Path) -> Result<tokio::net::UnixListener, std::io::Error> {
            for attempt in 0..3u8 {
                let _ = std::fs::remove_file(sock);
                match tokio::net::UnixListener::bind(sock) {
                    Ok(uds) => return Ok(uds),
                    Err(e) => {
                        tracing::warn!(source = "mcp_http", attempt, path = %sock.display(), "Unix socket bind failed: {e}");
                        if attempt < 2 {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        } else {
                            return Err(e);
                        }
                    }
                }
            }
            unreachable!()
        }

        match bind_unix_socket(&sock) {
            Err(e) => {
                tracing::error!(source = "mcp_http", path = %sock.display(), "Failed to bind Unix socket after retries: {e}");
                None
            }
            Ok(initial_uds) => {
                tracing::info!(source = "mcp_http", path = %sock.display(), "Unix socket listening");
                // Watchdog task: if axum::serve() returns (crash or abort), log it.
                // On graceful shutdown h.abort() is called — the task exits cleanly,
                // and the explicit remove_file below handles cleanup.
                Some(tokio::spawn({
                    let state = state.clone();
                    async move {
                        let mut uds = initial_uds;
                        loop {
                            let app = build_router(state.clone(), false, true);
                            let app = app.layer(axum::middleware::from_fn(inject_localhost_connect_info));
                            if let Err(e) = axum::serve(uds, app.into_make_service()).await {
                                tracing::error!(source = "mcp_http", "Unix socket server error: {e}");
                            } else {
                                // Clean exit (abort signal) — stop the watchdog.
                                break;
                            }
                            // Unexpected exit — rebind and restart.
                            tracing::warn!(source = "mcp_http", path = %sock.display(), "Unix socket server stopped unexpectedly, restarting…");
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                            match bind_unix_socket(&sock) {
                                Ok(new_uds) => {
                                    tracing::info!(source = "mcp_http", path = %sock.display(), "Unix socket rebound successfully");
                                    uds = new_uds;
                                }
                                Err(_) => {
                                    tracing::error!(source = "mcp_http", path = %sock.display(), "Unix socket rebind failed permanently — MCP bridge will be unavailable");
                                    break;
                                }
                            }
                        }
                    }
                }))
            }
        }
    };

    // --- Windows named pipe listener (always on, no auth) ---
    #[cfg(windows)]
    let pipe_handle = {
        match NamedPipeListener::new() {
            Ok(pipe) => {
                tracing::info!(source = "mcp_http", pipe = PIPE_NAME, "Named pipe listening");
                let app = build_router(state.clone(), false, true);
                let app = app.layer(axum::middleware::from_fn(inject_localhost_connect_info));
                Some(tokio::spawn(async move {
                    if let Err(e) = axum::serve(pipe, app.into_make_service()).await {
                        tracing::error!(source = "mcp_http", "Named pipe server error: {e}");
                    }
                }))
            }
            Err(e) => {
                tracing::error!(source = "mcp_http", pipe = PIPE_NAME, "Failed to create named pipe: {e}");
                None
            }
        }
    };

    // --- TCP listener (only for remote access with auth) ---
    let tcp_handle = if remote_enabled {
        let port = if config.remote_access_port == 0 { 0 } else { config.remote_access_port };
        let bind_addr = if config.ipv6_enabled {
            format!("[::]:{port}")
        } else {
            format!("0.0.0.0:{port}")
        };
        match tokio::net::TcpListener::bind(&bind_addr).await {
            Ok(listener) => {
                let addr = listener.local_addr().unwrap_or_else(|_| {
                    std::net::SocketAddr::from(([0, 0, 0, 0], 0))
                });
                tracing::info!(source = "mcp_http", %addr, "TCP listening (remote access enabled)");

                let app = build_router(state.clone(), true, mcp_enabled);
                Some(tokio::spawn(async move {
                    if let Err(e) = axum::serve(
                        listener,
                        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
                    ).await {
                        tracing::error!(source = "mcp_http", "TCP server error: {e}");
                    }
                }))
            }
            Err(e) => {
                tracing::error!(source = "mcp_http", bind_addr = %bind_addr, "Failed to bind TCP: {e}");
                None
            }
        }
    } else {
        None
    };

    // Wait for shutdown signal
    let _ = shutdown_rx.await;

    // Abort listeners
    #[cfg(unix)]
    if let Some(h) = socket_handle {
        h.abort();
    }
    #[cfg(windows)]
    if let Some(h) = pipe_handle {
        h.abort();
    }
    if let Some(h) = tcp_handle {
        h.abort();
    }
    reaper_handle.abort();

    // Cleanup socket file (Unix only — named pipes clean up automatically)
    #[cfg(unix)]
    let _ = std::fs::remove_file(socket_path());
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::connect_info::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use dashmap::DashMap;
    use tower::ServiceExt;
    use crate::MAX_CONCURRENT_SESSIONS;

    /// Build a POST request with ConnectInfo from the given address.
    fn mcp_post_from(url: &str, body: &serde_json::Value, addr: std::net::SocketAddr) -> Request<Body> {
        let mut req = Request::post(url)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(body).expect("serialize JSON body")))
            .expect("build POST request");
        req.extensions_mut().insert(ConnectInfo(addr));
        req
    }

    /// Build a POST request with ConnectInfo set to localhost (the common case).
    fn mcp_post(url: &str, body: &serde_json::Value) -> Request<Body> {
        mcp_post_from(url, body, std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
    }

    /// Build a PUT request with ConnectInfo from the given address.
    fn put_from(url: &str, body: &serde_json::Value, addr: std::net::SocketAddr) -> Request<Body> {
        let mut req = Request::put(url)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(body).expect("serialize JSON body")))
            .expect("build PUT request");
        req.extensions_mut().insert(ConnectInfo(addr));
        req
    }

    fn test_state() -> Arc<AppState> {
        Arc::new(AppState {
            sessions: DashMap::new(),
            worktrees_dir: std::env::temp_dir().join("test-worktrees"),
            metrics: crate::SessionMetrics::new(),
            output_buffers: DashMap::new(),
            mcp_sessions: DashMap::new(),
            ws_clients: DashMap::new(),
            config: parking_lot::RwLock::new(crate::config::AppConfig::default()),
            git_cache: crate::state::GitCacheState::new(),
            head_watchers: DashMap::new(),
            repo_watchers: DashMap::new(),
            dir_watchers: DashMap::new(),
            http_client: reqwest::Client::new(),
            github_token: parking_lot::RwLock::new(None),
            github_circuit_breaker: crate::github::GitHubCircuitBreaker::new(),
            server_shutdown: parking_lot::Mutex::new(None),
            session_token: parking_lot::RwLock::new(uuid::Uuid::new_v4().to_string()),
            app_handle: parking_lot::RwLock::new(None),
            plugin_watchers: DashMap::new(),
            vt_log_buffers: DashMap::new(),
            kitty_states: DashMap::new(),
            input_buffers: DashMap::new(),
            last_prompts: DashMap::new(),
            silence_states: DashMap::new(),
            claude_usage_cache: parking_lot::Mutex::new(std::collections::HashMap::new()),
            log_buffer: std::sync::Arc::new(parking_lot::Mutex::new(crate::app_logger::LogRingBuffer::new(crate::app_logger::LOG_RING_CAPACITY))),
            event_bus: tokio::sync::broadcast::channel(256).0,
            event_counter: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            session_states: dashmap::DashMap::new(),
            mcp_upstream_registry: std::sync::Arc::new(crate::mcp_proxy::registry::UpstreamRegistry::new()),
            mcp_tools_changed: tokio::sync::broadcast::channel(16).0,
            slash_mode: DashMap::new(),
            last_output_ms: DashMap::new(),
            shell_states: DashMap::new(),
            loaded_plugins: DashMap::new(),
            relay: crate::state::RelayState::new(),
        })
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
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ok"], true);
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
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json, serde_json::json!([]));
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
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
        let resp = app
            .oneshot(Request::get("/config").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // Should have default font_family
        assert!(config["font_family"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_config_strips_password_hash() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(Request::get("/config").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let config: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(config.get("remote_access_password_hash").is_none(),
            "Password hash should be stripped from HTTP response");
    }

    #[tokio::test]
    async fn test_config_save_rejects_non_loopback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let remote_addr = std::net::SocketAddr::from(([192, 168, 1, 100], 12345));
        let body = serde_json::to_value(crate::config::AppConfig::default())
            .expect("serialize default AppConfig");
        let resp = app
            .oneshot(put_from("/config", &body, remote_addr))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN,
            "Config save from non-loopback address should be rejected");
    }

    #[tokio::test]
    async fn test_notification_config_rejects_non_loopback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let remote_addr = std::net::SocketAddr::from(([192, 168, 1, 100], 12345));
        let body = serde_json::json!({"sound_enabled": false, "flash_enabled": false, "defer_secs": 10});
        let resp = app.oneshot(put_from("/config/notifications", &body, remote_addr)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN,
            "Notification config save from non-loopback should be rejected");
    }

    #[tokio::test]
    async fn test_ui_prefs_rejects_non_loopback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let remote_addr = std::net::SocketAddr::from(([10, 0, 0, 1], 9999));
        let body = serde_json::json!({});
        let resp = app.oneshot(put_from("/config/ui-prefs", &body, remote_addr)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN,
            "UI prefs save from non-loopback should be rejected");
    }

    #[tokio::test]
    async fn test_repo_settings_rejects_non_loopback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let remote_addr = std::net::SocketAddr::from(([172, 16, 0, 5], 4000));
        let body = serde_json::json!({});
        let resp = app.oneshot(put_from("/config/repo-settings", &body, remote_addr)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN,
            "Repo settings save from non-loopback should be rejected");
    }

    #[tokio::test]
    async fn test_repositories_rejects_non_loopback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let remote_addr = std::net::SocketAddr::from(([192, 168, 1, 50], 8080));
        let body = serde_json::json!({});
        let resp = app.oneshot(put_from("/config/repositories", &body, remote_addr)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN,
            "Repositories save from non-loopback should be rejected");
    }

    #[tokio::test]
    async fn test_prompt_library_rejects_non_loopback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let remote_addr = std::net::SocketAddr::from(([10, 10, 10, 1], 3000));
        let body = serde_json::json!({"prompts": []});
        let resp = app.oneshot(put_from("/config/prompt-library", &body, remote_addr)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN,
            "Prompt library save from non-loopback should be rejected");
    }

    #[tokio::test]
    async fn test_notes_rejects_non_loopback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let remote_addr = std::net::SocketAddr::from(([192, 168, 0, 1], 5000));
        let body = serde_json::json!({});
        let resp = app.oneshot(put_from("/config/notes", &body, remote_addr)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN,
            "Notes save from non-loopback should be rejected");
    }

    #[tokio::test]
    async fn test_read_external_rejects_path_outside_repos() {
        let state = test_state();
        let app = build_router(state, false, true);
        // No repos registered in test_state → any path should be rejected
        let resp = app
            .oneshot(Request::get("/fs/read-external?path=/etc/passwd").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN,
            "read-external should reject paths outside registered repos");
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
    }

    #[test]
    fn test_validate_repo_path_accepts_absolute_unix() {
        assert!(validate_repo_path("/Users/test/repos/my-project").is_ok());
    }

    #[test]
    fn test_validate_repo_path_accepts_absolute_windows() {
        assert!(validate_repo_path("C:\\Users\\test\\repos").is_ok());
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
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let agents = json.as_array().unwrap();
        assert_eq!(agents.len(), 5);
        let names: Vec<&str> = agents.iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"claude"));
        assert!(names.contains(&"lazygit"));
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
        let resp = app
            .oneshot(mcp_post("/mcp", &body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // Verify Mcp-Session-Id header is present
        let session_id = resp.headers().get("mcp-session-id");
        assert!(session_id.is_some(), "Initialize should return Mcp-Session-Id header");
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["result"]["protocolVersion"], "2025-03-26");
        assert!(json["result"]["serverInfo"]["name"].as_str().is_some());
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
        state.mcp_sessions.insert("test-sid".to_string(), std::time::Instant::now());
        let app = build_router(state.clone(), false, true);
        let mut req = Request::delete("/mcp")
            .header("mcp-session-id", "test-sid")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(state.mcp_sessions.get("test-sid").is_none(), "Session should be removed after DELETE");
    }

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
        let resp = app
            .oneshot(mcp_post("/mcp", &body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tools = json["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 7);

        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"session"));
        assert!(names.contains(&"git"));
        assert!(names.contains(&"agent"));
        assert!(names.contains(&"config"));
        assert!(names.contains(&"workspace"));
        assert!(names.contains(&"notify"));
        assert!(names.contains(&"plugin_dev_guide"));
    }

    #[test]
    fn test_mcp_tool_definitions_count() {
        let tools = mcp_transport::test_mcp_tool_definitions();
        let arr = tools.as_array().unwrap();
        assert_eq!(arr.len(), 7);
    }

    #[test]
    fn test_translate_special_key() {
        assert_eq!(mcp_transport::test_translate_special_key("enter"), Some("\r"));
        assert_eq!(mcp_transport::test_translate_special_key("ctrl+c"), Some("\x03"));
        assert_eq!(mcp_transport::test_translate_special_key("tab"), Some("\t"));
        assert_eq!(mcp_transport::test_translate_special_key("up"), Some("\x1b[A"));
        assert_eq!(mcp_transport::test_translate_special_key("unknown"), None);
    }

    // --- MCP tool call tests ---
    // Test tool calls through the SSE transport (tools/call via /messages endpoint)

    /// Build a POST request to /mcp with an mcp-session-id header.
    fn mcp_post_with_session(url: &str, body: &serde_json::Value, session_id: &str) -> Request<Body> {
        let mut req = Request::post(url)
            .header("content-type", "application/json")
            .header("mcp-session-id", session_id)
            .body(Body::from(serde_json::to_string(body).expect("serialize JSON body")))
            .expect("build POST request");
        req.extensions_mut().insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))));
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
    async fn call_mcp_tool(state: &Arc<AppState>, tool_name: &str, args: serde_json::Value) -> serde_json::Value {
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

        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
        assert!(result["error"].as_str().unwrap().contains("requires 'session_id'"));
    }

    #[tokio::test]
    async fn test_session_input_nonexistent_session() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({
            "action": "input",
            "session_id": "nonexistent",
            "input": "hello"
        })).await;
        assert_eq!(result["error"], "Session not found");
    }

    #[tokio::test]
    async fn test_session_input_no_input_or_key() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({
            "action": "input",
            "session_id": "some-id"
        })).await;
        assert!(result["error"].as_str().unwrap().contains("'input'"));
    }

    #[tokio::test]
    async fn test_session_input_unknown_special_key() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({
            "action": "input",
            "session_id": "some-id",
            "special_key": "nonexistent_key"
        })).await;
        assert!(result["error"].as_str().unwrap().contains("Unknown special key"));
    }

    #[tokio::test]
    async fn test_session_output_missing_session_id() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({"action": "output"})).await;
        assert!(result["error"].as_str().unwrap().contains("requires 'session_id'"));
    }

    #[tokio::test]
    async fn test_session_output_nonexistent_session() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({
            "action": "output",
            "session_id": "nonexistent"
        })).await;
        assert_eq!(result["error"], "Session not found");
    }

    #[tokio::test]
    async fn test_session_resize_missing_session_id() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({"action": "resize"})).await;
        assert!(result["error"].as_str().unwrap().contains("requires 'session_id'"));
    }

    #[tokio::test]
    async fn test_session_resize_nonexistent_session() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({
            "action": "resize",
            "session_id": "nonexistent",
            "rows": 40,
            "cols": 120
        })).await;
        assert_eq!(result["error"], "Session not found");
    }

    #[tokio::test]
    async fn test_session_close_missing_session_id() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({"action": "close"})).await;
        assert!(result["error"].as_str().unwrap().contains("requires 'session_id'"));
    }

    #[tokio::test]
    async fn test_session_close_nonexistent() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({
            "action": "close",
            "session_id": "nonexistent"
        })).await;
        assert_eq!(result["error"], "Session not found");
    }

    #[tokio::test]
    async fn test_session_pause_missing_session_id() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({"action": "pause"})).await;
        assert!(result["error"].as_str().unwrap().contains("requires 'session_id'"));
    }

    #[tokio::test]
    async fn test_session_pause_nonexistent() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({
            "action": "pause",
            "session_id": "nonexistent"
        })).await;
        assert_eq!(result["error"], "Session not found");
    }

    #[tokio::test]
    async fn test_session_resume_missing_session_id() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({"action": "resume"})).await;
        assert!(result["error"].as_str().unwrap().contains("requires 'session_id'"));
    }

    #[tokio::test]
    async fn test_session_resume_nonexistent() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({
            "action": "resume",
            "session_id": "nonexistent"
        })).await;
        assert_eq!(result["error"], "Session not found");
    }

    // --- Agent meta-command tests ---

    #[tokio::test]
    async fn test_agent_stats() {
        let state = test_state();
        let result = call_mcp_tool(&state, "agent", serde_json::json!({"action": "stats"})).await;
        assert_eq!(result["active_sessions"], 0);
        assert_eq!(result["max_sessions"], MAX_CONCURRENT_SESSIONS);
    }

    #[tokio::test]
    async fn test_agent_metrics() {
        let state = test_state();
        let result = call_mcp_tool(&state, "agent", serde_json::json!({"action": "metrics"})).await;
        assert_eq!(result["total_spawned"], 0);
        assert_eq!(result["active_sessions"], 0);
    }

    #[tokio::test]
    async fn test_agent_detect() {
        let state = test_state();
        let result = call_mcp_tool(&state, "agent", serde_json::json!({"action": "detect"})).await;
        let agents = result.as_array().unwrap();
        assert_eq!(agents.len(), 5, "Should detect 5 known agents");
        let names: Vec<&str> = agents.iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"claude"));
        assert!(names.contains(&"lazygit"));
    }

    #[tokio::test]
    async fn test_agent_spawn_missing_prompt() {
        let state = test_state();
        let result = call_mcp_tool(&state, "agent", serde_json::json!({"action": "spawn"})).await;
        assert!(result["error"].as_str().unwrap().contains("requires 'prompt'"));
    }

    #[tokio::test]
    async fn test_agent_spawn_unknown_binary() {
        let state = test_state();
        let result = call_mcp_tool(&state, "agent", serde_json::json!({
            "action": "spawn",
            "prompt": "test task",
            "agent_type": "nonexistent-agent"
        })).await;
        assert!(result["error"].as_str().unwrap().contains("not found"));
    }

    // --- Config meta-command tests ---

    #[tokio::test]
    async fn test_config_get() {
        let state = test_state();
        let result = call_mcp_tool(&state, "config", serde_json::json!({"action": "get"})).await;
        assert!(result["font_family"].as_str().is_some());
        assert!(result.get("remote_access_password_hash").is_none(),
            "Password hash should be stripped from MCP tool response");
    }

    #[tokio::test]
    async fn test_config_save_missing_config() {
        let state = test_state();
        let result = call_mcp_tool(&state, "config", serde_json::json!({"action": "save"})).await;
        assert!(result["error"].as_str().unwrap().contains("requires 'config'"));
    }

    #[tokio::test]
    async fn test_config_save_invalid_config() {
        let state = test_state();
        let result = call_mcp_tool(&state, "config", serde_json::json!({
            "action": "save",
            "config": "not an object"
        })).await;
        assert!(result["error"].as_str().unwrap().contains("Invalid config"));
    }

    // --- Git meta-command tests ---

    #[tokio::test]
    async fn test_git_info_missing_path() {
        let state = test_state();
        let result = call_mcp_tool(&state, "git", serde_json::json!({"action": "info"})).await;
        assert!(result["error"].as_str().unwrap().contains("requires 'path'"));
    }

    #[tokio::test]
    async fn test_git_info_nonexistent_path() {
        let state = test_state();
        let result = call_mcp_tool(&state, "git", serde_json::json!({
            "action": "info",
            "path": "/nonexistent/repo/path"
        })).await;
        assert!(result.is_object(), "Expected a JSON object for nonexistent repo path");
    }

    #[tokio::test]
    async fn test_git_diff_missing_path() {
        let state = test_state();
        let result = call_mcp_tool(&state, "git", serde_json::json!({"action": "diff"})).await;
        assert!(result["error"].as_str().unwrap().contains("requires 'path'"));
    }

    #[tokio::test]
    async fn test_git_files_missing_path() {
        let state = test_state();
        let result = call_mcp_tool(&state, "git", serde_json::json!({"action": "files"})).await;
        assert!(result["error"].as_str().unwrap().contains("requires 'path'"));
    }

    #[tokio::test]
    async fn test_git_github_missing_path() {
        let state = test_state();
        let result = call_mcp_tool(&state, "git", serde_json::json!({"action": "github"})).await;
        assert!(result["error"].as_str().unwrap().contains("requires 'path'"));
    }

    #[tokio::test]
    async fn test_git_prs_missing_path() {
        let state = test_state();
        let result = call_mcp_tool(&state, "git", serde_json::json!({"action": "prs"})).await;
        assert!(result["error"].as_str().unwrap().contains("requires 'path'"));
    }

    #[tokio::test]
    async fn test_git_branches_missing_path() {
        let state = test_state();
        let result = call_mcp_tool(&state, "git", serde_json::json!({"action": "branches"})).await;
        assert!(result["error"].as_str().unwrap().contains("requires 'path'"));
    }

    // --- Action routing error tests ---

    #[tokio::test]
    async fn test_session_missing_action() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({})).await;
        assert!(result["error"].as_str().unwrap().contains("Missing 'action'"));
        assert!(result["error"].as_str().unwrap().contains("list, create"));
    }

    #[tokio::test]
    async fn test_session_unknown_action() {
        let state = test_state();
        let result = call_mcp_tool(&state, "session", serde_json::json!({"action": "status"})).await;
        assert!(result["error"].as_str().unwrap().contains("Unknown action 'status'"));
        assert!(result["error"].as_str().unwrap().contains("session"));
    }

    #[tokio::test]
    async fn test_git_missing_action() {
        let state = test_state();
        let result = call_mcp_tool(&state, "git", serde_json::json!({})).await;
        assert!(result["error"].as_str().unwrap().contains("Missing 'action'"));
    }

    #[tokio::test]
    async fn test_git_unknown_action() {
        let state = test_state();
        let result = call_mcp_tool(&state, "git", serde_json::json!({"action": "commit"})).await;
        assert!(result["error"].as_str().unwrap().contains("Unknown action 'commit'"));
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let state = test_state();
        let result = call_mcp_tool(&state, "nonexistent_tool", serde_json::json!({})).await;
        assert!(result["error"].as_str().unwrap().contains("Unknown tool"));
        assert!(result["error"].as_str().unwrap().contains("session, git, agent, config"));
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
        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["result"]["isError"], true, "Error responses should set isError=true");
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
        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["result"]["isError"], false, "Success responses should set isError=false");
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
        let resp = app
            .oneshot(mcp_post("/mcp", &body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["error"]["code"], -32601, "Unknown method should return -32601");
        assert!(json["error"]["message"].as_str().unwrap().contains("Method not found"));
    }

    #[tokio::test]
    async fn test_tools_call_requires_session() {
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
        // Send without mcp-session-id header — should be rejected
        let resp = app
            .oneshot(mcp_post("/mcp", &body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(json["error"]["code"], -32600, "Missing session should return -32600");
        assert!(json["error"]["message"].as_str().unwrap().contains("mcp-session-id"));
    }

    // --- Auth validation tests ---
    // Tests for the pure validate_basic_auth function (no server needed).

    fn test_hash(password: &str) -> String {
        bcrypt::hash(password, 4).unwrap() // cost=4 for fast tests
    }

    fn basic_header(username: &str, password: &str) -> String {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(format!("{username}:{password}"));
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
        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            "nocolon",
        );
        let header = format!("Basic {encoded}");
        assert!(matches!(
            auth::validate_basic_auth(Some(&header), "admin", &hash),
            auth::AuthResult::Invalid
        ));
    }

    // --- Static file serving tests ---

    #[tokio::test]
    async fn test_serve_index_html() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/html"), "Expected text/html, got {ct}");
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("<!DOCTYPE html>") || text.contains("<html"), "index.html should contain HTML");
    }

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
            let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
            assert!(ct.contains("javascript"), "Expected javascript MIME, got {ct}");
        }
    }

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
            let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
            assert!(ct.contains("font") || ct.contains("woff2") || ct.contains("octet-stream"),
                "Expected font MIME, got {ct}");
        }
    }

    #[tokio::test]
    async fn test_spa_fallback() {
        let state = test_state();
        let app = build_router(state, false, true);
        let resp = app
            .oneshot(Request::get("/some/unknown/spa/route").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/html"), "SPA fallback should return HTML, got {ct}");
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
        assert!(cors.is_none(), "Unknown origin should not be allowed, got: {:?}", cors);
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
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
        let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx2, _rx2) = tokio::sync::mpsc::unbounded_channel::<String>();

        // Add clients
        state.ws_clients.entry("session-1".to_string()).or_default().push(tx1);
        state.ws_clients.entry("session-1".to_string()).or_default().push(tx2);
        assert_eq!(state.ws_clients.get("session-1").unwrap().len(), 2);

        // Remove session cleans up all clients
        state.ws_clients.remove("session-1");
        assert!(state.ws_clients.get("session-1").is_none());
    }

    #[test]
    fn test_ws_clients_retain_disconnected() {
        // Verify that retain removes closed channels
        let state = test_state();
        let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx2, rx2) = tokio::sync::mpsc::unbounded_channel::<String>();

        state.ws_clients.entry("sess".to_string()).or_default().push(tx1);
        state.ws_clients.entry("sess".to_string()).or_default().push(tx2);

        // Drop rx2 so tx2 is closed
        drop(rx2);

        // Retain should remove the closed channel
        if let Some(mut clients) = state.ws_clients.get_mut("sess") {
            clients.retain(|tx| tx.send("test".to_string()).is_ok());
        }

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
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            state.ws_clients.entry(session_id.clone()).or_default().push(tx);
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
        let remaining = state.ws_clients.get(&session_id)
            .map(|c| c.len())
            .unwrap_or(0);
        assert_eq!(remaining, 0, "dead senders should be purged on WS close");
    }

    #[test]
    fn test_ws_clients_purge_preserves_live_senders() {
        // Purge should only remove dead senders, keeping live ones
        let state = test_state();
        let session_id = "mixed-session".to_string();

        let (tx_live, _rx_live) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (tx_dead, rx_dead) = tokio::sync::mpsc::unbounded_channel::<String>();

        state.ws_clients.entry(session_id.clone()).or_default().push(tx_live);
        state.ws_clients.entry(session_id.clone()).or_default().push(tx_dead);

        // Kill one sender
        drop(rx_dead);

        crate::state::purge_dead_ws_clients(&state.ws_clients, &session_id);

        // Only the live sender should remain
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
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let tools = json["result"]["tools"].as_array().unwrap();
        // No upstream → exactly 7 native tools
        assert_eq!(tools.len(), 7);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"session"));
        assert!(names.contains(&"git"));
        assert!(names.contains(&"agent"));
        assert!(names.contains(&"config"));
        assert!(names.contains(&"plugin_dev_guide"));
    }

    /// tools/call with upstream-prefixed name returns error (no upstream registered).
    #[tokio::test]
    async fn test_tools_call_upstream_prefix_returns_error_when_no_upstream() {
        let state = test_state();
        // Inject a session so the session_valid check passes
        state.mcp_sessions.insert("test-sid-proxy".to_string(), std::time::Instant::now());

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
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // Should be an error response with isError:true
        assert_eq!(json["result"]["isError"], true, "Expected isError:true, got: {json}");
    }

    /// tools/call with a native tool name still works after wiring.
    #[tokio::test]
    async fn test_native_tool_call_still_works_after_wiring() {
        let state = test_state();
        state.mcp_sessions.insert("test-sid-native".to_string(), std::time::Instant::now());
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
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // Native tool — should succeed (empty session list, no error)
        assert_eq!(json["result"]["isError"], false, "Native tool should not error: {json}");
    }

    /// Changing disabled_native_tools via put_config fires mcp_tools_changed.
    #[tokio::test]
    async fn test_config_change_fires_tools_changed() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(tmp.path().to_path_buf());

        let state = test_state();
        let mut rx = state.mcp_tools_changed.subscribe();

        // Save config with a disabled tool
        let mut config = state.config.read().clone();
        config.disabled_native_tools = vec!["session".to_string()];
        let app = build_router(state.clone(), false, true);
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
        let resp = app.oneshot(put_from("/config", &serde_json::to_value(&config).unwrap(), addr)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Should have received a tools_changed signal
        let result = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await;
        assert!(result.is_ok(), "expected tools_changed signal after config change");
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
            axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
                .await.unwrap();
        });

        let client = reqwest::Client::new();

        // Establish MCP session via initialize
        let init_body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "initialize",
            "params": { "protocolVersion": "2025-03-26", "capabilities": {},
                        "clientInfo": { "name": "test", "version": "0.1" } }
        });
        let resp = client.post(format!("http://{addr}/mcp"))
            .json(&init_body)
            .send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let session_id = resp.headers()
            .get("mcp-session-id").expect("initialize should return session id")
            .to_str().unwrap().to_string();

        // GET /mcp with session header → SSE stream
        let resp = client.get(format!("http://{addr}/mcp"))
            .header("mcp-session-id", &session_id)
            .header("Accept", "text/event-stream")
            .send().await.unwrap();
        assert_eq!(resp.status(), 200, "GET /mcp should return 200 for SSE");
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap().to_string();
        assert!(ct.contains("text/event-stream"), "expected SSE content-type, got: {ct}");

        // Signal tools changed after a small delay so SSE stream is ready
        let tools_tx = state.mcp_tools_changed.clone();
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
        }).await;

        assert!(result.unwrap_or(false),
            "expected tools/list_changed notification in SSE stream, got: {collected}");
    }

    /// Unix socket listener: binds, serves health check, cleans up socket file on drop.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_unix_socket_serves_health() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let state = test_state();
        // Use /tmp directly — macOS $TMPDIR can exceed SUN_LEN (104 bytes)
        let tmp_dir = std::path::PathBuf::from("/tmp").join(format!("tuic-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        match std::fs::create_dir_all(&tmp_dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("Skipping test: cannot create dir in sandbox");
                return;
            }
            Err(e) => panic!("create_dir_all: {e}"),
        }
        let sock_path = tmp_dir.join("s");

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
        assert!(response.contains("200 OK"), "expected 200 OK, got: {response}");
        assert!(response.contains("\"ok\":true"), "expected JSON health body, got: {response}");

        server.abort();
        let _ = std::fs::remove_file(&sock_path);
        let _ = std::fs::remove_dir(&tmp_dir);
    }

    /// Verify that aborting the first server task does NOT remove the socket file
    /// rebound by a second instance — the race condition fixed by removing SocketGuard
    /// from the serve task.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_unix_socket_rebind_no_race() {
        let tmp_dir = std::path::PathBuf::from("/tmp")
            .join(format!("tuic-race-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        match std::fs::create_dir_all(&tmp_dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("Skipping test: cannot create dir in sandbox");
                return;
            }
            Err(e) => panic!("create_dir_all: {e}"),
        }
        let sock_path = tmp_dir.join("s");

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
        assert!(sock_path.exists(), "socket file removed by first instance abort — race condition present");

        server2.abort();
        let _ = std::fs::remove_file(&sock_path);
        let _ = std::fs::remove_dir(&tmp_dir);
    }

    // ---- VtLogBuffer HTTP integration tests ----

    #[tokio::test]
    async fn test_get_output_format_log_returns_lines() {
        use crate::state::{VtLogBuffer, VT_LOG_BUFFER_CAPACITY};

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
        let total = json["total_lines"].as_u64().expect("total_lines should be u64");
        assert_eq!(total, expected_total as u64);
        assert_eq!(lines.len(), expected_total);
        // Lines are LogLine objects: {spans: [{text: "log-line-N", ...}]}
        let has_expected = lines.iter().any(|l| {
            l["spans"].as_array()
                .and_then(|spans| spans.first())
                .and_then(|s| s["text"].as_str())
                .map(|t| t.starts_with("log-line-"))
                .unwrap_or(false)
        });
        assert!(has_expected, "should contain log-line-N entries, got: {json}");
    }

    #[tokio::test]
    async fn test_get_output_format_log_with_limit() {
        use crate::state::{VtLogBuffer, VT_LOG_BUFFER_CAPACITY};

        let state = test_state();
        let sid = "test-log-limit";

        let mut vt_log = VtLogBuffer::new(24, 80, VT_LOG_BUFFER_CAPACITY);
        for i in 0..30 {
            vt_log.process(format!("lim-{i}\r\n").as_bytes());
        }
        let total = vt_log.total_lines();
        state
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
}
