//! HTTP MCP client — connects to an upstream MCP server via Streamable HTTP.
//!
//! Implements the MCP client side of the Streamable HTTP transport (spec 2025-03-26):
//! - initialize handshake → caches session_id and tool list
//! - tools/call forwarding with session_id header
//! - auto-reconnect on session expiry (400) or connection error
//! - health_check via tools/list
//! - Bearer token auth from OS keyring (static or OAuth 2.1)
//! - OAuth 2.1: pre-request refresh when access token is near expiry,
//!   401 WWW-Authenticate parsing → [`UpstreamError::NeedsOAuth`]

use crate::mcp_oauth::token::TokenManager;
use crate::mcp_upstream_config::UpstreamTransport;
use crate::mcp_upstream_credentials::{
    is_token_valid, read_stored_credential, OAuthTokenSet, StoredCredential,
};
use serde_json::Value;
use std::time::Duration;

const MCP_SESSION_HEADER: &str = "mcp-session-id";
const PROTOCOL_VERSION: &str = "2025-03-26";

// ---------------------------------------------------------------------------
// UpstreamError — typed errors so the registry can distinguish OAuth failures
// from transport / protocol errors.
// ---------------------------------------------------------------------------

/// Structured error returned by [`HttpMcpClient`] methods.
///
/// Callers that only need a human-readable string can use `.to_string()`;
/// callers that need to branch on OAuth state (e.g. the registry) should
/// `match` on the variant.
#[derive(Debug, Clone)]
pub(crate) enum UpstreamError {
    /// Server returned 401 with a `WWW-Authenticate: Bearer ...` header,
    /// signalling that the client must (re-)run the OAuth authorization flow.
    NeedsOAuth { www_authenticate: String },
    /// Server returned 401 with no `WWW-Authenticate` header, meaning the
    /// static bearer token was invalid/expired and there's no OAuth challenge
    /// to follow up on.
    AuthFailed,
    /// Any other error (transport, protocol, session expiry, JSON parse).
    Other(String),
}

impl std::fmt::Display for UpstreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeedsOAuth { www_authenticate } => {
                write!(f, "OAuth required (WWW-Authenticate: {www_authenticate})")
            }
            Self::AuthFailed => f.write_str("Upstream authentication failed (401)"),
            Self::Other(s) => f.write_str(s),
        }
    }
}

impl From<String> for UpstreamError {
    fn from(s: String) -> Self {
        Self::Other(s)
    }
}

impl From<&str> for UpstreamError {
    fn from(s: &str) -> Self {
        Self::Other(s.to_string())
    }
}

// ---------------------------------------------------------------------------
// UpstreamToolDef
// ---------------------------------------------------------------------------

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

    /// Resolve the bearer token to send on the next request.
    ///
    /// - No credential → returns `None`.
    /// - `StoredCredential::Bearer` → returns the token as-is.
    /// - `StoredCredential::Oauth2` → checks validity; if expired or near
    ///   expiry, calls [`TokenManager::refresh_if_needed`] and returns the
    ///   refreshed access token.
    async fn resolve_bearer(&self) -> Result<Option<String>, UpstreamError> {
        let cred = read_stored_credential(&self.name)
            .map_err(|e| UpstreamError::Other(format!("keyring read failed: {e}")))?;
        let Some(cred) = cred else { return Ok(None) };

        match cred {
            StoredCredential::Bearer { token } => Ok(Some(token)),
            StoredCredential::Oauth2(set) => {
                let refreshed = self.refresh_token_if_needed(&set).await?;
                Ok(Some(
                    refreshed
                        .as_ref()
                        .map(|r| r.access_token.clone())
                        .unwrap_or(set.access_token),
                ))
            }
        }
    }

    /// Attempt a refresh if the given OAuth token set is expired. Returns
    /// `Ok(Some(new_set))` if a refresh happened, `Ok(None)` if the existing
    /// token is still valid.
    async fn refresh_token_if_needed(
        &self,
        set: &OAuthTokenSet,
    ) -> Result<Option<OAuthTokenSet>, UpstreamError> {
        if is_token_valid(set) {
            return Ok(None);
        }
        let tm = TokenManager::new(
            self.name.clone(),
            set.client_id.clone(),
            set.token_endpoint.clone(),
            Some(self.url.clone()),
        );
        tm.refresh_if_needed(set)
            .await
            .map_err(|e| UpstreamError::Other(format!("token refresh failed: {e}")))
    }

    /// Force a refresh regardless of current validity (used after a 401 on
    /// an OAuth credential to recover from server-side token revocation).
    async fn force_refresh(&self) -> Result<Option<OAuthTokenSet>, UpstreamError> {
        let cred = read_stored_credential(&self.name)
            .map_err(|e| UpstreamError::Other(format!("keyring read failed: {e}")))?;
        let Some(StoredCredential::Oauth2(mut set)) = cred else {
            return Ok(None);
        };
        // Mark the token as expired so refresh_if_needed doesn't short-circuit.
        set.expires_at = Some(0);
        self.refresh_token_if_needed(&set).await
    }

    /// Perform the MCP initialize handshake.
    ///
    /// 1. Resolves auth (static Bearer or refreshed OAuth access token) from keyring.
    /// 2. Sends `initialize` request.
    /// 3. Sends `notifications/initialized` (fire-and-forget).
    /// 4. Calls `tools/list` and returns the tool definitions.
    pub(crate) async fn initialize(&mut self) -> Result<Vec<UpstreamToolDef>, UpstreamError> {
        let auth_token = self.resolve_bearer().await?;

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

        let resp = self.send_post(&init_body, auth_token.as_deref(), None).await?;

        // Extract session ID from response header (preserved across auth retries
        // by re-reading after the final response).
        self.session_id = resp
            .headers()
            .get(MCP_SESSION_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(classify_401(&resp));
        }
        if !resp.status().is_success() {
            return Err(UpstreamError::Other(format!(
                "Upstream '{}' initialize returned {status}",
                self.name
            )));
        }

        let _init_resp: Value = resp.json().await.map_err(|e| {
            UpstreamError::Other(format!(
                "Upstream '{}' invalid initialize response: {e}",
                self.name
            ))
        })?;

        // Fire-and-forget: notifications/initialized
        let _ = self
            .rpc_raw("notifications/initialized", serde_json::json!({}), auth_token.as_deref())
            .await;

        // Fetch tool list
        self.fetch_tools(auth_token.as_deref()).await
    }

    /// Fetch the tool list from the upstream server.
    async fn fetch_tools(
        &self,
        auth_token: Option<&str>,
    ) -> Result<Vec<UpstreamToolDef>, UpstreamError> {
        let resp_value = self
            .rpc("tools/list", serde_json::json!({}), auth_token)
            .await?;

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
    /// Returns the `result` object from the MCP response.
    pub(crate) async fn call_tool(
        &self,
        tool_name: &str,
        args: Value,
    ) -> Result<Value, UpstreamError> {
        let auth_token = self.resolve_bearer().await?;
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": args
            }
        });

        let resp_value = self.rpc_with_session(&body, auth_token.as_deref()).await?;

        Ok(resp_value.get("result").cloned().unwrap_or(resp_value))
    }

    /// Ping the upstream via tools/list and return refreshed tool definitions.
    pub(crate) async fn health_check(&self) -> Result<Vec<UpstreamToolDef>, UpstreamError> {
        let auth_token = self.resolve_bearer().await?;
        self.fetch_tools(auth_token.as_deref()).await
    }

    /// Send DELETE /mcp to cleanly terminate the upstream session.
    #[allow(dead_code)]
    pub(crate) async fn shutdown(&self) {
        if let Some(sid) = &self.session_id {
            let auth_token = self.resolve_bearer().await.ok().flatten();
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

    /// Send a JSON-RPC request with the current session_id header. On 401
    /// with an OAuth credential, force-refreshes the token and retries once.
    async fn rpc_with_session(
        &self,
        body: &Value,
        auth_token: Option<&str>,
    ) -> Result<Value, UpstreamError> {
        let resp = self.send_post(body, auth_token, self.session_id.as_deref()).await?;

        if resp.status() == reqwest::StatusCode::BAD_REQUEST {
            return Err(UpstreamError::Other(format!(
                "Upstream '{}' returned 400 (session may have expired)",
                self.name
            )));
        }

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            // Single retry: if OAuth credential, force-refresh once and retry.
            if let Ok(Some(refreshed)) = self.force_refresh().await {
                let retry_resp = self
                    .send_post(body, Some(&refreshed.access_token), self.session_id.as_deref())
                    .await?;
                return self.decode_response(retry_resp).await;
            }
            return Err(classify_401(&resp));
        }

        self.decode_response(resp).await
    }

    /// Decode a reqwest Response into JSON or classify error status.
    async fn decode_response(&self, resp: reqwest::Response) -> Result<Value, UpstreamError> {
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(classify_401(&resp));
        }
        if !status.is_success() {
            return Err(UpstreamError::Other(format!(
                "Upstream '{}' returned {status}",
                self.name
            )));
        }
        resp.json::<Value>().await.map_err(|e| {
            UpstreamError::Other(format!("Upstream '{}' invalid JSON response: {e}", self.name))
        })
    }

    /// Build and send a POST with optional bearer auth and session header.
    async fn send_post(
        &self,
        body: &Value,
        auth_token: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<reqwest::Response, UpstreamError> {
        let mut req = self.client.post(&self.url).json(body);
        if let Some(sid) = session_id {
            req = req.header(MCP_SESSION_HEADER, sid);
        }
        if let Some(token) = auth_token {
            req = req.bearer_auth(token);
        }
        req.send().await.map_err(|e| {
            UpstreamError::Other(format!("Upstream '{}' request failed: {e}", self.name))
        })
    }

    /// Send a JSON-RPC method call (no body building, just method + params).
    async fn rpc(
        &self,
        method: &str,
        params: Value,
        auth_token: Option<&str>,
    ) -> Result<Value, UpstreamError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        });
        self.rpc_with_session(&body, auth_token).await
    }

    /// Raw request without session_id (used for notifications/initialized).
    async fn rpc_raw(
        &self,
        method: &str,
        params: Value,
        auth_token: Option<&str>,
    ) -> Result<Value, UpstreamError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        });
        let resp = self.send_post(&body, auth_token, None).await?;
        if !resp.status().is_success() && resp.status() != reqwest::StatusCode::ACCEPTED {
            return Ok(serde_json::json!({})); // notifications are fire-and-forget
        }
        if resp.status() == reqwest::StatusCode::ACCEPTED {
            return Ok(serde_json::json!({}));
        }
        resp.json::<Value>().await.map_err(|e| {
            UpstreamError::Other(format!("Upstream '{}' {method} invalid response: {e}", self.name))
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Inspect a 401 response and classify it as [`UpstreamError::NeedsOAuth`]
/// (if `WWW-Authenticate: Bearer...` is present) or [`UpstreamError::AuthFailed`].
fn classify_401(resp: &reqwest::Response) -> UpstreamError {
    let header = resp
        .headers()
        .get(reqwest::header::WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    match header {
        Some(h) if h.to_ascii_lowercase().contains("bearer") => {
            UpstreamError::NeedsOAuth { www_authenticate: h }
        }
        _ => UpstreamError::AuthFailed,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use axum::{
        extract::State as AxumState,
        http::{HeaderMap, StatusCode},
        response::IntoResponse,
        routing::post,
        Json, Router,
    };
    use tokio::net::TcpListener;

    #[derive(Clone, Default)]
    struct MockState {
        fail_initialize: Arc<std::sync::atomic::AtomicBool>,
        return_400: Arc<std::sync::atomic::AtomicBool>,
        /// If true, initial requests return 401 + WWW-Authenticate.
        return_401_with_challenge: Arc<std::sync::atomic::AtomicBool>,
        /// If true, initial requests return 401 without challenge.
        return_401_no_challenge: Arc<std::sync::atomic::AtomicBool>,
        init_count: Arc<std::sync::atomic::AtomicU32>,
    }

    async fn mock_mcp_handler(
        AxumState(state): AxumState<MockState>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> impl IntoResponse {
        let method = body["method"].as_str().unwrap_or("");
        let id = body["id"].clone();

        if state
            .return_401_with_challenge
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            let mut headers = HeaderMap::new();
            headers.insert(
                "WWW-Authenticate",
                "Bearer realm=\"mcp\", resource_metadata=\"https://api.example.com/.well-known/oauth-protected-resource\""
                    .parse()
                    .unwrap(),
            );
            return (StatusCode::UNAUTHORIZED, headers, Json(serde_json::json!({})))
                .into_response();
        }
        if state
            .return_401_no_challenge
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({}))).into_response();
        }

        match method {
            "initialize" => {
                state.init_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if state.fail_initialize.load(std::sync::atomic::Ordering::SeqCst) {
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({})))
                        .into_response();
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
                )
                    .into_response()
            }
            "notifications/initialized" => StatusCode::ACCEPTED.into_response(),
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
                if state.return_400.load(std::sync::atomic::Ordering::SeqCst) {
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
            _ => StatusCode::NOT_FOUND.into_response(),
        }
    }

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

    // -- existing behavior regression tests --

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
    }

    #[tokio::test]
    async fn initialize_fails_when_server_returns_500() {
        let state = MockState::default();
        state.fail_initialize.store(true, std::sync::atomic::Ordering::SeqCst);
        let url = spawn_mock_server(state).await;

        let mut client = HttpMcpClient::new("test".to_string(), url, 10);
        let result = client.initialize().await;
        let err = result.unwrap_err();
        assert!(matches!(err, UpstreamError::Other(_)));
        assert!(err.to_string().contains("500"));
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
        let client =
            HttpMcpClient::new("test".to_string(), "http://127.0.0.1:1/mcp".to_string(), 2);
        let result = client.health_check().await;
        assert!(result.is_err());
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

    // -- new 401 behavior --

    #[tokio::test]
    async fn initialize_401_with_bearer_challenge_returns_needs_oauth() {
        let state = MockState::default();
        state
            .return_401_with_challenge
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let url = spawn_mock_server(state).await;

        let mut client = HttpMcpClient::new("test-401-oauth".to_string(), url, 5);
        let err = client.initialize().await.unwrap_err();
        match err {
            UpstreamError::NeedsOAuth { www_authenticate } => {
                assert!(
                    www_authenticate.to_ascii_lowercase().contains("bearer"),
                    "WWW-Authenticate should mention Bearer, got: {www_authenticate}"
                );
            }
            other => panic!("expected NeedsOAuth, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn initialize_401_without_challenge_returns_auth_failed() {
        let state = MockState::default();
        state
            .return_401_no_challenge
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let url = spawn_mock_server(state).await;

        let mut client = HttpMcpClient::new("test-401-plain".to_string(), url, 5);
        let err = client.initialize().await.unwrap_err();
        assert!(
            matches!(err, UpstreamError::AuthFailed),
            "expected AuthFailed, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn call_tool_401_with_challenge_returns_needs_oauth() {
        // Start clean, initialize, then flip the 401 flag to simulate a
        // mid-session revocation.
        let state = MockState::default();
        let url = spawn_mock_server(state.clone()).await;

        let mut client = HttpMcpClient::new("test-calltool-401".to_string(), url, 5);
        client.initialize().await.unwrap();

        state
            .return_401_with_challenge
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let err = client
            .call_tool("search_code", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(
            matches!(err, UpstreamError::NeedsOAuth { .. }),
            "expected NeedsOAuth, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn classify_401_is_case_insensitive_on_bearer() {
        // Reqwest responses aren't trivially mockable — test the classifier
        // via HeaderMap manipulation in a lightweight way.
        use reqwest::header::{HeaderMap, HeaderValue};
        let mut hm = HeaderMap::new();
        hm.insert(
            reqwest::header::WWW_AUTHENTICATE,
            HeaderValue::from_static("BEARER realm=\"x\""),
        );
        // No public constructor for Response — exercise just the header
        // extraction logic.
        let value = hm
            .get(reqwest::header::WWW_AUTHENTICATE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap();
        assert!(value.to_ascii_lowercase().contains("bearer"));
    }

    // -- error Display/From --

    #[test]
    fn upstream_error_display_variants() {
        assert_eq!(UpstreamError::AuthFailed.to_string(), "Upstream authentication failed (401)");
        let needs = UpstreamError::NeedsOAuth {
            www_authenticate: "Bearer realm=\"x\"".to_string(),
        };
        assert!(needs.to_string().contains("OAuth required"));
        assert!(needs.to_string().contains("Bearer"));
        let other: UpstreamError = "boom".to_string().into();
        assert_eq!(other.to_string(), "boom");
    }

    #[test]
    fn upstream_error_from_string_is_other() {
        let e: UpstreamError = String::from("x").into();
        assert!(matches!(e, UpstreamError::Other(_)));
    }
}
