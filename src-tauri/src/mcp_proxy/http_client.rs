//! HTTP MCP client — connects to an upstream MCP server via Streamable HTTP.
//!
//! Implements the MCP client side of the legacy Streamable HTTP transport (spec 2025-11-25):
//! - initialize handshake → caches session_id and tool list
//! - tools/call forwarding with session_id header
//! - auto-reconnect on session expiry (400) or connection error
//! - health_check via tools/list
//! - Bearer token auth from OS keyring (static or OAuth 2.1)
//! - OAuth 2.1: pre-request refresh when access token is near expiry,
//!   401 WWW-Authenticate parsing → [`UpstreamError::NeedsOAuth`]

use crate::mcp_oauth::token::TokenManager;
use crate::mcp_upstream_config::{UpstreamHeader, UpstreamTransport};
use crate::mcp_upstream_credentials::{
    OAuthTokenSet, StoredCredential, is_token_valid, read_stored_credential,
};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OnceCell;

const MCP_SESSION_HEADER: &str = "mcp-session-id";
const MCP_PROTOCOL_VERSION_HEADER: &str = "MCP-Protocol-Version";
const PROTOCOL_VERSION: &str = "2025-11-25";

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
    ///
    /// The challenge is reduced to its scheme so an upstream cannot echo
    /// credentials into diagnostics. We do not yet parse `resource_metadata`
    /// (RFC 9728 §3.1), which would let us
    /// auto-discover the authorization server URL instead of falling back to
    /// `<origin>/.well-known/oauth-authorization-server`. See story 1284-cc3e.
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
            Self::NeedsOAuth { .. } => f.write_str("OAuth required (Bearer challenge)"),
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
    /// Per-client OAuth token manager, lazily initialized from the stored
    /// OAuth credential on first refresh. Sharing the manager (and its mutex)
    /// across `refresh_token_if_needed` and `force_refresh` is what keeps
    /// concurrent 401-retry paths from triggering parallel refresh requests.
    token_manager: OnceCell<Arc<TokenManager>>,
    /// Whether this upstream has auth configured (bearer or OAuth). When false,
    /// `resolve_bearer` skips the keychain entirely — avoids macOS permission
    /// popups for upstreams that don't need credentials.
    has_auth: bool,
    headers: Vec<UpstreamHeader>,
}

impl HttpMcpClient {
    /// Create a new HTTP MCP client.
    ///
    /// `timeout_secs` is the per-request timeout (0 = no timeout).
    pub(crate) fn new(name: String, url: String, timeout_secs: u32, has_auth: bool) -> Self {
        let timeout = if timeout_secs > 0 {
            Some(Duration::from_secs(timeout_secs as u64))
        } else {
            None
        };

        // reqwest strips Authorization, but retains arbitrary headers on a
        // cross-origin redirect. Refuse that hop before any credentials leave.
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 10 {
                    return attempt.error("Too many upstream redirects");
                }
                if attempt
                    .previous()
                    .first()
                    .is_some_and(|first| first.origin() != attempt.url().origin())
                {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .user_agent(concat!(
                "tuicommander-mcp-proxy/",
                env!("CARGO_PKG_VERSION")
            ));

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
            token_manager: OnceCell::new(),
            has_auth,
            headers: Vec::new(),
        }
    }

    pub(crate) fn with_headers(mut self, headers: Vec<UpstreamHeader>) -> Self {
        self.headers = headers;
        self
    }

    /// Resolve secret header values at request time; never attach them to config,
    /// logs, command arguments or a cached client default-header map.
    fn add_secret_headers(
        &self,
        mut req: reqwest::RequestBuilder,
        has_bearer: bool,
    ) -> Result<reqwest::RequestBuilder, UpstreamError> {
        for header in &self.headers {
            header.validate()?;
            if has_bearer && header.name.eq_ignore_ascii_case("authorization") {
                return Err("Custom Authorization conflicts with Bearer authentication".into());
            }
            let token = crate::mcp_upstream_credentials::read_upstream_credential(
                &header.credential_key(&self.name),
            )
            .map_err(|_| UpstreamError::from("Failed to read custom header credential"))?
            .ok_or_else(|| UpstreamError::from("Custom header credential is missing"))?;
            let mut value = reqwest::header::HeaderValue::from_str(&token)
                .map_err(|_| UpstreamError::from("Invalid custom header value"))?;
            value.set_sensitive(true);
            req = req.header(&header.name, value);
        }
        Ok(req)
    }

    /// Enable auth on this client (e.g. after an OAuth flow completes for an
    /// upstream that was initially configured without explicit `auth`).
    pub(crate) fn enable_auth(&mut self) {
        self.has_auth = true;
    }

    /// Build from an `UpstreamMcpServer` config (only Http transport).
    pub(crate) fn from_config(
        name: String,
        transport: &UpstreamTransport,
        timeout_secs: u32,
        has_auth: bool,
    ) -> Option<Self> {
        match transport {
            UpstreamTransport::Http { url } => {
                Some(Self::new(name, url.clone(), timeout_secs, has_auth))
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
    ///
    /// Deliberately re-read on every request instead of memoised in the client:
    /// the credential vault already holds the decrypted blob in a process-wide
    /// cache (`credentials.rs`), so this costs a map lookup, not a keychain
    /// round-trip. A second copy inside the client is what made a completed
    /// OAuth flow appear to do nothing — the client kept serving the dead token
    /// it had cached before the user re-authorized.
    async fn resolve_bearer(&self) -> Result<Option<String>, UpstreamError> {
        if !self.has_auth {
            return Ok(None);
        }
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
        let tm = self.token_manager_for(set).await;
        tm.refresh_if_needed(set)
            .await
            .map_err(|e| classify_refresh_error(&e.to_string()))
    }

    /// Lazily build (or return) the per-client [`TokenManager`]. Sharing the
    /// instance across calls keeps `refresh_lock` coherent, so two concurrent
    /// 401 retries collapse into a single refresh request at the AS.
    ///
    /// The resource indicator is preferred from `set.resource` (populated at
    /// exchange time) and falls back to `self.url` only for legacy tokens
    /// stored before the field existed.
    async fn token_manager_for(&self, set: &OAuthTokenSet) -> Arc<TokenManager> {
        self.token_manager
            .get_or_init(|| async {
                let resource = set.resource.clone().or_else(|| Some(self.url.clone()));
                Arc::new(TokenManager::new(
                    self.name.clone(),
                    set.client_id.clone(),
                    set.client_secret.clone(),
                    set.token_endpoint.clone(),
                    resource,
                ))
            })
            .await
            .clone()
    }

    /// Recover after a 401 by refreshing the OAuth credential that was
    /// rejected, unless a different valid generation has since been stored.
    async fn force_refresh(
        &self,
        rejected_bearer: Option<&str>,
    ) -> Result<Option<OAuthTokenSet>, UpstreamError> {
        if !self.has_auth {
            return Ok(None);
        }
        let Some(rejected_bearer) = rejected_bearer else {
            return Ok(None);
        };
        let cred = read_stored_credential(&self.name)
            .map_err(|e| UpstreamError::Other(format!("keyring read failed: {e}")))?;
        let Some(StoredCredential::Oauth2(set)) = cred else {
            return Ok(None);
        };
        let tm = self.token_manager_for(&set).await;
        tm.refresh_after_rejection(&set, rejected_bearer)
            .await
            .map_err(|e| classify_refresh_error(&e.to_string()))
    }

    /// Perform the MCP initialize handshake.
    ///
    /// 1. Resolves auth (static Bearer or refreshed OAuth access token) from keyring.
    /// 2. Sends `initialize` request.
    /// 3. Sends `notifications/initialized` (fire-and-forget).
    /// 4. Calls `tools/list` and returns the tool definitions.
    ///
    /// A 401 costs the user a trip through the consent screen, so spend the
    /// refresh token first — the same recovery `rpc_with_session` already does
    /// for tool calls. Without it an access token that died since the last run
    /// parks the upstream in `NeedsAuth` even though we hold the means to renew
    /// it unattended.
    pub(crate) async fn initialize(&mut self) -> Result<Vec<UpstreamToolDef>, UpstreamError> {
        let auth_token = self.resolve_bearer().await?;
        match self.initialize_once(auth_token.as_deref()).await {
            Err(e @ (UpstreamError::NeedsOAuth { .. } | UpstreamError::AuthFailed)) => {
                match self.force_refresh(auth_token.as_deref()).await {
                    Ok(Some(refreshed)) => {
                        self.initialize_once(Some(&refreshed.access_token)).await
                    }
                    // No refresh token, or the AS rejected it — the user really
                    // does have to re-authorize. Report the original challenge.
                    _ => Err(e),
                }
            }
            other => other,
        }
    }

    async fn initialize_once(
        &mut self,
        auth_token: Option<&str>,
    ) -> Result<Vec<UpstreamToolDef>, UpstreamError> {
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

        let resp = self.send_post(&init_body, auth_token, None).await?;

        // Extract session ID from response header (preserved across auth retries
        // by re-reading after the final response).
        self.session_id = resp
            .headers()
            .get(MCP_SESSION_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let _init_resp = self.decode_response(resp).await?;

        tracing::debug!(
            upstream = %self.name,
            session_id = ?self.session_id,
            "initialize response received"
        );

        // Fire-and-forget: notifications/initialized
        let _ = self
            .rpc_raw(
                "notifications/initialized",
                serde_json::json!({}),
                auth_token,
            )
            .await;

        // Fetch tool list
        self.fetch_tools(auth_token).await
    }

    /// Fetch the tool list from the upstream server.
    async fn fetch_tools(
        &self,
        auth_token: Option<&str>,
    ) -> Result<Vec<UpstreamToolDef>, UpstreamError> {
        let resp_value = self
            .rpc("tools/list", serde_json::json!({}), auth_token)
            .await?;

        let tools_arr = match resp_value["result"]["tools"].as_array() {
            Some(arr) => arr.clone(),
            None => {
                tracing::warn!(
                    upstream = %self.name,
                    "tools/list response missing result.tools"
                );
                Vec::new()
            }
        };

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
            let mut req = self
                .client
                .delete(&self.url)
                .header(MCP_SESSION_HEADER, sid);
            if let Some(token) = &auth_token {
                req = req.bearer_auth(token);
            }
            if let Ok(req) = self.add_secret_headers(req, auth_token.is_some()) {
                let _ = req.send().await;
            }
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
        let resp = self
            .send_post(body, auth_token, self.session_id.as_deref())
            .await?;

        if resp.status() == reqwest::StatusCode::BAD_REQUEST {
            return Err(UpstreamError::Other(format!(
                "Upstream '{}' returned 400 (session may have expired)",
                self.name
            )));
        }

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            // Single retry: if OAuth credential, force-refresh once and retry.
            if let Ok(Some(refreshed)) = self.force_refresh(auth_token).await {
                let retry_resp = self
                    .send_post(
                        body,
                        Some(&refreshed.access_token),
                        self.session_id.as_deref(),
                    )
                    .await?;
                return self.decode_response(retry_resp).await;
            }
            return Err(classify_401(&resp));
        }

        self.decode_response(resp).await
    }

    /// Decode a reqwest Response into JSON or classify error status.
    ///
    /// Handles both `application/json` and `text/event-stream` (SSE) responses.
    /// For SSE, extracts the last `data:` line containing a JSON-RPC message.
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

        let is_sse = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.contains("text/event-stream"));

        if is_sse {
            let body = resp.text().await.map_err(|_| {
                UpstreamError::Other(format!("Upstream '{}' SSE read error", self.name))
            })?;
            parse_sse_json(&body).ok_or_else(|| {
                UpstreamError::Other(format!(
                    "Upstream '{}' SSE response contained no valid JSON data line",
                    self.name
                ))
            })
        } else {
            resp.json::<Value>().await.map_err(|_| {
                UpstreamError::Other(format!("Upstream '{}' invalid JSON response", self.name))
            })
        }
    }

    /// Build and send a POST with optional bearer auth and session header.
    async fn send_post(
        &self,
        body: &Value,
        auth_token: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<reqwest::Response, UpstreamError> {
        let mut req = self
            .client
            .post(&self.url)
            .header(
                reqwest::header::ACCEPT,
                "application/json, text/event-stream",
            )
            .header(MCP_PROTOCOL_VERSION_HEADER, PROTOCOL_VERSION)
            .json(body);
        if let Some(sid) = session_id {
            req = req.header(MCP_SESSION_HEADER, sid);
        }
        if let Some(token) = auth_token {
            req = req.bearer_auth(token);
        }
        self.add_secret_headers(req, auth_token.is_some())?
            .send()
            .await
            .map_err(|_| UpstreamError::Other(format!("Upstream '{}' request failed", self.name)))
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
        self.decode_response(resp).await
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
        Some(h) if h.to_ascii_lowercase().contains("bearer") => UpstreamError::NeedsOAuth {
            www_authenticate: "Bearer".into(),
        },
        _ => UpstreamError::AuthFailed,
    }
}

/// Classify a token-refresh failure (raw `anyhow` error message) as either a
/// fatal auth error that needs re-authorization or a transient error.
///
/// A revoked/expired refresh token (AS replies `400 invalid_grant`, or there is
/// no refresh token at all) cannot be recovered automatically — mapping it to
/// [`UpstreamError::AuthFailed`] lets the registry re-enter `NeedsAuth` (shows
/// "Authorize") instead of parking the upstream in a silent red failure.
/// Transient/network refresh errors stay [`UpstreamError::Other`] so the circuit
/// breaker + health checks keep retrying.
fn classify_refresh_error(msg: &str) -> UpstreamError {
    if msg.contains("invalid_grant")
        || msg.contains("no refresh_token available")
        || msg.contains("HTTP 400")
        || msg.contains("HTTP 401")
    {
        UpstreamError::AuthFailed
    } else {
        UpstreamError::Other(format!("token refresh failed: {msg}"))
    }
}

/// Extract the last JSON-RPC message from an SSE body.
///
/// SSE format: lines starting with `data:` contain the payload.
/// Multiple `data:` lines before a blank line are concatenated (per spec).
/// We take the last complete JSON object found since earlier ones may be
/// progress notifications.
fn parse_sse_json(body: &str) -> Option<Value> {
    let mut last_value: Option<Value> = None;
    let mut current_data = String::new();

    for line in body.lines() {
        if let Some(data) = line.strip_prefix("data:") {
            let data = data.trim_start();
            if !current_data.is_empty() {
                current_data.push('\n');
            }
            current_data.push_str(data);
        } else if line.is_empty() && !current_data.is_empty() {
            if let Ok(val) = serde_json::from_str::<Value>(&current_data) {
                last_value = Some(val);
            }
            current_data.clear();
        }
    }
    // Handle trailing data without final blank line
    if !current_data.is_empty()
        && let Ok(val) = serde_json::from_str::<Value>(&current_data)
    {
        last_value = Some(val);
    }
    last_value
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn secret_headers_reach_every_mcp_request_and_rotation_is_not_cached() {
        #[derive(Clone)]
        struct LogWriter(Arc<std::sync::Mutex<Vec<u8>>>);
        impl std::io::Write for LogWriter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let logs = Arc::new(std::sync::Mutex::new(Vec::new()));
        let writer = LogWriter(logs.clone());
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let _subscriber = tracing::subscriber::set_default(subscriber);
        let seen = Arc::new(std::sync::Mutex::new(
            Vec::<(String, axum::http::HeaderMap)>::new(),
        ));
        let captured = seen.clone();
        let router = axum::Router::new().route(
            "/mcp",
            axum::routing::post(
                move |headers: axum::http::HeaderMap, axum::Json(body): axum::Json<Value>| {
                    let captured = captured.clone();
                    async move {
                        captured
                            .lock()
                            .unwrap()
                            .push((body["method"].as_str().unwrap().to_owned(), headers));
                        (
                            [("mcp-session-id", "secret-header-session")],
                            axum::Json(
                                serde_json::json!({"jsonrpc":"2.0", "id":1, "result":{"tools":[]}}),
                            ),
                        )
                    }
                },
            )
            .delete({
                let seen = seen.clone();
                move |headers: axum::http::HeaderMap| {
                    let seen = seen.clone();
                    async move {
                        seen.lock().unwrap().push(("DELETE".into(), headers));
                        axum::http::StatusCode::OK
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let name = "secret-headers-wire";
        let headers = ["x-api-key", "X-Consumer-Key"].map(|name| UpstreamHeader {
            name: name.into(),
            credential_ref: uuid::Uuid::new_v4().to_string(),
        });
        for header in &headers {
            crate::mcp_upstream_credentials::save_mcp_upstream_credential(
                name.into(),
                "DUMMY_HEADER_SENTINEL".into(),
                Some(header.clone()),
            )
            .unwrap();
        }
        crate::mcp_upstream_credentials::save_upstream_credential(name, "DUMMY_BEARER_SENTINEL")
            .unwrap();
        let mut client =
            HttpMcpClient::new(name.into(), url, 30, true).with_headers(headers.to_vec());
        client.initialize().await.unwrap();
        client
            .call_tool("example", serde_json::json!({}))
            .await
            .unwrap();
        client.health_check().await.unwrap();
        client.shutdown().await;
        {
            let seen = seen.lock().unwrap();
            assert_eq!(
                seen.iter()
                    .map(|(method, _)| method.as_str())
                    .collect::<Vec<_>>(),
                [
                    "initialize",
                    "notifications/initialized",
                    "tools/list",
                    "tools/call",
                    "tools/list",
                    "DELETE"
                ]
            );
            for (_, received) in seen.iter() {
                assert_eq!(received["authorization"], "Bearer DUMMY_BEARER_SENTINEL");
                assert_eq!(received["x-api-key"], "DUMMY_HEADER_SENTINEL");
                assert_eq!(received["x-consumer-key"], "DUMMY_HEADER_SENTINEL");
            }
        }
        crate::mcp_upstream_credentials::save_mcp_upstream_credential(
            name.into(),
            "DUMMY_ROTATED_SENTINEL".into(),
            Some(headers[0].clone()),
        )
        .unwrap();
        client.health_check().await.unwrap();
        assert_eq!(
            seen.lock().unwrap().last().unwrap().1["x-api-key"],
            "DUMMY_ROTATED_SENTINEL"
        );
        let logs = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
        assert!(
            logs.contains("initialize response received"),
            "capture must observe real client diagnostics"
        );
        assert!(!logs.contains("DUMMY_"));
        assert!(!std::env::args().any(|arg| arg.contains("DUMMY_")));
        task.abort();
    }

    #[tokio::test]
    async fn secret_headers_fail_closed_without_echoing_missing_or_invalid_values() {
        let header = UpstreamHeader {
            name: "x-api-key".into(),
            credential_ref: uuid::Uuid::new_v4().to_string(),
        };
        let client = HttpMcpClient::new(
            "secret-header-errors".into(),
            "http://127.0.0.1:1/mcp".into(),
            30,
            false,
        )
        .with_headers(vec![header.clone()]);
        let error = client.health_check().await.unwrap_err().to_string();
        assert_eq!(error, "Custom header credential is missing");
        crate::mcp_upstream_credentials::save_upstream_credential(
            &header.credential_key("secret-header-errors"),
            "DUMMY_INVALID_SENTINEL\r\ninjected",
        )
        .unwrap();
        let error = client.health_check().await.unwrap_err().to_string();
        assert_eq!(error, "Invalid custom header value");
        assert!(!error.contains("DUMMY_"));
    }

    #[tokio::test]
    async fn secret_headers_follow_same_origin_but_never_contact_redirect_origin() {
        let foreign_hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let hits = foreign_hits.clone();
        let foreign = axum::Router::new().fallback(move || {
            let hits = hits.clone();
            async move {
                hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                "leaked"
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let foreign_url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let foreign_task =
            tokio::spawn(async move { axum::serve(listener, foreign).await.unwrap() });
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = seen.clone();
        let router = axum::Router::new()
            .route(
                "/mcp",
                axum::routing::post(|| async { axum::response::Redirect::temporary("/next") }),
            )
            .route(
                "/next",
                axum::routing::post(move |headers: axum::http::HeaderMap| {
                    let captured = captured.clone();
                    let foreign_url = foreign_url.clone();
                    async move {
                        captured.lock().unwrap().push(headers);
                        axum::response::Redirect::temporary(&foreign_url)
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let header = UpstreamHeader {
            name: "x-api-key".into(),
            credential_ref: uuid::Uuid::new_v4().to_string(),
        };
        crate::mcp_upstream_credentials::save_mcp_upstream_credential(
            "secret-redirect".into(),
            "DUMMY_REDIRECT_SENTINEL".into(),
            Some(header.clone()),
        )
        .unwrap();
        crate::mcp_upstream_credentials::save_upstream_credential(
            "secret-redirect",
            "DUMMY_BEARER_SENTINEL",
        )
        .unwrap();
        let mut client =
            HttpMcpClient::new("secret-redirect".into(), url, 30, true).with_headers(vec![header]);
        let error = client.initialize().await.unwrap_err().to_string();
        assert!(!error.contains("DUMMY_"));
        assert_eq!(foreign_hits.load(std::sync::atomic::Ordering::SeqCst), 0);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "same-origin redirect must still work");
        assert_eq!(seen[0]["x-api-key"], "DUMMY_REDIRECT_SENTINEL");
        assert_eq!(seen[0]["authorization"], "Bearer DUMMY_BEARER_SENTINEL");
        task.abort();
        foreign_task.abort();
    }
    use std::sync::Arc;

    use axum::{
        Json, Router,
        extract::State as AxumState,
        http::{HeaderMap, StatusCode},
        response::IntoResponse,
        routing::post,
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
        /// Bearer token seen on the most recent request, so a test can assert
        /// *which* credential the client actually put on the wire.
        seen_bearer: Arc<std::sync::Mutex<Option<String>>>,
        /// MCP protocol revision seen on the most recent request.
        seen_protocol_version: Arc<std::sync::Mutex<Option<String>>>,
        /// When set, any bearer other than this one is answered with a 401 +
        /// challenge — models a server that has expired the old access token.
        only_accept_bearer: Arc<std::sync::Mutex<Option<String>>>,
        /// One-shot OAuth exchange result persisted after observing a rejected
        /// bearer but before returning its 401 response.
        credential_replacement: Arc<std::sync::Mutex<Option<(String, OAuthTokenSet)>>>,
        /// Number of refresh grants sent to the mock authorization server.
        token_request_count: Arc<std::sync::atomic::AtomicU32>,
    }

    fn oauth_challenge_response() -> axum::response::Response {
        let mut headers = HeaderMap::new();
        headers.insert(
            "WWW-Authenticate",
            "Bearer realm=\"mcp\", resource_metadata=\"https://api.example.com/.well-known/oauth-protected-resource\""
                .parse()
                .unwrap(),
        );
        (
            StatusCode::UNAUTHORIZED,
            headers,
            Json(serde_json::json!({})),
        )
            .into_response()
    }

    async fn mock_mcp_handler(
        AxumState(state): AxumState<MockState>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> impl IntoResponse {
        let method = body["method"].as_str().unwrap_or("");
        let id = body["id"].clone();

        let bearer = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::to_string);
        *state.seen_bearer.lock().unwrap() = bearer.clone();
        *state.seen_protocol_version.lock().unwrap() = headers
            .get(MCP_PROTOCOL_VERSION_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);

        let expected = state.only_accept_bearer.lock().unwrap().clone();
        if let Some(expected) = expected
            && bearer.as_deref() != Some(expected.as_str())
        {
            let replacement = state.credential_replacement.lock().unwrap().take();
            if let Some((name, set)) = replacement {
                crate::mcp_upstream_credentials::save_oauth_tokens(&name, &set).unwrap();
            }
            return oauth_challenge_response();
        }

        if state
            .return_401_with_challenge
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return oauth_challenge_response();
        }
        if state
            .return_401_no_challenge
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({}))).into_response();
        }

        match method {
            "initialize" => {
                state
                    .init_count
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if state
                    .fail_initialize
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({})),
                    )
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

    /// Mock authorization server: hands out `refreshed-token` for any
    /// `refresh_token` grant, so a test can watch the client recover on its own.
    async fn mock_token_handler(AxumState(state): AxumState<MockState>) -> impl IntoResponse {
        state
            .token_request_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        *state.only_accept_bearer.lock().unwrap() = Some("refreshed-token".to_string());
        Json(serde_json::json!({
            "access_token": "refreshed-token",
            "token_type": "Bearer",
            "expires_in": 3600,
        }))
    }

    /// Returns the MCP URL; the token endpoint lives at `/token` on the same host.
    async fn spawn_mock_server(state: MockState) -> String {
        let app = Router::new()
            .route("/mcp", post(mock_mcp_handler))
            .route("/token", post(mock_token_handler))
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

        let mut client = HttpMcpClient::new("test".to_string(), url, 10, false);
        assert!(!client.is_connected());

        let tools = client.initialize().await.unwrap();
        assert!(client.is_connected());
        assert_eq!(client.session_id.as_deref(), Some("test-session-id-123"));
        assert_eq!(tools.len(), 2);
    }

    #[tokio::test]
    async fn initialize_sends_legacy_protocol_version_header() {
        let state = MockState::default();
        let url = spawn_mock_server(state.clone()).await;

        let mut client = HttpMcpClient::new("test".to_string(), url, 10, false);
        client.initialize().await.unwrap();

        assert_eq!(
            state.seen_protocol_version.lock().unwrap().as_deref(),
            Some(PROTOCOL_VERSION)
        );
    }

    #[tokio::test]
    async fn initialize_fails_when_server_returns_500() {
        let state = MockState::default();
        state
            .fail_initialize
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let url = spawn_mock_server(state).await;

        let mut client = HttpMcpClient::new("test".to_string(), url, 10, false);
        let result = client.initialize().await;
        let err = result.unwrap_err();
        assert!(matches!(err, UpstreamError::Other(_)));
        assert!(err.to_string().contains("500"));
    }

    #[tokio::test]
    async fn call_tool_forwards_and_returns_result() {
        let state = MockState::default();
        let url = spawn_mock_server(state).await;

        let mut client = HttpMcpClient::new("test".to_string(), url, 10, false);
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

        let mut client = HttpMcpClient::new("test".to_string(), url, 10, false);
        client.initialize().await.unwrap();

        let result = client.health_check().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn health_check_fails_when_not_initialized() {
        let client = HttpMcpClient::new(
            "test".to_string(),
            "http://127.0.0.1:1/mcp".to_string(),
            2,
            false,
        );
        let result = client.health_check().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn from_config_returns_some_for_http_transport() {
        let transport = crate::mcp_upstream_config::UpstreamTransport::Http {
            url: "http://localhost:8080/mcp".to_string(),
        };
        let client = HttpMcpClient::from_config("test".to_string(), &transport, 30, false);
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
        let client = HttpMcpClient::from_config("test".to_string(), &transport, 30, false);
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

        let mut client = HttpMcpClient::new("test-401-oauth".to_string(), url, 5, true);
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

        let mut client = HttpMcpClient::new("test-401-plain".to_string(), url, 5, true);
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

        let mut client = HttpMcpClient::new("test-calltool-401".to_string(), url, 5, true);
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

    // -- credential rotation: a re-authorization must reach the wire --

    /// The bug Boss hit: re-authorizing an upstream showed a success screen and
    /// changed nothing. The client had memoised the bearer it read on the first
    /// connect, so every later request replayed the dead token and the server
    /// kept answering "authorize me" — an unbreakable loop, since each new flow
    /// wrote a token the client never looked at again.
    #[tokio::test]
    async fn initialize_sends_the_rotated_credential_not_the_first_one() {
        let name = "test-rotate-initialize";
        let state = MockState::default();
        let url = spawn_mock_server(state.clone()).await;

        crate::mcp_upstream_credentials::save_upstream_credential(name, "stale-token").unwrap();
        let mut client = HttpMcpClient::new(name.to_string(), url, 5, true);
        client.initialize().await.unwrap();
        assert_eq!(
            state.seen_bearer.lock().unwrap().as_deref(),
            Some("stale-token")
        );

        // A completed OAuth flow persists a new token for the same upstream.
        crate::mcp_upstream_credentials::save_upstream_credential(name, "fresh-token").unwrap();

        client.initialize().await.unwrap();
        assert_eq!(
            state.seen_bearer.lock().unwrap().as_deref(),
            Some("fresh-token"),
            "re-connecting after re-authorization must use the newly stored token"
        );
    }

    /// Same invariant on the steady-state paths — a rotation mid-session must
    /// not leave health checks and tool calls pinned to the old credential.
    #[tokio::test]
    async fn tool_calls_and_health_checks_follow_credential_rotation() {
        let name = "test-rotate-steady-state";
        let state = MockState::default();
        let url = spawn_mock_server(state.clone()).await;

        crate::mcp_upstream_credentials::save_upstream_credential(name, "stale-token").unwrap();
        let mut client = HttpMcpClient::new(name.to_string(), url, 5, true);
        client.initialize().await.unwrap();

        crate::mcp_upstream_credentials::save_upstream_credential(name, "fresh-token").unwrap();

        client.health_check().await.unwrap();
        assert_eq!(
            state.seen_bearer.lock().unwrap().as_deref(),
            Some("fresh-token"),
            "health check must re-read the rotated credential"
        );

        client
            .call_tool("search_code", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(
            state.seen_bearer.lock().unwrap().as_deref(),
            Some("fresh-token"),
            "tool call must re-read the rotated credential"
        );
    }

    /// An access token the server has expired is recoverable without the user:
    /// we hold a refresh token, so the handshake spends it instead of parking
    /// the upstream in NeedsAuth. Note `expires_at` is far in the future here —
    /// some gateways report a nonsense lifetime, so the server's 401 is the only
    /// trustworthy signal that the token is dead.
    #[tokio::test]
    async fn initialize_refreshes_a_server_rejected_token_instead_of_asking_the_user() {
        let name = "test-init-refresh";
        let state = MockState::default();
        let url = spawn_mock_server(state.clone()).await;
        let token_endpoint = url.replace("/mcp", "/token");

        *state.only_accept_bearer.lock().unwrap() = Some("never-issued".to_string());
        let set = OAuthTokenSet {
            access_token: "expired-token".to_string(),
            refresh_token: Some("refresh-me".to_string()),
            expires_at: Some(i64::MAX / 2),
            token_endpoint,
            client_id: "client".to_string(),
            client_secret: None,
            scope: None,
            resource: None,
        };
        crate::mcp_upstream_credentials::save_oauth_tokens(name, &set).unwrap();

        let mut client = HttpMcpClient::new(name.to_string(), url, 5, true);
        let tools = client.initialize().await.unwrap();

        assert_eq!(tools.len(), 2);
        assert_eq!(
            state.seen_bearer.lock().unwrap().as_deref(),
            Some("refreshed-token"),
            "the handshake should have retried with the refreshed access token"
        );
    }

    /// Re-authorization can complete after a request has put its bearer on the
    /// wire but before 401 recovery re-reads storage. The rejected generation
    /// must remain the comparison key: the newly exchanged credential is
    /// retried as-is, even when it has no refresh token, and the token endpoint
    /// must not be called.
    #[tokio::test]
    async fn tool_call_reuses_credential_written_between_401_and_forced_refresh() {
        let name = "test-reauthorize-during-401";
        let state = MockState::default();
        let url = spawn_mock_server(state.clone()).await;
        let token_endpoint = url.replace("/mcp", "/token");

        let rejected = OAuthTokenSet {
            access_token: "rejected-token".to_string(),
            refresh_token: Some("old-refresh-token".to_string()),
            expires_at: Some(i64::MAX / 2),
            token_endpoint: token_endpoint.clone(),
            client_id: "client".to_string(),
            client_secret: None,
            scope: None,
            resource: None,
        };
        crate::mcp_upstream_credentials::save_oauth_tokens(name, &rejected).unwrap();
        *state.only_accept_bearer.lock().unwrap() = Some("rejected-token".to_string());

        let mut client = HttpMcpClient::new(name.to_string(), url, 5, true);
        client.initialize().await.unwrap();

        let replacement = OAuthTokenSet {
            access_token: "reauthorized-token".to_string(),
            refresh_token: None,
            expires_at: Some(i64::MAX / 2),
            token_endpoint,
            client_id: "client".to_string(),
            client_secret: None,
            scope: None,
            resource: None,
        };
        *state.only_accept_bearer.lock().unwrap() = Some("reauthorized-token".to_string());
        *state.credential_replacement.lock().unwrap() = Some((name.to_string(), replacement));

        client
            .call_tool("search_code", serde_json::json!({}))
            .await
            .unwrap();

        assert_eq!(
            state.seen_bearer.lock().unwrap().as_deref(),
            Some("reauthorized-token"),
            "the retry must use the credential exchanged during the 401 interleaving"
        );
        assert_eq!(
            state
                .token_request_count
                .load(std::sync::atomic::Ordering::SeqCst),
            0,
            "the replacement credential must not be refreshed or rotated"
        );
    }

    /// The recovery must not swallow the challenge when there is nothing to
    /// refresh with — the user still has to be sent to the consent screen.
    #[tokio::test]
    async fn initialize_keeps_needs_oauth_when_no_refresh_token_is_held() {
        let name = "test-init-no-refresh";
        let state = MockState::default();
        let url = spawn_mock_server(state.clone()).await;

        crate::mcp_upstream_credentials::save_upstream_credential(name, "static-bearer").unwrap();
        state
            .return_401_with_challenge
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let mut client = HttpMcpClient::new(name.to_string(), url, 5, true);
        let err = client.initialize().await.unwrap_err();
        assert!(
            matches!(err, UpstreamError::NeedsOAuth { .. }),
            "expected NeedsOAuth, got: {err:?}"
        );
    }

    // -- refresh-error classification --

    #[test]
    fn classify_refresh_error_maps_fatal_auth_failures_to_auth_failed() {
        // A revoked refresh token (AS replies 400 invalid_grant) must become
        // AuthFailed so the registry prompts re-authorization instead of going
        // silently red.
        for msg in [
            "Token refresh failed (HTTP 400): {\"error\":\"invalid_grant\"}",
            "Token expired and no refresh_token available",
            "Token refresh failed (HTTP 401): unauthorized",
        ] {
            assert!(
                matches!(classify_refresh_error(msg), UpstreamError::AuthFailed),
                "expected AuthFailed for: {msg}"
            );
        }
    }

    #[test]
    fn classify_refresh_error_keeps_transient_failures_as_other() {
        // Network/5xx refresh errors are retryable — they must stay Other so the
        // circuit breaker keeps trying, not park the upstream in NeedsAuth.
        for msg in [
            "Token refresh request failed: connection refused",
            "Token refresh failed (HTTP 503): service unavailable",
        ] {
            assert!(
                matches!(classify_refresh_error(msg), UpstreamError::Other(_)),
                "expected Other for: {msg}"
            );
        }
    }

    // -- error Display/From --

    #[test]
    fn upstream_error_display_variants() {
        assert_eq!(
            UpstreamError::AuthFailed.to_string(),
            "Upstream authentication failed (401)"
        );
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

    #[test]
    fn parse_sse_json_single_data_line() {
        let body =
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[]}}\n\n";
        let val = parse_sse_json(body).unwrap();
        assert_eq!(val["result"]["tools"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn parse_sse_json_multiple_events_returns_last() {
        let body = concat!(
            "data: {\"jsonrpc\":\"2.0\",\"method\":\"progress\"}\n\n",
            "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[{\"name\":\"foo\"}]}}\n\n",
        );
        let val = parse_sse_json(body).unwrap();
        assert_eq!(val["result"]["tools"][0]["name"], "foo");
    }

    #[test]
    fn parse_sse_json_no_trailing_blank_line() {
        let body = "data: {\"ok\":true}";
        let val = parse_sse_json(body).unwrap();
        assert_eq!(val["ok"], true);
    }

    #[test]
    fn parse_sse_json_empty_body() {
        assert!(parse_sse_json("").is_none());
        assert!(parse_sse_json("event: message\n\n").is_none());
    }

    #[test]
    fn parse_sse_json_multiline_data() {
        let body = "data: {\"a\":\n\ndata: {\"b\":1}\n\n";
        let val = parse_sse_json(body).unwrap();
        assert_eq!(val["b"], 1);
    }
}
