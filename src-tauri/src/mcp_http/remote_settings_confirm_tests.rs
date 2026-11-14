//! Authenticated remote MCP follows the same settings proposal and confirmation flow as loopback.
use super::*;
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "remote-settings-fixture";

fn isolated() -> (tempfile::TempDir, impl Drop, Arc<AppState>) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
    let state = tests::test_state();
    *state.session_token.write() = TOKEN.into();
    state.config.write().services.auth.lan_auth_bypass = false;
    (dir, guard, state)
}

/// One `/mcp` POST from a public address, through the production auth layer.
async fn post(
    state: &Arc<AppState>,
    body: Value,
    session: Option<&str>,
    authenticated: bool,
) -> (StatusCode, Option<String>, Value) {
    let uri = if authenticated {
        format!("/mcp?token={TOKEN}")
    } else {
        "/mcp".to_owned()
    };
    let mut request = Request::post(uri)
        .header(header::HOST, "127.0.0.1:9876")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(session) = session {
        request = request.header("mcp-session-id", session);
    }
    let mut request = request.body(Body::from(body.to_string())).unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "203.0.113.7:4444".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = build_router(state.clone(), true, true)
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let session = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        session,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn initialize(state: &Arc<AppState>) -> String {
    let body = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-03-26","capabilities":{},
        "clientInfo":{"name":"phone-automation","version":"1.0"}}});
    let (status, session, _) = post(state, body, None, true).await;
    assert_eq!(status, StatusCode::OK);
    session.expect("initialize must issue a session id")
}

async fn call(state: &Arc<AppState>, session: &str, name: &str, arguments: Value) -> Value {
    let body = json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":name,"arguments":arguments}});
    let (status, _, reply) = post(state, body, Some(session), true).await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    let text = reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("tool reply has no text: {reply}"));
    serde_json::from_str(text).unwrap_or_else(|_| json!({"text": text}))
}

fn set(revision: &Value, username: &str) -> Value {
    json!({"action":"set","target":{"kind":"machine"},"store":"app",
        "path":"/services/auth/username","value":username,"expectedRevision":revision})
}

async fn revision(state: &Arc<AppState>, session: &str) -> Value {
    let read = call(
        state,
        session,
        "settings",
        json!({"action":"get","target":{"kind":"machine"},"store":"app",
            "paths":["/services/auth/username"]}),
    )
    .await;
    assert!(read["revision"].is_string(), "{read}");
    read["revision"].clone()
}

async fn propose(state: &Arc<AppState>, session: &str, username: &str) -> (Value, String) {
    let revision = revision(state, session).await;
    let proposed = call(state, session, "settings", set(&revision, username)).await;
    assert_eq!(
        proposed["error"]["code"], "confirmation_required",
        "{proposed}"
    );
    let id = proposed["error"]["proposalId"]
        .as_str()
        .unwrap_or_else(|| panic!("no proposal id: {proposed}"))
        .to_owned();
    (revision, id)
}

/// Run `ui confirm` and answer the real dialog request with `approved`.
async fn confirm(state: &Arc<AppState>, session: &str, id: &str, approved: bool) -> Value {
    let mut events = state.event_bus.subscribe();
    let host = state.clone();
    let (session, id) = (session.to_owned(), id.to_owned());
    let mut task = tokio::spawn(async move {
        call(
            &host,
            &session,
            "ui",
            json!({"action":"confirm","proposalId":id}),
        )
        .await
    });
    loop {
        let event = tokio::select! {
            event = events.recv() => event.unwrap(),
            result = &mut task => return result.unwrap(),
        };
        if let crate::state::AppEvent::McpConfirm { request_id, .. } = event {
            resolve_mcp_confirm(state, &request_id, approved);
            return task.await.unwrap();
        }
    }
}

fn stored_username(dir: &tempfile::TempDir) -> Option<Value> {
    let bytes = std::fs::read(dir.path().join("config.json")).ok()?;
    let disk: Value = serde_json::from_slice(&bytes).unwrap();
    Some(disk["services"]["auth"]["username"].clone())
}

// Catches: the owner cannot complete the localhost proposal flow from an authenticated phone.
#[tokio::test]
#[serial_test::serial]
async fn authenticated_remote_settings_proposal_can_be_confirmed() {
    let (dir, _guard, state) = isolated();
    let session = initialize(&state).await;
    let (revision, id) = propose(&state, &session, "remote-owner").await;
    assert_eq!(stored_username(&dir), None, "a proposal must not write");

    let confirmed = confirm(&state, &session, &id, true).await;
    assert_eq!(confirmed["confirmed"], true, "{confirmed}");

    let mut replay = set(&revision, "remote-owner");
    replay["approval"] = json!(id);
    let committed = call(&state, &session, "settings", replay).await;
    assert_eq!(committed["persisted"], true, "{committed}");
    assert_eq!(stored_username(&dir), Some(json!("remote-owner")));
}

// Catches: an unauthenticated or injected request approving another caller's proposal or a changed edit.
#[tokio::test]
#[serial_test::serial]
async fn settings_wrong_caller_or_changed_proposal_cannot_confirm() {
    let (dir, _guard, state) = isolated();
    let owner = initialize(&state).await;
    let (revision, id) = propose(&state, &owner, "remote-owner").await;

    // No credentials: the auth layer refuses before any dialog exists.
    let body = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
        "name":"ui","arguments":{"action":"confirm","proposalId":id}}});
    let (status, _, _) = post(&state, body, Some(&owner), false).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(state.confirm_responses.is_empty());

    // Another authenticated connection cannot lease the owner's proposal.
    let other = initialize(&state).await;
    let mut events = state.event_bus.subscribe();
    let foreign = call(
        &state,
        &other,
        "ui",
        json!({"action":"confirm","proposalId":id}),
    )
    .await;
    assert_eq!(foreign["error"]["code"], "confirmation_denied", "{foreign}");
    assert!(
        !std::iter::from_fn(|| events.try_recv().ok())
            .any(|event| matches!(event, crate::state::AppEvent::McpConfirm { .. })),
        "a foreign caller must never reach the human dialog"
    );

    // The owner's human approval does not cover a changed edit, and the grant is spent.
    assert_eq!(confirm(&state, &owner, &id, true).await["confirmed"], true);
    let mut changed = set(&revision, "substituted-user");
    changed["approval"] = json!(id);
    let refused = call(&state, &owner, "settings", changed).await;
    assert_eq!(refused["error"]["code"], "confirmation_denied", "{refused}");
    let mut exact = set(&revision, "remote-owner");
    exact["approval"] = json!(id);
    let spent = call(&state, &owner, "settings", exact).await;
    assert_eq!(spent["error"]["code"], "confirmation_denied", "{spent}");
    assert_eq!(stored_username(&dir), None);
}
