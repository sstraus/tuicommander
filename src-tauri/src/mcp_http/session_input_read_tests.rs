// `session input` with `wait_ms`: type into a shell and read what it printed.

fn emulate_shell_output(state: &Arc<AppState>, sid: &str, text: &str) {
    state
        .grid
        .vt_log_buffers
        .get(sid)
        .unwrap()
        .lock()
        .process(text.as_bytes());
    state
        .session_maps
        .output_buffers
        .get(sid)
        .unwrap()
        .lock()
        .write(text.as_bytes());
}

/// Catches: the fused call returning before the command printed (the shell still
/// reads idle right after the write), returning old scrollback instead of the new
/// output, or accepting an agent composer, which must go through `submit`.
#[tokio::test]
async fn input_with_wait_ms_returns_what_the_shell_printed() {
    let state = test_state();
    let sid = "input-read-shell";
    crate::test_support::insert_recording_session(&state, sid);
    state.grid.vt_log_buffers.insert(
        sid.into(),
        parking_lot::Mutex::new(crate::state::VtLogBuffer::new(24, 80, 1000)),
    );
    state.session_maps.output_buffers.insert(
        sid.into(),
        parking_lot::Mutex::new(crate::OutputRingBuffer::new(65536)),
    );
    emulate_shell_output(&state, sid, "$ echo before-marker\r\nbefore-marker\r\n$ ");

    // The command prints in two bursts, the second after 200 ms: a call that
    // returned on the first quiet poll, or on idle shell state, would miss it.
    let shell = Arc::clone(&state);
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        emulate_shell_output(&shell, sid, "sleep 0.2; echo after\r\n");
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        emulate_shell_output(&shell, sid, "after-42\r\n$ ");
    });
    let read = handle_session_input_and_read(
        &state,
        &serde_json::json!({"action": "input", "session_id": sid,
            "input": "sleep 0.2; echo after", "special_key": "enter", "wait_ms": 10000}),
        None,
    )
    .await;
    assert_eq!(read["ok"], true, "{read}");
    assert_eq!(read["settled"], true, "{read}");
    let printed = read["output"].as_str().unwrap_or_default();
    assert!(
        printed.contains("after-42"),
        "command output missing: {read}"
    );
    assert!(
        !printed.contains("before-marker"),
        "old scrollback returned: {read}"
    );

    state
        .session_maps
        .session_states
        .entry(sid.to_owned())
        .or_default()
        .agent_type = Some("claude".into());
    let refused = handle_session_input_and_read(
        &state,
        &serde_json::json!({"action": "input", "session_id": sid, "input": "x", "wait_ms": 100}),
        None,
    )
    .await;
    assert!(
        refused["error"]
            .as_str()
            .is_some_and(|e| e.contains("submit")),
        "agent composer must be refused: {refused}"
    );
}
