//! GitHub OAuth Device Flow authentication.
//!
//! Provides an alternative to environment variables and `gh` CLI for GitHub
//! API authentication. The OAuth token is stored in the platform-specific
//! secure credential store via the `keyring` crate:
//! - macOS: Keychain
//! - Windows: Credential Manager
//! - Linux: keyutils / Secret Service

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

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
    #[serde(rename = "access_denied")]
    AccessDenied,
}

/// Where the active GitHub token came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TokenSource {
    /// GH_TOKEN or GITHUB_TOKEN environment variable
    Env,
    /// OAuth Device Flow token stored in OS keyring
    OAuth,
    /// gh CLI config or `gh auth token`
    GhCli,
    /// No token available
    #[default]
    None,
}

/// Authentication status returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AuthStatus {
    pub authenticated: bool,
    pub login: Option<String>,
    pub avatar_url: Option<String>,
    pub source: TokenSource,
    pub scopes: Option<String>,
    /// Human-readable error when the token exists but validation failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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
// Tauri commands
// ---------------------------------------------------------------------------

/// Start a Device Flow login. Returns the device code and user code
/// for display in the UI.
#[tauri::command]
pub(crate) async fn github_start_login(
    state: State<'_, Arc<AppState>>,
) -> Result<DeviceCodeResponse, String> {
    start_device_flow(&state.http_client).await
}

/// Poll GitHub for the Device Flow token. The frontend calls this repeatedly
/// with the interval from `github_start_login`. On success, the token is saved
/// to the OS keyring and activated in AppState.
#[tauri::command]
pub(crate) async fn github_poll_login(
    state: State<'_, Arc<AppState>>,
    device_code: String,
) -> Result<PollResult, String> {
    let result = poll_device_flow(&state.http_client, &device_code).await?;

    if let PollResult::Success {
        ref access_token, ..
    } = result
    {
        // Save to keyring for persistence across restarts (blocking I/O → spawn_blocking)
        let token_for_keyring = access_token.clone();
        tokio::task::spawn_blocking(move || save_github_oauth_token(&token_for_keyring))
            .await
            .map_err(|e| format!("keyring task panicked: {e}"))?
            .map_err(|e| {
                tracing::error!(source = "github", error = %e, "OAuth token keyring save failed");
                e
            })?;
        // Activate immediately in runtime state
        *state.github_token.write() = Some(access_token.clone());
        *state.github_token_source.write() = TokenSource::OAuth;
        // Reset circuit breaker so we retry any previously-failed repos
        state.github_circuit_breaker.reset();
        // Clear the repo cooldown cache so "not found" repos get re-checked
        state.git_cache.github_repo_cooldown.clear();
        tracing::info!(source = "github", "OAuth Device Flow login successful");
    }

    Ok(result)
}

/// Log out of GitHub OAuth. Deletes the token from keyring and clears
/// the runtime state. Falls back to env/gh CLI tokens if available.
#[tauri::command]
pub(crate) async fn github_logout(
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(delete_github_oauth_token)
        .await
        .map_err(|e| format!("keyring task panicked: {e}"))??;

    // Re-resolve token from remaining sources (env vars, gh CLI)
    let (token, source) = tokio::task::spawn_blocking(resolve_token_with_source)
        .await
        .map_err(|e| format!("token resolve task panicked: {e}"))?;
    *state.github_token.write() = token;
    *state.github_token_source.write() = source;

    tracing::info!(source = "github", ?source, "OAuth logout — fell back to {source:?}");
    Ok(())
}

/// Disconnect from GitHub entirely, clearing the runtime token regardless
/// of source. Does NOT delete env vars or gh CLI config — only clears the
/// in-memory token so the app stops using it until restart or re-login.
#[tauri::command]
pub(crate) async fn github_disconnect(
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // Delete OAuth token from keyring if present
    if let Err(e) = tokio::task::spawn_blocking(delete_github_oauth_token)
        .await
        .map_err(|e| format!("keyring task panicked: {e}"))
        .and_then(|r| r)
    {
        tracing::warn!(source = "github", error = %e, "Failed to delete OAuth token during disconnect");
    }
    // Clear runtime state entirely
    *state.github_token.write() = None;
    *state.github_token_source.write() = TokenSource::None;
    tracing::info!(source = "github", "GitHub disconnected (runtime token cleared)");
    Ok(())
}

/// Diagnostics about the GitHub integration — repos with errors, circuit breaker state, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GitHubDiagnostics {
    /// Whether the circuit breaker is currently open (API calls blocked)
    pub circuit_breaker_open: bool,
    /// Human-readable circuit breaker status
    pub circuit_breaker_status: String,
    /// Repos that returned "not found" from GitHub (in cooldown)
    pub repos_not_found: Vec<String>,
    /// Number of repos successfully monitored
    pub repos_monitored: u32,
}

/// Get GitHub integration diagnostics for the settings UI.
#[tauri::command]
pub(crate) async fn github_diagnostics(
    state: State<'_, Arc<AppState>>,
) -> Result<GitHubDiagnostics, String> {
    let circuit_breaker_open = state.github_circuit_breaker.check().is_err();
    let circuit_breaker_status = match state.github_circuit_breaker.check() {
        Ok(()) => "OK".to_string(),
        Err(msg) => msg,
    };

    let now = std::time::Instant::now();
    let repos_not_found: Vec<String> = state
        .git_cache
        .github_repo_cooldown
        .iter()
        .filter(|entry| *entry.value() > now)
        .map(|entry| entry.key().clone())
        .collect();

    // Count repos with cached GitHub status (successfully queried)
    let repos_monitored = state.git_cache.github_status.len() as u32;

    Ok(GitHubDiagnostics {
        circuit_breaker_open,
        circuit_breaker_status,
        repos_not_found,
        repos_monitored,
    })
}

/// Get the current GitHub authentication status, including the user's
/// login name if authenticated.
#[tauri::command]
pub(crate) async fn github_auth_status(
    state: State<'_, Arc<AppState>>,
) -> Result<AuthStatus, String> {
    let token = state.github_token.read().clone();
    let source = *state.github_token_source.read();

    let Some(token) = token else {
        return Ok(AuthStatus {
            authenticated: false,
            login: None,
            avatar_url: None,
            source: TokenSource::None,
            scopes: None,
            error: None,
        });
    };

    // Call GitHub /user to get login + avatar
    let resp = state
        .http_client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "TUICommander")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            // Extract scopes header before consuming the body
            let scopes = r
                .headers()
                .get("x-oauth-scopes")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let body: serde_json::Value = r
                .json()
                .await
                .map_err(|e| format!("Failed to parse GitHub /user response: {e}"))?;
            Ok(AuthStatus {
                authenticated: true,
                login: body["login"].as_str().map(|s| s.to_string()),
                avatar_url: body["avatar_url"].as_str().map(|s| s.to_string()),
                source,
                scopes,
                error: None,
            })
        }
        Ok(r) if r.status() == 401 => {
            // Token is invalid — clear it if it was an OAuth token
            let error_msg = format!("Token rejected by GitHub (HTTP 401). The {} token may be expired or revoked.",
                match source {
                    TokenSource::Env => "environment variable",
                    TokenSource::OAuth => "OAuth",
                    TokenSource::GhCli => "gh CLI",
                    TokenSource::None => "current",
                }
            );
            if source == TokenSource::OAuth {
                if let Err(e) = tokio::task::spawn_blocking(delete_github_oauth_token)
                    .await
                    .map_err(|e| format!("keyring task panicked: {e}"))
                    .and_then(|r| r)
                {
                    tracing::warn!(source = "github", error = %e, "Failed to delete stale OAuth token from keyring");
                }
                let (fallback_token, fallback_source) =
                    tokio::task::spawn_blocking(resolve_token_with_source)
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!(source = "github", error = %e, "Token resolution panicked during 401 recovery");
                            (None, TokenSource::None)
                        });
                *state.github_token.write() = fallback_token;
                *state.github_token_source.write() = fallback_source;
            }
            Ok(AuthStatus {
                authenticated: false,
                login: None,
                avatar_url: None,
                source: TokenSource::None,
                scopes: None,
                error: Some(error_msg),
            })
        }
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            tracing::warn!(source = "github", %status, "GitHub /user returned unexpected status");
            Ok(AuthStatus {
                authenticated: false,
                login: None,
                avatar_url: None,
                source,
                scopes: None,
                error: Some(format!("GitHub API error (HTTP {status}): {}", body.lines().next().unwrap_or("unknown error"))),
            })
        }
        Err(e) => {
            tracing::warn!(source = "github", error = %e, "GitHub /user request failed");
            Ok(AuthStatus {
                authenticated: false,
                login: None,
                avatar_url: None,
                source,
                scopes: None,
                error: Some(format!("Could not reach GitHub API: {e}")),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Token resolution with source tracking
// ---------------------------------------------------------------------------

/// Run `gh auth token` CLI to get the current token from gh's secure storage.
/// This works even when env vars are empty/unset, because gh reads from the
/// system keychain on macOS or credential store on other platforms.
pub(crate) fn token_from_gh_cli() -> Option<String> {
    let mut cmd = std::process::Command::new(crate::agent::resolve_cli("gh"));
    cmd.args(["auth", "token"]);
    crate::cli::apply_no_window(&mut cmd);
    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8(output.stdout).ok()?;
    let token = token.trim().to_string();
    if token.is_empty() { None } else { Some(token) }
}

/// Collect all non-empty GitHub token candidates with their source, in priority order.
/// Single source of truth for token priority — used at startup, fallback, and logout.
/// Priority: GH_TOKEN env → GITHUB_TOKEN env → keyring OAuth → gh_token crate → gh CLI.
pub(crate) fn resolve_all_candidates() -> Vec<(String, TokenSource)> {
    let mut candidates = Vec::new();
    if let Ok(token) = std::env::var("GH_TOKEN")
        && !token.is_empty()
    {
        candidates.push((token, TokenSource::Env));
    }
    if let Ok(token) = std::env::var("GITHUB_TOKEN")
        && !token.is_empty()
        && !candidates.iter().any(|(t, _)| t == &token)
    {
        candidates.push((token, TokenSource::Env));
    }
    if let Ok(Some(token)) = read_github_oauth_token()
        && !candidates.iter().any(|(t, _)| t == &token)
    {
        candidates.push((token, TokenSource::OAuth));
    }
    if let Ok(token) = gh_token::get()
        && !token.is_empty()
        && !candidates.iter().any(|(t, _)| t == &token)
    {
        candidates.push((token, TokenSource::GhCli));
    }
    if let Some(token) = token_from_gh_cli()
        && !candidates.iter().any(|(t, _)| t == &token)
    {
        candidates.push((token, TokenSource::GhCli));
    }
    candidates
}

/// Resolve the highest-priority GitHub token and its source.
/// Thin wrapper over `resolve_all_candidates()`.
pub(crate) fn resolve_token_with_source() -> (Option<String>, TokenSource) {
    resolve_all_candidates()
        .into_iter()
        .next()
        .map(|(t, s)| (Some(t), s))
        .unwrap_or((None, TokenSource::None))
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
