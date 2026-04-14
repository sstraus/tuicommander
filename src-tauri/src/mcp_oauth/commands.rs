//! Tauri commands that bridge the frontend and the OAuth flow orchestrator.
//!
//! Two commands are exposed:
//!
//! - [`start_mcp_upstream_oauth`] — kicks off a flow for a named upstream
//!   (status → `Authenticating`, `McpOAuthStart` event emitted, browser URL
//!   returned so the frontend can open it via `tauri-plugin-opener`).
//! - [`mcp_oauth_callback`] — invoked by the deep-link handler with the
//!   authorization `code` and `state` from the browser redirect. Exchanges
//!   the code, persists tokens, and resumes the upstream via
//!   [`UpstreamRegistry::on_oauth_complete`].

use std::sync::Arc;
use tauri::State;

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
#[tauri::command]
pub(crate) async fn start_mcp_upstream_oauth(
    state: State<'_, Arc<AppState>>,
    name: String,
) -> Result<StartOAuthResponse, String> {
    let registry = state.mcp_upstream_registry.clone();
    let flow_mgr = state.oauth_flow_manager.clone();

    // Pull the upstream's config and transition it to Authenticating.
    let (server_url, auth) = {
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
        let auth = entry
            .config
            .auth
            .clone()
            .ok_or_else(|| format!("Upstream '{name}' has no auth configuration"))?;
        registry.set_authenticating(&name);
        (server_url, auth)
    };

    // `tuic://` is the deep-link the OS hands back to the running Tauri app.
    let redirect_uri = "tuic://oauth-callback".to_string();
    let outcome = flow_mgr
        .start_flow(&name, &server_url, &auth, &redirect_uri)
        .await
        .map_err(|e| e.to_string())?;

    // Emit the event so other listeners (e.g. the settings panel) can react.
    registry.emit_oauth_start(&name, &outcome.authorization_url);

    Ok(StartOAuthResponse {
        authorization_url: outcome.authorization_url,
        state: outcome.state,
    })
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
