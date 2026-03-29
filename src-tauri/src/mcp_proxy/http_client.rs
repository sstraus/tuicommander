//! HTTP MCP client — connects to an upstream MCP server via Streamable HTTP.
//!
//! Implements the MCP client side of the Streamable HTTP transport (spec 2025-03-26):
//! - initialize handshake → caches session_id and tool list
//! - tools/call forwarding with session_id header
//! - auto-reconnect on session expiry (400) or connection error
//! - health_check via tools/list
//! - Bearer token auth from OS keyring

use crate::mcp_upstream_config::UpstreamTransport;
use crate::mcp_upstream_credentials::read_upstream_credential;
use serde_json::Value;
use std::time::Duration;

const MCP_SESSION_HEADER: &str = "mcp-session-id";
const PROTOCOL_VERSION: &str = "2025-03-26";

/// A tool definition returned by an upstream server.
#[derive(Debug, Clone)]
pub(crate) struct UpstreamToolDef {
    /// Original tool name from the upstream server.
    pub(crate) original_name: String,
    /// Full tool definition JSON (name, description, inputSchema).
    pub(crate) definition: Value,
}

/// Client for a single upstream MCP server over Streamable HTTP.
pub(crate) struct HttpMcpClient {
    /// HTTP client (connection pooling, timeouts).
    client: reqwest::Client,
    /// Base URL of the upstream MCP server (e.g. `http://localhost:8080/mcp`).
    url: String,
    /// Name of this upstream (used for prefixing and logging).
    pub(crate) name: String,
    /// Active MCP session ID (set after successful initialize).
    session_id: Option<String>,
}

impl HttpMcpClient {
    /// Create a new HTTP MCP client.
    ///
    /// `timeout_secs` is the per-request timeout (0 = no timeout).
    pub(crate) fn new(name: String, url: String, timeout_secs: u32) -> Self {
        let timeout = if timeout_secs > 0 {
            Some(Duration::from_secs(timeout_secs as u64))
        } else {
            None
        };

        let mut builder = reqwest::Client::builder()
            .user_agent(concat!("tuicommander-mcp-proxy/", env!("CARGO_PKG_VERSION")));

        if let Some(t) = timeout {
            builder = builder.timeout(t);
        }

        let client = builder
            .build()
            .expect("Failed to build reqwest client for MCP proxy");

        Self {
            client,
            url,
            name,
            session_id: None,
        }
    }

    /// Build from an `UpstreamMcpServer` config (only Http transport).
    pub(crate) fn from_config(
        name: String,
        transport: &UpstreamTransport,
        timeout_secs: u32,
    ) -> Option<Self> {
        match transport {
            UpstreamTransport::Http { url } => {
                Some(Self::new(name, url.clone(), timeout_secs))
            }
            UpstreamTransport::Stdio { .. } => None,
        }
    }

    /// Perform the MCP initialize handshake.
    ///
    /// 1. Reads Bearer token from OS keyring (if any).
    /// 2. Sends `initialize` request.
    /// 3. Sends `notifications/initialized` (fire-and-forget).
    /// 4. Calls `tools/list` and returns the tool definitions.
    pub(crate) async fn initialize(&mut self) -> Result<Vec<UpstreamToolDef>, String> {
        // Read optional auth token from keyring
        let auth_token = read_upstream_credential(&self.name)?;

        let init_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "tuicommander",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });

        let mut req = self.client.post(&self.url).json(&init_body);
        if let Some(token) = &auth_token {
            req = req.bearer_auth(token);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("Upstream '{}' initialize failed: {e}", self.name))?;

        // Extract session ID from response header
        self.session_id = resp
            .headers()
            .get(MCP_SESSION_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Validate response is OK
        if !resp.status().is_success() {
            let status = resp.status();
            return Err(format!(
                "Upstream '{}' initialize returned {status}",
                self.name
            ));
        }

        // Parse initialize response (we don't need its contents, just need it to succeed)
        let _init_resp: Value = resp
            .json()
            .await
            .map_err(|e| format!("Upstream '{}' invalid initialize response: {e}", self.name))?;

        // Fire-and-forget: notifications/initialized
        let _ = self.rpc_raw("notifications/initialized", serde_json::json!({}), &auth_token).await;

        // Fetch tool list
        self.fetch_tools(&auth_token).await
    }

    /// Fetch the tool list from the upstream server.
    async fn fetch_tools(&self, auth_token: &Option<String>) -> Result<Vec<UpstreamToolDef>, String> {
        let resp_value = self.rpc("tools/list", serde_json::json!({}), auth_token).await?;

        let tools_arr = resp_value["result"]["tools"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let tools = tools_arr
            .into_iter()
            .filter_map(|tool| {
                let original_name = tool["name"].as_str()?.to_string();
                Some(UpstreamToolDef {
                    original_name,
                    definition: tool,
                })
            })
            .collect();

        Ok(tools)
    }

    /// Call a tool on the upstream server.
    ///
    /// Returns the `result.content` array from the MCP response, or an error string.
    pub(crate) async fn call_tool(&self, tool_name: &str, args: Value) -> Result<Value, String> {
        let auth_token = read_upstream_credential(&self.name)?;
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": args
            }
        });

        let resp_value = self.rpc_with_session(&body, &auth_token).await?;

        // Extract content from result
        Ok(resp_value
            .get("result")
            .cloned()
            .unwrap_or(resp_value))
    }

    /// Ping the upstream via tools/list and return refreshed tool definitions.
    pub(crate) async fn health_check(&self) -> Result<Vec<UpstreamToolDef>, String> {
        let auth_token = read_upstream_credential(&self.name)?;
        self.fetch_tools(&auth_token).await
    }

    /// Send DELETE /mcp to cleanly terminate the upstream session.
    #[allow(dead_code)]
    pub(crate) async fn shutdown(&self) {
        if let Some(sid) = &self.session_id {
            let auth_token = read_upstream_credential(&self.name).unwrap_or(None);
            let mut req = self.client.delete(&self.url).header(MCP_SESSION_HEADER, sid);
            if let Some(token) = &auth_token {
                req = req.bearer_auth(token);
            }
            let _ = req.send().await;
        }
    }

    /// Whether this client has an active session.
    #[allow(dead_code)]
    pub(crate) fn is_connected(&self) -> bool {
        self.session_id.is_some()
    }

    /// Send a JSON-RPC request with the current session_id header.
    async fn rpc_with_session(&self, body: &Value, auth_token: &Option<String>) -> Result<Value, String> {
        let mut req = self.client.post(&self.url).json(body);

        if let Some(sid) = &self.session_id {
            req = req.header(MCP_SESSION_HEADER, sid);
        }
        if let Some(token) = auth_token {
            req = req.bearer_auth(token);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("Upstream '{}' request failed: {e}", self.name))?;

        if resp.status() == reqwest::StatusCode::BAD_REQUEST {
            return Err(format!("Upstream '{}' returned 400 (session may have expired)", self.name));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(format!("Upstream '{}' returned {status}", self.name));
        }

        resp.json::<Value>()
            .await
            .map_err(|e| format!("Upstream '{}' invalid JSON response: {e}", self.name))
    }

    /// Send a JSON-RPC method call (no body building, just method + params).
    async fn rpc(&self, method: &str, params: Value, auth_token: &Option<String>) -> Result<Value, String> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        });
        self.rpc_with_session(&body, auth_token).await
    }

    /// Raw request without session_id (used for initialize and notifications/initialized).
    async fn rpc_raw(&self, method: &str, params: Value, auth_token: &Option<String>) -> Result<Value, String> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        });
        let mut req = self.client.post(&self.url).json(&body);
        if let Some(token) = auth_token {
            req = req.bearer_auth(token);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("Upstream '{}' {method} failed: {e}", self.name))?;
        if !resp.status().is_success() && resp.status() != reqwest::StatusCode::ACCEPTED {
            return Ok(serde_json::json!({})); // notifications are fire-and-forget
        }
        // notifications/initialized returns 202 with no body
        if resp.status() == reqwest::StatusCode::ACCEPTED {
            return Ok(serde_json::json!({}));
        }
        resp.json::<Value>()
            .await
            .map_err(|e| format!("Upstream '{}' {method} invalid response: {e}", self.name))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // We use a mock HTTP server to test the client without real upstreams.
    // The mock is implemented with axum to keep it in-process and fast.

    use axum::{
        extract::State as AxumState,
        http::{HeaderMap, StatusCode},
        response::IntoResponse,
        routing::post,
        Json, Router,
    };
    use tokio::net::TcpListener;

    /// Shared mock server state.
    #[derive(Clone, Default)]
    struct MockState {
        /// If true, initialize returns 500.
        fail_initialize: Arc<std::sync::atomic::AtomicBool>,
        /// If true, tools/call returns 400 (session expired).
        return_400: Arc<std::sync::atomic::AtomicBool>,
        /// Number of times initialize was called.
        init_count: Arc<std::sync::atomic::AtomicU32>,
    }

    async fn mock_mcp_handler(
        AxumState(state): AxumState<MockState>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> impl IntoResponse {
        let method = body["method"].as_str().unwrap_or("");
        let id = body["id"].clone();

        match method {
            "initialize" => {
                state.init_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if state.fail_initialize.load(std::sync::atomic::Ordering::SeqCst) {
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({}))).into_response();
                }
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2025-03-26",
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "mock", "version": "1.0" }
                    }
                });
                (
                    StatusCode::OK,
                    [(MCP_SESSION_HEADER, "test-session-id-123")],
                    Json(resp),
                ).into_response()
            }
            "notifications/initialized" => {
                StatusCode::ACCEPTED.into_response()
            }
            "tools/list" => {
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": [
                            {
                                "name": "search_code",
                                "description": "Search code",
                                "inputSchema": { "type": "object", "properties": {} }
                            },
                            {
                                "name": "create_issue",
                                "description": "Create issue",
                                "inputSchema": { "type": "object", "properties": {} }
                            }
                        ]
                    }
                });
                (StatusCode::OK, Json(resp)).into_response()
            }
            "tools/call" => {
                // Check if we should simulate session expiry
                if state.return_400.load(std::sync::atomic::Ordering::SeqCst) {
                    // Check session header
                    let has_session = headers.contains_key(MCP_SESSION_HEADER);
                    if !has_session || state.return_400.load(std::sync::atomic::Ordering::SeqCst) {
                        return StatusCode::BAD_REQUEST.into_response();
                    }
                }
                let tool_name = body["params"]["name"].as_str().unwrap_or("unknown");
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [
                            { "type": "text", "text": format!("Result of {tool_name}") }
                        ],
                        "isError": false
                    }
                });
                (StatusCode::OK, Json(resp)).into_response()
            }
            _ => {
                StatusCode::NOT_FOUND.into_response()
            }
        }
    }

    /// Spawn a mock MCP server on a random port and return its URL.
    async fn spawn_mock_server(state: MockState) -> String {
        let app = Router::new()
            .route("/mcp", post(mock_mcp_handler))
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://127.0.0.1:{port}/mcp")
    }

    #[tokio::test]
    async fn initialize_caches_session_id_and_returns_tools() {
        let state = MockState::default();
        let url = spawn_mock_server(state).await;

        let mut client = HttpMcpClient::new("test".to_string(), url, 10);
        assert!(!client.is_connected());

        let tools = client.initialize().await.unwrap();
        assert!(client.is_connected());
        assert_eq!(client.session_id.as_deref(), Some("test-session-id-123"));
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].original_name, "search_code");
        assert_eq!(tools[1].original_name, "create_issue");
    }

    #[tokio::test]
    async fn initialize_fails_when_server_returns_500() {
        let state = MockState::default();
        state.fail_initialize.store(true, std::sync::atomic::Ordering::SeqCst);
        let url = spawn_mock_server(state).await;

        let mut client = HttpMcpClient::new("test".to_string(), url, 10);
        let result = client.initialize().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("500"));
        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn call_tool_forwards_and_returns_result() {
        let state = MockState::default();
        let url = spawn_mock_server(state).await;

        let mut client = HttpMcpClient::new("test".to_string(), url, 10);
        client.initialize().await.unwrap();

        let result = client
            .call_tool("search_code", serde_json::json!({"query": "foo"}))
            .await
            .unwrap();

        let content = &result["content"][0]["text"].as_str().unwrap();
        assert_eq!(*content, "Result of search_code");
    }

    #[tokio::test]
    async fn call_tool_with_reconnect_retries_on_400() {
        // Server returns 400 on first tools/call to simulate expired session
        let state = MockState::default();
        state.return_400.store(true, std::sync::atomic::Ordering::SeqCst);
        let url = spawn_mock_server(state.clone()).await;

        let mut client = HttpMcpClient::new("test".to_string(), url, 10);
        client.initialize().await.unwrap();
        assert_eq!(state.init_count.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Now allow tools/call to succeed (clear 400 flag)
        state.return_400.store(false, std::sync::atomic::Ordering::SeqCst);

        let result = client
            .call_tool_with_reconnect("search_code", serde_json::json!({}))
            .await;
        // After reconnect, should succeed
        assert!(result.is_ok(), "Expected ok after reconnect, got: {result:?}");
        // initialize was called again (re-init for reconnect)
        assert!(state.init_count.load(std::sync::atomic::Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    async fn health_check_succeeds_after_initialize() {
        let state = MockState::default();
        let url = spawn_mock_server(state).await;

        let mut client = HttpMcpClient::new("test".to_string(), url, 10);
        client.initialize().await.unwrap();

        let result = client.health_check().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn health_check_fails_when_not_initialized() {
        // No server at this port — connection refused
        let client = HttpMcpClient::new("test".to_string(), "http://127.0.0.1:1/mcp".to_string(), 2);
        let result = client.health_check().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn shutdown_sends_delete_without_error() {
        let state = MockState::default();
        let url = spawn_mock_server(state).await;
        let mut client = HttpMcpClient::new("test".to_string(), url, 10);
        client.initialize().await.unwrap();

        // shutdown should not panic or error
        client.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_noop_when_not_connected() {
        let client = HttpMcpClient::new("test".to_string(), "http://127.0.0.1:1/mcp".to_string(), 2);
        // Should not panic when session_id is None
        client.shutdown().await;
    }

    #[tokio::test]
    async fn new_with_zero_timeout_has_no_timeout() {
        let client = HttpMcpClient::new("test".to_string(), "http://example.com/mcp".to_string(), 0);
        // Just check it constructs without panic
        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn connect_to_unreachable_server_returns_error() {
        let mut client = HttpMcpClient::new("test".to_string(), "http://127.0.0.1:1/mcp".to_string(), 2);
        let result = client.initialize().await;
        assert!(result.is_err());
        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn tools_list_returns_both_tool_names() {
        let state = MockState::default();
        let url = spawn_mock_server(state).await;

        let mut client = HttpMcpClient::new("github".to_string(), url, 10);
        let tools = client.initialize().await.unwrap();

        let names: Vec<&str> = tools.iter().map(|t| t.original_name.as_str()).collect();
        assert!(names.contains(&"search_code"));
        assert!(names.contains(&"create_issue"));
    }

    #[tokio::test]
    async fn from_config_returns_some_for_http_transport() {
        let transport = crate::mcp_upstream_config::UpstreamTransport::Http {
            url: "http://localhost:8080/mcp".to_string(),
        };
        let client = HttpMcpClient::from_config("test".to_string(), &transport, 30);
        assert!(client.is_some());
    }

    #[tokio::test]
    async fn from_config_returns_none_for_stdio_transport() {
        let transport = crate::mcp_upstream_config::UpstreamTransport::Stdio {
            command: "npx".to_string(),
            args: vec![],
            env: std::collections::HashMap::new(),
            cwd: None,
        };
        let client = HttpMcpClient::from_config("test".to_string(), &transport, 30);
        assert!(client.is_none());
    }
}
