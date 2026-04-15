//! Credential management for upstream MCP servers via OS keyring.
//!
//! Credentials are stored in the platform-specific secure store:
//! - macOS: Keychain
//! - Windows: Credential Manager
//! - Linux: keyutils / Secret Service
//!
//! The config file (`mcp-upstreams.json`) never contains secrets — only the
//! upstream name is needed to look up the credential at runtime.
//!
//! ## Storage format
//!
//! The keyring stores a single string per upstream name. Two formats are
//! supported:
//! - **Legacy**: a plain string (not valid JSON object) → interpreted as a
//!   static Bearer token.
//! - **Structured**: a JSON object with `"type": "bearer"` or
//!   `"type": "oauth2"` discriminant → deserialized into [`StoredCredential`].

use serde::{Deserialize, Serialize};

const SERVICE_NAME: &str = "tuicommander-mcp";

// ---------------------------------------------------------------------------
// Credential types
// ---------------------------------------------------------------------------

/// Token set obtained via OAuth 2.1 Authorization Code + PKCE flow.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct OAuthTokenSet {
    pub(crate) access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) refresh_token: Option<String>,
    /// Unix timestamp (seconds) when `access_token` expires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) expires_at: Option<i64>,
    /// Token endpoint URL for refresh requests.
    pub(crate) token_endpoint: String,
    /// OAuth client ID used during the flow.
    pub(crate) client_id: String,
    /// Space-separated scopes granted by the AS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) scope: Option<String>,
    /// RFC 8707 resource indicator used at token exchange. Replayed on refresh
    /// so the AS rebinds the new token to the same resource; without this we
    /// risk drift when the caller's MCP URL differs from the canonical
    /// resource. Optional for backward compatibility with tokens stored before
    /// this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) resource: Option<String>,
}

/// Credential stored in the OS keyring, tagged by type.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum StoredCredential {
    Bearer {
        token: String,
    },
    Oauth2(OAuthTokenSet),
}

impl StoredCredential {
    /// Return the access/bearer token string regardless of variant.
    pub(crate) fn access_token(&self) -> &str {
        match self {
            Self::Bearer { token } => token,
            Self::Oauth2(set) => &set.access_token,
        }
    }
}

/// Check whether an [`OAuthTokenSet`] token is still valid.
///
/// `expires_in` is OPTIONAL in RFC 6749 §5.1 — some ASes legitimately omit it.
/// When `expires_at` is `None` we cannot know when to refresh, so we trust the
/// token until the upstream returns 401 (at which point the proxy transitions
/// into `NeedsAuth` and the user is prompted). Returning `false` here would
/// force a refresh on every request and, when no refresh token is held, loop
/// the call path into a storm of failed re-exchanges.
///
/// Returns `true` unless:
/// - current time is within 60 seconds of a known `expires_at`
pub(crate) fn is_token_valid(token_set: &OAuthTokenSet) -> bool {
    const EXPIRY_MARGIN_SECS: i64 = 60;
    match token_set.expires_at {
        None => true,
        Some(exp) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            now + EXPIRY_MARGIN_SECS < exp
        }
    }
}

/// Parse a raw keyring string into a [`StoredCredential`].
///
/// - If the string parses as a JSON object with a `"type"` field → structured
///   credential.
/// - Otherwise → legacy plain Bearer token.
pub(crate) fn parse_credential(raw: &str) -> StoredCredential {
    if let Ok(cred) = serde_json::from_str::<StoredCredential>(raw) {
        cred
    } else {
        StoredCredential::Bearer {
            token: raw.to_string(),
        }
    }
}

/// Read and parse a credential for an upstream MCP server.
/// Returns `None` if no credential is stored (not an error).
pub(crate) fn read_stored_credential(
    upstream_name: &str,
) -> Result<Option<StoredCredential>, String> {
    match read_upstream_credential(upstream_name)? {
        Some(raw) => Ok(Some(parse_credential(&raw))),
        None => Ok(None),
    }
}

/// Save an [`OAuthTokenSet`] to the keyring as a structured JSON blob.
pub(crate) fn save_oauth_tokens(
    upstream_name: &str,
    token_set: &OAuthTokenSet,
) -> Result<(), String> {
    let cred = StoredCredential::Oauth2(token_set.clone());
    let json = serde_json::to_string(&cred)
        .map_err(|e| format!("Failed to serialize OAuth tokens: {e}"))?;
    save_upstream_credential(upstream_name, &json)
}

/// Validate that an upstream name is safe for use as a keyring key.
/// Must match `[a-z0-9_-]+` (same rule as upstream config validation).
fn validate_keyring_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Upstream name must not be empty".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(format!(
            "Upstream name '{name}' is invalid: must contain only lowercase letters, digits, hyphens, and underscores"
        ));
    }
    Ok(())
}

/// Read a credential for an upstream MCP server.
/// Returns `None` if no credential is stored (not an error).
pub(crate) fn read_upstream_credential(upstream_name: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(SERVICE_NAME, upstream_name)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;

    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!(
            "Failed to read credential for upstream '{upstream_name}': {e}"
        )),
    }
}

/// Store a credential for an upstream MCP server.
pub(crate) fn save_upstream_credential(
    upstream_name: &str,
    token: &str,
) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, upstream_name)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;

    entry
        .set_password(token)
        .map_err(|e| format!("Failed to save credential for upstream '{upstream_name}': {e}"))
}

/// Delete a credential for an upstream MCP server.
/// Returns Ok(()) even if no credential existed (idempotent).
pub(crate) fn delete_upstream_credential(upstream_name: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, upstream_name)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;

    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // Already gone — idempotent
        Err(e) => Err(format!(
            "Failed to delete credential for upstream '{upstream_name}': {e}"
        )),
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub(crate) fn save_mcp_upstream_credential(
    name: String,
    token: String,
) -> Result<(), String> {
    validate_keyring_name(&name)?;
    save_upstream_credential(&name, &token)
}

#[tauri::command]
pub(crate) fn delete_mcp_upstream_credential(name: String) -> Result<(), String> {
    validate_keyring_name(&name)?;
    delete_upstream_credential(&name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // These tests interact with the real OS keyring. Write tests are #[ignore]
    // because macOS prompts for Keychain access interactively, blocking CI/automated runs.
    // Run manually with: cargo test mcp_upstream_credentials -- --ignored

    const TEST_UPSTREAM: &str = "tuicommander-test-upstream-credential";

    #[test]
    fn read_nonexistent_returns_none() {
        let result = read_upstream_credential("nonexistent-upstream-that-does-not-exist-xyz");
        match result {
            Ok(None) => {} // Expected: no credential stored
            Ok(Some(_)) => panic!("Expected None for nonexistent credential"),
            Err(e) => {
                // On systems without a keyring backend, this may error — skip gracefully
                eprintln!("Skipping test (no keyring backend): {e}");
            }
        }
    }

    #[test]
    #[ignore] // Requires interactive Keychain access on macOS
    fn save_read_delete_round_trip() {
        let token = "test-token-abc123";

        // Save
        match save_upstream_credential(TEST_UPSTREAM, token) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("Skipping test (no keyring backend): {e}");
                return;
            }
        }

        // Read back
        let read = read_upstream_credential(TEST_UPSTREAM).unwrap();
        assert_eq!(read, Some(token.to_string()));

        // Delete
        delete_upstream_credential(TEST_UPSTREAM).unwrap();

        // Verify deleted
        let after_delete = read_upstream_credential(TEST_UPSTREAM).unwrap();
        assert_eq!(after_delete, None);
    }

    // -- validate_keyring_name --

    #[test]
    fn valid_keyring_names() {
        assert!(validate_keyring_name("my-server").is_ok());
        assert!(validate_keyring_name("server_1").is_ok());
        assert!(validate_keyring_name("a").is_ok());
        assert!(validate_keyring_name("abc-def_123").is_ok());
    }

    #[test]
    fn empty_keyring_name_rejected() {
        assert!(validate_keyring_name("").is_err());
    }

    #[test]
    fn uppercase_keyring_name_rejected() {
        assert!(validate_keyring_name("MyServer").is_err());
    }

    #[test]
    fn keyring_name_with_spaces_rejected() {
        assert!(validate_keyring_name("my server").is_err());
    }

    #[test]
    fn keyring_name_with_path_separators_rejected() {
        assert!(validate_keyring_name("../etc/passwd").is_err());
    }

    #[test]
    fn keyring_name_with_special_chars_rejected() {
        assert!(validate_keyring_name("server;drop").is_err());
    }

    #[test]
    fn delete_nonexistent_is_idempotent() {
        let result = delete_upstream_credential("nonexistent-upstream-that-does-not-exist-xyz");
        match result {
            Ok(()) => {} // Expected: idempotent
            Err(e) => {
                eprintln!("Skipping test (no keyring backend): {e}");
            }
        }
    }

    // -- StoredCredential / OAuthTokenSet tests --

    fn sample_oauth_token_set() -> OAuthTokenSet {
        OAuthTokenSet {
            access_token: "eyJhbGciOiJSUzI1NiJ9.test".to_string(),
            refresh_token: Some("dGVzdC1yZWZyZXNo".to_string()),
            expires_at: Some(1_700_000_000),
            token_endpoint: "https://auth.example.com/token".to_string(),
            client_id: "tuic-client-abc".to_string(),
            scope: Some("read write".to_string()),
            resource: Some("https://api.example.com".to_string()),
        }
    }

    #[test]
    fn stored_credential_bearer_serde_round_trip() {
        let cred = StoredCredential::Bearer {
            token: "sk-test-123".to_string(),
        };
        let json = serde_json::to_string(&cred).unwrap();
        let parsed: StoredCredential = serde_json::from_str(&json).unwrap();
        assert_eq!(cred, parsed);
        assert!(json.contains(r#""type":"bearer""#));
    }

    #[test]
    fn stored_credential_oauth2_serde_round_trip() {
        let cred = StoredCredential::Oauth2(sample_oauth_token_set());
        let json = serde_json::to_string(&cred).unwrap();
        let parsed: StoredCredential = serde_json::from_str(&json).unwrap();
        assert_eq!(cred, parsed);
        assert!(json.contains(r#""type":"oauth2""#));
    }

    #[test]
    fn stored_credential_oauth2_without_optionals() {
        let set = OAuthTokenSet {
            access_token: "token".to_string(),
            refresh_token: None,
            expires_at: None,
            token_endpoint: "https://auth.example.com/token".to_string(),
            client_id: "client".to_string(),
            scope: None,
            resource: None,
        };
        let cred = StoredCredential::Oauth2(set);
        let json = serde_json::to_string(&cred).unwrap();
        // Optionals should be omitted
        assert!(!json.contains("refresh_token"));
        assert!(!json.contains("expires_at"));
        assert!(!json.contains("scope"));
        assert!(!json.contains("\"resource\""));
        // Round-trip
        let parsed: StoredCredential = serde_json::from_str(&json).unwrap();
        assert_eq!(cred, parsed);
    }

    #[test]
    fn parse_credential_plain_string_becomes_bearer() {
        let cred = parse_credential("sk-my-static-token");
        assert_eq!(
            cred,
            StoredCredential::Bearer {
                token: "sk-my-static-token".to_string()
            }
        );
    }

    #[test]
    fn parse_credential_json_bearer() {
        let json = r#"{"type":"bearer","token":"sk-structured"}"#;
        let cred = parse_credential(json);
        assert_eq!(
            cred,
            StoredCredential::Bearer {
                token: "sk-structured".to_string()
            }
        );
    }

    #[test]
    fn parse_credential_json_oauth2() {
        let set = sample_oauth_token_set();
        let json = serde_json::to_string(&StoredCredential::Oauth2(set.clone())).unwrap();
        let cred = parse_credential(&json);
        assert_eq!(cred, StoredCredential::Oauth2(set));
    }

    #[test]
    fn parse_credential_malformed_json_becomes_bearer() {
        // A JSON array is not a valid StoredCredential
        let cred = parse_credential("[1,2,3]");
        assert_eq!(
            cred,
            StoredCredential::Bearer {
                token: "[1,2,3]".to_string()
            }
        );
    }

    #[test]
    fn parse_credential_empty_string_becomes_bearer() {
        let cred = parse_credential("");
        assert_eq!(
            cred,
            StoredCredential::Bearer {
                token: String::new()
            }
        );
    }

    #[test]
    fn access_token_returns_correct_value() {
        let bearer = StoredCredential::Bearer {
            token: "bearer-tok".to_string(),
        };
        assert_eq!(bearer.access_token(), "bearer-tok");

        let oauth = StoredCredential::Oauth2(sample_oauth_token_set());
        assert_eq!(oauth.access_token(), "eyJhbGciOiJSUzI1NiJ9.test");
    }

    #[test]
    fn is_token_valid_no_expiry_returns_true() {
        // RFC 6749 §5.1: expires_in is OPTIONAL. ASes that omit it must not
        // force a refresh on every request — we trust the token until the
        // upstream returns 401 and we transition into NeedsAuth.
        let mut set = sample_oauth_token_set();
        set.expires_at = None;
        assert!(is_token_valid(&set));
    }

    #[test]
    fn is_token_valid_expired_returns_false() {
        let mut set = sample_oauth_token_set();
        // Set expiry to 1970 — definitely expired
        set.expires_at = Some(100);
        assert!(!is_token_valid(&set));
    }

    #[test]
    fn is_token_valid_within_margin_returns_false() {
        let mut set = sample_oauth_token_set();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        // Expires in 30 seconds — within 60s margin
        set.expires_at = Some(now + 30);
        assert!(!is_token_valid(&set));
    }

    #[test]
    fn is_token_valid_future_returns_true() {
        let mut set = sample_oauth_token_set();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        // Expires in 5 minutes — well beyond margin
        set.expires_at = Some(now + 300);
        assert!(is_token_valid(&set));
    }

    #[test]
    #[ignore] // Requires interactive Keychain access on macOS
    fn overwrite_credential() {
        let token_v1 = "token-v1";
        let token_v2 = "token-v2";

        match save_upstream_credential(TEST_UPSTREAM, token_v1) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("Skipping test (no keyring backend): {e}");
                return;
            }
        }

        // Overwrite
        save_upstream_credential(TEST_UPSTREAM, token_v2).unwrap();

        // Read latest
        let read = read_upstream_credential(TEST_UPSTREAM).unwrap();
        assert_eq!(read, Some(token_v2.to_string()));

        // Cleanup
        delete_upstream_credential(TEST_UPSTREAM).unwrap();
    }
}
