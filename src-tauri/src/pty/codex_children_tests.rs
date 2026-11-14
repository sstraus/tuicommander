// Codex native child roster (#2635-8052), replayed from the real Codex 0.166.0
// hook stdin in fixtures/codex_children_2635 through the production processor.

/// Line `index` of the recorded child capture, re-owned by this test's native
/// session. A child row keeps a transcript of its own, as Codex sends it.
#[cfg(unix)]
fn codex_child_receipt(index: usize, native: &serde_json::Value) -> serde_json::Value {
    let mut event: serde_json::Value = serde_json::from_str(
        include_str!("../fixtures/codex_children_2635/hooks.jsonl")
            .lines()
            .nth(index)
            .unwrap(),
    )
    .unwrap();
    let child = event.get("agent_id").is_some();
    event["session_id"] = native["session_id"].clone();
    event["cwd"] = native["cwd"].clone();
    event["transcript_path"] = if child && event["hook_event_name"] == "UserPromptSubmit" {
        format!(
            "{}.child.jsonl",
            native["transcript_path"].as_str().unwrap()
        )
        .into()
    } else {
        native["transcript_path"].clone()
    };
    event
}

/// Feed `frames` as one hook body, the way the generated hook writes it.
#[cfg(unix)]
fn feed_codex_frames(tab: &mut CodexHookTab, state: &AppState, frames: &str) {
    let silence = state
        .session_maps
        .silence_states
        .get(&tab.sid)
        .unwrap()
        .clone();
    tab.processor
        .process_chunk(frames, &silence, &tab.sid, state);
}

/// The frames of the generated Codex hook for `event`.
#[cfg(unix)]
fn codex_hook_frames(tab: &CodexHookTab, event: &serde_json::Value) -> String {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(event.to_string());
    match event["hook_event_name"].as_str().unwrap() {
        "SessionStart" => format!(
            "\x1b]7770;codex-session={}:{encoded}\x1b\\\x1b]7770;state=busy\x1b\\",
            tab.tuic
        ),
        "UserPromptSubmit" => format!(
            "\x1b]7770;codex-session={}:{encoded}\x1b\\\x1b]7770;state=prompt\x1b\\",
            tab.tuic
        ),
        "Stop" => "\x1b]7770;state=idle\x1b\\".to_string(),
        _ => format!("\x1b]7770;codex-tool={}:{encoded}\x1b\\", tab.tuic),
    }
}

#[cfg(unix)]
fn codex_shell(state: &AppState, sid: &str) -> u8 {
    state
        .session_maps
        .shell_states
        .get(sid)
        .unwrap()
        .load(Ordering::Acquire)
}

#[cfg(unix)]
fn codex_child_tab(
    state: &Arc<AppState>,
    sid: &str,
    dir: &std::path::Path,
) -> (CodexHookTab, serde_json::Value) {
    let native = codex_hook_rollout(dir, sid);
    let mut tab = CodexHookTab::new(state, sid, dir);
    tab.emit(state, &native);
    (tab, native)
}

// Catches: a lead Stop hides a live Codex child (the tab reads idle while the
// child works), or a child whose SubagentStop never arrives holds the lead busy
// across later turns (#2635-8052).
#[cfg(unix)]
#[tokio::test]
async fn missing_child_stop_does_not_latch_parent_forever() {
    let sid = "codex-child-latch";
    let state = accumulating_state(sid);
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let (mut tab, native) = codex_child_tab(&state, sid, scratch.path());
    // Lead prompt, spawn_agent, SubagentStart, the child's own prompt, the
    // lead's wait_agent — and no SubagentStop.
    for index in [1, 2, 3, 4, 5, 6] {
        let frames = codex_hook_frames(&tab, &codex_child_receipt(index, &native));
        feed_codex_frames(&mut tab, &state, &frames);
    }
    let stop = codex_hook_frames(&tab, &codex_child_receipt(9, &native));
    feed_codex_frames(&mut tab, &state, &stop);
    assert_eq!(
        codex_shell(&state, sid),
        SHELL_BUSY,
        "a live child must keep the settled lead busy"
    );
    assert!(
        await_session(&state, sid, |s| s.native_children == ["default"]).await,
        "the live child must be published"
    );

    // The next lead turn does not inherit the child whose stop went missing.
    let mut next = codex_child_receipt(1, &native);
    next["turn_id"] = uuid::Uuid::new_v4().to_string().into();
    let frames = codex_hook_frames(&tab, &next);
    feed_codex_frames(&mut tab, &state, &frames);
    feed_codex_frames(&mut tab, &state, &stop);
    assert_eq!(codex_shell(&state, sid), SHELL_IDLE);
    assert!(await_session(&state, sid, |s| s.native_children.is_empty()).await);
}

// Catches: the child's settlement is lost or applied twice, so a lead that
// settled while its child worked never returns to idle, or a repeated or early
// SubagentStop reopens the turn.
#[cfg(unix)]
#[tokio::test]
async fn last_child_stop_settles_a_lead_that_already_stopped() {
    let sid = "codex-child-settle";
    let state = accumulating_state(sid);
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let (mut tab, native) = codex_child_tab(&state, sid, scratch.path());
    // A stop for a child that never started changes nothing.
    let early_stop = codex_hook_frames(&tab, &codex_child_receipt(7, &native));
    for index in [1, 2, 3] {
        let frames = codex_hook_frames(&tab, &codex_child_receipt(index, &native));
        feed_codex_frames(&mut tab, &state, &frames);
    }
    feed_codex_frames(&mut tab, &state, &early_stop);
    assert_eq!(codex_shell(&state, sid), SHELL_BUSY);
    let mut other = codex_child_receipt(4, &native);
    other["agent_id"] = uuid::Uuid::new_v4().to_string().into();
    let frames = codex_hook_frames(&tab, &other);
    feed_codex_frames(&mut tab, &state, &frames);
    let stop = codex_hook_frames(&tab, &codex_child_receipt(9, &native));
    feed_codex_frames(&mut tab, &state, &stop);
    assert_eq!(codex_shell(&state, sid), SHELL_BUSY);
    let mut other_stop = codex_child_receipt(7, &native);
    other_stop["agent_id"] = other["agent_id"].clone();
    let frames = codex_hook_frames(&tab, &other_stop);
    feed_codex_frames(&mut tab, &state, &frames);
    assert_eq!(
        codex_shell(&state, sid),
        SHELL_IDLE,
        "the last child settles the lead"
    );
    feed_codex_frames(&mut tab, &state, &frames);
    assert_eq!(
        codex_shell(&state, sid),
        SHELL_IDLE,
        "a repeated stop is idempotent"
    );
}

// Catches: a Codex child's own UserPromptSubmit (lead session id, child
// rollout) rebinds the tab's Chat to the child transcript or moves the lead
// turn, so the lead's own tool hooks are ignored afterwards.
#[cfg(unix)]
#[tokio::test]
async fn codex_child_prompt_keeps_the_lead_binding_and_turn() {
    let sid = "codex-child-binding";
    let state = accumulating_state(sid);
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let (mut tab, native) = codex_child_tab(&state, sid, scratch.path());
    for index in [1, 2, 3, 4] {
        let frames = codex_hook_frames(&tab, &codex_child_receipt(index, &native));
        feed_codex_frames(&mut tab, &state, &frames);
    }
    let binding = |state: &AppState| {
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .codex_binding
            .clone()
    };
    let lead = binding(&state);
    assert!(lead.is_some());
    let frames = codex_hook_frames(&tab, &codex_child_receipt(5, &native));
    feed_codex_frames(&mut tab, &state, &frames);
    assert_eq!(binding(&state), lead, "the child prompt rebound the lead");
    tab.processor.handle_tuic_state("idle", sid, &state);
    let frames = codex_hook_frames(&tab, &codex_child_receipt(6, &native));
    feed_codex_frames(&mut tab, &state, &frames);
    assert_eq!(
        codex_shell(&state, sid),
        SHELL_BUSY,
        "the lead's wait_agent hook must still belong to the lead turn"
    );
}

// Catches: a SubagentStop that reaches TUIC before its SubagentStart (two hook
// processes, out of order) is dropped, so the late start adds a child that
// never stops and holds the settled lead busy until the 5-minute backstop
// (story 2635-8052 validation: stop-before-start).
#[cfg(unix)]
#[tokio::test]
async fn child_stop_before_its_start_does_not_hold_the_lead() {
    let sid = "codex-child-stop-first";
    let state = accumulating_state(sid);
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let (mut tab, native) = codex_child_tab(&state, sid, scratch.path());
    // Lead prompt and spawn_agent, then the child's stop before its start.
    for index in [1, 2, 3, 7, 4] {
        let frames = codex_hook_frames(&tab, &codex_child_receipt(index, &native));
        feed_codex_frames(&mut tab, &state, &frames);
    }
    let stop = codex_hook_frames(&tab, &codex_child_receipt(9, &native));
    feed_codex_frames(&mut tab, &state, &stop);
    assert_eq!(
        codex_shell(&state, sid),
        SHELL_IDLE,
        "a child that already stopped must not hold the lead"
    );
}
