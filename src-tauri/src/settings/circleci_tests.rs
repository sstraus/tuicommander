//! Isolated public Settings contracts; no CircleCI network or production credential access.
use super::{api, policy::Caller};
use serde_json::{Value, json};

async fn execute(state: &std::sync::Arc<crate::AppState>, request: Value) -> Value {
    api::execute(state, request, Caller::native()).await
}
fn read() -> Value {
    json!({"action":"get","target":{"kind":"machine"},"store":"circleci","paths":["/token"]})
}
fn perform(operation: Value) -> Value {
    json!({"action":"perform","target":{"kind":"machine"},"store":"circleci","operation":operation})
}

// Catches: a durable CircleCI setting missing from discovery or exposing its secret on readback.
#[tokio::test]
async fn circleci_discovery_and_write_only_readback() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    crate::credentials::reset_test_faults();
    crate::circleci::circleci_delete_token().await.unwrap();
    let description = execute(&state, json!({"action":"describe","store":"circleci"})).await;
    assert_eq!(
        description["fields"][0]["id"], "circleci.token",
        "{description}"
    );
    assert_eq!(description["fields"][0]["path"], "/token");
    assert_eq!(description["fields"][0]["sensitivity"], "secret");
    assert_eq!(
        execute(&state, read()).await["values"]["/token"]["stored"]["status"],
        "unset"
    );
    let mut events = state.event_bus.subscribe();
    let written = execute(
        &state,
        perform(json!({"action":"set_token","token":"circleci-settings-canary"})),
    )
    .await;
    assert_eq!(written["token"]["status"], "set", "{written}");
    // Catches: merging a new credential owner without wiring its live-refresh receipt.
    match events
        .try_recv()
        .expect("durable CircleCI write must refresh settings")
    {
        crate::state::AppEvent::SettingsChanged(notice) => {
            assert_eq!(notice.section, "circleci");
            assert!(notice.persisted);
            assert_eq!(Some(notice.generation), written["generation"].as_u64());
        }
        other => panic!("unexpected settings event: {other:?}"),
    }
    let snapshot = execute(&state, read()).await;
    assert_eq!(snapshot["values"]["/token"]["stored"]["status"], "set");
    assert!(!format!("{description}{written}{snapshot}").contains("circleci-settings-canary"));
    // Existing IPC/HTTP owner sees the same slot, and clearing through Settings clears that slot.
    assert_eq!(
        crate::credentials::get(crate::credentials::Credential::CircleCiToken)
            .unwrap()
            .as_deref(),
        Some("circleci-settings-canary")
    );
    let cleared = execute(&state, perform(json!({"action":"clear_token"}))).await;
    assert_eq!(cleared["token"]["status"], "unset", "{cleared}");
    assert!(
        crate::credentials::get(crate::credentials::Credential::CircleCiToken)
            .unwrap()
            .is_none()
    );
}

// Catches: vault errors leaking secrets, unreadable becoming unset, or rejected input replacing a token.
#[tokio::test]
async fn circleci_owner_errors_and_unavailable_are_redacted() {
    use std::sync::atomic::Ordering::SeqCst;
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    crate::credentials::reset_test_faults();
    crate::circleci::circleci_delete_token().await.unwrap();
    let blank = execute(&state, perform(json!({"action":"set_token","token":"  "}))).await;
    assert_eq!(blank["error"]["code"], "store_failed");
    crate::credentials::MOCK_FAIL_WRITES.store(true, SeqCst);
    let failed = execute(
        &state,
        perform(json!({"action":"set_token","token":"circleci-error-canary"})),
    )
    .await;
    crate::credentials::reset_test_faults();
    assert_eq!(failed["error"]["code"], "store_failed", "{failed}");
    assert!(!failed.to_string().contains("circleci-error-canary"));
    assert_eq!(
        execute(&state, read()).await["values"]["/token"]["stored"]["status"],
        "unset"
    );
    crate::credentials::MOCK_FAIL_READS.store(true, SeqCst);
    let unavailable = execute(&state, read()).await;
    crate::credentials::reset_test_faults();
    assert_eq!(
        unavailable["values"]["/token"]["stored"]["status"], "unavailable",
        "{unavailable}"
    );
}

// Catches: a typed CircleCI owner action bypassing the common machine/capability policy.
#[tokio::test]
async fn circleci_foreign_target_and_disabled_capability_cannot_write() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    crate::credentials::reset_test_faults();
    crate::circleci::circleci_delete_token().await.unwrap();
    let mut request = perform(json!({"action":"set_token","token":"circleci-forbidden-canary"}));
    request["target"] = json!({"kind":"instance"});
    assert_eq!(execute(&state, request).await["error"]["code"], "not_found");
    let disabled = Caller::mcp("fixture".into(), false);
    let result = api::execute(
        &state,
        perform(json!({"action":"set_token","token":"circleci-forbidden-canary"})),
        disabled,
    )
    .await;
    assert_eq!(result["error"]["code"], "unavailable");
    assert!(
        crate::credentials::get(crate::credentials::Credential::CircleCiToken)
            .unwrap()
            .is_none()
    );
}

// Catches: a malformed CircleCI workflow echoing token input or accepting unknown owner fields.
#[tokio::test]
async fn circleci_invalid_workflow_never_echoes_input() {
    let (_dir, _guard, state) = super::accounts_tests::isolated();
    let result = execute(
        &state,
        perform(json!({"action":"set_token","token":"invalid-input-canary","unexpected":true})),
    )
    .await;
    assert_eq!(result["error"]["code"], "invalid_type");
    assert!(!result.to_string().contains("invalid-input-canary"));
    let schema = api::request_schema().to_string();
    assert!(schema.contains("circleci"));
    assert!(schema.contains("writeOnly"));
}
