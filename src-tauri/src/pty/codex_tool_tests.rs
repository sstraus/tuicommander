#[cfg(unix)]
fn codex_tool_receipt(index: usize, native: &serde_json::Value) -> serde_json::Value {
    let mut event: serde_json::Value = serde_json::from_str(
        include_str!("../fixtures/codex_tools_2626/hooks.jsonl")
            .lines()
            .nth(index)
            .unwrap(),
    )
    .unwrap();
    // Only substitute the identities and paths belonging to this test topology.
    event["session_id"] = native["session_id"].clone();
    event["cwd"] = native["cwd"].clone();
    event["transcript_path"] = native["transcript_path"].clone();
    event
}

#[cfg(unix)]
fn feed_codex_tool(tab: &mut CodexHookTab, state: &AppState, event: &serde_json::Value) {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(event.to_string());
    let frame = format!("\x1b]7770;codex-tool={}:{encoded}\x1b\\", tab.tuic);
    let silence = state
        .session_maps
        .silence_states
        .get(&tab.sid)
        .unwrap()
        .clone();
    tab.processor
        .process_chunk(&frame, &silence, &tab.sid, state);
}

// Catches: missing tool hook handling leaves an exact-bound Codex terminal idle mid-turn.
#[cfg(unix)]
#[tokio::test]
async fn codex_tool_edges_restore_busy_without_rebinding_identity() {
    let sid = "codex-tool-activity";
    let state = accumulating_state(sid);
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let native = codex_hook_rollout(scratch.path(), "tool activity");
    let mut tab = CodexHookTab::new(&state, sid, scratch.path());
    tab.emit(&state, &native);
    let prompt = codex_tool_receipt(1, &native);
    tab.emit(&state, &prompt);
    let binding = state
        .session_maps
        .session_states
        .get(sid)
        .unwrap()
        .codex_binding
        .clone();
    for index in [2, 3] {
        tab.processor.handle_tuic_state("idle", sid, &state);
        feed_codex_tool(&mut tab, &state, &codex_tool_receipt(index, &native));
        assert_eq!(
            state
                .session_maps
                .shell_states
                .get(sid)
                .unwrap()
                .load(Ordering::Acquire),
            SHELL_BUSY,
            "native tool edge must restore busy after delayed idle"
        );
        assert_eq!(
            state
                .session_maps
                .session_states
                .get(sid)
                .unwrap()
                .codex_binding,
            binding
        );
    }
}

// Catches: a foreign native session or TUIC owner changing another terminal's activity.
#[cfg(unix)]
#[tokio::test]
async fn codex_tool_edges_ignore_foreign_identity() {
    let sid = "codex-tool-owner";
    let state = accumulating_state(sid);
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let native = codex_hook_rollout(scratch.path(), "own");
    let mut tab = CodexHookTab::new(&state, sid, scratch.path());
    tab.emit(&state, &native);
    tab.emit(&state, &codex_tool_receipt(1, &native));
    tab.processor.handle_tuic_state("idle", sid, &state);
    let mut foreign = codex_tool_receipt(2, &native);
    foreign["session_id"] = uuid::Uuid::new_v4().to_string().into();
    feed_codex_tool(&mut tab, &state, &foreign);
    let owner = tab.tuic.clone();
    tab.tuic = "another-tab".into();
    feed_codex_tool(&mut tab, &state, &codex_tool_receipt(2, &native));
    tab.tuic = owner;
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_IDLE
    );
}

// Catches: native user-input hooks become generic busy and unrelated tool outcomes clear the question.
#[cfg(unix)]
#[tokio::test]
async fn codex_native_question_uses_matching_structured_wait_and_settlement() {
    let sid = "codex-native-hook-question";
    let state = accumulating_state(sid);
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let native = codex_hook_rollout(scratch.path(), "question");
    let mut tab = CodexHookTab::new(&state, sid, scratch.path());
    tab.emit(&state, &native);
    tab.emit(&state, &codex_tool_receipt(5, &native));
    feed_codex_tool(&mut tab, &state, &codex_tool_receipt(6, &native));
    assert!(
        await_session(&state, sid, |s| s.awaiting_input
            && s.question_confident
            && s.question_text.as_deref() == Some("Pick a color"))
        .await
    );
    // A background tool in the same turn cannot settle the native question.
    let mut tool = codex_tool_receipt(3, &native);
    tool["turn_id"] = codex_tool_receipt(5, &native)["turn_id"].clone();
    let mut events = state.event_bus.subscribe();
    feed_codex_tool(&mut tab, &state, &tool);
    while let Ok(event) = events.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = event {
            assert_ne!(parsed["type"], "user-input");
        }
    }
    assert!(
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .awaiting_input
    );
    feed_codex_tool(&mut tab, &state, &codex_tool_receipt(7, &native));
    assert!(await_session(&state, sid, |s| !s.awaiting_input).await);
}

// Catches: cancellation/stop/exit or a replacement prompt leaves a confident hook question latched.
#[cfg(unix)]
#[tokio::test]
async fn codex_native_question_retracts_on_lifecycle_and_ignores_late_repaint() {
    for ending in ["Interrupt", "SessionEnd", "Stop", "UserPromptSubmit"] {
        let sid = "codex-hook-question-end";
        let state = accumulating_state(sid);
        let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let native = codex_hook_rollout(scratch.path(), "lifecycle");
        let mut tab = CodexHookTab::new(&state, sid, scratch.path());
        tab.emit(&state, &native);
        tab.emit(&state, &codex_tool_receipt(5, &native));
        let question = codex_tool_receipt(6, &native);
        feed_codex_tool(&mut tab, &state, &question);
        assert!(await_session(&state, sid, |s| s.awaiting_input).await);
        match ending {
            "Interrupt" | "SessionEnd" => {
                let index = if ending == "Interrupt" { 3 } else { 4 };
                let mut event: serde_json::Value = serde_json::from_str(
                    include_str!("../fixtures/codex_tools_2626/interrupt.jsonl")
                        .lines()
                        .nth(index)
                        .unwrap(),
                )
                .unwrap();
                event["session_id"] = native["session_id"].clone();
                if ending == "Interrupt" {
                    event["turn_id"] = question["turn_id"].clone();
                }
                feed_codex_tool(&mut tab, &state, &event);
            }
            "UserPromptSubmit" => {
                tab.emit(&state, &codex_tool_receipt(1, &native));
                let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
                tab.processor
                    .process_chunk("\x1b]7770;state=prompt\x07", &silence, sid, &state);
            }
            _ => {
                let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
                tab.processor
                    .process_chunk("\x1b]7770;state=idle\x07", &silence, sid, &state);
            }
        }
        assert!(
            await_session(&state, sid, |s| !s.awaiting_input).await,
            "{ending}"
        );
        let mut events = state.event_bus.subscribe();
        feed_codex_tool(&mut tab, &state, &question);
        while let Ok(event) = events.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = event {
                assert_ne!(parsed["type"], "question", "late question after {ending}");
            }
        }
    }
}

// Catches: adding busy hooks dismisses the existing human approval screen path.
#[cfg(unix)]
#[tokio::test]
async fn codex_tool_activity_preserves_existing_human_approval() {
    let sid = "codex-existing-approval";
    let state = accumulating_state(sid);
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let native = codex_hook_rollout(scratch.path(), "approval");
    let mut tab = CodexHookTab::new(&state, sid, scratch.path());
    tab.emit(&state, &native);
    tab.emit(&state, &codex_tool_receipt(1, &native));
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    tab.processor
        .process_chunk("\x1b]7770;state=awaiting\x07", &silence, sid, &state);
    assert!(await_session(&state, sid, |s| s.awaiting_input).await);
    let mut events = state.event_bus.subscribe();
    for index in [2, 3] {
        feed_codex_tool(&mut tab, &state, &codex_tool_receipt(index, &native));
    }
    while let Ok(event) = events.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = event {
            assert!(!matches!(
                parsed["type"].as_str(),
                Some("user-input" | "question-cleared")
            ));
        }
    }
    assert!(
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .awaiting_input
    );
}

// Catches: a build log over 16 KiB silently drops the native post-tool busy edge.
#[cfg(unix)]
#[tokio::test]
async fn codex_tool_large_result_restores_busy() {
    assert_large_codex_receipt_restores_busy(false).await;
}

// Catches: pasting a source file makes every tool edge in that turn disappear.
#[cfg(unix)]
#[tokio::test]
async fn codex_tool_large_prompt_keeps_turn_activity() {
    assert_large_codex_receipt_restores_busy(true).await;
}

#[cfg(unix)]
async fn assert_large_codex_receipt_restores_busy(large_prompt: bool) {
    let sid = "codex-large-native-receipt";
    let state = accumulating_state(sid);
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let native = codex_hook_rollout(scratch.path(), "large receipt");
    let mut tab = CodexHookTab::new(&state, sid, scratch.path());
    tab.emit(&state, &native);
    let mut prompt = codex_tool_receipt(1, &native);
    if large_prompt {
        prompt["prompt"] = serde_json::Value::String("source line\n".repeat(1800));
    }
    tab.emit(&state, &prompt);
    tab.processor.handle_tuic_state("idle", sid, &state);
    let mut tool = codex_tool_receipt(if large_prompt { 2 } else { 3 }, &native);
    if !large_prompt {
        tool["tool_response"] = serde_json::Value::String("build output\n".repeat(1800));
    }
    feed_codex_tool(&mut tab, &state, &tool);
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY,
        "native tool activity must survive a large {}",
        if large_prompt {
            "submitted prompt"
        } else {
            "tool result"
        },
    );
}
