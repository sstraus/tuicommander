//! Tauri commands that bridge the frontend and the OAuth flow orchestrator.
//!
//! Commands exposed:
//!
//! - [`start_mcp_upstream_oauth`] — kicks off a flow for a named upstream
//!   (status → `Authenticating`, starts a localhost callback server, emits
//!   `McpOAuthStart` event, and returns the browser URL so the frontend can
//!   open it via `tauri-plugin-opener`). The callback server handles the
//!   authorization response autonomously — no deep-link handler needed.
//! - [`mcp_oauth_callback`] — manual fallback invoked by the deep-link
//!   handler with the authorization `code` and `state`.
//! - [`cancel_mcp_upstream_oauth`] — cancels an in-progress flow.

use std::sync::Arc;
use tauri::State;

use crate::mcp_oauth::callback_server;
use crate::mcp_oauth::dcr::{register_client, DcrRequest};
use crate::mcp_oauth::discovery::{discover_auth_server, discover_auth_server_relaxed, discover_protected_resource};
use crate::mcp_upstream_config::UpstreamAuth;
use crate::state::AppState;

/// Output of [`start_mcp_upstream_oauth`] — the frontend opens
/// `authorization_url` in the user's browser.
#[derive(Debug, serde::Serialize)]
pub(crate) struct StartOAuthResponse {
    pub(crate) authorization_url: String,
    pub(crate) state: String,
}

/// Start the OAuth flow for the given upstream. Returns the authorization URL
/// the frontend must open in the user's browser.
///
/// Internally starts a localhost HTTP callback server on a random port. The
/// browser redirects to `http://127.0.0.1:{port}/oauth/callback` after the
/// user grants consent. The callback server completes the token exchange and
/// resumes the upstream connection automatically.
#[tauri::command]
pub(crate) async fn start_mcp_upstream_oauth(
    state: State<'_, Arc<AppState>>,
    name: String,
) -> Result<StartOAuthResponse, String> {
    let registry = state.mcp_upstream_registry.clone();
    let flow_mgr = state.oauth_flow_manager.clone();

    // Ensure the upstream is in the live registry (it might be missing if
    // the app was restarted after the config was saved but before the entry
    // was loaded, or if it was added while the app was running via a
    // different code path).
    if registry.entry(&name).is_none() {
        tracing::warn!(source = "mcp_oauth", %name, "upstream not in registry, loading from config");
        let config: crate::mcp_upstream_config::UpstreamMcpConfig =
            crate::config::load_json_config(crate::mcp_upstream_config::UPSTREAMS_FILE);
        let server = config
            .servers
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| format!("Upstream '{name}' not found in config or registry"))?;
        registry.connect_upstream(server, None).await.map_err(|e| {
            format!("Failed to register upstream '{name}' from config: {e}")
        })?;
        // Give the initialize task a moment to run and detect NeedsOAuth
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Pull the upstream's config and transition it to Authenticating.
    let (server_url, existing_auth) = {
        let entry = registry
            .entry(&name)
            .ok_or_else(|| format!("Unknown upstream '{name}'"))?;
        let server_url = match &entry.config.transport {
            crate::mcp_upstream_config::UpstreamTransport::Http { url } => url.clone(),
            crate::mcp_upstream_config::UpstreamTransport::Stdio { .. } => {
                return Err(format!(
                    "Upstream '{name}' uses stdio transport — OAuth is only supported for HTTP"
                ));
            }
        };
        let auth = entry.config.auth.clone();
        registry.set_authenticating(&name);
        (server_url, auth)
    };

    // Start the localhost callback server — the redirect_uri is dynamic
    // based on the OS-assigned port.
    let cb_server = callback_server::spawn(flow_mgr.clone(), registry.clone())
        .await
        .map_err(|e| {
            registry.rollback_authenticating(&name);
            format!("Failed to start OAuth callback server: {e}")
        })?;
    let redirect_uri = callback_server::redirect_uri(cb_server.port);

    // Everything after set_authenticating is fallible — rollback on error so
    // the UI returns to "needs_auth" (retryable) instead of stuck on "authenticating".
    let result = async {
        // If no auth config, attempt DCR (RFC 7591) to obtain a client_id.
        let auth = match existing_auth {
            Some(a) => a,
            None => {
                let http_client = reqwest::Client::new();

                // Try RFC 9728 (Protected Resource Metadata) first; fall back
                // to RFC 8414 on the resource server's origin when unavailable
                // (e.g. Atlassian exposes only /.well-known/oauth-authorization-server).
                let as_meta = if let Ok(pr_meta) = discover_protected_resource(&http_client, &server_url).await {
                    let as_url = &pr_meta.authorization_servers[0];
                    discover_auth_server(&http_client, as_url)
                        .await
                        .map_err(|e| format!("AS discovery failed for '{name}': {e}"))?
                } else {
                    let origin = url::Url::parse(&server_url)
                        .map_err(|e| format!("Invalid server URL '{server_url}': {e}"))?
                        .origin()
                        .unicode_serialization();
                    discover_auth_server_relaxed(&http_client, &origin)
                        .await
                        .map_err(|e| format!(
                            "OAuth discovery failed for '{name}': no RFC 9728 metadata \
                             and RFC 8414 fallback on {origin} also failed: {e}"
                        ))?
                };

                let reg_endpoint = as_meta.registration_endpoint.ok_or_else(|| {
                    format!(
                        "Upstream '{name}' requires OAuth but has no client_id configured \
                         and the authorization server does not support Dynamic Client Registration. \
                         Please configure a client_id manually."
                    )
                })?;

                let dcr_req = DcrRequest::for_tuicommander(&redirect_uri, None);
                let dcr_resp = register_client(&http_client, &reg_endpoint, &dcr_req)
                    .await
                    .map_err(|e| format!("Dynamic Client Registration failed for '{name}': {e}"))?;

                let auth = UpstreamAuth::OAuth2 {
                    client_id: dcr_resp.client_id,
                    client_secret: dcr_resp.client_secret,
                    scopes: vec![],
                    authorization_endpoint: Some(as_meta.authorization_endpoint.clone()),
                    token_endpoint: Some(as_meta.token_endpoint.clone()),
                };

                if let Err(e) = crate::mcp_upstream_config::update_upstream_auth(&name, auth.clone()) {
                    tracing::warn!(source = "mcp_oauth", %name, "Failed to persist DCR client_id: {e}");
                }

                auth
            }
        };

        let outcome = flow_mgr
            .start_flow(&name, &server_url, &auth, &redirect_uri)
            .await
            .map_err(|e| e.to_string())?;

        registry.emit_oauth_start(&name, &outcome.authorization_url);

        // Keep the callback server alive until the flow completes or times out.
        // The server handle is moved into a background task — dropping it would
        // shut down the listener before the browser can redirect back.
        let flow_mgr_bg = flow_mgr.clone();
        let state_nonce = outcome.state.clone();
        tokio::spawn(async move {
            let _keep_alive = cb_server;
            // Wait until the flow is consumed (complete or cancel) or 5 min timeout.
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            for _ in 0..150 {
                interval.tick().await;
                if flow_mgr_bg.upstream_name_for_state(&state_nonce).is_none() {
                    break;
                }
            }
        });

        Ok::<_, String>(StartOAuthResponse {
            authorization_url: outcome.authorization_url,
            state: outcome.state,
        })
    }.await;

    if result.is_err() {
        registry.rollback_authenticating(&name);
    }
    result
}

/// Finalize the OAuth flow after the browser redirects back with `code` and
/// `state`. On success, upstream tokens are persisted and the upstream is
/// transitioned back to `Connecting`.
#[tauri::command]
pub(crate) async fn mcp_oauth_callback(
    state: State<'_, Arc<AppState>>,
    code: String,
    oauth_state: String,
) -> Result<(), String> {
    let (upstream_name, _tokens) = state
        .oauth_flow_manager
        .complete_flow(&oauth_state, &code)
        .await
        .map_err(|e| e.to_string())?;

    state
        .mcp_upstream_registry
        .on_oauth_complete(&upstream_name)
        .await
}

/// Cancel any in-progress OAuth flows for the named upstream and transition
/// its status out of `Authenticating`. Safe to call even if no flow is
/// pending — it becomes a no-op.
#[tauri::command]
pub(crate) async fn cancel_mcp_upstream_oauth(
    state: State<'_, Arc<AppState>>,
    name: String,
) -> Result<(), String> {
    state.oauth_flow_manager.cancel_flows_for(&name);
    state.mcp_upstream_registry.cancel_authenticating(&name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Parameter contract tests — ensure the deep link handler's params line
    // up with what the Tauri command expects. Integration with the real
    // AppState is exercised indirectly via the upstream registry tests.
    // -----------------------------------------------------------------------

    #[test]
    fn start_oauth_response_serializes_expected_fields() {
        let resp = StartOAuthResponse {
            authorization_url: "https://as.example/authorize?x=1".into(),
            state: "nonce".into(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["authorization_url"], "https://as.example/authorize?x=1");
        assert_eq!(json["state"], "nonce");
    }
}
