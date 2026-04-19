//! Ephemeral localhost HTTP server that receives the OAuth authorization
//! callback from the browser.
//!
//! Binds to `127.0.0.1:0` (OS-assigned port), serves a single
//! `/oauth/callback` endpoint, captures `(state, code)`, calls
//! [`OAuthFlowManager::complete_flow`], and triggers
//! [`UpstreamRegistry::on_oauth_complete`] to resume the upstream connection.
//!
//! The server shuts down automatically after the first successful callback
//! or after the flow timeout (5 minutes).

use std::sync::Arc;

use anyhow::{anyhow, Result};
use axum::extract::Query;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use std::net::SocketAddr;

use super::flow::OAuthFlowManager;
use crate::mcp_proxy::registry::UpstreamRegistry;

/// Build the redirect URI for a callback server bound to the given port.
pub(crate) fn redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}/oauth/callback")
}

#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html><head><title>TUICommander</title>
<style>body{font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#1a1a2e;color:#e0e0e0}
.card{text-align:center;padding:2rem;border-radius:12px;background:#16213e;box-shadow:0 4px 20px rgba(0,0,0,.3)}
h1{color:#4ecca3;margin-bottom:.5rem}p{color:#a0a0b0}</style></head>
<body><div class="card"><h1>&#10003; Authentication complete</h1><p>You can close this tab and return to TUICommander.</p></div></body></html>"#;

const ERROR_HTML: &str = r#"<!DOCTYPE html>
<html><head><title>TUICommander</title>
<style>body{font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#1a1a2e;color:#e0e0e0}
.card{text-align:center;padding:2rem;border-radius:12px;background:#16213e;box-shadow:0 4px 20px rgba(0,0,0,.3)}
h1{color:#e74c3c;margin-bottom:.5rem}p{color:#a0a0b0}</style></head>
<body><div class="card"><h1>&#10007; Authentication failed</h1><p>Check the TUICommander logs for details.</p></div></body></html>"#;

/// Handle returned by [`spawn`] — dropping triggers graceful shutdown.
pub(crate) struct CallbackServer {
    pub(crate) port: u16,
    _shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

/// Spawn the callback server. It completes the OAuth flow and resumes the
/// upstream connection automatically when the browser redirects back.
pub(crate) async fn spawn(
    flow_manager: Arc<OAuthFlowManager>,
    registry: Arc<UpstreamRegistry>,
) -> Result<CallbackServer> {
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let done_notify = Arc::new(tokio::sync::Notify::new());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let mgr = flow_manager;
    let reg = registry;
    let done = done_notify.clone();

    let app = Router::new().route(
        "/oauth/callback",
        get(move |Query(params): Query<CallbackParams>| {
            let mgr = mgr.clone();
            let reg = reg.clone();
            let done = done.clone();
            async move {
                let html = match handle_callback(mgr, reg, params).await {
                    Ok(()) => SUCCESS_HTML,
                    Err(e) => {
                        tracing::error!(target: "mcp_oauth", error = %e, "OAuth callback failed");
                        ERROR_HTML
                    }
                };
                done.notify_one();
                Html(html).into_response()
            }
        }),
    );

    tokio::spawn(async move {
        let server = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = shutdown_rx => {}
                _ = done_notify.notified() => {
                    // Give the browser time to receive the HTML response.
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        });
        if let Err(e) = server.await {
            tracing::warn!(target: "mcp_oauth", error = %e, "callback server exited with error");
        }
    });

    tracing::info!(target: "mcp_oauth", port, "OAuth callback server listening");

    Ok(CallbackServer {
        port,
        _shutdown_tx: shutdown_tx,
    })
}

async fn handle_callback(
    manager: Arc<OAuthFlowManager>,
    registry: Arc<UpstreamRegistry>,
    params: CallbackParams,
) -> Result<()> {
    if let Some(err) = params.error {
        let desc = params.error_description.unwrap_or_default();
        // Try to extract the upstream name from state to rollback
        if let Some(state) = &params.state
            && let Some(name) = manager.upstream_name_for_state(state)
        {
            registry.rollback_authenticating(&name);
        }
        return Err(anyhow!(
            "Authorization server returned error: {err}{}", if desc.is_empty() { String::new() } else { format!(" ({desc})") }
        ));
    }

    let code = params
        .code
        .ok_or_else(|| anyhow!("Missing 'code' parameter in callback"))?;
    let state = params
        .state
        .ok_or_else(|| anyhow!("Missing 'state' parameter in callback"))?;

    let (upstream_name, _tokens) = manager.complete_flow(&state, &code).await?;

    registry
        .on_oauth_complete(&upstream_name)
        .await
        .map_err(|e| anyhow!("{e}"))?;

    tracing::info!(target: "mcp_oauth", upstream = %upstream_name, "OAuth flow completed via callback server");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    #[test]
    fn redirect_uri_format() {
        let uri = redirect_uri(12345);
        assert_eq!(uri, "http://127.0.0.1:12345/oauth/callback");
    }

    #[tokio::test]
    async fn spawn_binds_to_random_port() {
        let mgr = Arc::new(OAuthFlowManager::new(Arc::new(Semaphore::new(1))));
        let reg = Arc::new(UpstreamRegistry::new());
        let server = spawn(mgr, reg).await.unwrap();
        assert!(server.port > 0);
    }
}
