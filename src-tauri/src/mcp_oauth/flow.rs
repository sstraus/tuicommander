//! OAuth flow orchestrator.
//!
//! Coordinates the full OAuth 2.1 Authorization Code + PKCE flow for upstream
//! MCP servers:
//!
//! 1. [`OAuthFlowManager::start_flow`]: acquires the auth semaphore (serializing
//!    concurrent flows), resolves authorization/token endpoints (discovery or
//!    config override), generates a PKCE S256 challenge and a cryptographically
//!    random `state` nonce, inserts a [`PendingFlow`], and returns the
//!    authorization URL for the caller to open in the browser.
//! 2. [`OAuthFlowManager::complete_flow`]: called from the deep-link handler
//!    (or the dev-mode localhost callback server). Looks up the pending flow
//!    by keyed `state`, calls [`TokenManager::exchange_code`], and returns the
//!    resulting [`OAuthTokenSet`]. See that method's docs for the threat
//!    model that justifies skipping constant-time comparison here.
//! 3. [`OAuthFlowManager::cancel_flow`]: removes a pending flow (e.g. on user
//!    cancellation or upstream transition to `Failed`).
//!
//! A background task started via [`OAuthFlowManager::spawn_cleanup_task`]
//! periodically removes expired pending flows (default 5 minutes).
//!
//! In development (`#[cfg(debug_assertions)]`), use
//! [`DevCallbackServer::spawn`] to start an ephemeral localhost axum server
//! that receives the authorization response when deep links are unavailable
//! (Tauri dev mode).

use anyhow::{anyhow, bail, Result};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

use crate::mcp_oauth::discovery::{
    discover_auth_server, discover_protected_resource, registrable_domain,
};
use crate::mcp_oauth::token::{PkceChallengePair, TokenManager};
use crate::mcp_upstream_config::UpstreamAuth;
use crate::mcp_upstream_credentials::OAuthTokenSet;

/// Default timeout after which a pending flow is considered abandoned.
const DEFAULT_FLOW_TIMEOUT: Duration = Duration::from_secs(300);

/// Interval between cleanup sweeps for expired flows.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Pending flow
// ---------------------------------------------------------------------------

/// A flow in progress, awaiting the authorization callback.
#[derive(Clone)]
struct PendingFlow {
    /// Upstream name this flow is for.
    upstream_name: String,
    /// Full `state` nonce (also the DashMap key for this entry).
    state: String,
    /// PKCE verifier used when exchanging the code.
    pkce_verifier: String,
    /// Redirect URI used in the authorization request (tuic:// or localhost).
    redirect_uri: String,
    /// Resolved token endpoint.
    token_endpoint: String,
    /// OAuth client ID.
    client_id: String,
    /// Optional RFC 8707 resource indicator.
    resource: Option<String>,
    /// When this flow was created (for timeout cleanup).
    created_at: Instant,
}

/// Outcome of [`OAuthFlowManager::start_flow`] — enough information for the
/// caller to open the user's browser to the authorization endpoint.
#[derive(Debug, Clone)]
pub(crate) struct StartFlowOutcome {
    /// Fully built authorization URL with all query parameters.
    pub(crate) authorization_url: String,
    /// The `state` nonce generated for this flow (also embedded in the URL).
    pub(crate) state: String,
    /// Upstream name (echoed for convenience).
    #[allow(dead_code)]
    pub(crate) upstream_name: String,
}

// ---------------------------------------------------------------------------
// OAuthFlowManager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of in-flight OAuth flows.
///
/// Clone-safe via internal `Arc`s (`DashMap`, `Arc<Semaphore>`); callers
/// should wrap `OAuthFlowManager` itself in `Arc` for storage in `AppState`.
pub(crate) struct OAuthFlowManager {
    /// Concurrent flow serialization (shared with `UpstreamRegistry`).
    semaphore: Arc<Semaphore>,
    /// state nonce → pending flow.
    pending: Arc<DashMap<String, PendingFlow>>,
    /// HTTP client used for discovery and token exchange.
    http_client: reqwest::Client,
    /// Timeout after which a flow is considered abandoned.
    timeout: Duration,
    /// Active permit guards: state → permit. Dropping a permit releases the
    /// semaphore for the next queued flow.
    ///
    /// Stored separately from `pending` so that a completed flow can be
    /// removed from `pending` while the permit is released only after the
    /// caller explicitly finishes (avoids double-drop semantics).
    permits: Arc<DashMap<String, tokio::sync::OwnedSemaphorePermit>>,
}

impl OAuthFlowManager {
    pub(crate) fn new(semaphore: Arc<Semaphore>) -> Self {
        Self::with_timeout(semaphore, DEFAULT_FLOW_TIMEOUT)
    }

    pub(crate) fn with_timeout(semaphore: Arc<Semaphore>, timeout: Duration) -> Self {
        Self {
            semaphore,
            pending: Arc::new(DashMap::new()),
            http_client: reqwest::Client::new(),
            timeout,
            permits: Arc::new(DashMap::new()),
        }
    }

    /// Number of flows currently pending (test/diagnostics helper).
    #[cfg(test)]
    pub(crate) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Start a new OAuth flow.
    ///
    /// Acquires the shared auth semaphore (blocks if another flow is active),
    /// resolves endpoints via RFC 9728/8414 discovery when not pre-configured,
    /// generates PKCE + state nonce, and returns the authorization URL.
    ///
    /// The caller is responsible for opening the URL in the user's browser
    /// (typically via `tauri-plugin-opener`).
    pub(crate) async fn start_flow(
        &self,
        upstream_name: &str,
        server_url: &str,
        auth: &UpstreamAuth,
        redirect_uri: &str,
    ) -> Result<StartFlowOutcome> {
        // Extract OAuth2 fields (only OAuth2 variant starts a flow).
        let (client_id, scopes, authz_override, token_override) = match auth {
            UpstreamAuth::OAuth2 {
                client_id,
                scopes,
                authorization_endpoint,
                token_endpoint,
            } => (
                client_id.clone(),
                scopes.clone(),
                authorization_endpoint.clone(),
                token_endpoint.clone(),
            ),
            UpstreamAuth::Bearer { .. } => {
                bail!("OAuth flow requires OAuth2 auth config, got Bearer")
            }
        };

        // Acquire the flow semaphore (serialize concurrent flows).
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| anyhow!("OAuth semaphore closed: {e}"))?;

        // Resolve endpoints. If both overrides are present, skip discovery.
        let (authorization_endpoint, token_endpoint) =
            match (authz_override.clone(), token_override.clone()) {
                (Some(a), Some(t)) => (a, t),
                _ => self.resolve_endpoints(server_url).await?,
            };

        // Use the override as final if provided (fill missing from discovery).
        let authorization_endpoint = authz_override.unwrap_or(authorization_endpoint);
        let token_endpoint = token_override.unwrap_or(token_endpoint);

        // Generate PKCE + state nonce.
        let pkce = TokenManager::generate_pkce();
        let state = generate_state_nonce();

        // Build the authorization URL.
        let authorization_url = build_authorization_url(
            &authorization_endpoint,
            &client_id,
            redirect_uri,
            &scopes,
            &state,
            &pkce,
            Some(server_url),
        );

        // Store pending flow + permit.
        let flow = PendingFlow {
            upstream_name: upstream_name.to_string(),
            state: state.clone(),
            pkce_verifier: pkce.verifier.clone(),
            redirect_uri: redirect_uri.to_string(),
            token_endpoint,
            client_id,
            resource: Some(server_url.to_string()),
            created_at: Instant::now(),
        };
        self.pending.insert(state.clone(), flow);
        self.permits.insert(state.clone(), permit);

        Ok(StartFlowOutcome {
            authorization_url,
            state,
            upstream_name: upstream_name.to_string(),
        })
    }

    /// Complete a pending flow with the authorization `code` received on the
    /// callback. On success, the access/refresh tokens are saved to the
    /// keyring and returned.
    ///
    /// Threat model: this runs in a desktop Tauri app. The `state` parameter
    /// arrives via an OS-routed `tuic://` deep link, not over the network —
    /// there is no remote attacker who can probe the `pending` map with
    /// timing oracles. The miss branch below already returns `Err` in
    /// variable time, so any extra constant-time equality check on the hit
    /// branch would be a no-op (a `DashMap::remove(key)` hit proves
    /// `flow.state == state` by `String`'s `PartialEq`). We therefore rely on
    /// the keyed lookup alone for state binding.
    pub(crate) async fn complete_flow(
        &self,
        state: &str,
        code: &str,
    ) -> Result<(String, OAuthTokenSet)> {
        let (_removed_key, flow) = self
            .pending
            .remove(state)
            .ok_or_else(|| anyhow!("OAuth state mismatch or flow expired"))?;
        // Drop the permit regardless of exchange outcome — the browser round-
        // trip is over. `let _` here to make the intent explicit.
        let _permit = self.permits.remove(state);

        // Check timeout.
        if flow.created_at.elapsed() > self.timeout {
            bail!("OAuth flow expired ({} s)", self.timeout.as_secs());
        }

        // Exchange code for tokens.
        let token_mgr = TokenManager::new(
            flow.upstream_name.clone(),
            flow.client_id.clone(),
            flow.token_endpoint.clone(),
            flow.resource.clone(),
        );
        let tokens = token_mgr
            .exchange_code(code, &flow.pkce_verifier, &flow.redirect_uri)
            .await?;
        Ok((flow.upstream_name, tokens))
    }

    /// Cancel a pending flow, dropping its permit. Returns true if a flow
    /// with the given state was found and removed.
    pub(crate) fn cancel_flow(&self, state: &str) -> bool {
        let removed_pending = self.pending.remove(state).is_some();
        let _ = self.permits.remove(state);
        removed_pending
    }

    /// Cancel all flows for a given upstream name (e.g. on disconnect).
    pub(crate) fn cancel_flows_for(&self, upstream_name: &str) -> usize {
        let to_remove: Vec<String> = self
            .pending
            .iter()
            .filter(|e| e.value().upstream_name == upstream_name)
            .map(|e| e.key().clone())
            .collect();
        let n = to_remove.len();
        for state in to_remove {
            self.cancel_flow(&state);
        }
        n
    }

    /// Remove expired pending flows. Returns the number cleaned up.
    pub(crate) fn cleanup_expired(&self) -> usize {
        let timeout = self.timeout;
        let expired: Vec<String> = self
            .pending
            .iter()
            .filter(|e| e.value().created_at.elapsed() > timeout)
            .map(|e| e.key().clone())
            .collect();
        let n = expired.len();
        for state in expired {
            self.cancel_flow(&state);
        }
        n
    }

    /// Spawn a background task that periodically removes expired pending
    /// flows. The task runs for the lifetime of the returned `Arc`; drop all
    /// outer references to stop it.
    #[allow(dead_code)] // wired in by state.rs at app startup
    pub(crate) fn spawn_cleanup_task(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(CLEANUP_INTERVAL);
            ticker.tick().await; // skip immediate tick
            loop {
                ticker.tick().await;
                let Some(strong) = weak.upgrade() else { break };
                let n = strong.cleanup_expired();
                if n > 0 {
                    tracing::info!(
                        target: "mcp_oauth",
                        cleaned = n,
                        "Cleaned up expired OAuth flows"
                    );
                }
            }
        })
    }

    /// Resolve authorization and token endpoints via RFC 9728 + 8414 discovery.
    async fn resolve_endpoints(&self, server_url: &str) -> Result<(String, String)> {
        let pr_meta = discover_protected_resource(&self.http_client, server_url).await?;
        let issuer = pr_meta
            .authorization_servers
            .first()
            .ok_or_else(|| anyhow!("Protected resource returned no authorization servers"))?;
        // AS mix-up defence: refuse to follow an issuer whose registrable
        // domain differs from the resource's (unless both are loopback).
        check_issuer_matches_resource(server_url, issuer)?;
        let as_meta = discover_auth_server(&self.http_client, issuer).await?;
        Ok((as_meta.authorization_endpoint, as_meta.token_endpoint))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build the full authorization request URL with query parameters.
fn build_authorization_url(
    authorization_endpoint: &str,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
    state: &str,
    pkce: &PkceChallengePair,
    resource: Option<&str>,
) -> String {
    let mut url = url::Url::parse(authorization_endpoint)
        .unwrap_or_else(|_| url::Url::parse("https://invalid.example/").unwrap());
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code");
        q.append_pair("client_id", client_id);
        q.append_pair("redirect_uri", redirect_uri);
        if !scopes.is_empty() {
            q.append_pair("scope", &scopes.join(" "));
        }
        q.append_pair("state", state);
        q.append_pair("code_challenge", &pkce.challenge);
        q.append_pair("code_challenge_method", &pkce.method);
        if let Some(r) = resource {
            q.append_pair("resource", r);
        }
    }
    url.to_string()
}

/// AS mix-up defence (RFC 9700 §4.6): reject a discovered authorization
/// server whose registrable domain differs from the protected resource's.
///
/// A compromised or hostile resource server can put any HTTPS URL in the
/// `authorization_servers` array of its RFC 9728 metadata. Following that URL
/// blindly lets the attacker route the user to their own consent page.
/// Explicit overrides in `UpstreamAuth::OAuth2 { authorization_endpoint, ... }`
/// bypass discovery entirely, so this check only applies to the discovery
/// path — the user has already vetted the hard-coded endpoints in that case.
///
/// The error message includes both hostnames so the frontend surfaces them
/// in the consent UI (criterion #3). Loopback addresses are intentionally
/// treated as an automatic match — dev environments frequently put the AS
/// and resource on different localhost ports.
fn check_issuer_matches_resource(server_url: &str, issuer_url: &str) -> Result<()> {
    let server_domain = registrable_domain(server_url)
        .ok_or_else(|| anyhow!("MCP server_url \"{server_url}\" is not a valid URL"))?;
    let issuer_domain = registrable_domain(issuer_url)
        .ok_or_else(|| anyhow!("Authorization server issuer \"{issuer_url}\" is not a valid URL"))?;

    // Loopback — always allow (dev environments).
    if matches!(server_domain.as_str(), "localhost" | "127.0.0.1")
        || matches!(issuer_domain.as_str(), "localhost" | "127.0.0.1")
    {
        return Ok(());
    }

    if server_domain != issuer_domain {
        bail!(
            "Authorization server mix-up: MCP resource \"{server_url}\" advertises \
             issuer \"{issuer_url}\" whose registrable domain \"{issuer_domain}\" does \
             not match the resource's \"{server_domain}\". Configure an explicit \
             authorization_endpoint in the MCP auth config if this is intentional."
        );
    }
    Ok(())
}

/// Generate a 32-byte cryptographically random `state` nonce, URL-safe
/// base64-encoded (no padding).
fn generate_state_nonce() -> String {
    let bytes: [u8; 32] = rand::random();
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn oauth2_config() -> UpstreamAuth {
        UpstreamAuth::OAuth2 {
            client_id: "test-client".into(),
            scopes: vec!["read".into(), "write".into()],
            authorization_endpoint: Some("https://auth.example.com/authorize".into()),
            token_endpoint: Some("https://auth.example.com/token".into()),
        }
    }

    fn mgr() -> OAuthFlowManager {
        OAuthFlowManager::new(Arc::new(Semaphore::new(1)))
    }

    // -- helpers --

    #[test]
    fn state_nonce_is_random_and_long() {
        let a = generate_state_nonce();
        let b = generate_state_nonce();
        assert_ne!(a, b, "two nonces must differ");
        // base64 URL_SAFE_NO_PAD of 32 bytes = 43 chars
        assert!(a.len() >= 40, "state too short: {} chars", a.len());
    }

    #[test]
    fn build_authorization_url_includes_all_params() {
        let pkce = PkceChallengePair {
            challenge: "abc".into(),
            method: "S256".into(),
            verifier: "xyz".into(),
        };
        let url = build_authorization_url(
            "https://auth.example.com/authorize",
            "client-1",
            "tuic://oauth-callback",
            &["read".into(), "write".into()],
            "state-xyz",
            &pkce,
            Some("https://api.example.com"),
        );
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=client-1"));
        assert!(url.contains("scope=read+write"));
        assert!(url.contains("state=state-xyz"));
        assert!(url.contains("code_challenge=abc"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("resource="));
        assert!(url.contains("redirect_uri=tuic"));
    }

    // -- AS mix-up defence (#1268-40e8) --

    #[test]
    fn check_issuer_allows_same_registrable_domain() {
        assert!(check_issuer_matches_resource(
            "https://api.example.com/mcp",
            "https://auth.example.com",
        )
        .is_ok());
    }

    #[test]
    fn check_issuer_allows_exact_host_match() {
        assert!(check_issuer_matches_resource(
            "https://example.com/mcp",
            "https://example.com",
        )
        .is_ok());
    }

    #[test]
    fn check_issuer_rejects_cross_domain_attacker() {
        let err = check_issuer_matches_resource(
            "https://api.example.com/mcp",
            "https://attacker.example.org",
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("mix-up"), "msg: {msg}");
        assert!(msg.contains("api.example.com"), "msg: {msg}");
        assert!(msg.contains("attacker.example.org"), "msg: {msg}");
    }

    #[test]
    fn check_issuer_allows_loopback_dev() {
        assert!(check_issuer_matches_resource(
            "http://127.0.0.1:8080/mcp",
            "http://localhost:9090",
        )
        .is_ok());
    }

    #[test]
    fn check_issuer_rejects_malformed_server_url() {
        let err = check_issuer_matches_resource("not a url", "https://auth.example.com")
            .unwrap_err();
        assert!(err.to_string().contains("server_url"));
    }

    // -- start_flow --

    #[tokio::test]
    async fn start_flow_returns_url_with_state() {
        let m = mgr();
        let out = m
            .start_flow("test", "https://api.example.com", &oauth2_config(), "tuic://oauth-callback")
            .await
            .unwrap();
        assert!(out.authorization_url.starts_with("https://auth.example.com/authorize?"));
        assert!(out.authorization_url.contains(&format!("state={}", out.state)));
        assert_eq!(m.pending_count(), 1);
    }

    #[tokio::test]
    async fn start_flow_rejects_bearer_auth() {
        let m = mgr();
        let err = m
            .start_flow(
                "test",
                "https://api.example.com",
                &UpstreamAuth::Bearer { token: "x".into() },
                "tuic://cb",
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("OAuth2"));
    }

    #[tokio::test]
    async fn concurrent_flows_are_serialized() {
        // semaphore permits = 1 → second start_flow must block until first
        // releases (cancel or complete).
        let m = Arc::new(mgr());
        let _out1 = m
            .start_flow("a", "https://api.example.com", &oauth2_config(), "tuic://cb")
            .await
            .unwrap();
        // Second call must not resolve while first holds the permit.
        let m2 = m.clone();
        let pending = tokio::spawn(async move {
            m2.start_flow("b", "https://api.example.com", &oauth2_config(), "tuic://cb")
                .await
        });
        // Give the pending task a chance to run.
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(
            !pending.is_finished(),
            "second flow should block until first releases"
        );
        // Release by cancelling the first flow — permit drops.
        let first_state = m.pending.iter().next().unwrap().key().clone();
        assert!(m.cancel_flow(&first_state));
        // Now the second flow should complete.
        let out2 = tokio::time::timeout(Duration::from_secs(2), pending)
            .await
            .expect("second flow did not unblock")
            .unwrap()
            .unwrap();
        assert!(out2.authorization_url.contains("state="));
    }

    // -- complete_flow --

    #[tokio::test]
    async fn complete_flow_rejects_unknown_state() {
        let m = mgr();
        let err = m.complete_flow("bogus-state", "code").await.unwrap_err();
        assert!(
            err.to_string().contains("state mismatch") || err.to_string().contains("expired"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn complete_flow_rejects_expired_flow() {
        let m = OAuthFlowManager::with_timeout(
            Arc::new(Semaphore::new(1)),
            Duration::from_millis(1),
        );
        let out = m
            .start_flow("test", "https://api.example.com", &oauth2_config(), "tuic://cb")
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        let err = m.complete_flow(&out.state, "code").await.unwrap_err();
        assert!(err.to_string().contains("expired"), "got: {err}");
    }

    // -- cancel_flow --

    #[tokio::test]
    async fn cancel_flow_removes_pending_and_releases_permit() {
        let m = mgr();
        let out = m
            .start_flow("test", "https://api.example.com", &oauth2_config(), "tuic://cb")
            .await
            .unwrap();
        assert_eq!(m.pending_count(), 1);
        assert!(m.cancel_flow(&out.state));
        assert_eq!(m.pending_count(), 0);
        // Semaphore is now free — a new flow should start immediately.
        let _out2 = tokio::time::timeout(
            Duration::from_millis(200),
            m.start_flow("test2", "https://api.example.com", &oauth2_config(), "tuic://cb"),
        )
        .await
        .expect("second flow timed out");
    }

    #[tokio::test]
    async fn cancel_flow_unknown_state_returns_false() {
        let m = mgr();
        assert!(!m.cancel_flow("not-there"));
    }

    #[tokio::test]
    async fn cancel_flows_for_upstream() {
        let m = Arc::new(OAuthFlowManager::with_timeout(
            Arc::new(Semaphore::new(10)), // allow multiple concurrent for test
            DEFAULT_FLOW_TIMEOUT,
        ));
        let _a = m
            .start_flow("srv-a", "https://api.example.com", &oauth2_config(), "tuic://cb")
            .await
            .unwrap();
        let _b1 = m
            .start_flow("srv-b", "https://api.example.com", &oauth2_config(), "tuic://cb")
            .await
            .unwrap();
        let _b2 = m
            .start_flow("srv-b", "https://api.example.com", &oauth2_config(), "tuic://cb")
            .await
            .unwrap();
        assert_eq!(m.pending_count(), 3);
        assert_eq!(m.cancel_flows_for("srv-b"), 2);
        assert_eq!(m.pending_count(), 1);
    }

    // -- cleanup_expired --

    #[tokio::test]
    async fn cleanup_expired_removes_stale_flows() {
        let m = OAuthFlowManager::with_timeout(
            Arc::new(Semaphore::new(10)),
            Duration::from_millis(1),
        );
        let _a = m
            .start_flow("test", "https://api.example.com", &oauth2_config(), "tuic://cb")
            .await
            .unwrap();
        assert_eq!(m.pending_count(), 1);
        tokio::time::sleep(Duration::from_millis(20)).await;
        let n = m.cleanup_expired();
        assert_eq!(n, 1);
        assert_eq!(m.pending_count(), 0);
    }

    #[tokio::test]
    async fn cleanup_expired_keeps_fresh_flows() {
        let m = mgr();
        let _a = m
            .start_flow("test", "https://api.example.com", &oauth2_config(), "tuic://cb")
            .await
            .unwrap();
        let n = m.cleanup_expired();
        assert_eq!(n, 0);
        assert_eq!(m.pending_count(), 1);
    }
}

// ---------------------------------------------------------------------------
// Dev-mode callback server (debug builds only)
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
pub(crate) mod dev_server {
    //! Ephemeral localhost HTTP server to receive the OAuth authorization
    //! callback when deep links are unavailable (e.g. `tauri dev`).
    //!
    //! Binds to `127.0.0.1:0` (OS-assigned port), serves a single
    //! `/oauth/callback` endpoint, and forwards the captured `(state, code)`
    //! to the supplied [`OAuthFlowManager`]. The server shuts down after the
    //! first callback.

    use super::*;
    use axum::extract::Query;
    use axum::response::{Html, IntoResponse};
    use axum::routing::get;
    use axum::Router;
    use serde::Deserialize;
    use std::net::SocketAddr;

    #[derive(Debug, Deserialize)]
    struct CallbackParams {
        code: Option<String>,
        state: Option<String>,
        error: Option<String>,
        error_description: Option<String>,
    }

    /// Handle returned by [`spawn`] — drop to stop the server.
    pub(crate) struct DevCallbackServer {
        pub(crate) port: u16,
        /// Triggers graceful shutdown when sent.
        #[allow(dead_code)]
        shutdown_tx: tokio::sync::oneshot::Sender<()>,
        /// Receives the completed [`OAuthTokenSet`] (or error message).
        pub(crate) result_rx: tokio::sync::oneshot::Receiver<Result<(String, OAuthTokenSet)>>,
    }

    /// Spawn the dev callback server. Returns the assigned port and a handle
    /// to receive the exchange result.
    pub(crate) async fn spawn(manager: Arc<OAuthFlowManager>) -> Result<DevCallbackServer> {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let (result_tx, result_rx) = tokio::sync::oneshot::channel::<Result<(String, OAuthTokenSet)>>();
        let result_tx = Arc::new(tokio::sync::Mutex::new(Some(result_tx)));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();

        let mgr_clone = manager.clone();
        let result_tx_clone = result_tx.clone();
        let app = Router::new().route(
            "/oauth/callback",
            get(move |Query(params): Query<CallbackParams>| {
                let mgr = mgr_clone.clone();
                let tx_slot = result_tx_clone.clone();
                async move {
                    let outcome = handle_callback(mgr, params).await;
                    let html = match &outcome {
                        Ok(_) => "<html><body><h1>Authentication complete</h1>\
                            <p>You can close this tab.</p></body></html>",
                        Err(_) => "<html><body><h1>Authentication failed</h1>\
                            <p>Check the TUIC logs for details.</p></body></html>",
                    };
                    // Deliver the result to the caller (first write wins).
                    if let Some(tx) = tx_slot.lock().await.take() {
                        let _ = tx.send(outcome);
                    }
                    Html(html).into_response()
                }
            }),
        );

        tokio::spawn(async move {
            let server = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            });
            if let Err(e) = server.await {
                tracing::warn!(target: "mcp_oauth", error = %e, "dev callback server exited");
            }
        });

        Ok(DevCallbackServer {
            port,
            shutdown_tx,
            result_rx,
        })
    }

    async fn handle_callback(
        manager: Arc<OAuthFlowManager>,
        params: CallbackParams,
    ) -> Result<(String, OAuthTokenSet)> {
        if let Some(err) = params.error {
            bail!(
                "Authorization server returned error: {} ({})",
                err,
                params.error_description.unwrap_or_default()
            );
        }
        let code = params
            .code
            .ok_or_else(|| anyhow!("Missing 'code' parameter in callback"))?;
        let state = params
            .state
            .ok_or_else(|| anyhow!("Missing 'state' parameter in callback"))?;
        manager.complete_flow(&state, &code).await
    }
}
