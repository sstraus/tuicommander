//! OAuth 2.1 token exchange and refresh with PKCE and RFC 8707.
//!
//! [`TokenManager`] handles:
//! - PKCE S256 code challenge generation
//! - Authorization code → token exchange
//! - Transparent token refresh with thundering-herd protection
//! - RFC 8707 `resource` parameter on exchange and refresh
//! - Immediate keyring persistence after every token operation

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::mcp_upstream_credentials::{OAuthTokenSet, is_token_valid, save_oauth_tokens};

// ---------------------------------------------------------------------------
// TokenManager
// ---------------------------------------------------------------------------

/// Manages OAuth token lifecycle for a single upstream MCP server.
///
/// Thread-safe: the inner `Mutex` serializes refresh operations so that
/// concurrent callers don't trigger parallel refresh requests (thundering herd).
pub(crate) struct TokenManager {
    /// Upstream name (used as keyring key).
    upstream_name: String,
    upstream_url: String,
    /// OAuth client ID.
    client_id: String,
    /// OAuth client secret (confidential clients only).
    client_secret: Option<String>,
    /// Token endpoint URL.
    token_endpoint: String,
    /// RFC 8707 resource indicator (the upstream's URL).
    resource: Option<String>,
    /// Guards concurrent refresh — only one refresh executes at a time.
    refresh_lock: Arc<Mutex<()>>,
}

/// Result of generating a PKCE challenge for the authorization request.
pub(crate) struct PkceChallengePair {
    /// The challenge string to include in the authorization URL.
    pub(crate) challenge: String,
    /// The challenge method (always "S256").
    pub(crate) method: String,
    /// The verifier to use during code exchange (keep secret, don't send to AS).
    pub(crate) verifier: String,
}

fn pkce_s256(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

impl TokenManager {
    pub(crate) fn new(
        upstream_name: String,
        upstream_url: String,
        client_id: String,
        client_secret: Option<String>,
        token_endpoint: String,
        resource: Option<String>,
    ) -> Self {
        Self {
            upstream_name,
            upstream_url,
            client_id,
            client_secret,
            token_endpoint,
            resource,
            refresh_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Generate a new PKCE S256 challenge pair for an authorization request.
    pub(crate) fn generate_pkce() -> PkceChallengePair {
        let verifier = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
        PkceChallengePair {
            challenge: pkce_s256(&verifier),
            method: "S256".to_string(),
            verifier,
        }
    }

    /// Exchange an authorization code for tokens.
    ///
    /// Includes the PKCE `code_verifier`, `redirect_uri`, and optional
    /// RFC 8707 `resource` parameter.
    pub(crate) async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
        redirect_uri: &str,
    ) -> Result<OAuthTokenSet> {
        let http_client = reqwest::Client::new();
        let mut params = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", &self.client_id),
            ("redirect_uri", redirect_uri),
            ("code_verifier", code_verifier),
        ];

        if let Some(ref secret) = self.client_secret {
            params.push(("client_secret", secret));
        }

        let resource_val;
        if let Some(ref res) = self.resource {
            resource_val = res.clone();
            params.push(("resource", &resource_val));
        }

        let resp = http_client
            .post(&self.token_endpoint)
            .form(&params)
            .send()
            .await
            .context("Token exchange request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("Token exchange failed (HTTP {status}): {body}");
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .context("Failed to parse token exchange response")?;

        let token_set = self.token_response_to_set(token_resp);
        save_oauth_tokens(&self.upstream_name, &token_set, &self.upstream_url)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(token_set)
    }

    /// Refresh the access token if it's expired or about to expire.
    ///
    /// Uses a double-check pattern with a mutex to prevent thundering herd:
    /// 1. Check if token is still valid (no lock).
    /// 2. If expired, acquire the refresh lock.
    /// 3. Re-check under lock (another caller may have refreshed already).
    /// 4. If still expired, perform the refresh request.
    ///
    /// "Another caller refreshed" means the stored access token is no longer the
    /// one `current` holds — not merely that its `expires_at` reads valid. The
    /// distinction is the whole point: a server can revoke a token years before
    /// its stated expiry (and some issuers state a nonsense expiry to begin
    /// with), so a caller that just ate a 401 arrives here with a token the
    /// keyring still believes in. Trusting `expires_at` alone handed that exact
    /// dead token straight back and made 401 recovery a no-op.
    pub(crate) async fn refresh_if_needed(
        &self,
        current: &OAuthTokenSet,
    ) -> Result<Option<OAuthTokenSet>> {
        // Fast path: token is still valid
        if is_token_valid(current) {
            return Ok(None);
        }

        // Acquire refresh lock — serializes concurrent refresh attempts
        let _guard = self.refresh_lock.lock().await;

        // Double-check: re-read from keyring in case another caller refreshed
        if let Ok(Some(cred)) = crate::mcp_upstream_credentials::read_stored_credential_for_url(
            &self.upstream_name,
            &self.upstream_url,
        ) && let crate::mcp_upstream_credentials::StoredCredential::Oauth2(ref fresh) = cred
            && fresh.access_token != current.access_token
            && is_token_valid(fresh)
        {
            return Ok(Some(fresh.clone()));
        }

        let refresh_token = current
            .refresh_token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Token expired and no refresh_token available"))?;

        // Still expired — perform refresh
        let http_client = reqwest::Client::new();
        let mut params = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
            ("client_id", self.client_id.as_str()),
        ];

        if let Some(ref secret) = self.client_secret {
            params.push(("client_secret", secret.as_str()));
        }

        let resource_val;
        if let Some(ref res) = self.resource {
            resource_val = res.clone();
            params.push(("resource", &resource_val));
        }

        let resp = http_client
            .post(&self.token_endpoint)
            .form(&params)
            .send()
            .await
            .context("Token refresh request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("Token refresh failed (HTTP {status}): {body}");
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .context("Failed to parse token refresh response")?;

        // Preserve the old refresh_token if the AS didn't issue a new one
        let mut token_set = self.token_response_to_set(token_resp);
        if token_set.refresh_token.is_none() {
            token_set.refresh_token = current.refresh_token.clone();
        }

        save_oauth_tokens(&self.upstream_name, &token_set, &self.upstream_url)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Some(token_set))
    }

    /// Recover after the server rejected a specific access token.
    ///
    /// `stored` is the latest credential available when recovery begins, while
    /// `rejected_access_token` is the bearer that was actually sent. Keeping
    /// those generations distinct lets the double-check under `refresh_lock`
    /// reuse a valid credential written between the request and its 401
    /// response instead of immediately refreshing that replacement.
    pub(crate) async fn refresh_after_rejection(
        &self,
        stored: &OAuthTokenSet,
        rejected_access_token: &str,
    ) -> Result<Option<OAuthTokenSet>> {
        let mut rejected = stored.clone();
        rejected.access_token = rejected_access_token.to_string();
        rejected.expires_at = Some(0);
        self.refresh_if_needed(&rejected).await
    }

    /// Convert a raw token endpoint response into our internal type.
    fn token_response_to_set(&self, resp: TokenResponse) -> OAuthTokenSet {
        let expires_at = resp.expires_in.map(|secs| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64
                + secs as i64
        });

        OAuthTokenSet {
            access_token: resp.access_token,
            refresh_token: resp.refresh_token,
            expires_at,
            token_endpoint: self.token_endpoint.clone(),
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            scope: resp.scope,
            resource: self.resource.clone(),
        }
    }
}

/// Raw token endpoint response (RFC 6749 §5.1).
#[derive(Debug, serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    scope: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    token_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Catches wrong S256 encoding and generating a challenge for another verifier.
    #[test]
    fn pkce_s256_matches_rfc7636_appendix_b() {
        assert_eq!(
            pkce_s256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );

        use base64ct::{Base64UrlUnpadded, Encoding as _};

        let pair = TokenManager::generate_pkce();
        assert_eq!(pair.method, "S256");
        assert!((43..=128).contains(&pair.verifier.len()));
        assert!(
            pair.verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
        );
        // Use independent primitives, not pkce_s256 or its SHA/base64 helpers.
        let digest = ring::digest::digest(&ring::digest::SHA256, pair.verifier.as_bytes());
        assert_eq!(
            pair.challenge,
            Base64UrlUnpadded::encode_string(digest.as_ref())
        );
    }

    #[test]
    fn pkce_generates_s256() {
        let pair = TokenManager::generate_pkce();
        assert_eq!(pair.method, "S256");
        assert!(!pair.challenge.is_empty());
        assert!(!pair.verifier.is_empty());
        // Challenge and verifier must differ
        assert_ne!(pair.challenge, pair.verifier);
    }

    #[test]
    fn pkce_unique_each_call() {
        let a = TokenManager::generate_pkce();
        let b = TokenManager::generate_pkce();
        assert_ne!(a.verifier, b.verifier);
        assert_ne!(a.challenge, b.challenge);
    }

    #[test]
    fn token_response_to_set_with_all_fields() {
        let mgr = TokenManager::new(
            "test".into(),
            "https://api.example.com/mcp".into(),
            "client-1".into(),
            None,
            "https://auth.example.com/token".into(),
            Some("https://api.example.com".into()),
        );
        let resp = TokenResponse {
            access_token: "at-123".into(),
            refresh_token: Some("rt-456".into()),
            expires_in: Some(3600),
            scope: Some("read write".into()),
            token_type: Some("Bearer".into()),
        };
        let set = mgr.token_response_to_set(resp);
        assert_eq!(set.access_token, "at-123");
        assert_eq!(set.refresh_token, Some("rt-456".into()));
        assert!(set.expires_at.is_some());
        assert_eq!(set.client_id, "client-1");
        assert_eq!(set.token_endpoint, "https://auth.example.com/token");
        assert_eq!(set.scope, Some("read write".into()));
    }

    #[test]
    fn token_response_to_set_without_optionals() {
        let mgr = TokenManager::new(
            "test".into(),
            "https://api.example.com/mcp".into(),
            "client-1".into(),
            None,
            "https://auth.example.com/token".into(),
            None,
        );
        let resp = TokenResponse {
            access_token: "at-789".into(),
            refresh_token: None,
            expires_in: None,
            scope: None,
            token_type: None,
        };
        let set = mgr.token_response_to_set(resp);
        assert_eq!(set.access_token, "at-789");
        assert!(set.refresh_token.is_none());
        assert!(set.expires_at.is_none());
    }

    #[tokio::test]
    async fn exchange_code_error_response() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/token")
            .with_status(400)
            .with_body(r#"{"error":"invalid_grant"}"#)
            .create_async()
            .await;

        let mgr = TokenManager::new(
            "test-exchange".into(),
            "https://api.example.com/mcp".into(),
            "client".into(),
            None,
            format!("{}/token", server.url()),
            None,
        );
        let err = mgr
            .exchange_code("bad-code", "verifier", "http://localhost/callback")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("400"), "got: {err}");
    }

    #[tokio::test]
    async fn exchange_code_happy_path() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/token")
            .match_body(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("grant_type".into(), "authorization_code".into()),
                mockito::Matcher::UrlEncoded("code".into(), "auth-code-123".into()),
                mockito::Matcher::UrlEncoded("client_id".into(), "my-client".into()),
                mockito::Matcher::UrlEncoded("code_verifier".into(), "pkce-verifier".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "access_token": "new-at",
                    "refresh_token": "new-rt",
                    "expires_in": 3600,
                    "token_type": "Bearer",
                    "scope": "read"
                })
                .to_string(),
            )
            .create_async()
            .await;

        let mgr = TokenManager::new(
            "test-exchange-happy".into(),
            "https://api.example.com/mcp".into(),
            "my-client".into(),
            None,
            format!("{}/token", server.url()),
            None,
        );
        let set = mgr
            .exchange_code(
                "auth-code-123",
                "pkce-verifier",
                "http://localhost/callback",
            )
            .await
            .expect("exchange_code should succeed against mock keyring + mock AS");
        assert_eq!(set.access_token, "new-at");
        assert_eq!(set.refresh_token, Some("new-rt".into()));
        assert_eq!(set.scope, Some("read".into()));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn exchange_code_includes_resource_param() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/token")
            .match_body(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("grant_type".into(), "authorization_code".into()),
                mockito::Matcher::UrlEncoded("resource".into(), "https://api.example.com".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "access_token": "at-with-resource",
                    "token_type": "Bearer"
                })
                .to_string(),
            )
            .create_async()
            .await;

        let mgr = TokenManager::new(
            "test-resource".into(),
            "https://api.example.com/mcp".into(),
            "client".into(),
            None,
            format!("{}/token", server.url()),
            Some("https://api.example.com".into()),
        );
        let set = mgr
            .exchange_code("code", "verifier", "http://localhost/cb")
            .await
            .expect("exchange_code should succeed with mock keyring");
        assert_eq!(set.access_token, "at-with-resource");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn exchange_code_includes_client_secret_for_confidential_client() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/token")
            .match_body(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("grant_type".into(), "authorization_code".into()),
                mockito::Matcher::UrlEncoded("client_id".into(), "conf-client".into()),
                mockito::Matcher::UrlEncoded("client_secret".into(), "s3cret".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "access_token": "at-confidential",
                    "token_type": "Bearer"
                })
                .to_string(),
            )
            .create_async()
            .await;

        let mgr = TokenManager::new(
            "test-confidential".into(),
            "https://api.example.com/mcp".into(),
            "conf-client".into(),
            Some("s3cret".into()),
            format!("{}/token", server.url()),
            None,
        );
        let set = mgr
            .exchange_code("code", "verifier", "http://127.0.0.1:9999/oauth/callback")
            .await
            .expect("exchange_code should succeed for confidential client");
        assert_eq!(set.access_token, "at-confidential");
        assert_eq!(set.client_secret, Some("s3cret".into()));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn refresh_if_needed_skips_when_valid() {
        let mgr = TokenManager::new(
            "test-refresh-skip".into(),
            "https://api.example.com/mcp".into(),
            "client".into(),
            None,
            "https://unused/token".into(),
            None,
        );
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let current = OAuthTokenSet {
            access_token: "still-valid".into(),
            refresh_token: Some("rt".into()),
            expires_at: Some(now + 300), // 5 min from now
            token_endpoint: "https://unused/token".into(),
            client_id: "client".into(),
            client_secret: None,
            scope: None,
            resource: None,
        };

        let result = mgr.refresh_if_needed(&current).await.unwrap();
        assert!(result.is_none(), "should skip refresh for valid token");
    }

    #[tokio::test]
    async fn refresh_if_needed_skips_when_expires_at_is_none() {
        // RFC 6749 §5.1 allows omitting `expires_in`. A token with None expiry
        // must be used as-is — triggering a refresh on every request would
        // storm the token endpoint (and outright fail when no refresh_token
        // is held, as is the case here). #1269-99f2.
        let mgr = TokenManager::new(
            "test-refresh-none-expiry".into(),
            "https://api.example.com/mcp".into(),
            "client".into(),
            None,
            "https://unused/token".into(),
            None,
        );
        let current = OAuthTokenSet {
            access_token: "opaque".into(),
            refresh_token: None,
            expires_at: None,
            token_endpoint: "https://unused/token".into(),
            client_id: "client".into(),
            client_secret: None,
            scope: None,
            resource: None,
        };

        let result = mgr
            .refresh_if_needed(&current)
            .await
            .expect("must not attempt refresh when expires_at is None");
        assert!(
            result.is_none(),
            "should skip refresh for None-expiry token"
        );
    }

    #[tokio::test]
    async fn refresh_if_needed_errors_without_refresh_token() {
        let mgr = TokenManager::new(
            "test-refresh-notoken".into(),
            "https://api.example.com/mcp".into(),
            "client".into(),
            None,
            "https://unused/token".into(),
            None,
        );
        let current = OAuthTokenSet {
            access_token: "expired".into(),
            refresh_token: None,
            expires_at: Some(100), // long expired
            token_endpoint: "https://unused/token".into(),
            client_id: "client".into(),
            client_secret: None,
            scope: None,
            resource: None,
        };

        let err = mgr.refresh_if_needed(&current).await.unwrap_err();
        assert!(err.to_string().contains("no refresh_token"), "got: {err}");
    }

    /// Two concurrent 401-driven refreshes on a shared [`TokenManager`] must
    /// collapse into exactly one refresh request at the token endpoint: the
    /// first caller acquires the refresh_lock and writes the new token to the
    /// (mocked) keyring; the second caller wakes up, re-reads the now-valid
    /// token via the double-check pattern, and returns without calling the AS.
    /// If the mutex were bypassed (story 1270-f952 regression) both callers
    /// would race and hits would be 2.
    #[tokio::test]
    async fn refresh_if_needed_serialises_concurrent_callers() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "access_token": "refreshed-at",
                    "refresh_token": "new-rt",
                    "expires_in": 3600,
                    "token_type": "Bearer"
                })
                .to_string(),
            )
            .expect(1)
            .create_async()
            .await;

        let mgr = Arc::new(TokenManager::new(
            "test-concurrent-refresh".into(),
            "https://api.example.com/mcp".into(),
            "client".into(),
            None,
            format!("{}/token", server.url()),
            None,
        ));

        let expired = OAuthTokenSet {
            access_token: "expired-at".into(),
            refresh_token: Some("rt".into()),
            expires_at: Some(0),
            token_endpoint: format!("{}/token", server.url()),
            client_id: "client".into(),
            client_secret: None,
            scope: None,
            resource: None,
        };

        let mgr_a = mgr.clone();
        let expired_a = expired.clone();
        let mgr_b = mgr.clone();
        let expired_b = expired.clone();

        let (res_a, res_b) = tokio::join!(
            tokio::spawn(async move { mgr_a.refresh_if_needed(&expired_a).await }),
            tokio::spawn(async move { mgr_b.refresh_if_needed(&expired_b).await }),
        );

        // Both tasks must succeed against the mock keyring; the mutex-driven
        // double-check means only one HTTP refresh actually fires (asserted
        // by mock.expect(1)).
        let a = res_a.expect("task A panicked").expect("refresh A failed");
        let b = res_b.expect("task B panicked").expect("refresh B failed");
        // Whichever task ran first did the refresh; both end up with a token.
        let token_a = a.unwrap().access_token;
        let token_b = b.unwrap().access_token;
        assert_eq!(token_a, "refreshed-at");
        assert_eq!(token_b, "refreshed-at");

        mock.assert_async().await;
    }

    /// A server can revoke an access token long before its stated expiry — and
    /// some issuers state an absurd one (mcp-s.com hands out `expires_in`
    /// ~86_400_000, parking `expires_at` in 2029 for a token that dies daily).
    /// The caller then arrives here after a 401 holding a token the keyring
    /// still believes in. The double-check must not hand that same dead token
    /// back: it must notice the stored token IS the one being rejected and go
    /// to the AS.
    #[tokio::test]
    async fn refresh_if_needed_renews_a_revoked_token_the_keyring_still_calls_valid() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "access_token": "genuinely-new",
                    "refresh_token": "rt",
                    "expires_in": 3600,
                    "token_type": "Bearer"
                })
                .to_string(),
            )
            .expect(1)
            .create_async()
            .await;

        let name = "test-revoked-but-unexpired";
        let endpoint = format!("{}/token", server.url());

        // What the keyring holds: the revoked token, with a far-future expiry.
        let stored = OAuthTokenSet {
            access_token: "revoked-but-unexpired".into(),
            refresh_token: Some("rt".into()),
            expires_at: Some(i64::MAX / 2),
            token_endpoint: endpoint.clone(),
            client_id: "client".into(),
            client_secret: None,
            scope: None,
            resource: None,
        };
        crate::mcp_upstream_credentials::save_oauth_tokens(
            name,
            &stored,
            "https://api.example.com/mcp",
        )
        .unwrap();

        let mgr = TokenManager::new(
            name.into(),
            "https://api.example.com/mcp".into(),
            "client".into(),
            None,
            endpoint,
            None,
        );
        let refreshed = mgr
            .refresh_after_rejection(&stored, "revoked-but-unexpired")
            .await
            .expect("refresh must reach the AS")
            .expect("a revoked token must yield a new one");
        assert_eq!(refreshed.access_token, "genuinely-new");

        mock.assert_async().await;
    }

    /// The flip side: when a *different* caller already rotated the credential,
    /// the stored token is not the one we were rejected on, so the double-check
    /// still short-circuits and no second request hits the AS.
    #[tokio::test]
    async fn refresh_if_needed_yields_to_a_peer_rotation_without_calling_the_as() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/token").expect(0).create_async().await;

        let name = "test-peer-rotated";
        let endpoint = format!("{}/token", server.url());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let rotated = OAuthTokenSet {
            access_token: "rotated-by-a-peer".into(),
            refresh_token: Some("rt".into()),
            expires_at: Some(now + 3600),
            token_endpoint: endpoint.clone(),
            client_id: "client".into(),
            client_secret: None,
            scope: None,
            resource: None,
        };
        crate::mcp_upstream_credentials::save_oauth_tokens(
            name,
            &rotated,
            "https://api.example.com/mcp",
        )
        .unwrap();

        let mgr = TokenManager::new(
            name.into(),
            "https://api.example.com/mcp".into(),
            "client".into(),
            None,
            endpoint,
            None,
        );
        let result = mgr
            .refresh_after_rejection(&rotated, "the-one-we-got-401-on")
            .await
            .expect("peer rotation must satisfy the caller")
            .expect("the peer's token must be returned");
        assert_eq!(result.access_token, "rotated-by-a-peer");

        mock.assert_async().await;
    }
}
