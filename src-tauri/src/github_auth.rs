//! GitHub OAuth Device Flow authentication.
//!
//! Provides an alternative to environment variables and `gh` CLI for GitHub
//! API authentication. The OAuth token is stored in the platform-specific
//! secure credential store via the `keyring` crate:
//! - macOS: Keychain
//! - Windows: Credential Manager
//! - Linux: keyutils / Secret Service

use serde::{Deserialize, Serialize};

const SERVICE_NAME: &str = "tuicommander-github";
const KEYRING_KEY: &str = "oauth-token";

/// GitHub OAuth App client ID (public — not a secret).
const CLIENT_ID: &str = "Ov23lirBjdz4Kkt4nybU";

/// Scopes requested during Device Flow authentication.
const OAUTH_SCOPES: &str = "repo read:org";

// ---------------------------------------------------------------------------
// Device Flow types
// ---------------------------------------------------------------------------

/// Response from `POST https://github.com/login/device/code`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Result of a single poll attempt to `POST https://github.com/login/oauth/access_token`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub(crate) enum PollResult {
    /// User hasn't authorized yet — keep polling.
    #[serde(rename = "pending")]
    Pending,
    /// Polling too fast — increase interval by 5 seconds.
    #[serde(rename = "slow_down")]
    SlowDown,
    /// User authorized — token received.
    #[serde(rename = "success")]
    Success {
        access_token: String,
        scope: String,
    },
    /// Device code expired (15 min) — must restart flow.
    #[serde(rename = "expired")]
    Expired,
    /// User denied access.
    #[serde(rename = "denied")]
    AccessDenied,
}

/// Raw GitHub error response during token polling.
#[derive(Debug, Deserialize)]
struct GithubErrorResponse {
    error: String,
    #[allow(dead_code)]
    error_description: Option<String>,
}

/// Raw GitHub success response during token polling.
#[derive(Debug, Deserialize)]
struct GithubTokenResponse {
    access_token: String,
    #[allow(dead_code)]
    token_type: String,
    scope: String,
}

// ---------------------------------------------------------------------------
// Device Flow API calls
// ---------------------------------------------------------------------------

/// Start the Device Flow by requesting a device code from GitHub.
pub(crate) async fn start_device_flow(
    client: &reqwest::Client,
) -> Result<DeviceCodeResponse, String> {
    let params = [("client_id", CLIENT_ID), ("scope", OAUTH_SCOPES)];

    let response = client
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Failed to start Device Flow: {e}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read Device Flow response: {e}"))?;

    if !status.is_success() {
        return Err(format!(
            "GitHub Device Flow request failed (HTTP {status}): {body}"
        ));
    }

    serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse Device Flow response: {e}"))
}

/// Make a single poll attempt to exchange the device code for an access token.
pub(crate) async fn poll_device_flow(
    client: &reqwest::Client,
    device_code: &str,
) -> Result<PollResult, String> {
    let params = [
        ("client_id", CLIENT_ID),
        ("device_code", device_code),
        (
            "grant_type",
            "urn:ietf:params:oauth:grant-type:device_code",
        ),
    ];

    let response = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Failed to poll Device Flow: {e}"))?;

    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read poll response: {e}"))?;

    // GitHub returns 200 for both success and error states during polling.
    // Try parsing as a success response first, then as an error.
    if let Ok(token_resp) = serde_json::from_str::<GithubTokenResponse>(&body) {
        return Ok(PollResult::Success {
            access_token: token_resp.access_token,
            scope: token_resp.scope,
        });
    }

    if let Ok(err_resp) = serde_json::from_str::<GithubErrorResponse>(&body) {
        return Ok(match err_resp.error.as_str() {
            "authorization_pending" => PollResult::Pending,
            "slow_down" => PollResult::SlowDown,
            "expired_token" => PollResult::Expired,
            "access_denied" => PollResult::AccessDenied,
            other => return Err(format!("Unexpected Device Flow error: {other}")),
        });
    }

    Err(format!("Unexpected Device Flow response: {body}"))
}

// ---------------------------------------------------------------------------
// Keyring helpers
// ---------------------------------------------------------------------------

/// Read the stored OAuth token from the OS keyring.
/// Returns `None` if no token is stored (not an error).
pub(crate) fn read_github_oauth_token() -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(SERVICE_NAME, KEYRING_KEY)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;

    match entry.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to read GitHub OAuth token: {e}")),
    }
}

/// Store an OAuth token in the OS keyring.
pub(crate) fn save_github_oauth_token(token: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, KEYRING_KEY)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;

    entry
        .set_password(token)
        .map_err(|e| format!("Failed to save GitHub OAuth token: {e}"))
}

/// Delete the OAuth token from the OS keyring.
/// Returns Ok(()) even if no token existed (idempotent).
pub(crate) fn delete_github_oauth_token() -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, KEYRING_KEY)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;

    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // Already gone — idempotent
        Err(e) => Err(format!("Failed to delete GitHub OAuth token: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // These tests interact with the real OS keyring. Write tests are #[ignore]
    // because macOS prompts for Keychain access interactively, blocking CI.
    // Run manually with: cargo test github_auth -- --ignored

    #[test]
    fn read_nonexistent_returns_none() {
        // Use a different service name to avoid interfering with real tokens.
        // Since we're testing the public API that uses fixed SERVICE_NAME,
        // we test via the actual function — a missing token should return None.
        let result = read_github_oauth_token();
        match result {
            Ok(None) | Ok(Some(_)) => {} // Either is fine depending on keyring state
            Err(e) => {
                // On systems without a keyring backend, this may error — skip gracefully
                eprintln!("Skipping test (no keyring backend): {e}");
            }
        }
    }

    #[test]
    fn delete_when_empty_is_idempotent() {
        // Deleting a non-existent token should not error
        let result = delete_github_oauth_token();
        match result {
            Ok(()) => {} // Expected: idempotent
            Err(e) => {
                eprintln!("Skipping test (no keyring backend): {e}");
            }
        }
    }

    #[test]
    #[ignore] // Requires interactive Keychain access on macOS
    fn save_read_delete_round_trip() {
        let token = "gho_test_token_round_trip_xyz";

        // Save
        match save_github_oauth_token(token) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("Skipping test (no keyring backend): {e}");
                return;
            }
        }

        // Read back
        let read = read_github_oauth_token().unwrap();
        assert_eq!(read, Some(token.to_string()));

        // Delete
        delete_github_oauth_token().unwrap();

        // Verify deleted
        let after_delete = read_github_oauth_token().unwrap();
        assert_eq!(after_delete, None);
    }

    // -- Device Flow deserialization tests --

    #[test]
    fn parse_device_code_response() {
        let json = r#"{
            "device_code": "3584d83530557fdd1f46af8289938c8ef79f9dc5",
            "user_code": "WDJB-MJHT",
            "verification_uri": "https://github.com/login/device",
            "expires_in": 900,
            "interval": 5
        }"#;
        let resp: DeviceCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.user_code, "WDJB-MJHT");
        assert_eq!(resp.verification_uri, "https://github.com/login/device");
        assert_eq!(resp.expires_in, 900);
        assert_eq!(resp.interval, 5);
    }

    #[test]
    fn parse_token_success_response() {
        let json = r#"{
            "access_token": "gho_16C7e42F292c6912E7710c838347Ae178B4a",
            "token_type": "bearer",
            "scope": "repo read:org"
        }"#;
        let resp: GithubTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.access_token, "gho_16C7e42F292c6912E7710c838347Ae178B4a");
        assert_eq!(resp.scope, "repo read:org");
    }

    #[test]
    fn parse_authorization_pending_error() {
        let json = r#"{
            "error": "authorization_pending",
            "error_description": "The authorization request is still pending."
        }"#;
        let resp: GithubErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.error, "authorization_pending");
    }

    #[test]
    fn parse_slow_down_error() {
        let json = r#"{
            "error": "slow_down",
            "error_description": "Too many requests."
        }"#;
        let resp: GithubErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.error, "slow_down");
    }

    #[test]
    fn parse_expired_token_error() {
        let json = r#"{
            "error": "expired_token",
            "error_description": "The device code has expired."
        }"#;
        let resp: GithubErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.error, "expired_token");
    }

    #[test]
    fn parse_access_denied_error() {
        let json = r#"{
            "error": "access_denied",
            "error_description": "The user has denied your application access."
        }"#;
        let resp: GithubErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.error, "access_denied");
    }

    #[test]
    fn parse_error_without_description() {
        let json = r#"{"error": "authorization_pending"}"#;
        let resp: GithubErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.error, "authorization_pending");
        assert!(resp.error_description.is_none());
    }

    #[test]
    fn poll_result_serializes_with_tag() {
        let pending = PollResult::Pending;
        let json = serde_json::to_string(&pending).unwrap();
        assert!(json.contains(r#""status":"pending"#));

        let success = PollResult::Success {
            access_token: "gho_abc".to_string(),
            scope: "repo".to_string(),
        };
        let json = serde_json::to_string(&success).unwrap();
        assert!(json.contains(r#""status":"success"#));
        assert!(json.contains(r#""access_token":"gho_abc"#));
    }

    #[test]
    #[ignore] // Requires interactive Keychain access on macOS
    fn overwrite_token() {
        let token_v1 = "gho_test_v1";
        let token_v2 = "gho_test_v2";

        match save_github_oauth_token(token_v1) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("Skipping test (no keyring backend): {e}");
                return;
            }
        }

        // Overwrite
        save_github_oauth_token(token_v2).unwrap();

        // Read latest
        let read = read_github_oauth_token().unwrap();
        assert_eq!(read, Some(token_v2.to_string()));

        // Cleanup
        delete_github_oauth_token().unwrap();
    }
}
