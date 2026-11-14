// Tool readout (story 2636-5551): native tool metadata reaches
// `SessionState.current_tool` through the real chunk pipeline and accumulator.

fn current_tool(state: &AppState, sid: &str) -> Option<String> {
    state
        .session_maps
        .session_states
        .get(sid)
        .unwrap()
        .current_tool
        .clone()
}

fn shell_state(state: &AppState, sid: &str) -> u8 {
    state
        .session_maps
        .shell_states
        .get(sid)
        .unwrap()
        .load(Ordering::Acquire)
}

// Catches: the recorded Claude PreToolUse name is not published, its
// PostToolUse leaves it, or a subagent's tool replaces the main readout.
#[tokio::test]
async fn recorded_claude_tool_receipts_drive_the_readout() {
    let sid = "claude-tool-readout";
    let (state, mut processor) = claude_permission_state(sid);
    let events = recorded_claude_permissions("accept");
    for event in &events[..3] {
        feed_claude_permission(&state, &mut processor, sid, event);
    }
    assert!(await_session(&state, sid, |s| s.current_tool.as_deref() == Some("Bash")).await);
    let mut subagent = events[2].clone();
    subagent["agent_id"] = "subagent-1".into();
    subagent["tool_use_id"] = "toolu_subagent".into();
    subagent["tool_name"] = "Grep".into();
    feed_claude_permission(&state, &mut processor, sid, &subagent);
    for event in &events[3..5] {
        feed_claude_permission(&state, &mut processor, sid, event);
    }
    assert!(await_session(&state, sid, |s| s.current_tool.is_none()).await);
}

// Catches: the recorded Grok camelCase `toolName` is not read, or its
// post_tool_use leaves the readout.
#[tokio::test]
async fn recorded_grok_tool_receipts_drive_the_readout() {
    let sid = "grok-tool-readout";
    let (state, mut processor) = grok_hook_state(sid);
    let events = recorded_grok_hooks("allow");
    for event in &events[..3] {
        feed_grok_hook(&state, &mut processor, sid, event);
    }
    assert!(
        await_session(&state, sid, |s| s.current_tool.as_deref()
            == Some("run_terminal_command"))
        .await
    );
    feed_grok_hook(&state, &mut processor, sid, &events[4]);
    assert!(await_session(&state, sid, |s| s.current_tool.is_none()).await);
}

// Catches: the recorded Codex PreToolUse of the bound turn is not published,
// or its PostToolUse leaves the readout.
#[cfg(unix)]
#[tokio::test]
async fn recorded_codex_tool_receipts_drive_the_readout() {
    let sid = "codex-tool-readout";
    let state = accumulating_state(sid);
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let native = codex_hook_rollout(scratch.path(), "tool readout");
    let mut tab = CodexHookTab::new(&state, sid, scratch.path());
    tab.emit(&state, &native);
    tab.emit(&state, &codex_tool_receipt(1, &native));
    feed_codex_tool(&mut tab, &state, &codex_tool_receipt(2, &native));
    assert!(await_session(&state, sid, |s| s.current_tool.as_deref() == Some("Bash")).await);
    feed_codex_tool(&mut tab, &state, &codex_tool_receipt(3, &native));
    assert!(await_session(&state, sid, |s| s.current_tool.is_none()).await);
}

fn agent_tool_frame(owner: &str, json: serde_json::Value) -> String {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(json.to_string());
    format!("\x1b]7770;agent-tool={owner}:{encoded}\x1b\\")
}

fn pi_tab(state: &Arc<AppState>, sid: &str, owner: &str) -> ChunkProcessor {
    agent_session(state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("pi".into());
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    ChunkProcessor::new(None, Some(owner.into()))
}

fn feed(state: &AppState, processor: &mut ChunkProcessor, sid: &str, chunk: &str) {
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    processor.process_chunk(chunk, &silence, sid, state);
}

// Catches: a tool frame changes coarse state, a frame of one tab reaches the
// other tab, or a coarse idle leaves a tool with no end behind.
#[tokio::test]
async fn agent_tool_frames_stay_metadata_scoped_to_their_tab() {
    let state = accumulating_state("pi-a");
    state
        .session_maps
        .session_states
        .insert("pi-b".into(), crate::state::SessionState::default());
    let mut a = pi_tab(&state, "pi-a", "owner-a");
    let mut b = pi_tab(&state, "pi-b", "owner-b");
    let start = |owner: &str, id: &str, name: &str| {
        agent_tool_frame(
            owner,
            serde_json::json!({"event": "start", "id": id, "name": name}),
        )
    };
    feed(&state, &mut a, "pi-a", &start("owner-a", "call-1", "bash"));
    feed(&state, &mut b, "pi-b", &start("owner-a", "call-2", "read"));
    assert!(
        await_session(&state, "pi-a", |s| s.current_tool.as_deref()
            == Some("bash"))
        .await
    );
    assert_eq!(shell_state(&state, "pi-a"), SHELL_IDLE, "metadata set busy");
    assert_eq!(
        current_tool(&state, "pi-b"),
        None,
        "foreign owner reached tab b"
    );
    // A shell that is idle shows no tool, even before the turn end arrives.
    let snapshot = state.session_state_with_shell("pi-a").unwrap();
    assert_eq!(snapshot.current_tool, None);

    feed(&state, &mut a, "pi-a", "\x1b]7770;state=busy\x1b\\");
    assert_eq!(
        state
            .session_state_with_shell("pi-a")
            .unwrap()
            .current_tool
            .as_deref(),
        Some("bash")
    );
    feed(&state, &mut a, "pi-a", "\x1b]7770;state=idle\x1b\\");
    assert!(await_session(&state, "pi-a", |s| s.current_tool.is_none()).await);
}
