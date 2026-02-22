mod agent_routes;
mod auth;
mod config_routes;
mod git_routes;
mod github_routes;
mod mcp_transport;
mod plugin_docs;
mod session;
mod static_files;
mod types;
mod worktree_routes;

use crate::AppState;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{header, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{extract::Path as AxumPath, Json, Router};
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
/// Rejects empty paths, relative traversals, and non-absolute paths.
fn validate_path_string(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("path is required".to_string());
    }
    if path.contains("..") {
        return Err("Path traversal is not allowed".to_string());
    }
    if !path.starts_with('/') && !path.contains(":\\") && !path.starts_with("\\\\") {
        return Err("Path must be absolute".to_string());
    }
    Ok(())
}

/// Validate a repo path for HTTP handlers, returning a 400 error response on failure.
fn validate_repo_path(path: &str) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    validate_path_string(path)
        .map_err(|msg| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": msg}))))
}

/// Port file path: <config_dir>/mcp-port
fn port_file_path() -> std::path::PathBuf {
    crate::config::config_dir().join("mcp-port")
}

/// Return the plugin development guide as JSON.
async fn plugin_dev_guide_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({"content": plugin_docs::PLUGIN_DOCS}))
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

/// Build the router (exposed for testing).
/// When `remote_auth` is true, applies Basic Auth middleware (requires ConnectInfo).
/// When `mcp_enabled` is false, excludes MCP SSE transport routes (/sse, /messages).
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
        .route("/repo/diff", get(git_routes::repo_diff))
        .route("/repo/diff-stats", get(git_routes::repo_diff_stats))
        .route("/repo/files", get(git_routes::repo_changed_files))
        .route("/repo/github", get(github_routes::repo_github_status))
        .route("/repo/prs", get(github_routes::repo_pr_statuses))
        .route("/repo/branches", get(git_routes::repo_branches))
        .route("/repo/ci", get(github_routes::repo_ci_checks))
        // Config
        .route("/config", get(config_routes::get_config).put(config_routes::put_config))
        .route("/config/hash-password", post(config_routes::hash_password_http))
        .route("/config/notifications", get(config_routes::get_notification_config).put(config_routes::put_notification_config))
        .route("/config/ui-prefs", get(config_routes::get_ui_prefs).put(config_routes::put_ui_prefs))
        .route("/config/repo-settings", get(config_routes::get_repo_settings).put(config_routes::put_repo_settings))
        .route("/config/repo-settings/has-custom", get(config_routes::check_has_custom_settings_http))
        .route("/config/repositories", get(config_routes::get_repositories).put(config_routes::put_repositories))
        .route("/config/prompt-library", get(config_routes::get_prompt_library).put(config_routes::put_prompt_library))
        // Worktrees
        .route("/worktrees", get(worktree_routes::list_worktrees_http).post(worktree_routes::create_worktree_http))
        .route("/worktrees/dir", get(worktree_routes::get_worktrees_dir_http))
        .route("/worktrees/paths", get(worktree_routes::get_worktree_paths_http))
        .route("/worktrees/generate-name", post(worktree_routes::generate_worktree_name_http))
        .route("/worktrees/{branch}", delete(worktree_routes::remove_worktree_http))
        // File operations
        .route("/repo/file", get(git_routes::read_file_http))
        .route("/repo/file-diff", get(git_routes::get_file_diff_http))
        .route("/repo/markdown-files", get(git_routes::list_markdown_files_http))
        // Branch operations
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
        // MCP status
        .route("/mcp/status", get(config_routes::get_mcp_status_http))
        // Plugin docs (for MCP bridge)
        .route("/plugins/docs", get(plugin_dev_guide_handler))
        // Plugin data (for external HTTP clients)
        .route("/api/plugins/{plugin_id}/data/{*path}", get(plugin_data_http));

    // MCP SSE transport — only when MCP is enabled
    if mcp_enabled {
        routes = routes
            .route("/sse", get(mcp_transport::mcp_sse_connect))
            .route("/messages", post(mcp_transport::mcp_messages));
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
/// When remote access is enabled, binds to 0.0.0.0:{configured_port} with Basic Auth.
/// Otherwise, binds to 127.0.0.1:0 (localhost only, no auth).
/// Writes the port number to ~/.tuicommander/mcp-port for the MCP bridge to discover.
pub async fn start_server(state: Arc<AppState>, mcp_enabled: bool, remote_enabled: bool) {
    let config = state.config.read().clone();

    let bind_addr = if remote_enabled {
        let port = if config.remote_access_port == 0 { 0 } else { config.remote_access_port };
        format!("0.0.0.0:{port}")
    } else {
        "127.0.0.1:0".to_string()
    };

    let app = build_router(state.clone(), remote_enabled, mcp_enabled);

    let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("MCP HTTP: failed to bind {bind_addr}: {e}");
            // Fall back to localhost with OS-assigned port (remote access disabled)
            let fallback = "127.0.0.1:0";
            eprintln!("MCP HTTP: falling back to {fallback} (remote access disabled)");
            match tokio::net::TcpListener::bind(fallback).await {
                Ok(l) => l,
                Err(e2) => {
                    eprintln!("MCP HTTP: fallback bind also failed: {e2}, server disabled");
                    return;
                }
            }
        }
    };

    let addr = match listener.local_addr() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("MCP HTTP: failed to get local address: {e}");
            return;
        }
    };
    if remote_enabled {
        eprintln!("MCP HTTP API listening on {addr} (remote access enabled)");
    } else {
        eprintln!("MCP HTTP API listening on {addr}");
    }

    // Write port to file for bridge discovery
    let port_file = port_file_path();
    if let Some(parent) = port_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&port_file, addr.port().to_string()) {
        eprintln!("MCP HTTP: failed to write port file: {e}");
    }

    // Register shutdown channel so save_config can restart server without requiring app restart
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    *state.server_shutdown.lock() = Some(shutdown_tx);

    // Serve with ConnectInfo for auth middleware; graceful shutdown via channel
    let result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async { shutdown_rx.await.ok(); })
    .await;

    // Cleanup port file
    let _ = std::fs::remove_file(port_file_path());

    if let Err(e) = result {
        eprintln!("MCP HTTP server error: {e}");
    }
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
        // Create blocking client on a separate OS thread because
        // reqwest::blocking::Client::new() creates an internal tokio runtime
        // which panics when constructed inside an existing async context (#[tokio::test]).
        let http_client = std::thread::spawn(|| reqwest::blocking::Client::new())
            .join()
            .expect("blocking client construction thread panicked");
        Arc::new(AppState {
            sessions: DashMap::new(),
            worktrees_dir: std::env::temp_dir().join("test-worktrees"),
            metrics: crate::SessionMetrics::new(),
            output_buffers: DashMap::new(),
            mcp_sse_sessions: DashMap::new(),
            ws_clients: DashMap::new(),
            config: parking_lot::RwLock::new(crate::config::AppConfig::default()),
            repo_info_cache: DashMap::new(),
            github_status_cache: DashMap::new(),
            head_watchers: DashMap::new(),
            repo_watchers: DashMap::new(),
            http_client: std::mem::ManuallyDrop::new(http_client),
            github_token: parking_lot::RwLock::new(None),
            github_circuit_breaker: crate::github::GitHubCircuitBreaker::new(),
            server_shutdown: parking_lot::Mutex::new(None),
            session_token: uuid::Uuid::new_v4().to_string(),
            app_handle: parking_lot::RwLock::new(None),
            plugin_watchers: DashMap::new(),
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
    async fn test_mcp_messages_invalid_session() {
        let state = test_state();
        let app = build_router(state, false, true);
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        let resp = app
            .oneshot(mcp_post("/messages?sessionId=nonexistent", &body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_mcp_messages_with_valid_session() {
        let state = test_state();

        // Manually create an SSE session channel
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let session_id = "test-session-123";
        state.mcp_sse_sessions.insert(session_id.to_string(), tx);

        // Send initialize request
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1.0" }
            }
        });
        let app = build_router(state.clone(), false, true);
        let resp = app
            .oneshot(mcp_post(&format!("/messages?sessionId={}", session_id), &body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Check that the SSE channel received the response
        let msg = rx.try_recv().unwrap();
        let json: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert!(json["result"]["protocolVersion"].as_str().is_some());
        assert!(json["result"]["serverInfo"]["name"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_mcp_tools_list() {
        let state = test_state();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let session_id = "test-tools-session";
        state.mcp_sse_sessions.insert(session_id.to_string(), tx);

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });
        let app = build_router(state.clone(), false, true);
        let resp = app
            .oneshot(mcp_post(&format!("/messages?sessionId={}", session_id), &body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let msg = rx.try_recv().unwrap();
        let json: serde_json::Value = serde_json::from_str(&msg).unwrap();
        let tools = json["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 5);

        // Verify meta-commands are present
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"session"));
        assert!(names.contains(&"git"));
        assert!(names.contains(&"agent"));
        assert!(names.contains(&"config"));
        assert!(names.contains(&"plugin_dev_guide"));
    }

    #[test]
    fn test_mcp_tool_definitions_count() {
        let tools = mcp_transport::test_mcp_tool_definitions();
        let arr = tools.as_array().unwrap();
        assert_eq!(arr.len(), 5);
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

    /// Helper: send a tools/call MCP request and return the parsed result content
    async fn call_mcp_tool(state: &Arc<AppState>, tool_name: &str, args: serde_json::Value) -> serde_json::Value {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let session_id = format!("tool-test-{}", uuid::Uuid::new_v4());
        state.mcp_sse_sessions.insert(session_id.clone(), tx);

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
            .oneshot(mcp_post(&format!("/messages?sessionId={}", session_id), &body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let msg = rx.try_recv().unwrap();
        let json: serde_json::Value = serde_json::from_str(&msg).unwrap();
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
    async fn test_agent_spawn_not_implemented_via_sse() {
        let state = test_state();
        let result = call_mcp_tool(&state, "agent", serde_json::json!({
            "action": "spawn",
            "prompt": "test task"
        })).await;
        assert!(result["error"].as_str().unwrap().contains("not yet implemented"));
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

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let session_id = "test-is-error-flag";
        state.mcp_sse_sessions.insert(session_id.to_string(), tx);

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 50,
            "method": "tools/call",
            "params": {
                "name": "nonexistent_tool",
                "arguments": {}
            }
        });
        let app = build_router(state.clone(), false, true);
        let resp = app
            .oneshot(mcp_post(&format!("/messages?sessionId={}", session_id), &body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let msg = rx.try_recv().unwrap();
        let json: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(json["result"]["isError"], true, "Error responses should set isError=true");
    }

    #[tokio::test]
    async fn test_tool_call_success_flag() {
        let state = test_state();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let session_id = "test-success-flag";
        state.mcp_sse_sessions.insert(session_id.to_string(), tx);

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 51,
            "method": "tools/call",
            "params": {
                "name": "session",
                "arguments": {"action": "list"}
            }
        });
        let app = build_router(state.clone(), false, true);
        let resp = app
            .oneshot(mcp_post(&format!("/messages?sessionId={}", session_id), &body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let msg = rx.try_recv().unwrap();
        let json: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(json["result"]["isError"], false, "Success responses should set isError=false");
    }

    #[tokio::test]
    async fn test_mcp_unknown_method() {
        // Verify that unknown JSON-RPC methods return a proper error
        let state = test_state();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let session_id = "test-unknown-method";
        state.mcp_sse_sessions.insert(session_id.to_string(), tx);

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 60,
            "method": "resources/list",
            "params": {}
        });
        let app = build_router(state.clone(), false, true);
        let resp = app
            .oneshot(mcp_post(&format!("/messages?sessionId={}", session_id), &body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let msg = rx.try_recv().unwrap();
        let json: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(json["error"]["code"], -32601, "Unknown method should return -32601");
        assert!(json["error"]["message"].as_str().unwrap().contains("Method not found"));
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
}
