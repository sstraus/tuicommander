use super::*;
use crate::state::AppEvent;
use axum::Json;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const TASKS_LIST: &str = include_str!("fixtures/fabbrica/tasks_list.json");
const HELP_LIST: &str = include_str!("fixtures/fabbrica/help_list.json");
const OUTSIDE_REQUEST_ANSWER: &str = include_str!("fixtures/fabbrica/outside_request_answer.json");
const OUTSIDE_REQUESTS_LIST: &str = include_str!("fixtures/fabbrica/outside_requests_list.json");

fn fixture(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap()
}

/// The recorded help_list reply after the question was answered: Fabbrica's
/// help page is a replacement snapshot, so a settled question leaves it.
fn answered_help_list() -> Value {
    let mut reply = fixture(HELP_LIST);
    let help = serde_json::json!({ "questions": [] });
    reply["result"]["structuredContent"]["help"] = help.clone();
    let mut text: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    text["help"] = help;
    reply["result"]["content"][0]["text"] = Value::String(text.to_string());
    reply
}

/// The recorded outside_requests_list reply once the request left the ask state.
fn empty_outside_requests() -> Value {
    let mut reply = fixture(OUTSIDE_REQUESTS_LIST);
    reply["result"]["structuredContent"]["requests"] = serde_json::json!([]);
    reply["result"]["content"][0]["text"] = Value::String(r#"{"requests":[]}"#.into());
    reply
}

/// A control MCP that replays the recorded replies; `answered` switches
/// help_list to the reply after the owner answered in the Fabbrica console.
async fn spawn_control_mcp(answered: Arc<AtomicBool>) -> String {
    let app = axum::Router::new().route(
        "/mcp",
        axum::routing::post(move |Json(body): Json<Value>| {
            let answered = answered.clone();
            async move {
                let mut reply = match body["params"]["name"].as_str() {
                    Some("tasks_list") => fixture(TASKS_LIST),
                    Some("help_list") if answered.load(Ordering::SeqCst) => answered_help_list(),
                    Some("help_list") => fixture(HELP_LIST),
                    Some("outside_request_answer") => {
                        // The recorded call: request_id and decision, nothing else.
                        assert_eq!(
                            body["params"]["arguments"],
                            serde_json::json!({"request_id": "d1", "decision": "accept"})
                        );
                        answered.store(true, Ordering::SeqCst);
                        fixture(OUTSIDE_REQUEST_ANSWER)
                    }
                    Some("outside_requests_list") if answered.load(Ordering::SeqCst) => {
                        empty_outside_requests()
                    }
                    Some("outside_requests_list") => fixture(OUTSIDE_REQUESTS_LIST),
                    other => panic!("unexpected control MCP tool {other:?}"),
                };
                reply["id"] = body["id"].clone();
                Json(reply)
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://127.0.0.1:{port}/mcp")
}

fn attention_changes(rx: &mut tokio::sync::broadcast::Receiver<AppEvent>) -> usize {
    std::iter::from_fn(|| rx.try_recv().ok())
        .filter(|event| matches!(event, AppEvent::BossAttentionChanged))
        .count()
}

// Catches: an outage reads as nothing to do — a failed control MCP read
// publishes Ready with no items instead of an explicit unknown state.
#[tokio::test]
async fn unreachable_fabbrica_shows_unknown_not_empty() {
    let state = crate::state::tests_support::make_test_app_state();
    state.mcp.upstream_registry.inject_ready_http_upstream(
        UPSTREAM,
        "http://127.0.0.1:1/mcp",
        &[
            "tasks_list",
            "help_list",
            "outside_requests_list",
            "outside_request_answer",
        ],
    );
    refresh(&state).await;
    let view = state.mcp.fabbrica.read().clone();
    assert!(
        matches!(view.state, FabbricaState::Unknown { .. }),
        "an unreachable control MCP must read as unknown, got {:?}",
        view.state
    );
    assert_eq!(view.items, [] as [AttentionItem; 0]);
    let mut result = BossAttention::default();
    add_to(&mut result, &state);
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["fabbrica"]["state"], "unknown");
}

// Catches: an upstream that waits for OAuth consent reads as an empty list
// instead of asking the owner to connect Fabbrica.
#[tokio::test]
async fn fabbrica_without_consent_asks_to_connect() {
    let state = crate::state::tests_support::make_test_app_state();
    let registry = &state.mcp.upstream_registry;
    registry.inject_ready_http_upstream(UPSTREAM, "http://127.0.0.1:1/mcp", &[]);
    *registry.entry(UPSTREAM).unwrap().status.write() =
        crate::mcp_proxy::registry::UpstreamStatus::NeedsAuth;
    refresh(&state).await;
    assert_eq!(state.mcp.fabbrica.read().state, FabbricaState::Connect);
}

// Catches: a stale item stays after the answer — the projection keeps the
// previous poll's question once help_list no longer lists it.
#[tokio::test]
async fn answered_fabbrica_item_leaves_view() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut events = state.event_bus.subscribe();
    let answered = Arc::new(AtomicBool::new(false));
    let url = spawn_control_mcp(answered.clone()).await;
    state.mcp.upstream_registry.inject_ready_http_upstream(
        UPSTREAM,
        &url,
        &[
            "tasks_list",
            "help_list",
            "outside_requests_list",
            "outside_request_answer",
        ],
    );

    refresh(&state).await;
    let mut result = BossAttention::default();
    add_to(&mut result, &state);
    assert_eq!(result.items.len(), 2, "{:?}", result.items);
    let outside = &result.items[1];
    assert_eq!(outside.id, "fabbrica:outside:d1");
    assert_eq!(outside.kind, AttentionKind::OutsideRequest);
    assert_eq!(
        outside.title,
        "Run https://github.com/acme/app/issues/5#d1?"
    );
    assert!(outside.detail.contains("acme/app"), "{}", outside.detail);
    let item = &result.items[0];
    assert_eq!(item.id, "fabbrica:asks:connection/1/session:q1");
    assert_eq!(item.kind, AttentionKind::Approval);
    assert_eq!(item.title, "Push branch feat/x?");
    assert_eq!(item.options[0].label, "Allow once");
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["fabbrica"]["state"], "ready");
    assert_eq!(
        json["fabbrica"]["console"],
        url.replace("/mcp", "/console/")
    );
    assert_eq!(attention_changes(&mut events), 1);

    // An unchanged poll does not wake the readers again.
    refresh(&state).await;
    assert_eq!(attention_changes(&mut events), 0);

    answered.store(true, Ordering::SeqCst);
    refresh(&state).await;
    let mut result = BossAttention::default();
    add_to(&mut result, &state);
    assert!(result.items.is_empty(), "{:?}", result.items);
    assert_eq!(state.mcp.fabbrica.read().state, FabbricaState::Ready);
    assert_eq!(attention_changes(&mut events), 1);
}

// Catches: an answer that leaves the request listed — the answer call does not
// reread, so the section and bell keep a request Fabbrica already settled; and an
// unknown decision reaching Fabbrica.
#[tokio::test]
async fn answered_outside_request_leaves_view_at_once() {
    let state = crate::state::tests_support::make_test_app_state();
    let answered = Arc::new(AtomicBool::new(false));
    let url = spawn_control_mcp(answered.clone()).await;
    state.mcp.upstream_registry.inject_ready_http_upstream(
        UPSTREAM,
        &url,
        &[
            "tasks_list",
            "help_list",
            "outside_requests_list",
            "outside_request_answer",
        ],
    );
    refresh(&state).await;
    assert!(
        state
            .mcp
            .fabbrica
            .read()
            .items
            .iter()
            .any(|i| i.id == "fabbrica:outside:d1")
    );

    assert!(answer_outside_request(&state, "d1", "maybe").await.is_err());
    assert!(
        !answered.load(Ordering::SeqCst),
        "an unknown decision reached Fabbrica"
    );

    answer_outside_request(&state, "d1", "accept")
        .await
        .unwrap();
    assert!(answered.load(Ordering::SeqCst));
    let items = state.mcp.fabbrica.read().items.clone();
    assert!(
        items
            .iter()
            .all(|i| i.kind != AttentionKind::OutsideRequest),
        "{items:?}"
    );
}
