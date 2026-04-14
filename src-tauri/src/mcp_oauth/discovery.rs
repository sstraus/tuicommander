//! OAuth discovery for upstream MCP servers.
//!
//! Implements:
//! - RFC 9728: OAuth 2.0 Protected Resource Metadata
//! - RFC 8414: OAuth 2.0 Authorization Server Metadata Discovery
//! - OIDC Discovery fallback (/.well-known/openid-configuration)

use anyhow::{bail, Context, Result};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Metadata about a Protected Resource (RFC 9728).
///
/// Fetched from `{resource}/.well-known/oauth-protected-resource`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ProtectedResourceMetadata {
    /// The resource identifier (MUST match the URL used for discovery).
    pub(crate) resource: String,
    /// Authorization servers that protect this resource.
    pub(crate) authorization_servers: Vec<String>,
    /// Scopes that may be used at this resource.
    #[serde(default)]
    pub(crate) scopes_supported: Option<Vec<String>>,
}

/// Metadata about an Authorization Server (RFC 8414 / OIDC).
///
/// Fetched from `{issuer}/.well-known/oauth-authorization-server` or
/// `{issuer}/.well-known/openid-configuration` (fallback).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AuthServerMetadata {
    /// The issuer identifier (MUST match the URL used for discovery).
    pub(crate) issuer: String,
    /// URL of the authorization endpoint.
    pub(crate) authorization_endpoint: String,
    /// URL of the token endpoint.
    pub(crate) token_endpoint: String,
    /// Scopes supported by this AS.
    #[serde(default)]
    pub(crate) scopes_supported: Option<Vec<String>>,
    /// PKCE code challenge methods supported.
    #[serde(default)]
    pub(crate) code_challenge_methods_supported: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Discovery functions
// ---------------------------------------------------------------------------

/// Discover Protected Resource Metadata (RFC 9728).
///
/// Fetches `{resource_url}/.well-known/oauth-protected-resource` and returns
/// the parsed metadata including the list of authorization servers.
pub(crate) async fn discover_protected_resource(
    client: &reqwest::Client,
    resource_url: &str,
) -> Result<ProtectedResourceMetadata> {
    let url = format!(
        "{}/.well-known/oauth-protected-resource",
        resource_url.trim_end_matches('/')
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Failed to fetch protected resource metadata from {url}"))?;

    if !resp.status().is_success() {
        bail!(
            "Protected resource metadata endpoint returned HTTP {}",
            resp.status()
        );
    }

    let meta: ProtectedResourceMetadata = resp
        .json()
        .await
        .context("Failed to parse protected resource metadata JSON")?;

    if meta.authorization_servers.is_empty() {
        bail!("Protected resource metadata has no authorization_servers");
    }

    Ok(meta)
}

/// Discover Authorization Server Metadata (RFC 8414 with OIDC fallback).
///
/// 1. Tries `{issuer_url}/.well-known/oauth-authorization-server`
/// 2. On 404, falls back to `{issuer_url}/.well-known/openid-configuration`
/// 3. Validates that `issuer` in the response matches `issuer_url` (mix-up attack prevention)
/// 4. Validates that authorization and token endpoints use HTTPS (localhost exempt)
pub(crate) async fn discover_auth_server(
    client: &reqwest::Client,
    issuer_url: &str,
) -> Result<AuthServerMetadata> {
    let base = issuer_url.trim_end_matches('/');

    // Try RFC 8414 first
    let rfc8414_url = format!("{base}/.well-known/oauth-authorization-server");
    let resp = client
        .get(&rfc8414_url)
        .send()
        .await
        .with_context(|| format!("Failed to fetch AS metadata from {rfc8414_url}"))?;

    let meta = if resp.status() == reqwest::StatusCode::NOT_FOUND {
        // Fallback to OIDC discovery
        let oidc_url = format!("{base}/.well-known/openid-configuration");
        let oidc_resp = client
            .get(&oidc_url)
            .send()
            .await
            .with_context(|| format!("Failed to fetch OIDC metadata from {oidc_url}"))?;

        if !oidc_resp.status().is_success() {
            bail!(
                "Both RFC 8414 (404) and OIDC discovery (HTTP {}) failed for {base}",
                oidc_resp.status()
            );
        }

        oidc_resp
            .json::<AuthServerMetadata>()
            .await
            .context("Failed to parse OIDC discovery metadata JSON")?
    } else if resp.status().is_success() {
        resp.json::<AuthServerMetadata>()
            .await
            .context("Failed to parse AS metadata JSON")?
    } else {
        bail!(
            "AS metadata endpoint returned HTTP {} for {rfc8414_url}",
            resp.status()
        );
    };

    // Issuer validation (prevents mix-up attack)
    let expected_issuer = base.to_string();
    if meta.issuer != expected_issuer {
        bail!(
            "Issuer mismatch: expected \"{expected_issuer}\", got \"{}\". \
             This may indicate an authorization server mix-up attack.",
            meta.issuer
        );
    }

    // HTTPS enforcement for endpoints (localhost exempt for dev)
    validate_endpoint_https(&meta.authorization_endpoint, "authorization_endpoint")?;
    validate_endpoint_https(&meta.token_endpoint, "token_endpoint")?;

    Ok(meta)
}

/// Validate that an endpoint URL uses HTTPS.
/// `http://localhost` and `http://127.0.0.1` are allowed for development.
fn validate_endpoint_https(endpoint: &str, field_name: &str) -> Result<()> {
    if endpoint.starts_with("https://") {
        return Ok(());
    }
    if endpoint.starts_with("http://localhost") || endpoint.starts_with("http://127.0.0.1") {
        return Ok(());
    }
    bail!(
        "{field_name} must use HTTPS (got \"{endpoint}\"). \
         Only http://localhost and http://127.0.0.1 are exempt."
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- discover_protected_resource --

    #[tokio::test]
    async fn discover_protected_resource_happy_path() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/.well-known/oauth-protected-resource")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "resource": server.url(),
                    "authorization_servers": ["https://auth.example.com"],
                    "scopes_supported": ["read", "write"]
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let meta = discover_protected_resource(&client, &server.url())
            .await
            .unwrap();

        assert_eq!(meta.resource, server.url());
        assert_eq!(meta.authorization_servers, vec!["https://auth.example.com"]);
        assert_eq!(
            meta.scopes_supported,
            Some(vec!["read".to_string(), "write".to_string()])
        );
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn discover_protected_resource_empty_auth_servers() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/.well-known/oauth-protected-resource")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "resource": server.url(),
                    "authorization_servers": []
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let err = discover_protected_resource(&client, &server.url())
            .await
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("no authorization_servers"),
            "expected 'no authorization_servers', got: {err}"
        );
    }

    #[tokio::test]
    async fn discover_protected_resource_404() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/.well-known/oauth-protected-resource")
            .with_status(404)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let err = discover_protected_resource(&client, &server.url())
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("HTTP 404"),
            "expected HTTP 404 error, got: {err}"
        );
    }

    #[tokio::test]
    async fn discover_protected_resource_malformed_json() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/.well-known/oauth-protected-resource")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("not valid json {{{")
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let err = discover_protected_resource(&client, &server.url())
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("parse"),
            "expected parse error, got: {err}"
        );
    }

    // -- discover_auth_server --

    #[tokio::test]
    async fn discover_auth_server_rfc8414_happy_path() {
        let mut server = mockito::Server::new_async().await;
        let issuer = server.url();
        server
            .mock("GET", "/.well-known/oauth-authorization-server")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "issuer": &issuer,
                    "authorization_endpoint": "https://auth.example.com/authorize",
                    "token_endpoint": "https://auth.example.com/token",
                    "scopes_supported": ["openid", "profile"],
                    "code_challenge_methods_supported": ["S256"]
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let meta = discover_auth_server(&client, &issuer).await.unwrap();

        assert_eq!(meta.issuer, issuer);
        assert_eq!(
            meta.authorization_endpoint,
            "https://auth.example.com/authorize"
        );
        assert_eq!(meta.token_endpoint, "https://auth.example.com/token");
        assert_eq!(
            meta.code_challenge_methods_supported,
            Some(vec!["S256".to_string()])
        );
    }

    #[tokio::test]
    async fn discover_auth_server_oidc_fallback_on_404() {
        let mut server = mockito::Server::new_async().await;
        let issuer = server.url();

        // RFC 8414 returns 404
        server
            .mock("GET", "/.well-known/oauth-authorization-server")
            .with_status(404)
            .create_async()
            .await;

        // OIDC fallback succeeds
        server
            .mock("GET", "/.well-known/openid-configuration")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "issuer": &issuer,
                    "authorization_endpoint": "https://oidc.example.com/authorize",
                    "token_endpoint": "https://oidc.example.com/token"
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let meta = discover_auth_server(&client, &issuer).await.unwrap();

        assert_eq!(meta.issuer, issuer);
        assert_eq!(
            meta.authorization_endpoint,
            "https://oidc.example.com/authorize"
        );
        assert_eq!(meta.token_endpoint, "https://oidc.example.com/token");
    }

    #[tokio::test]
    async fn discover_auth_server_issuer_mismatch() {
        let mut server = mockito::Server::new_async().await;
        let issuer = server.url();
        server
            .mock("GET", "/.well-known/oauth-authorization-server")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "issuer": "https://evil.example.com",
                    "authorization_endpoint": "https://evil.example.com/authorize",
                    "token_endpoint": "https://evil.example.com/token"
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let err = discover_auth_server(&client, &issuer).await.unwrap_err();

        assert!(
            err.to_string().contains("Issuer mismatch"),
            "expected issuer mismatch error, got: {err}"
        );
        assert!(
            err.to_string().contains("mix-up attack"),
            "should mention mix-up attack, got: {err}"
        );
    }

    #[tokio::test]
    async fn discover_auth_server_malformed_json() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/.well-known/oauth-authorization-server")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{invalid json")
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let err = discover_auth_server(&client, &server.url())
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("parse") || err.to_string().contains("Parse"),
            "expected parse error, got: {err}"
        );
    }

    #[tokio::test]
    async fn discover_auth_server_both_endpoints_fail() {
        let mut server = mockito::Server::new_async().await;

        server
            .mock("GET", "/.well-known/oauth-authorization-server")
            .with_status(404)
            .create_async()
            .await;

        server
            .mock("GET", "/.well-known/openid-configuration")
            .with_status(500)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let err = discover_auth_server(&client, &server.url())
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("RFC 8414")
                && err.to_string().contains("OIDC"),
            "should mention both failures, got: {err}"
        );
    }

    // -- HTTPS enforcement --

    #[tokio::test]
    async fn discover_auth_server_rejects_http_endpoints() {
        let mut server = mockito::Server::new_async().await;
        let issuer = server.url();
        server
            .mock("GET", "/.well-known/oauth-authorization-server")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "issuer": &issuer,
                    "authorization_endpoint": "http://insecure.example.com/authorize",
                    "token_endpoint": "https://auth.example.com/token"
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let err = discover_auth_server(&client, &issuer).await.unwrap_err();

        assert!(
            err.to_string().contains("HTTPS"),
            "should require HTTPS, got: {err}"
        );
    }

    #[tokio::test]
    async fn discover_auth_server_allows_localhost_http() {
        let mut server = mockito::Server::new_async().await;
        let issuer = server.url();
        server
            .mock("GET", "/.well-known/oauth-authorization-server")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "issuer": &issuer,
                    "authorization_endpoint": "http://localhost:8080/authorize",
                    "token_endpoint": "http://127.0.0.1:9090/token"
                })
                .to_string(),
            )
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let meta = discover_auth_server(&client, &issuer).await.unwrap();

        assert_eq!(
            meta.authorization_endpoint,
            "http://localhost:8080/authorize"
        );
        assert_eq!(meta.token_endpoint, "http://127.0.0.1:9090/token");
    }

    // -- validate_endpoint_https unit tests --

    #[test]
    fn validate_https_accepts_https() {
        assert!(validate_endpoint_https("https://example.com/token", "test").is_ok());
    }

    #[test]
    fn validate_https_accepts_localhost() {
        assert!(validate_endpoint_https("http://localhost:8080/token", "test").is_ok());
        assert!(validate_endpoint_https("http://127.0.0.1:9090/token", "test").is_ok());
    }

    #[test]
    fn validate_https_rejects_plain_http() {
        let err = validate_endpoint_https("http://evil.example.com/token", "token_endpoint")
            .unwrap_err();
        assert!(err.to_string().contains("HTTPS"));
        assert!(err.to_string().contains("token_endpoint"));
    }
}
