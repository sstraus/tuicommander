// Owner-controlled literal terminal reads (#2643-6b38).

const LITERAL_FIXTURE_2643: &str = "TEST_TOKEN=not-a-secret";

/// One session whose fixture line sits in scrollback and again on the screen,
/// so the tail (screen), delta, absolute and raw reads all have it to return.
fn literal_read_session_2643(state: &Arc<AppState>, sid: &str, extra: &str) {
    use crate::OutputRingBuffer;
    use crate::state::VtLogBuffer;
    let mut text = format!("{LITERAL_FIXTURE_2643}{extra}\r\n");
    for row in 0..30 {
        text.push_str(&format!(
            "filler row {row:02} of the literal read fixture\r\n"
        ));
    }
    text.push_str(&format!("{LITERAL_FIXTURE_2643}{extra}\r\n"));
    let mut ring = OutputRingBuffer::new(1 << 16);
    ring.write(text.as_bytes());
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), parking_lot::Mutex::new(ring));
    let mut vt = VtLogBuffer::new(24, 80, 100);
    vt.process(text.as_bytes());
    state
        .grid
        .vt_log_buffers
        .insert(sid.to_string(), parking_lot::Mutex::new(vt));
}

/// The four read shapes of `session action=output`.
fn literal_read_modes_2643(sid: &str, literal: bool) -> Vec<(&'static str, serde_json::Value)> {
    let base = |extra: serde_json::Value| {
        let mut args = serde_json::json!({"action": "output", "session_id": sid, "limit": 200});
        if literal {
            args["literal"] = true.into();
        }
        for (key, value) in extra.as_object().unwrap() {
            args[key] = value.clone();
        }
        args
    };
    vec![
        ("screen", base(serde_json::json!({}))),
        ("delta", base(serde_json::json!({"since_cursor": 0}))),
        ("absolute", base(serde_json::json!({"from_line": 0}))),
        (
            "raw",
            base(serde_json::json!({"format": "raw", "from_byte": 0, "limit": 100000})),
        ),
    ]
}

// Catches: trusted owner cannot inspect TEST_TOKEN=not-a-secret literally.
#[test]
fn owner_literal_read_preserves_fake_token() {
    let state = test_state();
    let sid = "literal-owner-2643";
    literal_read_session_2643(&state, sid, "");
    state.secrets.set_literal_reads(sid, true);
    for (mode, args) in literal_read_modes_2643(sid, true) {
        let response = handle_session(&state, &args, None);
        let data = response["data"]
            .as_str()
            .unwrap_or_else(|| panic!("{mode}: {response}"));
        assert!(
            data.contains(LITERAL_FIXTURE_2643),
            "{mode}: owner-granted literal read lost the fixture: {data}"
        );
        assert_eq!(response["literal"], true, "{mode}: {response}");
    }
    // The grant does not change a reader that did not ask for literal text.
    for (mode, args) in literal_read_modes_2643(sid, false) {
        let response = handle_session(&state, &args, None);
        let data = response["data"].as_str().unwrap();
        assert!(
            !data.contains("not-a-secret"),
            "{mode}: default read unmasked: {data}"
        );
        assert!(
            data.contains("TEST_TOKEN="),
            "{mode}: masked read lost the key: {data}"
        );
    }
    // Revocation and session close both restore masking.
    state.secrets.set_literal_reads(sid, false);
    for (mode, args) in literal_read_modes_2643(sid, true) {
        let data = handle_session(&state, &args, None)["data"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(
            !data.contains("not-a-secret"),
            "{mode}: revoked grant still literal: {data}"
        );
    }
    state.secrets.set_literal_reads(sid, true);
    state.secrets.release_session(sid);
    assert!(!state.secrets.literal_reads_granted(sid));
}

// Catches: prompt-injected model arguments enable literal reads or disclose
// registered scoped secrets.
#[tokio::test]
async fn model_cannot_enable_literal_or_reveal_scoped_secret() {
    let state = test_state();
    let sid = "literal-model-2643";
    let scoped = "canary-scope-2643-value";
    // The scoped value is registered after it reached the buffers, so ingest
    // masking never saw it: only read-time masking can keep it hidden.
    literal_read_session_2643(&state, sid, &format!(" SCOPED={scoped}"));

    // No owner grant: every model-chosen argument shape stays masked.
    for (mode, mut args) in literal_read_modes_2643(sid, true) {
        args["owner"] = true.into();
        args["redaction"] = "off".into();
        let response = handle_session(&state, &args, None);
        let data = response["data"].as_str().unwrap();
        assert!(
            !data.contains("not-a-secret"),
            "{mode}: model enabled literal: {data}"
        );
        assert!(
            data.contains("TEST_TOKEN="),
            "{mode}: masked read lost the key: {data}"
        );
        assert_eq!(response["literal"], false, "{mode}: {response}");
    }
    // A grant for another session does not leak into this one.
    state.secrets.set_literal_reads("some-other-session", true);
    for (mode, args) in literal_read_modes_2643(sid, true) {
        let data = handle_session(&state, &args, None)["data"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(
            !data.contains("not-a-secret"),
            "{mode}: foreign grant applied: {data}"
        );
    }
    // The session tool offers the model no way to grant itself.
    let attempt = handle_session(
        &state,
        &serde_json::json!({"action": "literal_reads", "session_id": sid, "enabled": true}),
        None,
    );
    assert!(attempt.get("error").is_some(), "{attempt}");
    assert!(!state.secrets.literal_reads_granted(sid));

    // With the owner grant, the scoped secret stays masked in every mode.
    let stored = crate::secrets::handle_secret(
        &state,
        &serde_json::json!({"action": "put", "names": ["SCOPED"], "values": [scoped],
            "scope": "task-literal-2643", "ttl": 60}),
    )
    .await;
    assert_eq!(stored["status"], "stored", "{stored}");
    let environment = state
        .secrets
        .scope_environment("task-literal-2643")
        .unwrap();
    state.secrets.bind_scope(sid, &environment);
    state.secrets.set_literal_reads(sid, true);
    for (mode, args) in literal_read_modes_2643(sid, true) {
        let response = handle_session(&state, &args, None);
        let data = response["data"].as_str().unwrap();
        assert!(data.contains(LITERAL_FIXTURE_2643), "{mode}: {data}");
        assert!(
            !data.contains(scoped),
            "{mode}: literal read revealed a scoped secret: {data}"
        );
    }

    // A literal snapshot cannot be continued after the owner revokes the grant.
    let mut paged = literal_read_modes_2643(sid, true).remove(0).1;
    paged["byte_budget"] = 1024.into();
    let first = handle_session(&state, &paged, None);
    let token = first["continuation"]
        .as_str()
        .unwrap_or_else(|| panic!("{first}"))
        .to_owned();
    state.secrets.set_literal_reads(sid, false);
    paged["continuation"] = token.into();
    let next = handle_session(&state, &paged, None);
    assert!(
        next.get("error").is_some(),
        "revoked literal page continued: {next}"
    );
}
