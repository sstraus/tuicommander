// Catches: send silently discards report metadata before inbox serialization.
#[test]
fn typed_report_state_survives_inbox_roundtrip() {
    let state = test_state();
    register_peer(&state, TEST_UUID_A, "reporter", "report-sender");
    register_peer(&state, TEST_UUID_B, "coordinator", "report-reader");
    let report = serde_json::json!({"state":"waiting","wait_for":"boss",
        "question":{"id":"scope","text":"Which scope?","options":[
            {"id":"small","label":"Small"}],"recommended_option_id":"small"}});
    let sent = handle_messaging(
        &state,
        &serde_json::json!({"action":"send",
        "to":TEST_UUID_B,"message":"Need scope","report":report}),
        Some("report-sender"),
    );
    assert!(sent.get("error").is_none(), "{sent}");
    let inbox = handle_messaging(
        &state,
        &serde_json::json!({"action":"inbox"}),
        Some("report-reader"),
    );
    assert_eq!(
        inbox["messages"][0]["report"], report,
        "typed report discarded"
    );
}

// Catches: a closed PTY retains its prior waiting_on_review stage.
#[tokio::test]
async fn closed_session_never_shows_a_live_stage() {
    let state = test_state();
    #[cfg(unix)]
    let shell = "/bin/sh";
    #[cfg(windows)]
    let shell = "cmd.exe";
    let created = handle_session(
        &state,
        &serde_json::json!({"action":"create","shell":shell}),
        None,
    );
    let session = created["session_id"].as_str().expect("real PTY creation");
    register_peer(&state, session, "reporter", "closed-report-sender");
    register_peer(&state, TEST_UUID_B, "coordinator", "closed-report-reader");
    let sent = handle_messaging(
        &state,
        &serde_json::json!({"action":"send",
        "to":TEST_UUID_B,"message":"Ready for review","report":{"state":"ready"}}),
        Some("closed-report-sender"),
    );
    let before = serde_json::to_value(super::super::session::local_session_rows(&state)).unwrap();
    let killed = handle_session(
        &state,
        &serde_json::json!({"action":"kill","session_id":session}),
        None,
    );
    let after = serde_json::to_value(super::super::session::local_session_rows(&state)).unwrap();
    assert!(sent.get("error").is_none(), "{sent}");
    assert!(killed.get("error").is_none(), "{killed}");
    let live = before
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["session_id"] == session)
        .unwrap();
    assert_eq!(
        live["stage"], "waiting_on_review",
        "live report stage missing"
    );
    assert!(
        after
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["session_id"] != session || row["stage"].is_null()),
        "closed session has live stage: {after}"
    );
}

// Catches: report states default to ready, or durable/wait readers discard fields.
#[tokio::test]
async fn typed_reports_preserve_every_state_through_wait_and_subscription() {
    let state = test_state();
    register_peer(&state, TEST_UUID_A, "reporter", "typed-sender");
    register_peer(&state, TEST_UUID_B, "coordinator", "typed-reader");
    let subscribed = handle_messaging(
        &state,
        &serde_json::json!({"action":"subscribe","subscription_id":"typed"}),
        Some("typed-reader"),
    );
    assert!(subscribed.get("error").is_none(), "{subscribed}");
    for name in [
        "ready", "waiting", "blocked", "failed", "dropped", "progress",
    ] {
        let mut report = serde_json::json!({"state":name});
        if name == "waiting" {
            report["wait_for"] = serde_json::json!("review");
        }
        let sent = handle_messaging(
            &state,
            &serde_json::json!({"action":"send","to":TEST_UUID_B,"message":name,"report":report}),
            Some("typed-sender"),
        );
        assert!(sent.get("error").is_none(), "{sent}");
        let read = handle_agent_wait(
            &state,
            &serde_json::json!({"action":"wait","timeout_ms":1}),
            Some("typed-reader"),
        )
        .await;
        assert_eq!(read["messages"][0]["report"], report, "{read}");
    }
    let durable = handle_messaging(
        &state,
        &serde_json::json!({"action":"inbox","subscription_id":"typed"}),
        Some("typed-reader"),
    );
    let states: Vec<_> = durable["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["report"]["state"].as_str().unwrap())
        .collect();
    assert_eq!(
        states,
        [
            "ready", "waiting", "blocked", "failed", "dropped", "progress"
        ]
    );
    let ack = handle_messaging(
        &state,
        &serde_json::json!({"action":"ack","subscription_id":"typed","cursor":durable["cursor"]}),
        Some("typed-reader"),
    );
    assert!(ack.get("error").is_none(), "{ack}");
}

// Catches: malformed metadata is silently accepted as ordinary text mail.
#[test]
fn typed_report_validation_rejects_before_mail_is_stored() {
    let state = test_state();
    register_peer(&state, TEST_UUID_A, "reporter", "invalid-sender");
    register_peer(&state, TEST_UUID_B, "coordinator", "invalid-reader");
    for report in [
        serde_json::json!(null),
        serde_json::json!({"state":"bogus"}),
        serde_json::json!({"state":"waiting"}),
        serde_json::json!({"state":"ready","wait_for":"review"}),
        serde_json::json!({"state":"ready","question":{"id":"q","text":"Scope?"}}),
        serde_json::json!({"state":"waiting","wait_for":"boss","question":{"id":"q","text":"Scope?","recommended_option_id":"absent"}}),
        serde_json::json!({"state":"waiting","wait_for":"boss","question":{"id":"q","text":"Scope?","options":[{"id":"x","label":"A"},{"id":"x","label":"B"}]}}),
        serde_json::json!({"state":"ready","typo":true}),
    ] {
        let sent = handle_messaging(
            &state,
            &serde_json::json!({"action":"send","to":TEST_UUID_B,"message":"invalid","report":report}),
            Some("invalid-sender"),
        );
        assert!(sent.get("error").is_some(), "accepted {report}: {sent}");
    }
    let read = handle_messaging(
        &state,
        &serde_json::json!({"action":"inbox"}),
        Some("invalid-reader"),
    );
    assert_eq!(read["count"], 0);
    let missing_message = handle_messaging(
        &state,
        &serde_json::json!({"action":"send","to":TEST_UUID_B,"report":{"state":"ready"}}),
        Some("invalid-sender"),
    );
    assert!(missing_message.get("error").is_some());
}
