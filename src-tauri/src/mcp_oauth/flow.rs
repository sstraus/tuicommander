//! OAuth flow orchestrator.
//!
//! Coordinates the full OAuth 2.1 Authorization Code + PKCE flow for upstream
//! MCP servers:
//!
//! 1. [`OAuthFlowManager::start_flow`]: resolves authorization/token endpoints
//!    (discovery or config override), generates a PKCE S256 challenge and a
//!    cryptographically random `state` nonce, inserts a [`PendingFlow`], and
//!    returns the authorization URL for the caller to open in the browser.
//!
//!    Flows are **not** serialized. Everything a flow owns — PKCE verifier,
//!    `state` nonce, callback server port, `pending` entry — is per-flow, so
//!    concurrent flows share nothing. An earlier design held a single-permit
//!    semaphore across the whole browser round-trip; a second Authorize click
//!    then blocked inside `start_flow` for up to five minutes with no browser,
//!    no dialog and no error, and `cancel_flows_for` could not reach it because
//!    the queued flow was not yet in `pending`.
//! 2. [`OAuthFlowManager::complete_flow`]: called from the localhost callback
//!    server (see [`super::callback_server`]). Looks up the pending flow by
//!    keyed `state`, calls [`TokenManager::exchange_code`], and returns the
//!    resulting [`OAuthTokenSet`].
//! 3. [`OAuthFlowManager::cancel_flow`]: removes a pending flow (e.g. on user
//!    cancellation or upstream transition to `Failed`).
//!
//! A background task started via [`OAuthFlowManager::spawn_cleanup_task`]
//! periodically removes expired pending flows (default 5 minutes).

use anyhow::{Result, anyhow, bail};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    upstream_url: String,
    /// Full `state` nonce (also the DashMap key for this entry).
    state: String,
    /// PKCE verifier used when exchanging the code.
    pkce_verifier: String,
    /// Redirect URI used in the authorization request (localhost callback server).
    redirect_uri: String,
    /// Resolved token endpoint.
    token_endpoint: String,
    /// OAuth client ID.
    client_id: String,
    /// OAuth client secret (confidential clients only).
    client_secret: Option<String>,
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
    /// `true` when the discovered authorization server lives on a different
    /// registrable domain than the MCP resource. Not an error — gateways make
    /// this normal — but the consent dialog calls it out explicitly.
    pub(crate) cross_domain_as: bool,
}

// ---------------------------------------------------------------------------
// OAuthFlowManager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of in-flight OAuth flows.
///
/// Clone-safe via internal `Arc`s (`DashMap`); callers should wrap
/// `OAuthFlowManager` itself in `Arc` for storage in `AppState`.
pub(crate) struct OAuthFlowManager {
    /// state nonce → pending flow.
    pending: Arc<DashMap<String, PendingFlow>>,
    /// HTTP client used for discovery and token exchange.
    http_client: reqwest::Client,
    /// Timeout after which a flow is considered abandoned.
    timeout: Duration,
}

impl OAuthFlowManager {
    pub(crate) fn new() -> Self {
        Self::with_timeout(DEFAULT_FLOW_TIMEOUT)
    }

    pub(crate) fn with_timeout(timeout: Duration) -> Self {
        Self {
            pending: Arc::new(DashMap::new()),
            http_client: reqwest::Client::new(),
            timeout,
        }
    }

    /// How long a pending flow stays valid — the callback server must outlive
    /// this, otherwise a late redirect hits a closed port.
    pub(crate) fn flow_timeout(&self) -> Duration {
        self.timeout
    }

    /// Number of flows currently pending (test/diagnostics helper).
    #[cfg(test)]
    pub(crate) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Look up the upstream name for a given state nonce without consuming the flow.
    pub(crate) fn upstream_name_for_state(&self, state: &str) -> Option<String> {
        self.pending.get(state).map(|e| e.upstream_name.clone())
    }

    /// Start a new OAuth flow.
    ///
    /// Resolves endpoints via RFC 9728/8414 discovery when not pre-configured,
    /// generates PKCE + state nonce, and returns the authorization URL. Never
    /// waits on another flow — see the module docs.
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
        let (client_id, client_secret, scopes, authz_override, token_override) = match auth {
            UpstreamAuth::OAuth2 {
                client_id,
                client_secret,
                scopes,
                authorization_endpoint,
                token_endpoint,
            } => (
                client_id.clone(),
                client_secret.clone(),
                scopes.clone(),
                authorization_endpoint.clone(),
                token_endpoint.clone(),
            ),
            UpstreamAuth::Bearer { .. } => {
                bail!("OAuth flow requires OAuth2 auth config, got Bearer")
            }
        };

        // Resolve endpoints (and scopes when not pre-configured).
        let (authorization_endpoint, token_endpoint, discovered_scopes, cross_domain_as) =
            match (authz_override.clone(), token_override.clone()) {
                // Explicit overrides bypass discovery — the user vetted these.
                (Some(a), Some(t)) => (a, t, vec![], false),
                _ => self.resolve_discovery(server_url).await?,
            };

        // Use the override as final if provided (fill missing from discovery).
        let authorization_endpoint = authz_override.unwrap_or(authorization_endpoint);
        let token_endpoint = token_override.unwrap_or(token_endpoint);

        // Auto-detect scopes from discovery when not explicitly configured.
        let scopes = if scopes.is_empty() {
            discovered_scopes
        } else {
            scopes
        };
        // Always request a refresh token (see `with_offline_access`).
        let scopes = with_offline_access(scopes);

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

        // Store pending flow.
        let flow = PendingFlow {
            upstream_name: upstream_name.to_string(),
            upstream_url: server_url.to_string(),
            state: state.clone(),
            pkce_verifier: pkce.verifier.clone(),
            redirect_uri: redirect_uri.to_string(),
            token_endpoint,
            client_id,
            client_secret,
            resource: Some(server_url.to_string()),
            created_at: Instant::now(),
        };
        self.pending.insert(state.clone(), flow);

        Ok(StartFlowOutcome {
            authorization_url,
            state,
            upstream_name: upstream_name.to_string(),
            cross_domain_as,
        })
    }

    /// Complete a pending flow with the authorization `code` received on the
    /// callback. On success, the access/refresh tokens are saved to the
    /// keyring and returned.
    ///
    /// Threat model: the `state` parameter arrives via a localhost-only HTTP
    /// callback server (127.0.0.1, OS-assigned port). There is no remote
    /// attacker who can probe the `pending` map with timing oracles.
    pub(crate) async fn complete_flow(
        &self,
        state: &str,
        code: &str,
    ) -> Result<(String, OAuthTokenSet)> {
        let (_removed_key, flow) = self
            .pending
            .remove(state)
            .ok_or_else(|| anyhow!("OAuth state mismatch or flow expired"))?;

        // Check timeout.
        if flow.created_at.elapsed() > self.timeout {
            bail!("OAuth flow expired ({} s)", self.timeout.as_secs());
        }

        // Exchange code for tokens.
        let token_mgr = TokenManager::new(
            flow.upstream_name.clone(),
            flow.upstream_url.clone(),
            flow.client_id.clone(),
            flow.client_secret.clone(),
            flow.token_endpoint.clone(),
            flow.resource.clone(),
        );
        let tokens = token_mgr
            .exchange_code(code, &flow.pkce_verifier, &flow.redirect_uri)
            .await?;
        Ok((flow.upstream_name, tokens))
    }

    /// Cancel a pending flow. Returns true if a flow with the given state was
    /// found and removed.
    pub(crate) fn cancel_flow(&self, state: &str) -> bool {
        self.pending.remove(state).is_some()
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

    /// Remove expired pending flows. Returns the upstream name of each one, so
    /// the caller can transition it out of `Authenticating` — without that the
    /// upstream sits on "Awaiting authorization…" forever, with no path back to
    /// a retryable state.
    pub(crate) fn cleanup_expired(&self) -> Vec<String> {
        let timeout = self.timeout;
        let expired: Vec<(String, String)> = self
            .pending
            .iter()
            .filter(|e| e.value().created_at.elapsed() > timeout)
            .map(|e| (e.key().clone(), e.value().upstream_name.clone()))
            .collect();
        expired
            .into_iter()
            .filter(|(state, _)| self.cancel_flow(state))
            .map(|(_, name)| name)
            .collect()
    }

    /// Spawn a background task that periodically removes expired pending flows
    /// and returns each one's upstream to `NeedsAuth`. The task runs for the
    /// lifetime of the returned `Arc`; drop all outer references to stop it.
    /// Wired in by `spawn_background_tasks`.
    ///
    /// `registry` is taken as a parameter rather than stored on the manager:
    /// the registry already holds a `Weak` back to this manager, and a strong
    /// field here would close the cycle.
    pub(crate) fn spawn_cleanup_task(
        self: &Arc<Self>,
        registry: Arc<crate::mcp_proxy::registry::UpstreamRegistry>,
    ) -> tokio::task::JoinHandle<()> {
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(CLEANUP_INTERVAL);
            ticker.tick().await; // skip immediate tick
            loop {
                ticker.tick().await;
                let Some(strong) = weak.upgrade() else { break };
                let expired = strong.cleanup_expired();
                if !expired.is_empty() {
                    tracing::info!(
                        target: "mcp_oauth",
                        cleaned = expired.len(),
                        upstreams = %expired.join(", "),
                        "Cleaned up expired OAuth flows"
                    );
                    for name in expired {
                        registry.rollback_authenticating(&name);
                    }
                }
            }
        })
    }

    /// Resolve authorization endpoint, token endpoint, and scopes via RFC 9728 + 8414 discovery.
    /// Returns `(authorization_endpoint, token_endpoint, scopes, cross_domain_as)`.
    async fn resolve_discovery(
        &self,
        server_url: &str,
    ) -> Result<(String, String, Vec<String>, bool)> {
        let pr_meta = discover_protected_resource(&self.http_client, server_url).await?;
        let issuer = pr_meta
            .authorization_servers
            .first()
            .ok_or_else(|| anyhow!("Protected resource returned no authorization servers"))?;
        let as_meta = discover_auth_server(&self.http_client, issuer).await?;
        // The AS may legitimately live off-domain (gateways, IdP tenants). Flag
        // it for the consent dialog instead of refusing to continue; judge the
        // endpoint we will actually send the user to, not the advertised issuer.
        let cross_domain_as = is_cross_domain_as(server_url, &as_meta.authorization_endpoint);
        let scopes = resolve_scopes(&pr_meta, &as_meta);
        Ok((
            as_meta.authorization_endpoint,
            as_meta.token_endpoint,
            scopes,
            cross_domain_as,
        ))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Ensure the requested scope set includes `offline_access`.
///
/// Most OIDC/OAuth2 authorization servers only issue a refresh token when
/// `offline_access` is part of the authorization request. Without it the
/// upstream's access token expires (often within ~48h) and the connection
/// silently drops to `NeedsAuth` — `refresh_if_needed` then fails with
/// "no refresh_token available", forcing the user to re-authorize by hand.
/// Idempotent: a scope set that already contains `offline_access` is unchanged.
fn with_offline_access(mut scopes: Vec<String>) -> Vec<String> {
    if !scopes.iter().any(|s| s == "offline_access") {
        scopes.push("offline_access".to_string());
    }
    scopes
}

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

/// Report whether the authorization server we are about to send the user to
/// sits on a different registrable domain than the MCP resource.
///
/// This used to be a hard block (RFC 9700 §4.6 mix-up defence). It isn't one
/// any more: routing OAuth through a different domain is the *normal* topology
/// for MCP gateways (`mcp-s.com`), corporate proxies, and hosted IdP tenants
/// (`*.auth0.com`, `*.okta.com`) — blocking it made perfectly legitimate
/// servers unusable with no way through. The mix-up threat is instead handled
/// where the user can act on it: the consent dialog names the AS origin, and
/// this flag makes it say so loudly when the domain differs.
///
/// Only `authorization_endpoint` is compared — that is the URL the browser
/// actually opens, so it is the one the user can recognise or reject. The
/// advertised `issuer` is deliberately ignored: gateways name the upstream IdP
/// there as a matter of course, and warning on it fires on the common case
/// without telling the user anything they can act on. Loopback is exempt — dev
/// setups routinely split AS and resource across localhost ports.
fn is_cross_domain_as(server_url: &str, authorization_endpoint: &str) -> bool {
    let Some(server_domain) = registrable_domain(server_url) else {
        // Can't prove same-origin — err toward warning the user.
        return true;
    };
    if is_loopback_domain(&server_domain) {
        return false;
    }
    match registrable_domain(authorization_endpoint) {
        Some(as_domain) => !is_loopback_domain(&as_domain) && as_domain != server_domain,
        None => true,
    }
}

fn is_loopback_domain(domain: &str) -> bool {
    matches!(domain, "localhost" | "127.0.0.1")
}

/// Resolve OAuth scopes from discovery metadata.
///
/// Priority: PR metadata `scopes_supported` → AS metadata `scopes_supported` → empty.
/// An empty `scopes_supported` list in PR metadata is treated as absent (falls through).
fn resolve_scopes(
    pr_meta: &crate::mcp_oauth::discovery::ProtectedResourceMetadata,
    as_meta: &crate::mcp_oauth::discovery::AuthServerMetadata,
) -> Vec<String> {
    if let Some(scopes) = &pr_meta.scopes_supported
        && !scopes.is_empty()
    {
        return scopes.clone();
    }
    if let Some(scopes) = &as_meta.scopes_supported
        && !scopes.is_empty()
    {
        return scopes.clone();
    }
    vec![]
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

    #[test]
    fn with_offline_access_appends_when_absent() {
        assert_eq!(
            with_offline_access(vec!["read".into(), "write".into()]),
            vec![
                "read".to_string(),
                "write".to_string(),
                "offline_access".to_string()
            ]
        );
    }

    #[test]
    fn with_offline_access_is_idempotent() {
        let out = with_offline_access(vec!["offline_access".into(), "read".into()]);
        assert_eq!(
            out.iter().filter(|s| *s == "offline_access").count(),
            1,
            "must not duplicate offline_access"
        );
    }

    #[test]
    fn with_offline_access_adds_to_empty_scope_set() {
        assert_eq!(
            with_offline_access(vec![]),
            vec!["offline_access".to_string()]
        );
    }

    fn oauth2_config() -> UpstreamAuth {
        UpstreamAuth::OAuth2 {
            client_id: "test-client".into(),
            client_secret: None,
            scopes: vec!["read".into(), "write".into()],
            authorization_endpoint: Some("https://auth.example.com/authorize".into()),
            token_endpoint: Some("https://auth.example.com/token".into()),
        }
    }

    fn mgr() -> OAuthFlowManager {
        OAuthFlowManager::new()
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
            "http://127.0.0.1:9999/oauth/callback",
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
        assert!(url.contains("redirect_uri=http"));
    }

    // -- cross-domain AS flag (advisory, never blocking) --

    #[test]
    fn cross_domain_false_for_sibling_subdomain() {
        assert!(!is_cross_domain_as(
            "https://api.example.com/mcp",
            "https://auth.example.com/authorize",
        ));
    }

    #[test]
    fn cross_domain_false_for_exact_host_match() {
        assert!(!is_cross_domain_as(
            "https://example.com/mcp",
            "https://example.com/authorize",
        ));
    }

    #[test]
    fn cross_domain_true_for_unrelated_domain() {
        assert!(is_cross_domain_as(
            "https://api.example.com/mcp",
            "https://attacker.example.org/authorize",
        ));
    }

    /// Gateway shape: the resource proxies AS metadata pointing at the upstream
    /// IdP, so the browser lands off-domain. Flagged, never blocked.
    #[test]
    fn cross_domain_true_when_gateway_sends_user_to_upstream_idp() {
        assert!(is_cross_domain_as(
            "https://tenant.gateway.example/mcp/mcp/calendar",
            "https://auth.example-idp.com/authorize",
        ));
    }

    /// A gateway that also proxies the authorize endpoint keeps the browser on
    /// its own domain — no notice, even though its metadata advertises the
    /// upstream IdP as `issuer`. The issuer deliberately has no say here.
    #[test]
    fn cross_domain_false_when_gateway_proxies_the_authorize_endpoint() {
        assert!(!is_cross_domain_as(
            "https://tenant.gateway.example/mcp/mcp/calendar",
            "https://tenant.gateway.example/oauth/authorize",
        ));
    }

    #[test]
    fn cross_domain_false_for_loopback_dev() {
        assert!(!is_cross_domain_as(
            "http://127.0.0.1:8080/mcp",
            "http://localhost:9090/authorize",
        ));
    }

    #[test]
    fn cross_domain_true_for_malformed_server_url() {
        assert!(is_cross_domain_as(
            "not a url",
            "https://auth.example.com/authorize",
        ));
    }

    // -- resolve_scopes --

    use crate::mcp_oauth::discovery::{AuthServerMetadata, ProtectedResourceMetadata};

    fn pr_meta_with_scopes(scopes: &[&str]) -> ProtectedResourceMetadata {
        ProtectedResourceMetadata {
            resource: "https://api.example.com".into(),
            authorization_servers: vec!["https://auth.example.com".into()],
            scopes_supported: Some(scopes.iter().map(|s| s.to_string()).collect()),
        }
    }

    fn pr_meta_no_scopes() -> ProtectedResourceMetadata {
        ProtectedResourceMetadata {
            resource: "https://api.example.com".into(),
            authorization_servers: vec!["https://auth.example.com".into()],
            scopes_supported: None,
        }
    }

    fn as_meta_with_scopes(scopes: &[&str]) -> AuthServerMetadata {
        AuthServerMetadata {
            issuer: "https://auth.example.com".into(),
            authorization_endpoint: "https://auth.example.com/authorize".into(),
            token_endpoint: "https://auth.example.com/token".into(),
            registration_endpoint: None,
            scopes_supported: Some(scopes.iter().map(|s| s.to_string()).collect()),
            code_challenge_methods_supported: None,
        }
    }

    fn as_meta_no_scopes() -> AuthServerMetadata {
        AuthServerMetadata {
            issuer: "https://auth.example.com".into(),
            authorization_endpoint: "https://auth.example.com/authorize".into(),
            token_endpoint: "https://auth.example.com/token".into(),
            registration_endpoint: None,
            scopes_supported: None,
            code_challenge_methods_supported: None,
        }
    }

    #[test]
    fn resolve_scopes_prefers_pr_metadata() {
        let pr = pr_meta_with_scopes(&["pr-scope-a", "pr-scope-b"]);
        let as_ = as_meta_with_scopes(&["as-scope"]);
        let scopes = resolve_scopes(&pr, &as_);
        assert_eq!(scopes, vec!["pr-scope-a", "pr-scope-b"]);
    }

    #[test]
    fn resolve_scopes_falls_back_to_as_when_pr_has_none() {
        let pr = pr_meta_no_scopes();
        let as_ = as_meta_with_scopes(&["as-read", "as-write"]);
        let scopes = resolve_scopes(&pr, &as_);
        assert_eq!(scopes, vec!["as-read", "as-write"]);
    }

    #[test]
    fn resolve_scopes_returns_empty_when_both_missing() {
        let pr = pr_meta_no_scopes();
        let as_ = as_meta_no_scopes();
        let scopes = resolve_scopes(&pr, &as_);
        assert!(scopes.is_empty());
    }

    #[test]
    fn resolve_scopes_falls_back_to_as_when_pr_scopes_empty_vec() {
        let pr = ProtectedResourceMetadata {
            resource: "https://api.example.com".into(),
            authorization_servers: vec!["https://auth.example.com".into()],
            scopes_supported: Some(vec![]),
        };
        let as_ = as_meta_with_scopes(&["fallback-scope"]);
        let scopes = resolve_scopes(&pr, &as_);
        assert_eq!(scopes, vec!["fallback-scope"]);
    }

    // -- start_flow --

    #[tokio::test]
    async fn start_flow_returns_url_with_state() {
        let m = mgr();
        let out = m
            .start_flow(
                "test",
                "https://api.example.com",
                &oauth2_config(),
                "http://127.0.0.1:9999/oauth/callback",
            )
            .await
            .unwrap();
        assert!(
            out.authorization_url
                .starts_with("https://auth.example.com/authorize?")
        );
        assert!(
            out.authorization_url
                .contains(&format!("state={}", out.state))
        );
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
                "http://127.0.0.1:9999/oauth/callback",
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("OAuth2"));
    }

    /// Replaces `concurrent_flows_are_serialized`, which asserted the very
    /// behaviour that broke authorization: a single-permit semaphore held across
    /// the whole browser round-trip. A second Authorize click blocked inside
    /// `start_flow` for the full 5-minute flow timeout — no browser, no dialog,
    /// no error — and because the queued flow never reached `pending`, Cancel
    /// could not release it either. Flows share no state, so they must not wait
    /// on each other.
    #[tokio::test]
    async fn concurrent_flows_do_not_block_each_other() {
        let m = Arc::new(mgr());
        let _out1 = m
            .start_flow(
                "a",
                "https://api.example.com",
                &oauth2_config(),
                "http://127.0.0.1:9999/oauth/callback",
            )
            .await
            .unwrap();

        // A second flow for a different upstream must resolve immediately while
        // the first is still pending — not after it completes or expires.
        let out2 = tokio::time::timeout(
            Duration::from_millis(500),
            m.start_flow(
                "b",
                "https://api.example.com",
                &oauth2_config(),
                "http://127.0.0.1:9999/oauth/callback",
            ),
        )
        .await
        .expect("second flow blocked behind the first")
        .unwrap();

        assert!(out2.authorization_url.contains("state="));
        assert_eq!(m.pending_count(), 2, "both flows must be pending");
    }

    /// The queued-flow bug's second half: a flow that has started is always in
    /// `pending`, so `cancel_flows_for` can actually reach it. Under the old
    /// semaphore a blocked flow was invisible to cancel, which is why "Cancel
    /// then Authorize again" did nothing but lengthen the queue.
    #[tokio::test]
    async fn a_started_flow_is_immediately_cancellable() {
        let m = mgr();
        let _out = m
            .start_flow(
                "outlook-mail",
                "https://api.example.com",
                &oauth2_config(),
                "http://127.0.0.1:9999/oauth/callback",
            )
            .await
            .unwrap();
        assert_eq!(m.cancel_flows_for("outlook-mail"), 1);
        assert_eq!(m.pending_count(), 0);
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
        let m = OAuthFlowManager::with_timeout(Duration::from_millis(1));
        let out = m
            .start_flow(
                "test",
                "https://api.example.com",
                &oauth2_config(),
                "http://127.0.0.1:9999/oauth/callback",
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        let err = m.complete_flow(&out.state, "code").await.unwrap_err();
        assert!(err.to_string().contains("expired"), "got: {err}");
    }

    // -- cancel_flow --

    #[tokio::test]
    async fn cancel_flow_removes_pending() {
        let m = mgr();
        let out = m
            .start_flow(
                "test",
                "https://api.example.com",
                &oauth2_config(),
                "http://127.0.0.1:9999/oauth/callback",
            )
            .await
            .unwrap();
        assert_eq!(m.pending_count(), 1);
        assert!(m.cancel_flow(&out.state));
        assert_eq!(m.pending_count(), 0);
    }

    #[tokio::test]
    async fn cancel_flow_unknown_state_returns_false() {
        let m = mgr();
        assert!(!m.cancel_flow("not-there"));
    }

    #[tokio::test]
    async fn cancel_flows_for_upstream() {
        let m = Arc::new(OAuthFlowManager::with_timeout(DEFAULT_FLOW_TIMEOUT));
        let _a = m
            .start_flow(
                "srv-a",
                "https://api.example.com",
                &oauth2_config(),
                "http://127.0.0.1:9999/oauth/callback",
            )
            .await
            .unwrap();
        let _b1 = m
            .start_flow(
                "srv-b",
                "https://api.example.com",
                &oauth2_config(),
                "http://127.0.0.1:9999/oauth/callback",
            )
            .await
            .unwrap();
        let _b2 = m
            .start_flow(
                "srv-b",
                "https://api.example.com",
                &oauth2_config(),
                "http://127.0.0.1:9999/oauth/callback",
            )
            .await
            .unwrap();
        assert_eq!(m.pending_count(), 3);
        assert_eq!(m.cancel_flows_for("srv-b"), 2);
        assert_eq!(m.pending_count(), 1);
    }

    // -- cleanup_expired --

    /// `cleanup_expired` must name the upstreams it dropped: the sweep is the
    /// only thing that can take an abandoned flow's upstream out of
    /// `Authenticating`, and it can only do that if it knows which one it was.
    /// Returning a bare count left the UI on "Awaiting authorization…" forever.
    #[tokio::test]
    async fn cleanup_expired_returns_the_upstreams_it_dropped() {
        let m = OAuthFlowManager::with_timeout(Duration::from_millis(1));
        for name in ["outlook-mail", "spinach"] {
            m.start_flow(
                name,
                "https://api.example.com",
                &oauth2_config(),
                "http://127.0.0.1:9999/oauth/callback",
            )
            .await
            .unwrap();
        }
        assert_eq!(m.pending_count(), 2);
        tokio::time::sleep(Duration::from_millis(20)).await;

        let mut expired = m.cleanup_expired();
        expired.sort();
        assert_eq!(expired, vec!["outlook-mail", "spinach"]);
        assert_eq!(m.pending_count(), 0);
    }

    #[tokio::test]
    async fn cleanup_expired_keeps_fresh_flows() {
        let m = mgr();
        let _a = m
            .start_flow(
                "test",
                "https://api.example.com",
                &oauth2_config(),
                "http://127.0.0.1:9999/oauth/callback",
            )
            .await
            .unwrap();
        assert!(m.cleanup_expired().is_empty());
        assert_eq!(m.pending_count(), 1);
    }
}
