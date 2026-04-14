//! OAuth 2.1 token exchange and refresh with PKCE and RFC 8707.
//!
//! [`TokenManager`] handles:
//! - PKCE S256 code challenge generation
//! - Authorization code → token exchange
//! - Transparent token refresh with thundering-herd protection
//! - RFC 8707 `resource` parameter on exchange and refresh
//! - Immediate keyring persistence after every token operation

use anyhow::{bail, Context, Result};
use oauth2::PkceCodeChallenge;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::mcp_upstream_credentials::{is_token_valid, save_oauth_tokens, OAuthTokenSet};

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
    /// OAuth client ID.
    client_id: String,
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

impl TokenManager {
    pub(crate) fn new(
        upstream_name: String,
        client_id: String,
        token_endpoint: String,
        resource: Option<String>,
    ) -> Self {
        Self {
            upstream_name,
            client_id,
            token_endpoint,
            resource,
            refresh_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Generate a new PKCE S256 challenge pair for an authorization request.
    pub(crate) fn generate_pkce() -> PkceChallengePair {
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        PkceChallengePair {
            challenge: challenge.as_str().to_string(),
            method: "S256".to_string(),
            verifier: verifier.secret().to_string(),
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
        save_oauth_tokens(&self.upstream_name, &token_set)
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
    pub(crate) async fn refresh_if_needed(
        &self,
        current: &OAuthTokenSet,
    ) -> Result<Option<OAuthTokenSet>> {
        // Fast path: token is still valid
        if is_token_valid(current) {
            return Ok(None);
        }

        let refresh_token = current
            .refresh_token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Token expired and no refresh_token available"))?;

        // Acquire refresh lock — serializes concurrent refresh attempts
        let _guard = self.refresh_lock.lock().await;

        // Double-check: re-read from keyring in case another caller refreshed
        if let Ok(Some(cred)) =
            crate::mcp_upstream_credentials::read_stored_credential(&self.upstream_name)
        {
            if let crate::mcp_upstream_credentials::StoredCredential::Oauth2(ref fresh) = cred {
                if is_token_valid(fresh) {
                    return Ok(Some(fresh.clone()));
                }
            }
        }

        // Still expired — perform refresh
        let http_client = reqwest::Client::new();
        let mut params = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
            ("client_id", self.client_id.as_str()),
        ];

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

        save_oauth_tokens(&self.upstream_name, &token_set)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Some(token_set))
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
            scope: resp.scope,
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
            "client-1".into(),
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
            "client-1".into(),
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
            "client".into(),
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
            // Use a unique name to avoid keyring conflicts in tests
            "test-exchange-happy".into(),
            "my-client".into(),
            format!("{}/token", server.url()),
            None,
        );
        let result = mgr
            .exchange_code("auth-code-123", "pkce-verifier", "http://localhost/callback")
            .await;

        // exchange_code also saves to keyring — may fail on CI without keyring
        match result {
            Ok(set) => {
                assert_eq!(set.access_token, "new-at");
                assert_eq!(set.refresh_token, Some("new-rt".into()));
                assert_eq!(set.scope, Some("read".into()));
                mock.assert_async().await;
            }
            Err(e) if e.to_string().contains("keyring") => {
                // No keyring in test environment — still verify the HTTP call happened
                eprintln!("Skipping keyring assertion: {e}");
                mock.assert_async().await;
            }
            Err(e) => panic!("Unexpected error: {e}"),
        }
    }

    #[tokio::test]
    async fn exchange_code_includes_resource_param() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/token")
            .match_body(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("grant_type".into(), "authorization_code".into()),
                mockito::Matcher::UrlEncoded(
                    "resource".into(),
                    "https://api.example.com".into(),
                ),
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
            "client".into(),
            format!("{}/token", server.url()),
            Some("https://api.example.com".into()),
        );
        let result = mgr
            .exchange_code("code", "verifier", "http://localhost/cb")
            .await;

        match result {
            Ok(set) => assert_eq!(set.access_token, "at-with-resource"),
            Err(e) if e.to_string().contains("keyring") => {
                eprintln!("Skipping keyring assertion: {e}");
            }
            Err(e) => panic!("Unexpected error: {e}"),
        }
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn refresh_if_needed_skips_when_valid() {
        let mgr = TokenManager::new(
            "test-refresh-skip".into(),
            "client".into(),
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
            scope: None,
        };

        let result = mgr.refresh_if_needed(&current).await.unwrap();
        assert!(result.is_none(), "should skip refresh for valid token");
    }

    #[tokio::test]
    async fn refresh_if_needed_errors_without_refresh_token() {
        let mgr = TokenManager::new(
            "test-refresh-notoken".into(),
            "client".into(),
            "https://unused/token".into(),
            None,
        );
        let current = OAuthTokenSet {
            access_token: "expired".into(),
            refresh_token: None,
            expires_at: Some(100), // long expired
            token_endpoint: "https://unused/token".into(),
            client_id: "client".into(),
            scope: None,
        };

        let err = mgr.refresh_if_needed(&current).await.unwrap_err();
        assert!(
            err.to_string().contains("no refresh_token"),
            "got: {err}"
        );
    }
}
