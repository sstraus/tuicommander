//! Durable mail adapters. Identity is the existing MCP binding, never a body field.
//! The inbound webhook is the one exception: its sender is always `webhook`.
use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
};
use serde_json::{Value, json};

use super::guards::{CallerAuthority, UserAuthenticated};
use crate::{AppState, mail_subscriptions};

fn peer(
    state: &AppState,
    caller: CallerAuthority,
    headers: &HeaderMap,
) -> Result<String, (StatusCode, &'static str)> {
    if !caller.is_owner() {
        return Err((
            StatusCode::FORBIDDEN,
            "Inter-agent messaging is restricted to localhost or authenticated connections",
        ));
    }
    if !state.config.read().mcp_capabilities.allows("agent", None) {
        return Err((
            StatusCode::FORBIDDEN,
            "Tool 'agent' is disabled by configuration",
        ));
    }
    headers
        .get("mcp-session-id")
        .and_then(|h| h.to_str().ok())
        .and_then(|id| state.mcp.to_session.get(id).map(|p| p.value().clone()))
        .ok_or((
            StatusCode::UNAUTHORIZED,
            "Register an MCP peer first and supply its Mcp-Session-Id header",
        ))
}

pub(super) async fn action(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    user: Option<Extension<UserAuthenticated>>,
    headers: HeaderMap,
    Json(args): Json<Value>,
) -> Response {
    let caller = CallerAuthority::from_request(addr, user.map(|Extension(marker)| marker));
    let peer = match peer(&state, caller, &headers) {
        Ok(peer) => peer,
        Err(reply) => return reply.into_response(),
    };
    if !matches!(
        args["action"].as_str(),
        Some("subscribe" | "ack" | "unsubscribe" | "inbox")
    ) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"Use subscribe, ack, unsubscribe or inbox"})),
        )
            .into_response();
    }
    Json(mail_subscriptions::read(&state, &peer, &args).await).into_response()
}

/// Inbound webhook: `POST /agent/webhook/{to}` files the raw body as mail for the
/// peer that `to` names. The route caps the body at the mail size limit.
pub(super) async fn webhook(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    auth: Option<Extension<super::guards::Authenticated>>,
    Path(to): Path<String>,
    body: String,
) -> Response {
    if let Err(reply) = super::guards::require_local_or_auth(&addr, auth.is_some()) {
        return reply.into_response();
    }
    let reply = super::mcp_transport_peer::webhook_send(&state, &to, &body).await;
    let status = if reply.get("error").is_some() {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::OK
    };
    (status, Json(reply)).into_response()
}

/// One connection carries all actionable batches; routine traffic does not emit.
/// Reconnect replays an unacknowledged batch; acknowledging it resumes the stream.
pub(super) async fn events(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    user: Option<Extension<UserAuthenticated>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let caller = CallerAuthority::from_request(addr, user.map(|Extension(marker)| marker));
    let peer = match peer(&state, caller, &headers) {
        Ok(peer) => peer,
        Err(reply) => return reply.into_response(),
    };
    let mut changes = state.subscribe_agent_inbox(&peer);
    let args = json!({"action":"inbox","subscription_id":id});
    let first = mail_subscriptions::read(&state, &peer, &args).await;
    if first.get("error").is_some() {
        return (StatusCode::BAD_REQUEST, Json(first)).into_response();
    }
    let stream = async_stream::stream! {
        let mut next = Some(first);
        let mut last_cursor = None;
        loop {
            let batch = match next.take() {
                Some(batch) => batch,
                None => mail_subscriptions::read(&state, &peer, &args).await,
            };
            if batch.get("error").is_some() {
                yield Ok::<_,Infallible>(Event::default().event("error").data(batch.to_string()));
                break;
            }
            let cursor = batch["cursor"].as_u64().unwrap_or(0);
            if mail_subscriptions::pending(&batch) && last_cursor != Some(cursor) {
                last_cursor = Some(cursor);
                yield Ok(Event::default().event("mail").id(cursor.to_string()).data(batch.to_string()));
            }
            if changes.changed().await.is_err() { break; }
        }
    };
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    // Catches: HTTP subscription operations use an independent validation path,
    // or the continuous stream returns routine lifecycle traffic as actionable.
    #[tokio::test]
    async fn actionable_subscription_http_stream_filters_and_acknowledges() {
        let state = super::super::tests::test_state();
        state
            .mcp
            .to_session
            .insert("mail-http".into(), "parent".into());
        let mut headers = HeaderMap::new();
        headers.insert("mcp-session-id", "mail-http".parse().unwrap());
        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        let reply = action(
            State(state.clone()),
            ConnectInfo(addr),
            None,
            headers.clone(),
            Json(json!({"action":"subscribe","subscription_id":"http"})),
        )
        .await;
        assert_eq!(reply.status(), StatusCode::OK);
        let mut routine = crate::state::AgentMessage {
            id: "tuic-auto-http-idle".into(),
            from_tuic_session: "child".into(),
            from_name: "tuic".into(),
            content: r#"{"type":"state_change","state":"idle"}"#.into(),
            timestamp: 1,
            report: None,
            delivered_via_channel: false,
        };
        state.push_agent_inbox("parent", routine.clone());
        let response = events(
            State(state.clone()),
            ConnectInfo(addr),
            None,
            headers.clone(),
            Path("http".into()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let mut stream = response.into_body().into_data_stream();
        let mut first = Box::pin(stream.next());
        assert!(
            futures_util::poll!(first.as_mut()).is_pending(),
            "no lifecycle event"
        );
        routine.id = "http-result".into();
        routine.from_name = "child".into();
        routine.content = "RESULT http stream".into();
        state.push_agent_inbox("parent", routine);
        let frame = tokio::time::timeout(std::time::Duration::from_secs(10), first)
            .await
            .expect("HTTP stream must carry result")
            .unwrap()
            .unwrap();
        let text = std::str::from_utf8(&frame).unwrap();
        assert!(text.contains("RESULT http stream"), "{text}");
        assert!(!text.contains("tuic-auto-http-idle"), "{text}");
        let batch = mail_subscriptions::read(
            &state,
            "parent",
            &json!({"action":"inbox","subscription_id":"http"}),
        )
        .await;
        let response = action(
            State(state.clone()),
            ConnectInfo(addr),
            None,
            headers.clone(),
            Json(json!({"action":"ack","subscription_id":"http","cursor":batch["cursor"]})),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            mail_subscriptions::read(
                &state,
                "parent",
                &json!({"action":"inbox","subscription_id":"http"})
            )
            .await["count"],
            0
        );
        let denied = events(
            State(state.clone()),
            ConnectInfo("192.0.2.1:1234".parse().unwrap()),
            None,
            headers.clone(),
            Path("http".into()),
        )
        .await;
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
        let denied = events(
            State(state),
            ConnectInfo(addr),
            None,
            HeaderMap::new(),
            Path("http".into()),
        )
        .await;
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    }

    const WEBHOOK_ALICE: &str = "550e8400-e29b-41d4-a716-446655440c01";
    const WEBHOOK_BOB: &str = "550e8400-e29b-41d4-a716-446655440c02";

    fn webhook_state() -> Arc<AppState> {
        let state = super::super::tests::test_state();
        for (tuic, name, mcp) in [
            (WEBHOOK_ALICE, "alice", "mcp-webhook-a"),
            (WEBHOOK_BOB, "bob", "mcp-webhook-b"),
        ] {
            super::super::mcp_transport_peer::handle_messaging(
                &state,
                &json!({"action":"register","tuic_session":tuic,"name":name}),
                Some(mcp),
            );
        }
        state
    }

    /// Posts through the production router, so the route's body limit applies.
    async fn post_webhook(state: &Arc<AppState>, to: &str, body: String) -> Response {
        use tower::ServiceExt;
        let mut request = axum::http::Request::post(format!("/agent/webhook/{to}"))
            .header(axum::http::header::HOST, "127.0.0.1:9876")
            .body(axum::body::Body::from(body))
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo("127.0.0.1:1234".parse::<SocketAddr>().unwrap()));
        super::super::build_router(state.clone(), false, true)
            .oneshot(request)
            .await
            .unwrap()
    }

    fn inbox(state: &AppState, tuic: &str) -> Vec<crate::state::AgentMessage> {
        state
            .agent_inbox
            .get(tuic)
            .map(|inbox| inbox.iter().cloned().collect())
            .unwrap_or_default()
    }

    // Catches: the webhook body reaching another peer's inbox, or being filed
    // under a peer identity instead of the fixed webhook sender.
    #[tokio::test]
    async fn webhook_files_the_body_as_mail_for_the_named_peer() {
        let state = webhook_state();
        let response = post_webhook(&state, "alice", "build finished: 3 failures".into()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let mail = inbox(&state, WEBHOOK_ALICE);
        assert_eq!(mail.len(), 1, "{mail:?}");
        assert_eq!(mail[0].content, "build finished: 3 failures");
        assert_eq!(mail[0].from_tuic_session, "webhook");
        assert_eq!(mail[0].from_name, "webhook");
        assert_eq!(
            inbox(&state, WEBHOOK_BOB),
            Vec::<crate::state::AgentMessage>::new()
        );
    }

    // Catches: a payload for an unknown recipient silently accepted, or routed
    // to some other registered agent.
    #[tokio::test]
    async fn webhook_refuses_an_unknown_recipient_and_files_nothing() {
        let state = webhook_state();
        let response = post_webhook(&state, "nobody", "result".into()).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert!(
            body["error"].as_str().unwrap().contains("not registered"),
            "{body}"
        );
        assert_eq!(
            inbox(&state, WEBHOOK_ALICE),
            Vec::<crate::state::AgentMessage>::new()
        );
        assert_eq!(
            inbox(&state, WEBHOOK_BOB),
            Vec::<crate::state::AgentMessage>::new()
        );
    }

    // Catches: an unbounded body accepted into the inbox.
    #[tokio::test]
    async fn webhook_refuses_an_oversized_payload() {
        let state = webhook_state();
        let oversized = "x".repeat(crate::state::AGENT_MESSAGE_MAX_BYTES + 1);
        let response = post_webhook(&state, "alice", oversized).await;
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(
            inbox(&state, WEBHOOK_ALICE),
            Vec::<crate::state::AgentMessage>::new()
        );
    }
}
