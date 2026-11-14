//! Story 2652-3887: a credential-verified remote MCP caller has the session and
//! agent authority of a loopback caller. The tests drive the real TCP router, so
//! the authority comes only from the server-issued auth context.
use super::super::tests::test_state;
use super::*;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;

const TOKEN: &str = "authority-token";
const OWNER: &str = "26520000-0000-4000-8000-000000000001";
const OTHER: &str = "26520000-0000-4000-8000-000000000002";

fn owner_state() -> Arc<AppState> {
    let state = test_state();
    *state.session_token.write() = TOKEN.into();
    state
}

/// A request from a LAN peer to the TCP listener, which always runs credential auth.
fn lan_request(body: &serde_json::Value, sid: Option<&str>, token: bool) -> Request<Body> {
    let uri = if token {
        format!("/mcp?token={TOKEN}")
    } else {
        "/mcp".to_string()
    };
    let mut builder = Request::post(uri)
        .header(header::HOST, "192.168.1.2:9876")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-tuic-authenticated", "true");
    if let Some(sid) = sid {
        builder = builder.header("mcp-session-id", sid);
    }
    builder
        .extension(ConnectInfo(SocketAddr::from(([192, 168, 1, 3], 40000))))
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn initialize(state: &Arc<AppState>) -> String {
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-03-26", "capabilities": {},
                   "clientInfo": {"name": "remote-owner", "version": "1.0"}}
    });
    let response = super::super::build_router(state.clone(), true, true)
        .oneshot(lan_request(&body, None, true))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.headers()["mcp-session-id"]
        .to_str()
        .unwrap()
        .to_string()
}

/// Call one tool over the router; returns the HTTP status and the tool result.
async fn call(
    state: &Arc<AppState>,
    sid: Option<&str>,
    token: bool,
    tool: &str,
    args: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": tool, "arguments": args}
    });
    let response = super::super::build_router(state.clone(), true, true)
        .oneshot(lan_request(&body, sid, token))
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
    let text = json["result"]["content"][0]["text"].as_str().unwrap_or("");
    let result = serde_json::from_str(text).unwrap_or_else(|_| serde_json::json!(text));
    (status, result)
}

fn refused_by_transport(result: &serde_json::Value) -> bool {
    result.to_string().contains("restricted to localhost")
}

// Catches: the owner must switch transports for session writes, launches or mail.
#[tokio::test]
async fn authenticated_remote_mcp_manages_owned_sessions_and_agents() {
    let state = owner_state();
    let sid = initialize(&state).await;
    let s = Some(sid.as_str());

    let (_, registered) = call(
        &state,
        s,
        true,
        "agent",
        serde_json::json!({"action": "register", "tuic_session": OWNER, "name": "remote-owner"}),
    )
    .await;
    assert!(!refused_by_transport(&registered), "register: {registered}");

    let (status, created) = call(
        &state,
        s,
        true,
        "session",
        serde_json::json!({"action": "create"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let session_id = created["session_id"]
        .as_str()
        .unwrap_or_else(|| panic!("create: {created}"))
        .to_string();
    assert!(state.session_maps.sessions.contains_key(&session_id));

    for args in [
        serde_json::json!({"action": "input", "session_id": session_id, "data": "true"}),
        serde_json::json!({"action": "resize", "session_id": session_id, "rows": 30, "cols": 100}),
        serde_json::json!({"action": "submit", "session_id": session_id, "input": "echo"}),
        serde_json::json!({"action": "force_retry", "session_id": session_id}),
        serde_json::json!({"action": "suspend", "session_id": session_id}),
    ] {
        let (_, result) = call(&state, s, true, "session", args.clone()).await;
        assert!(!refused_by_transport(&result), "{args}: {result}");
    }

    for args in [
        serde_json::json!({"action": "spawn", "agent_type": "no-such-agent", "prompt": "p"}),
        serde_json::json!({"action": "launch", "launch_id": "missing"}),
        serde_json::json!({"action": "send", "to": OWNER, "message": "hello self"}),
        serde_json::json!({"action": "subscribe", "subscription_id": "remote-owner"}),
        serde_json::json!({"action": "inbox"}),
        serde_json::json!({"action": "wait", "timeout_ms": 1}),
    ] {
        let (_, result) = call(&state, s, true, "agent", args.clone()).await;
        assert!(!refused_by_transport(&result), "{args}: {result}");
    }

    // The HTTP subscription route grants the same authority as the MCP tool.
    let inbox = serde_json::json!({"action": "inbox", "subscription_id": "remote-owner"});
    let mut http = lan_request(&inbox, s, true);
    *http.uri_mut() = format!("/agent/subscriptions/action?token={TOKEN}")
        .parse()
        .unwrap();
    let response = super::super::build_router(state.clone(), true, true)
        .oneshot(http)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let (_, workflow) = call(
        &state,
        s,
        true,
        "workflow",
        serde_json::json!({"action": "launch", "input": {
            "run_id": "missing", "attempt_id": "a1", "worktree_path": "/missing", "agent_type": "claude"
        }}),
    )
    .await;
    assert!(
        !refused_by_transport(&workflow),
        "workflow launch: {workflow}"
    );

    let own_task = state.tasks.create(
        crate::tasks::TaskKind::AgentSpawn,
        &super::super::mcp_transport_peer::task_owner_identity(Some(OWNER), Some(&sid)),
        None,
    );
    let (_, cancelled) = call(
        &state,
        s,
        true,
        "agent",
        serde_json::json!({"action": "task_cancel", "task_id": own_task}),
    )
    .await;
    assert_eq!(cancelled["cancelled"], true, "{cancelled}");

    let (_, closed) = call(
        &state,
        s,
        true,
        "session",
        serde_json::json!({"action": "close", "session_id": session_id}),
    )
    .await;
    assert!(!refused_by_transport(&closed), "close: {closed}");
    assert!(!state.session_maps.sessions.contains_key(&session_id));
}

// Catches: an injected or unauthenticated request spoofs authority, or a
// verified remote caller bypasses ownership of another identity's task.
#[tokio::test]
async fn remote_mcp_cannot_forge_owner_or_mutate_foreign_session() {
    let state = owner_state();
    state.config.write().services.auth.lan_auth_bypass = true;
    // No credentials: the forged header, a body flag and the legacy LAN bypass grant nothing.
    let (status, _) = call(
        &state,
        None,
        false,
        "session",
        serde_json::json!({"action": "create", "authenticated": true}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(state.session_maps.sessions.is_empty());

    // A transport admission without verified credentials is not owner evidence.
    let unverified = handle_mcp_tool_call(
        &state,
        "192.168.1.3:1".parse().unwrap(),
        "session",
        &serde_json::json!({"action": "create"}),
        None,
    )
    .await;
    assert!(refused_by_transport(&unverified), "{unverified}");
    assert!(state.session_maps.sessions.is_empty());

    let sid = initialize(&state).await;
    let s = Some(sid.as_str());
    call(
        &state,
        s,
        true,
        "agent",
        serde_json::json!({"action": "register", "tuic_session": OWNER, "name": "remote-owner"}),
    )
    .await;
    let own = state.tasks.create(
        crate::tasks::TaskKind::AgentSpawn,
        &super::super::mcp_transport_peer::task_owner_identity(Some(OWNER), Some(&sid)),
        None,
    );
    let foreign = state
        .tasks
        .create(crate::tasks::TaskKind::AgentSpawn, OTHER, None);
    let (_, refused) = call(
        &state,
        s,
        true,
        "agent",
        serde_json::json!({"action": "task_cancel", "task_id": foreign}),
    )
    .await;
    assert_eq!(
        refused["error"], "task_id is not owned by the calling identity",
        "{refused}"
    );
    assert_eq!(
        state.tasks.get(&foreign).unwrap().status,
        crate::tasks::TaskStatus::Working
    );
    // The same verified caller does own its task: ownership, not transport, decides.
    let (_, cancelled) = call(
        &state,
        s,
        true,
        "agent",
        serde_json::json!({"action": "task_cancel", "task_id": own}),
    )
    .await;
    assert_eq!(cancelled["cancelled"], true, "{cancelled}");
}
