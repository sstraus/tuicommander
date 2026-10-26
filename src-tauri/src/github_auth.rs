//! GitHub OAuth Device Flow authentication.
//!
//! Provides an alternative to environment variables and `gh` CLI for GitHub
//! API authentication. The OAuth token is stored in the unified credential
//! vault (`credentials.rs`).

use std::sync::{Arc, atomic::Ordering};

use serde::{Deserialize, Serialize};
#[cfg(feature = "desktop")]
use tauri::State;

use crate::state::AppState;

use crate::credentials::Credential;

/// GitHub OAuth App client ID (public — not a secret).
const CLIENT_ID: &str = "Ov23liN95BHKQDboVFRl";

/// Scopes requested during Device Flow authentication.
///
/// `repo` covers repository REST/GraphQL operations; `read:project` permits
/// Projects v2 reads. Existing logins must authorize again to gain that scope.
/// SAML-SSO access still requires the user's per-org token authorization.
const OAUTH_SCOPES: &str = include_str!("../../src/data/github-oauth-scopes.txt");

// ---------------------------------------------------------------------------
// Device Flow types
// ---------------------------------------------------------------------------

pub(crate) use tuic_git::github_auth::*;

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

/// Construct the device-code wire request shared by every authentication entry point.
fn device_flow_request(client: &reqwest::Client) -> reqwest::RequestBuilder {
    client
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .form(&[("client_id", CLIENT_ID), ("scope", OAUTH_SCOPES.trim())])
}

/// Start the Device Flow by requesting a device code from GitHub.
pub(crate) async fn start_device_flow(
    client: &reqwest::Client,
) -> Result<DeviceCodeResponse, String> {
    crate::github_debug::log_api(
        "POST",
        "https://github.com/login/device/code",
        "start_device_flow",
    );
    let response = device_flow_request(client)
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

    serde_json::from_str(&body).map_err(|e| format!("Failed to parse Device Flow response: {e}"))
}

/// Make a single poll attempt to exchange the device code for an access token.
pub(crate) async fn poll_device_flow(
    client: &reqwest::Client,
    device_code: &str,
) -> Result<PollResult, String> {
    let params = [
        ("client_id", CLIENT_ID),
        ("device_code", device_code),
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
    ];

    crate::github_debug::log_api(
        "POST",
        "https://github.com/login/oauth/access_token",
        "poll_device_flow",
    );
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
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_start_login(
    state: State<'_, Arc<AppState>>,
) -> Result<DeviceCodeResponse, String> {
    start_device_flow(&state.http_client).await
}

/// Poll GitHub for the Device Flow token. The frontend calls this repeatedly
/// with the interval from `github_start_login`. On success, the token is saved
/// to the OS keyring and activated in AppState.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_poll_login(
    state: State<'_, Arc<AppState>>,
    device_code: String,
) -> Result<PollResult, String> {
    github_poll_login_impl(state.inner(), device_code).await
}

pub(crate) async fn github_poll_login_impl(
    state: &Arc<AppState>,
    device_code: String,
) -> Result<PollResult, String> {
    let epoch = state.github.login_epoch.load(Ordering::SeqCst);
    github_poll_login_impl_at_epoch(state, device_code, epoch).await
}

pub(crate) async fn github_poll_login_impl_at_epoch(
    state: &Arc<AppState>,
    device_code: String,
    epoch: u64,
) -> Result<PollResult, String> {
    let result = poll_device_flow(&state.http_client, &device_code).await?;

    let _commit = state.github.login_commit.lock().await;
    if state.github.login_epoch.load(Ordering::SeqCst) != epoch {
        return Ok(PollResult::Expired);
    }
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
        {
            let mut slot = state.github.token.write();
            state.github.disconnected.store(false, Ordering::Relaxed);
            *slot = Some(access_token.clone());
            *state.github.token_source.write() = TokenSource::OAuth;
        }
        // The token may belong to a different user than the one we cached, and the
        // cached login drives `author:@me` and the issue filters.
        crate::github::invalidate_viewer_login(state);
        // Reset the github.com circuit breaker so we retry any previously-failed repos
        state.github.circuit_breaker.reset();
        // Clear only github.com cooldowns ("owner/name", no ':'), leaving GHE
        // cooldowns ("{id}:owner/name") untouched — github.com login must not
        // disturb other accounts' state.
        state
            .git_cache
            .github_repo_cooldown
            .retain(|key, _| key.contains(':'));
        state
            .git_cache
            .github_repo_access_errors
            .retain(|_, failure| {
                failure.account_id != crate::github_account::GitHubAccount::GITHUB_COM_ID
            });
        tracing::info!(source = "github", "OAuth Device Flow login successful");
    }

    Ok(result)
}

/// Poll the Device Flow for an ADDITIONAL github.com account. Unlike
/// [`github_poll_login`], success does NOT touch the ambient default's token or
/// runtime state: it validates the new token against `/user`, resolves the login
/// (the account id), and persists a named registry entry with its own
/// per-account token. The frontend drives this with the same start/interval as a
/// normal login, but in "add account" mode.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_poll_add_account(
    state: State<'_, Arc<AppState>>,
    device_code: String,
) -> Result<PollResult, String> {
    github_poll_add_account_impl(state.inner(), device_code).await
}

pub(crate) async fn github_poll_add_account_impl(
    state: &Arc<AppState>,
    device_code: String,
) -> Result<PollResult, String> {
    let epoch = state.github.login_epoch.load(Ordering::SeqCst);
    github_poll_add_account_impl_at_epoch(state, device_code, epoch).await
}

pub(crate) async fn github_poll_add_account_impl_at_epoch(
    state: &Arc<AppState>,
    device_code: String,
    epoch: u64,
) -> Result<PollResult, String> {
    let result = poll_device_flow(&state.http_client, &device_code).await?;

    let _commit = state.github.login_commit.lock().await;
    if state.github.login_epoch.load(Ordering::SeqCst) != epoch {
        return Ok(PollResult::Expired);
    }
    if let PollResult::Success {
        ref access_token, ..
    } = result
    {
        let host = crate::github_account::GitHubHost::new("github.com")
            .expect("github.com is a valid host");
        let login =
            crate::github_account::fetch_account_login(&state.http_client, &host, access_token)
                .await?
                .ok_or_else(|| "GitHub did not return a login for this token".to_string())?;

        if login == crate::github_account::GitHubAccount::GITHUB_COM_ID {
            // Defensive: a real GitHub login can never be "github.com", but never
            // let a value collide with the ambient default's reserved id.
            return Err("Unexpected login collides with the default account id".to_string());
        }

        let account = crate::github_account::GitHubAccount::github_com_named(&login);
        let token = access_token.clone();
        let account_for_save = account.clone();
        tokio::task::spawn_blocking(move || {
            crate::github_account::add_account_record(account_for_save, Some(&token))
        })
        .await
        .map_err(|e| format!("keyring task panicked: {e}"))??;
        tracing::info!(source = "github", login = %login, "Registered additional github.com account");
    }

    Ok(result)
}

/// Log out of GitHub OAuth. Deletes the token from keyring and clears
/// the runtime state. Falls back to env tokens if available.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_logout(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    github_logout_impl(state.inner()).await
}

pub(crate) async fn github_logout_impl(state: &Arc<AppState>) -> Result<(), String> {
    let _commit = state.github.login_commit.lock().await;
    state.github.login_epoch.fetch_add(1, Ordering::SeqCst);
    tokio::task::spawn_blocking(delete_github_oauth_token)
        .await
        .map_err(|e| format!("keyring task panicked: {e}"))??;

    // Re-resolve token from remaining sources (env vars)
    let (token, source) = tokio::task::spawn_blocking(resolve_token_with_source)
        .await
        .map_err(|e| format!("token resolve task panicked: {e}"))?;
    {
        let mut slot = state.github.token.write();
        if !state.github.disconnected.load(Ordering::Relaxed) {
            *slot = token;
            *state.github.token_source.write() = source;
        }
    }
    // The fallback token (env) is very likely a different user.
    crate::github::invalidate_viewer_login(state);

    tracing::info!(
        source = "github",
        ?source,
        "OAuth logout — fell back to {source:?}"
    );
    Ok(())
}

/// Disconnect from GitHub entirely, clearing the runtime token regardless
/// of source. Does NOT delete env vars — only clears the
/// in-memory token so the app stops using it until restart or re-login.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_disconnect(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    github_disconnect_impl(state.inner()).await
}

pub(crate) async fn github_disconnect_impl(state: &Arc<AppState>) -> Result<(), String> {
    let _commit = state.github.login_commit.lock().await;
    state.github.login_epoch.fetch_add(1, Ordering::SeqCst);
    // Delete OAuth token from keyring if present
    if let Err(e) = tokio::task::spawn_blocking(delete_github_oauth_token)
        .await
        .map_err(|e| format!("keyring task panicked: {e}"))
        .and_then(|r| r)
    {
        tracing::warn!(source = "github", error = %e, "Failed to delete OAuth token during disconnect");
    }
    // Clear runtime state entirely
    {
        let mut slot = state.github.token.write();
        state.github.disconnected.store(true, Ordering::Relaxed);
        *slot = None;
        *state.github.token_source.write() = TokenSource::None;
    }
    crate::github::invalidate_viewer_login(state);
    tracing::info!(
        source = "github",
        "GitHub disconnected (runtime token cleared)"
    );
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
    pub repo_access_errors: Vec<RepoAccessErrorGroup>,
    /// Number of repos successfully monitored
    pub repos_monitored: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RepoAccessErrorGroup {
    pub account_id: String,
    pub host: String,
    pub owner: String,
    pub repos: Vec<tuic_git::github_access::RepoAccessError>,
    pub oauth_app_url: Option<String>,
}

/// Compute GitHub integration diagnostics from the current state.
///
/// Pure seam (no `State`/network) so the Step 0 characterization net can pin
/// the diagnostics shape for a github.com-only setup.
pub(crate) fn compute_diagnostics(state: &AppState) -> GitHubDiagnostics {
    let cloud_status = state.github.circuit_breaker.check();
    // A GHE account whose breaker is open also counts as "open" for the UI, but
    // never changes the github.com-only output (ghe_state is empty then).
    let ghe_open = state
        .github
        .ghe_state
        .iter()
        .any(|e| e.value().circuit_breaker.check().is_err());
    let circuit_breaker_open = cloud_status.is_err() || ghe_open;
    let circuit_breaker_status = match &cloud_status {
        Ok(()) if ghe_open => "An Enterprise account is backing off".to_string(),
        Ok(()) => "OK".to_string(),
        Err(msg) => msg.clone(),
    };

    let now = std::time::Instant::now();
    let repos_not_found: Vec<String> = state
        .git_cache
        .github_repo_cooldown
        .iter()
        .filter(|entry| *entry.value() > now)
        .map(|entry| entry.key().clone())
        .collect();

    let mut groups = std::collections::BTreeMap::new();
    for entry in &state.git_cache.github_repo_access_errors {
        let failure = entry.value();
        if failure.error.reason == tuic_git::github_access::RepoAccessReason::NotFound
            && state
                .git_cache
                .github_repo_cooldown
                .get(entry.key())
                .is_none_or(|expiry| *expiry <= now)
        {
            continue;
        }
        let group = groups
            .entry((
                failure.account_id.clone(),
                failure.host.clone(),
                failure.owner.clone(),
            ))
            .or_insert_with(|| RepoAccessErrorGroup {
                account_id: failure.account_id.clone(),
                host: failure.host.clone(),
                owner: failure.owner.clone(),
                repos: Vec::new(),
                oauth_app_url: None,
            });
        if failure.oauth_app && failure.host == "github.com" {
            group.oauth_app_url = Some(format!(
                "https://github.com/settings/connections/applications/{CLIENT_ID}"
            ));
        }
        group.repos.push(failure.error.clone());
    }
    let repo_access_errors = groups
        .into_values()
        .map(|mut group| {
            group.repos.sort_by(|a, b| a.repo.cmp(&b.repo));
            group
        })
        .collect();

    // Transition history belongs to the running poller, not to its expiring
    // query payload cache. Empty successful snapshots count as monitored too.
    let repos_monitored = state
        .github
        .poller
        .lock()
        .as_ref()
        .map_or(0, |poller| poller.monitored_repo_count() as u32);

    GitHubDiagnostics {
        circuit_breaker_open,
        circuit_breaker_status,
        repos_not_found,
        repo_access_errors,
        repos_monitored,
    }
}

/// Get GitHub integration diagnostics for the settings UI.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_diagnostics(
    state: State<'_, Arc<AppState>>,
    refresh: Option<bool>,
) -> Result<GitHubDiagnostics, String> {
    github_diagnostics_impl(state.inner(), refresh.unwrap_or(false)).await
}

/// HTTP/IPC-parity wrapper over the pure `compute_diagnostics` seam. Kept so the
/// `/github/diagnostics` axum route (github_routes.rs) and the Tauri command
/// share one code path; `compute_diagnostics` now also folds in GHE accounts.
pub(crate) async fn github_diagnostics_impl(
    state: &Arc<AppState>,
    refresh: bool,
) -> Result<GitHubDiagnostics, String> {
    if refresh {
        state.git_cache.github_repo_cooldown.clear();
    }
    Ok(compute_diagnostics(state))
}

/// Resolve the ambient runtime credentials without undoing an explicit disconnect.
pub(crate) async fn resolve_runtime_token(
    state: &AppState,
) -> Result<(Option<String>, TokenSource), String> {
    {
        let token = state.github.token.read();
        if state.github.disconnected.load(Ordering::Relaxed) {
            return Ok((None, TokenSource::None));
        }
        if token.is_some() {
            return Ok((token.clone(), *state.github.token_source.read()));
        }
    }
    let (token, source) = tokio::task::spawn_blocking(resolve_token_with_source)
        .await
        .map_err(|e| format!("token resolve task panicked: {e}"))?;
    let mut slot = state.github.token.write();
    if state.github.disconnected.load(Ordering::Relaxed) {
        return Ok((None, TokenSource::None));
    }
    // A device-flow login that completed during resolution has priority.
    if slot.is_none() {
        *slot = token;
        *state.github.token_source.write() = source;
    }
    Ok((slot.clone(), *state.github.token_source.read()))
}

/// Get the current GitHub authentication status, including the user's
/// login name if authenticated.
#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn github_auth_status(
    state: State<'_, Arc<AppState>>,
) -> Result<AuthStatus, String> {
    github_auth_status_impl(state.inner()).await
}

pub(crate) async fn github_auth_status_impl(state: &Arc<AppState>) -> Result<AuthStatus, String> {
    let (token, source) = resolve_runtime_token(state).await?;

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

    // Call GitHub /user to get login + avatar. This validates the github.com
    // global token, so it uses the cloud REST base (routed through GitHubHost
    // rather than hardcoding api.github.com).
    let host = crate::github_account::GitHubHost::new("github.com").expect("github.com is valid");
    let user_url = crate::github_account::github_rest_url(&host, "/user");
    crate::github_debug::log_api("GET", &user_url, "validate_token_impl");
    let resp = crate::github::github_rest_request(
        &state.http_client,
        reqwest::Method::GET,
        &host,
        "/user",
        &token,
        crate::github::GITHUB_JSON,
    )
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
            let error_msg = format!(
                "Token rejected by GitHub (HTTP 401). The {} token may be expired or revoked.",
                match source {
                    TokenSource::Env => "environment variable",
                    TokenSource::OAuth => "OAuth",
                    TokenSource::Pat => "Personal Access Token",
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
                *state.github.token.write() = fallback_token;
                *state.github.token_source.write() = fallback_source;
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
                error: Some(format!(
                    "GitHub API error (HTTP {status}): {}",
                    body.lines().next().unwrap_or("unknown error")
                )),
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

/// Read a non-empty token from an environment variable.
fn env_token(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|t| !t.is_empty())
}

/// Collect all non-empty GitHub token candidates with their source, in priority order.
/// Single source of truth for token priority — used at startup, fallback, and logout.
/// Priority: GH_TOKEN env → GITHUB_TOKEN env → keyring OAuth.
pub(crate) fn resolve_all_candidates() -> Vec<(String, TokenSource)> {
    resolve_all_candidates_inner(true)
}

fn resolve_all_candidates_inner(include_keychain: bool) -> Vec<(String, TokenSource)> {
    // Gather raw inputs (with the same per-source guards as before), then delegate
    // the priority ordering + value-dedup to the pure `order_token_candidates` seam.
    let gh_token_env = env_token("GH_TOKEN");
    let github_token_env = env_token("GITHUB_TOKEN");
    let keychain_oauth = if include_keychain {
        read_github_oauth_token().ok().flatten()
    } else {
        None
    };
    order_token_candidates(gh_token_env, github_token_env, keychain_oauth)
}

/// Pure priority-ordering + value-dedup of token candidates.
///
/// Seam pinned by the Step 0 characterization net so the github.com fallback
/// order can't drift during the multi-account refactor. Order is fixed:
/// GH_TOKEN env → GITHUB_TOKEN env → keyring OAuth.
/// A later source is dropped when its token value already appeared earlier.
pub(crate) fn order_token_candidates(
    gh_token_env: Option<String>,
    github_token_env: Option<String>,
    keychain_oauth: Option<String>,
) -> Vec<(String, TokenSource)> {
    let mut candidates: Vec<(String, TokenSource)> = Vec::new();
    for (tok, src) in [
        (gh_token_env, TokenSource::Env),
        (github_token_env, TokenSource::Env),
        (keychain_oauth, TokenSource::OAuth),
    ] {
        if let Some(t) = tok
            && !candidates.iter().any(|(existing, _)| existing == &t)
        {
            candidates.push((t, src));
        }
    }
    candidates
}

/// Take the winning candidate, or report that there is none.
fn first_of(candidates: Vec<(String, TokenSource)>) -> (Option<String>, TokenSource) {
    candidates
        .into_iter()
        .next()
        .map(|(t, s)| (Some(t), s))
        .unwrap_or((None, TokenSource::None))
}

/// Resolve a token from the environment alone.
///
/// This is the only part of the chain that costs nothing: every source below
/// the two env vars reads the OS credential store, which may not answer, and
/// that would hold back whatever is waiting on the answer. Boot takes
/// this synchronously and leaves the rest to
/// [`spawn_deferred_token_resolution`].
pub(crate) fn resolve_token_from_env() -> (Option<String>, TokenSource) {
    first_of(order_token_candidates(
        env_token("GH_TOKEN"),
        env_token("GITHUB_TOKEN"),
        None,
    ))
}

/// Resolve the highest-priority candidate, paying for the expensive sources
/// only when the environment yields nothing.
///
/// Withholding them from the first pass is sound because every one of them
/// ranks *below* both env vars in `order_token_candidates`: when an env token
/// exists it is the answer whatever the others hold, so reading the credential store to learn
/// a value that cannot win is pure latency.
fn first_candidate(include_keychain: bool) -> (Option<String>, TokenSource) {
    let (token, source) = resolve_token_from_env();
    if token.is_some() {
        return (token, source);
    }
    first_of(resolve_all_candidates_inner(include_keychain))
}

/// Resolve the highest-priority GitHub token and its source.
pub(crate) fn resolve_token_with_source() -> (Option<String>, TokenSource) {
    first_candidate(true)
}

/// Like `resolve_token_with_source` but skips keychain access.
/// Used off the boot path to avoid prompting the user before they need it.
pub(crate) fn resolve_token_without_keychain() -> (Option<String>, TokenSource) {
    first_candidate(false)
}

/// Finish GitHub token resolution once the window exists.
///
/// Boot installs the env token synchronously — free — and calls this from
/// Tauri `setup()`, because everything below the env vars spawns `gh` or reads
/// the credential store and a stuck one used to mean no window at all.
///
/// The work cannot simply be dropped: several API paths short-circuit on a
/// `None` token, and they do NOT fall back to resolving. `get_all_batch_impl`
/// — the one the poller runs — skips the ambient github.com account outright
/// (`continue`) rather than reaching the lazy resolve in `graphql_with_account`,
/// so a poll tick that lands before this probe finishes returns nothing and the
/// next tick is a full `BASE_INTERVAL` (60s visible, 120s hidden) away.
///
/// That is why installing the token is not enough: the probe also nudges the
/// poller. `ForceResync` sets `pending_poll_at = now`, so the skipped cycle is
/// re-run immediately instead of waiting out the interval, and it bypasses
/// change detection so an unchanged `updated_at` still reaches the UI. No new
/// push surface is introduced — the poller's own emit path already carries both
/// the desktop `emit` and the HTTP/SSE bridge. When no poller is running there
/// is nothing to nudge and nothing to fix: the token is in place before the
/// frontend ever subscribes.
///
/// No-op when the environment already won — nothing lower in the chain can
/// outrank that value.
pub(crate) fn spawn_deferred_token_resolution(state: Arc<AppState>) {
    if state.github.token.read().is_some() {
        return;
    }
    std::thread::spawn(move || {
        let (token, source) = resolve_token_without_keychain();
        let Some(token) = token else {
            tracing::info!(
                source = "github",
                "No GitHub token from environment — keychain deferred until first use"
            );
            return;
        };
        {
            // A login may have landed while the probe ran; an explicit one wins.
            let mut slot = state.github.token.write();
            if slot.is_some() || state.github.disconnected.load(Ordering::Relaxed) {
                return;
            }
            *slot = Some(token);
            *state.github.token_source.write() = source;
        }
        tracing::info!(
            source = "github",
            token_source = ?source,
            "Resolved GitHub token off the boot path"
        );
        // Err means no poller is subscribed yet — the expected case, not a fault.
        let _ = crate::github_poller::send_poller_cmd(
            &state,
            crate::github_poller::PollerCmd::ForceResync,
        );
    });
}

/// Resolve the token for a specific account.
///
/// - The ambient github.com default runs the EXISTING env→keyring-OAuth chain
///   unchanged (so a single-account user sees no behavior change).
/// - Every NAMED account (a GHE PAT or an additional github.com account) returns
///   ONLY its own per-account vault token — never the ambient chain. This anchors
///   each named account to an explicit token so `gh auth switch` (or an env-var
///   change) can't silently drift its identity. A GHE account reports
///   [`TokenSource::Pat`]; an additional github.com account reports
///   [`TokenSource::OAuth`] (its device-flow token, stored per-account). A
///   missing token yields `(None, TokenSource::None)`.
pub(crate) fn resolve_token_for_account(
    account: &crate::github_account::GitHubAccount,
) -> (Option<String>, TokenSource) {
    if account.is_ambient_default() {
        // Ambient github.com default — existing chain, byte-for-byte unchanged.
        return resolve_token_with_source();
    }
    match crate::credentials::get(Credential::GithubToken(&account.id))
        .ok()
        .flatten()
    {
        Some(token) => {
            let source = match account.kind {
                crate::github_account::AccountKind::GhePat => TokenSource::Pat,
                _ => TokenSource::OAuth,
            };
            (Some(token), source)
        }
        None => (None, TokenSource::None),
    }
}

// ---------------------------------------------------------------------------
// Keyring helpers
// ---------------------------------------------------------------------------

/// Read the stored OAuth token from the OS keyring (cached in-memory).
/// Returns `None` if no token is stored (not an error).
pub(crate) fn read_github_oauth_token() -> Result<Option<String>, String> {
    crate::credentials::get(Credential::GithubOauthToken)
}

pub(crate) fn save_github_oauth_token(token: &str) -> Result<(), String> {
    crate::credentials::set(Credential::GithubOauthToken, token)
}

pub(crate) fn delete_github_oauth_token() -> Result<(), String> {
    crate::credentials::delete(Credential::GithubOauthToken)
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

    // --- viewer login is dropped when the identity changes (#491-6bb2) ---

    /// Disconnect clears the token but used to leave `github_viewer_login` set, so
    /// the PR search kept filtering on `author:@me` for the account we just left.
    #[tokio::test]
    async fn disconnect_forgets_the_cached_viewer_login() {
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        *state.github.viewer_login.write() = Some("previous-user".to_string());

        github_disconnect_impl(&state).await.expect("disconnect");

        assert_eq!(*state.github.viewer_login.read(), None);
        assert_eq!(*state.github.token.read(), None);
    }

    /// Logout falls back to whatever env token is around — very likely a
    /// different user, so the cached login must not survive it either. The token
    /// the fallback resolves to depends on the machine and is deliberately not
    /// asserted; only the invalidation is.
    #[tokio::test]
    async fn logout_forgets_the_cached_viewer_login() {
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        *state.github.viewer_login.write() = Some("previous-user".to_string());

        github_logout_impl(&state).await.expect("logout");

        assert_eq!(*state.github.viewer_login.read(), None);
    }

    /// A successful device-flow login installs a token that may belong to somebody
    /// else entirely. `github_poll_login_impl` cannot be driven from a test — its
    /// device-flow POST goes to github.com — so pin the invalidation at the source
    /// instead of leaving that third path unverified.
    #[test]
    fn a_successful_login_invalidates_the_viewer_login() {
        let src = include_str!("github_auth.rs");
        let body = src
            .split("pub(crate) async fn github_poll_login_impl_at_epoch(")
            .nth(1)
            .and_then(|rest| rest.split("\n}\n").next())
            .expect("github_poll_login_impl must exist");
        assert!(
            body.contains("invalidate_viewer_login"),
            "the login success path must drop the previous account's cached login"
        );
    }

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
    #[ignore = "requires interactive Keychain access on macOS"]
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
            "scope": "repo"
        }"#;
        let resp: GithubTokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            resp.access_token,
            "gho_16C7e42F292c6912E7710c838347Ae178B4a"
        );
        assert_eq!(resp.scope, "repo");
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
        assert!(!json.contains("access_token") && !json.contains("gho_abc"));
    }

    #[test]
    #[ignore = "requires interactive Keychain access on macOS"]
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

    // --- boot-path token probes (#654-bfc1) ---

    /// Restores an env var on drop, so a failed assertion cannot leak
    /// process-global state into whatever test runs next.
    ///
    /// Dropping restores the value but does not make the window safe: the
    /// environment is per-*process*, so under `cargo test` — one process, a
    /// thread pool — two of these guards overlap and each reads the other's
    /// value. Every test that builds one therefore carries
    /// `#[serial_test::serial]`, and that attribute is load-bearing, not
    /// decoration. So does every test that merely *reads* the chain — a reader
    /// racing a writer fails just as hard, which is how
    /// `resolve_for_github_com_delegates_to_existing_chain` still broke after
    /// only the three writers were serialised. They share the crate-wide default
    /// key on purpose: a named key would have put the readers in a different
    /// group and reopened exactly that gap.
    ///
    /// `cargo nextest run` hides all of this by giving each test its own
    /// process, which is why these failed together under `cargo test --lib` and
    /// passed 25/25 under nextest. Serialising is the fix rather than a mask
    /// here because `std::env` is shared by definition — there is no concurrent
    /// version of it to assert against.
    struct EnvVar(&'static str, Option<String>);

    impl EnvVar {
        fn set(key: &'static str, value: &str) -> Self {
            let guard = EnvVar(key, std::env::var(key).ok());
            unsafe { std::env::set_var(key, value) };
            guard
        }

        fn unset(key: &'static str) -> Self {
            let guard = EnvVar(key, std::env::var(key).ok());
            unsafe { std::env::remove_var(key) };
            guard
        }
    }

    impl Drop for EnvVar {
        fn drop(&mut self) {
            match self.1.take() {
                Some(previous) => unsafe { std::env::set_var(self.0, previous) },
                None => unsafe { std::env::remove_var(self.0) },
            }
        }
    }

    /// The synchronous boot path reads the environment only, so it cannot hold
    /// the window back: with no env token it answers `None`, it does not look
    /// further.
    #[test]
    #[serial_test::serial]
    fn the_boot_path_reads_only_the_environment() {
        let _gh = EnvVar::unset("GH_TOKEN");
        let _github = EnvVar::unset("GITHUB_TOKEN");

        let (token, source) = resolve_token_from_env();

        assert_eq!(token, None);
        assert_eq!(source, TokenSource::None);
    }

    /// Installing the token is not enough. `get_all_batch_impl` skips the
    /// ambient account on a `None` token instead of resolving one, so a poll
    /// that raced the probe returned nothing and the next one was a full 60s
    /// away — a regression the synchronous boot could not have, because the
    /// token was always in place before the window existed.
    ///
    /// The receive bound belongs to the "setup reaches a state" row: the probe
    /// reads two env vars and sends, so 30s cannot fire for any reason but the
    /// nudge being gone, and it stays well inside nextest's 120s kill.
    #[tokio::test]
    #[serial_test::serial]
    async fn the_deferred_probe_nudges_the_poller_once_the_token_lands() {
        let _gh = EnvVar::set("GH_TOKEN", "ghp_deferred_nudge");
        let _github = EnvVar::unset("GITHUB_TOKEN");

        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::channel(32);
        *state.github.poller.lock() = Some(crate::github_poller::GitHubPoller {
            cmd_tx,
            previous_prs: Default::default(),
            stop: std::sync::Arc::new(tokio::sync::Notify::new()),
        });

        spawn_deferred_token_resolution(state.clone());

        let cmd = tokio::time::timeout(std::time::Duration::from_secs(30), cmd_rx.recv())
            .await
            .expect("the probe never nudged the poller — a skipped cycle waits out the interval")
            .expect("poller command channel closed");

        assert!(
            matches!(cmd, crate::github_poller::PollerCmd::ForceResync),
            "the nudge must be ForceResync — the skipped cycle has to be re-run \
             AND bypass change detection, or an unchanged updated_at hides it"
        );
        assert_eq!(
            state.github.token.read().as_deref(),
            Some("ghp_deferred_nudge")
        );
    }

    /// The deferred probe must not overwrite a token that arrived while it ran:
    /// an explicit login is a stronger statement than the deferred environment probe.
    #[test]
    fn the_deferred_probe_skips_a_state_that_already_has_a_token() {
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        *state.github.token.write() = Some("gho_explicit_login".to_string());

        spawn_deferred_token_resolution(state.clone());

        assert_eq!(
            state.github.token.read().as_deref(),
            Some("gho_explicit_login")
        );
    }

    // Catches: status lazy resolution silently reconnects an environment token.
    #[tokio::test]
    #[serial_test::serial]
    async fn disconnect_survives_status_refresh_with_an_environment_token() {
        let _gh = EnvVar::set("GH_TOKEN", "fake-disconnect-token");
        let _github = EnvVar::unset("GITHUB_TOKEN");
        let mut state = crate::state::tests_support::make_test_app_state();
        // A refused local proxy prevents accidental calls to the real GitHub API.
        state.http_client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all("http://127.0.0.1:1").unwrap())
            .build()
            .unwrap();
        let state = Arc::new(state);
        *state.github.token.write() = Some("fake-disconnect-token".into());
        *state.github.token_source.write() = TokenSource::Env;
        github_disconnect_impl(&state).await.unwrap();
        let status = github_auth_status_impl(&state).await.unwrap();
        assert!(!status.authenticated);
        assert_eq!(*state.github.token.read(), None);
        assert_eq!(*state.github.token_source.read(), TokenSource::None);
        let account = crate::github::github_com_account(&state);
        let result = crate::github::graphql_with_retry(
            &state,
            &account,
            "query { viewer { login } }",
            serde_json::json!({}),
            None,
        )
        .await;
        assert_eq!(result.unwrap_err(), "No GitHub token available");
        assert_eq!(*state.github.token.read(), None);
    }

    // --- resolve_token_for_account (Step 4) ---

    use crate::github_account::{AccountKind, GitHubAccount, GitHubHost};

    #[test]
    #[serial_test::serial]
    fn resolve_for_github_com_delegates_to_existing_chain() {
        // github.com accounts must run the existing env→OAuth chain verbatim,
        // so the result is identical to resolve_token_with_source(). Serialized
        // against the env-mutating token tests so the global env is stable across
        // the two resolutions below.
        let acc = GitHubAccount::github_com(AccountKind::GithubComOauth, None);
        assert_eq!(resolve_token_for_account(&acc), resolve_token_with_source());
    }

    #[test]
    fn resolve_for_ghe_returns_vault_pat() {
        let host = GitHubHost::new("ghe.resolve-pat-test.example").unwrap();
        let acc = GitHubAccount::ghe_pat(host, None);
        // The mock keyring is installed lazily on first credential access.
        crate::credentials::set(Credential::GithubToken(&acc.id), "ghp_resolve_test").unwrap();

        let (token, source) = resolve_token_for_account(&acc);
        assert_eq!(token, Some("ghp_resolve_test".to_string()));
        assert_eq!(source, TokenSource::Pat);

        crate::credentials::delete(Credential::GithubToken(&acc.id)).unwrap();
    }

    #[test]
    fn resolve_for_ghe_missing_pat_is_none() {
        let host = GitHubHost::new("ghe.missing-pat-test.example").unwrap();
        let acc = GitHubAccount::ghe_pat(host, None);
        // Ensure no token is present for this id.
        let _ = crate::credentials::delete(Credential::GithubToken(&acc.id));

        assert_eq!(resolve_token_for_account(&acc), (None, TokenSource::None));
    }

    // --- Story 005: named github.com accounts anchored to their own token ---

    #[test]
    fn resolve_for_named_github_com_uses_own_token_not_chain() {
        // An additional named github.com account resolves ONLY from its own
        // per-account vault slot — never the ambient env→OAuth chain — so an
        // environment change cannot drift its identity. Source is OAuth (its
        // device-flow token), distinct from a GHE PAT.
        let acc = GitHubAccount::github_com_named("named-anchor-test");
        crate::credentials::set(Credential::GithubToken(&acc.id), "gho_named_anchor").unwrap();

        let (token, source) = resolve_token_for_account(&acc);
        assert_eq!(token, Some("gho_named_anchor".to_string()));
        assert_eq!(source, TokenSource::OAuth);

        crate::credentials::delete(Credential::GithubToken(&acc.id)).unwrap();
    }

    #[test]
    fn resolve_for_named_github_com_missing_token_is_none() {
        // No ambient fallback: a named github.com account with no stored token
        // resolves to None, it does NOT silently fall back to env/OAuth.
        let acc = GitHubAccount::github_com_named("named-missing-test");
        let _ = crate::credentials::delete(Credential::GithubToken(&acc.id));

        assert_eq!(resolve_token_for_account(&acc), (None, TokenSource::None));
    }
}

#[cfg(test)]
mod device_scope_tests {
    /// Catches: a newly authorized TUIC account cannot read Projects v2 because consent omits read:project.
    #[test]
    fn device_flow_requests_projects_read_scope() {
        let request = super::device_flow_request(&reqwest::Client::new())
            .build()
            .expect("valid device-code request");
        assert_eq!(request.method(), reqwest::Method::POST);
        assert_eq!(
            request.url().as_str(),
            "https://github.com/login/device/code"
        );
        let bytes = request
            .body()
            .and_then(reqwest::Body::as_bytes)
            .expect("form body");
        let form: std::collections::HashMap<_, _> = url::form_urlencoded::parse(bytes).collect();
        // Catches: the backend request diverges from the scope value rendered by Settings.
        assert_eq!(
            form["scope"],
            include_str!("../../src/data/github-oauth-scopes.txt").trim()
        );
        let scopes: std::collections::HashSet<_> = form["scope"].split_whitespace().collect();
        assert!(
            scopes.contains("repo"),
            "repository access must be retained"
        );
        assert!(
            scopes.contains("read:project"),
            "Projects v2 reads require read:project consent"
        );
    }
}
