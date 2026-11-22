use serde_json::{Value, json};

use super::super::tests::test_state;
use super::*;

fn loopback() -> super::super::guards::CallerAuthority {
    "127.0.0.1:12345"
        .parse::<std::net::SocketAddr>()
        .unwrap()
        .into()
}

async fn batch(state: &Arc<AppState>, args: Value) -> Value {
    super::super::mcp_transport::handle_mcp_tool_call_with_context(
        state,
        loopback(),
        BATCH_TOOL_NAME,
        &args,
        None,
        None,
    )
    .await
}

async fn alone(state: &Arc<AppState>, tool: &str, args: Value) -> Value {
    super::super::mcp_transport::handle_mcp_tool_call_with_context(
        state,
        loopback(),
        tool,
        &args,
        None,
        None,
    )
    .await
}

/// Catches: results out of call order, a failed call dropped from `results`,
/// a failure that stops the batch by default, or a result that differs from
/// the same call made alone.
#[tokio::test]
async fn mixed_calls_run_in_order_and_each_result_matches_the_call_alone() {
    let state = test_state();
    let reply = batch(
        &state,
        json!({"calls":[
            {"tool":"session","arguments":{"action":"list"}},
            {"tool":"nonsense_xyz","arguments":{}},
            {"tool":"repo","arguments":{"action":"nonsense_action"}}
        ]}),
    )
    .await;
    let results = reply["results"].as_array().expect("results");
    assert_eq!(results.len(), 3, "{reply}");
    assert_eq!(
        results[0],
        alone(&state, "session", json!({"action":"list"})).await
    );
    assert!(
        results[1]["error"]
            .as_str()
            .is_some_and(|e| e.contains("Unknown tool")),
        "{reply}"
    );
    assert_eq!(
        results[2],
        alone(&state, "repo", json!({"action":"nonsense_action"})).await
    );
    assert_eq!(reply["errors"], 2, "{reply}");
    assert!(reply.get("stopped_at").is_none(), "{reply}");
}

/// Catches: `stop_on_error` running the calls after the failure, or reporting
/// the wrong index.
#[tokio::test]
async fn stop_on_error_skips_every_call_after_the_first_failure() {
    let state = test_state();
    let reply = batch(
        &state,
        json!({"stop_on_error":true,"calls":[
            {"tool":"session","arguments":{"action":"list"}},
            {"tool":"nonsense_xyz"},
            {"tool":"session","arguments":{"action":"list"}}
        ]}),
    )
    .await;
    assert_eq!(
        reply["results"].as_array().map(Vec::len),
        Some(2),
        "{reply}"
    );
    assert_eq!(reply["stopped_at"], 1, "{reply}");
    assert_eq!(reply["errors"], 1, "{reply}");
}

/// Catches: a batch element dispatched past the configuration gate, so a
/// disabled tool runs when wrapped in a batch.
#[tokio::test]
async fn a_disabled_tool_stays_disabled_inside_a_batch() {
    let state = test_state();
    state.config.write().mcp_capabilities.denied = vec!["repo".to_string()];
    let reply = batch(
        &state,
        json!({"calls":[{"tool":"repo","arguments":{"action":"active"}}]}),
    )
    .await;
    assert!(
        reply["results"][0]["error"]
            .as_str()
            .is_some_and(|e| e.contains("disabled")),
        "{reply}"
    );
}

/// Catches: a caller-address restriction lost because the batch, not the
/// element, was checked against the caller.
#[tokio::test]
async fn a_localhost_only_action_stays_restricted_inside_a_batch() {
    let state = test_state();
    let remote: super::super::guards::CallerAuthority = "192.168.1.42:12345"
        .parse::<std::net::SocketAddr>()
        .unwrap()
        .into();
    let reply = super::super::mcp_transport::handle_mcp_tool_call_with_context(
        &state,
        remote,
        BATCH_TOOL_NAME,
        &json!({"calls":[{"tool":"agent","arguments":{"action":"spawn"}}]}),
        Some("remote-test"),
        None,
    )
    .await;
    assert!(
        reply["results"][0]["error"]
            .as_str()
            .is_some_and(|e| e.contains("restricted to localhost")),
        "{reply}"
    );
}

/// Catches: unbounded or recursive batches — a nested batch, directly or
/// through `call_tool`, multiplies the bound.
#[tokio::test]
async fn batches_are_bounded_and_never_nest() {
    let state = test_state();
    let too_many: Vec<Value> = (0..=MAX_BATCH_CALLS)
        .map(|_| json!({"tool":"session","arguments":{"action":"list"}}))
        .collect();
    let reply = batch(&state, json!({ "calls": too_many })).await;
    assert_eq!(reply["code"], "invalid_arguments", "{reply}");
    assert!(reply.get("results").is_none(), "{reply}");

    let empty = batch(&state, json!({"calls":[]})).await;
    assert_eq!(empty["code"], "invalid_arguments", "{empty}");

    let nested = batch(
        &state,
        json!({"calls":[
            {"tool":"batch","arguments":{"calls":[{"tool":"session","arguments":{"action":"list"}}]}},
            {"tool":"call_tool","arguments":{"tool_name":"batch","arguments":{}}}
        ]}),
    )
    .await;
    assert_eq!(nested["errors"], 2, "{nested}");
}

/// Catches: the tool reachable only in one catalogue mode, so a client on the
/// other mode never learns it exists.
#[test]
fn batch_is_listed_in_the_direct_and_the_collapsed_catalogue() {
    let state = test_state();
    for collapsed in [false, true] {
        let tools = super::super::mcp_transport_catalogue::merged_tool_definitions_for_mode(
            &state, None, collapsed,
        );
        assert!(
            tools
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == BATCH_TOOL_NAME),
            "collapsed={collapsed}"
        );
    }
}
