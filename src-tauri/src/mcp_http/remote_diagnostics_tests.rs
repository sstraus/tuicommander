//! Remote diagnostics and WebView recovery for authenticated callers (#2658-7301).
//!
//! The owner's phone reaches the desktop over the token-authenticated TCP
//! router. JS diagnostics and native WebView recovery must work for that
//! caller, while the private secret form stays uninspectable and desktop tab
//! publication stays a desktop-only producer role.

use super::{build_router, mcp_transport};
use crate::AppState;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Method, Request, StatusCode, header};
use std::net::SocketAddr;
use std::sync::Arc;
use tower::ServiceExt;

const TOKEN: &str = "remote-diagnostics-token";
const PHONE: [u8; 4] = [100, 64, 0, 3];

fn state() -> Arc<AppState> {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    *state.session_token.write() = TOKEN.into();
    // The owner enabled the default-denied diagnostics capability; this story
    // changes who may call `debug`, not whether the owner enabled it.
    state.config.write().mcp_capabilities.denied.clear();
    state
}

fn phone_request(method: Method, path: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(format!("{path}?token={TOKEN}"))
        .header(header::HOST, "100.64.0.2:9876")
        .header(header::CONTENT_TYPE, "application/json")
        .extension(ConnectInfo(SocketAddr::from((PHONE, 12345))))
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn send(state: Arc<AppState>, req: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = build_router(state, true, true).oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

async fn phone_mcp_debug(state: &Arc<AppState>, args: serde_json::Value) -> serde_json::Value {
    mcp_transport::handle_mcp_tool_call_with_context(
        state,
        // The phone passed token authentication, as `phone_request` does.
        super::guards::CallerAuthority::from_request(
            SocketAddr::from((PHONE, 12345)),
            Some(super::guards::UserAuthenticated),
        ),
        "debug",
        &args,
        None,
        None,
    )
    .await
}

fn open_secret_form(state: &AppState) {
    let form = crate::secrets::Form::request(
        vec![crate::secrets::Field {
            name: "TOKEN".into(),
            kind: crate::secrets::FieldKind::Password,
            display: None,
        }],
        "remote diagnostics test".into(),
    )
    .unwrap();
    state.secrets.open(form).unwrap();
    assert!(state.secrets.tools_blocked());
}

/// Catches: the phone cannot inspect or recover a blocked desktop window,
/// because the HTTP and MCP debug surfaces refuse every non-loopback caller
/// even after token authentication. The test state has no WebView, so a
/// handler that runs answers with the absent-window error, never a refusal.
#[cfg(feature = "desktop")]
#[tokio::test]
async fn authenticated_remote_diagnoses_and_recovers_webview() {
    let state = state();

    let (status, body) = send(
        state.clone(),
        phone_request(Method::POST, "/debug/invoke_js", r#"{"script":"return 1"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "invoke_js: {body}");
    assert_eq!(body["error"], "AppHandle not initialized");

    let (status, body) = send(
        state.clone(),
        phone_request(Method::POST, "/debug/reload_webview", ""),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "reload_webview: {body}");
    assert_eq!(body["error"], "main window not found");

    let body = phone_mcp_debug(
        &state,
        serde_json::json!({"action": "invoke_js", "script": "return 1"}),
    )
    .await;
    assert_eq!(body["error"], "AppHandle not initialized", "MCP invoke_js");

    let body = phone_mcp_debug(&state, serde_json::json!({"action": "frame_workload_dump"})).await;
    assert_eq!(
        body["error"], "AppHandle not initialized",
        "MCP frame_workload_dump"
    );
}

/// Catches: opening debug to authenticated remote callers also exposing the
/// private secret form to JS inspection, or giving a remote caller the
/// desktop producer identity for open-tab publication.
#[tokio::test]
async fn remote_debug_cannot_inspect_secret_form_or_publish_desktop_tabs() {
    let blocked = "Agent inspection is disabled while a private secret form is open";
    let state = state();
    open_secret_form(&state);

    let (status, body) = send(
        state.clone(),
        phone_request(
            Method::POST,
            "/debug/invoke_js",
            r#"{"script":"return document.body.innerHTML"}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "invoke_js: {body}");
    assert_eq!(body["error"], blocked);

    for args in [
        serde_json::json!({"action": "invoke_js", "script": "return 1"}),
        serde_json::json!({"action": "frame_workload_dump"}),
    ] {
        let body = phone_mcp_debug(&state, args.clone()).await;
        assert_eq!(body["error"], blocked, "MCP {args}");
    }

    let (status, body) = send(
        state,
        phone_request(Method::PUT, "/config/open-markdown-tabs", r#"{"tabs":[]}"#),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "publication: {body}");
}
