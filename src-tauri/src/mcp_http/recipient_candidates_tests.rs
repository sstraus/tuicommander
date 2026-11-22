// `agent send` to an unknown recipient names the peers it probably meant.

/// Catches: an unresolved recipient answered with prose only, so the caller needs a
/// separate `list_peers` round trip to repair it (observed `list_peers` → `send`
/// pairs), or candidates that include unrelated peers.
#[test]
fn unknown_recipient_lists_close_candidates() {
    let state = test_state();
    for (mcp, name) in [("cand-listener", "listener-abc"), ("cand-worker", "worker")] {
        let reg = handle_messaging(
            &state,
            &serde_json::json!({"action": "register", "name": name}),
            Some(mcp),
        );
        assert_eq!(reg["ok"], true, "{reg}");
    }
    let reg = handle_messaging(
        &state,
        &serde_json::json!({"action": "register", "name": "sender"}),
        Some("cand-sender"),
    );
    assert_eq!(reg["ok"], true, "{reg}");
    let refused = handle_messaging(
        &state,
        &serde_json::json!({"action": "send", "to": "listener", "message": "hi"}),
        Some("cand-sender"),
    );
    assert_eq!(refused["code"], "unknown_recipient", "{refused}");
    let names: Vec<&str> = refused["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["listener-abc"], "{refused}");
    let to = refused["candidates"][0]["to"].as_str().unwrap();
    let sent = handle_messaging(
        &state,
        &serde_json::json!({"action": "send", "to": to, "message": "hi"}),
        Some("cand-sender"),
    );
    assert!(
        sent.get("error").is_none(),
        "candidate must be a valid target: {sent}"
    );
}
