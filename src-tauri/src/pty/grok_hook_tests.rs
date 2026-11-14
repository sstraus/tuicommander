// Catches: cancellation has no registered completion signal and leaves Protocol busy.
#[tokio::test]
async fn grok_cancelled_turn_releases_protocol_busy() {
    let sid = "grok-interrupted-hook";
    let (state, mut processor) = grok_hook_state(sid);
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    // Reproduce the old installation's real prompt signal before its missing end.
    processor.process_chunk("\x1b]7770;state=prompt\x07", &silence, sid, &state);
    for event in recorded_grok_hooks("interrupt") {
        feed_grok_hook(&state, &mut processor, sid, &event);
        if event["hookEventName"] == "stop_cancelled" {
            assert_eq!(
                state
                    .session_maps
                    .shell_states
                    .get(sid)
                    .unwrap()
                    .load(Ordering::Acquire),
                SHELL_IDLE
            );
            assert!(await_session(&state, sid, |s| !s.awaiting_input).await);
            assert!(
                crate::agent_hook::grok_hook_map()
                    .iter()
                    .any(|(e, _, _)| *e == "StopCancelled")
            );
            return;
        }
    }
    panic!("recording must include the interrupted outcome");
}

// Catches: denying a real permission has no PostToolUse and latches awaiting forever.
#[tokio::test]
async fn grok_permission_wait_retracts_on_deny() {
    let sid = "grok-denied-hook";
    let (state, mut processor) = grok_hook_state(sid);
    for event in recorded_grok_hooks("deny") {
        feed_grok_hook(&state, &mut processor, sid, &event);
        match event["hookEventName"].as_str().unwrap() {
            "notification" => assert!(
                await_session(&state, sid, |s| s.awaiting_input && s.question_confident).await
            ),
            "permission_denied" => assert!(await_session(&state, sid, |s| !s.awaiting_input).await),
            "stop_cancelled" => assert_eq!(
                state
                    .session_maps
                    .shell_states
                    .get(sid)
                    .unwrap()
                    .load(Ordering::Acquire),
                SHELL_IDLE
            ),
            _ => {}
        }
    }
}

// Catches: answering with bare Enter has no text input; only matching tool completion retires the wait.
#[tokio::test]
async fn grok_permission_allow_retracts_without_input_text() {
    let sid = "grok-allowed-hook";
    let (state, mut processor) = grok_hook_state(sid);
    for event in recorded_grok_hooks("allow") {
        feed_grok_hook(&state, &mut processor, sid, &event);
        if event["hookEventName"] == "notification" {
            assert!(await_session(&state, sid, |s| s.awaiting_input).await);
        }
        if event["hookEventName"] == "post_tool_use" {
            assert!(await_session(&state, sid, |s| !s.awaiting_input).await);
        }
    }
}

// Catches: a sibling tool, inherited coarse hook or delayed old turn end clears a newer permission.
#[tokio::test]
async fn grok_permission_ignores_unrelated_settlement_and_imported_hooks() {
    let sid = "grok-matching-request";
    let (state, mut processor) = grok_hook_state(sid);
    let events = recorded_grok_hooks("deny");
    for event in &events[..4] {
        feed_grok_hook(&state, &mut processor, sid, event);
    }
    assert!(await_session(&state, sid, |s| s.awaiting_input).await);
    let mut published = state.event_bus.subscribe();
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    processor.process_chunk(
        "\x1b]7770;state=busy\x07\x1b]7770;state=idle\x07",
        &silence,
        sid,
        &state,
    );
    let mut other = events[4].clone();
    other["toolUseId"] = "other-request".into();
    feed_grok_hook(&state, &mut processor, sid, &other);
    let mut old_end = events[5].clone();
    old_end["promptId"] = "older-turn".into();
    feed_grok_hook(&state, &mut processor, sid, &old_end);
    // A generic notification is also not a request or a settlement.
    let mut notification = events[3].clone();
    notification["notificationType"] = "idle_prompt".into();
    feed_grok_hook(&state, &mut processor, sid, &notification);
    while let Ok(event) = published.try_recv() {
        if let crate::state::AppEvent::PtyParsed { parsed, .. } = event {
            assert!(!matches!(
                parsed["type"].as_str(),
                Some("user-input" | "protocol-question-cleared")
            ));
        }
    }
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
    feed_grok_hook(&state, &mut processor, sid, &events[4]);
    assert!(await_session(&state, sid, |s| !s.awaiting_input).await);
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
}

// Catches: queued question and completion in one PTY read rearm an already-settled wait.
#[tokio::test]
async fn grok_coalesced_hook_frames_finish_idle_without_a_wait() {
    let joined: String = recorded_grok_hooks("deny")
        .iter()
        .map(grok_hook_frame)
        .collect();
    for fragment_size in [joined.len(), 19] {
        let sid = format!("grok-coalesced-{fragment_size}");
        let (state, mut processor) = grok_hook_state(&sid);
        let silence = state.session_maps.silence_states.get(&sid).unwrap().clone();
        for fragment in joined.as_bytes().chunks(fragment_size) {
            processor.process_chunk(
                std::str::from_utf8(fragment).unwrap(),
                &silence,
                &sid,
                &state,
            );
        }
        assert_eq!(
            state
                .session_maps
                .shell_states
                .get(&sid)
                .unwrap()
                .load(Ordering::Acquire),
            SHELL_IDLE
        );
        assert!(await_session(&state, &sid, |s| !s.awaiting_input).await);
    }
}

// Catches: a supported settlement other than success/denial keeps a confident wait sticky.
#[tokio::test]
async fn grok_permission_clears_on_failure_prompt_stop_and_exit() {
    for outcome in [
        "post_tool_use_failure",
        "user_prompt_submit",
        "stop",
        "stop_failure",
        "stop_cancelled",
        "session_end",
    ] {
        let sid = format!("grok-settle-{outcome}");
        let (state, mut processor) = grok_hook_state(&sid);
        let events = recorded_grok_hooks("deny");
        for event in &events[..4] {
            feed_grok_hook(&state, &mut processor, &sid, event);
        }
        assert!(await_session(&state, &sid, |s| s.awaiting_input).await);
        // Perturb a recorded receipt to cover each documented settlement, not a new fixture.
        let mut settlement = if outcome == "post_tool_use_failure" {
            events[4].clone()
        } else {
            events[5].clone()
        };
        settlement["hookEventName"] = outcome.into();
        if outcome == "user_prompt_submit" {
            settlement["promptId"] = "next-turn".into();
        }
        feed_grok_hook(&state, &mut processor, &sid, &settlement);
        assert!(
            await_session(&state, &sid, |s| !s.awaiting_input).await,
            "{outcome}"
        );
        let expected = if matches!(outcome, "post_tool_use_failure" | "user_prompt_submit") {
            SHELL_BUSY
        } else {
            SHELL_IDLE
        };
        assert_eq!(
            state
                .session_maps
                .shell_states
                .get(&sid)
                .unwrap()
                .load(Ordering::Acquire),
            expected,
            "{outcome}"
        );
        // Replayed attention after settlement has no pending request to revive.
        feed_grok_hook(&state, &mut processor, &sid, &events[3]);
        assert!(
            !state
                .session_maps
                .session_states
                .get(&sid)
                .unwrap()
                .awaiting_input
        );
    }
}

// Catches: generic notifications or child/cross-tab receipts steal the parent's state.
#[tokio::test]
async fn grok_ignores_nonpermission_child_and_wrong_owner_events() {
    let sid = "grok-isolation";
    let (state, mut processor) = grok_hook_state(sid);
    let events = recorded_grok_hooks("deny");
    feed_grok_hook(&state, &mut processor, sid, &events[1]);
    for kind in ["idle_prompt", "task_complete", "other"] {
        let mut notification = events[3].clone();
        notification["notificationType"] = kind.into();
        feed_grok_hook(&state, &mut processor, sid, &notification);
    }
    let mut child = events[0].clone();
    child["subagentType"] = "worker".into();
    feed_grok_hook(&state, &mut processor, sid, &child);
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let wrong = grok_hook_frame(&events[0]).replace("grok-test-owner:", "different-tab:");
    processor.process_chunk(&wrong, &silence, sid, &state);
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_IDLE
    );
    assert!(
        !state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .awaiting_input
    );
}

// Catches: current native approval options fail to expose the recorded refusal key.
#[tokio::test]
async fn grok_recorded_hook_denial_exports_native_refusal() {
    let mut refusals = 0;
    replay_native_question("grok-hook-denial-2624.tcap", "grok", |record, session| {
        if record.direction == crate::pty_capture::CaptureDirection::Input && record.data == b"4" {
            let prompt = session
                .choice_prompt
                .as_ref()
                .expect("recorded native approval choices");
            assert!(session.awaiting_input && session.question_confident);
            assert!(
                prompt
                    .options
                    .iter()
                    .any(|option| option.key == "4" && option.label.contains("No, reject"))
            );
            refusals += 1;
        }
    });
    assert_eq!(refusals, 1);
}

// Catches: native permission ownership suppresses unrelated strict Ask selectors and text waits.
#[tokio::test]
async fn grok_native_hooks_preserve_ask_choices_and_free_text() {
    for fixture in [
        "grok-native-colour-2027.tcap",
        "grok-native-permission-2027.tcap",
        "grok-hook-ask-2624.tcap",
    ] {
        let mut choices = false;
        let mut free_text = false;
        replay_native_question_with_hooks(fixture, "grok", true, |record, session| {
            if session.awaiting_input && session.question_confident {
                choices |= session.choice_prompt.as_ref().is_some_and(|prompt| {
                    prompt
                        .options
                        .last()
                        .is_some_and(|option| option.key == "z")
                });
                free_text |= session.choice_prompt.is_none();
            }
            if record.direction == crate::pty_capture::CaptureDirection::Input
                && matches!(record.data.as_slice(), b"2" | b"z")
            {
                assert!(
                    session.awaiting_input && session.choice_prompt.is_some(),
                    "{fixture}"
                );
            }
        });
        assert!(choices, "{fixture}: strict Ask must export native choices");
        if fixture.contains("permission") || fixture.contains("hook-ask") {
            assert!(free_text, "z must replace choices with a text-only wait");
        }
    }
}
// Catches: SessionEnd permanently disables same-ID resume without OSC 133 shell markers.
#[tokio::test]
async fn grok_same_session_resume_reopens_prompt_without_shell_markers() {
    let sid = "grok-resumed-same-id";
    let (state, mut processor) = grok_hook_state(sid);
    let events = recorded_grok_hooks("deny");
    for event in &events[..4] {
        feed_grok_hook(&state, &mut processor, sid, event);
    }
    assert!(await_session(&state, sid, |s| s.awaiting_input).await);
    let mut end = events[5].clone();
    end["hookEventName"] = "session_end".into();
    feed_grok_hook(&state, &mut processor, sid, &end);
    assert!(await_session(&state, sid, |s| !s.awaiting_input).await);
    // A replay of the closed prompt is not a new lifecycle.
    feed_grok_hook(&state, &mut processor, sid, &events[0]);
    feed_grok_hook(&state, &mut processor, sid, &events[2]);
    feed_grok_hook(&state, &mut processor, sid, &events[3]);
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_IDLE
    );
    assert!(
        !state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .awaiting_input
    );
    let mut submit = events[0].clone();
    submit["promptId"] = "resumed-prompt".into();
    feed_grok_hook(&state, &mut processor, sid, &submit);
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
    for event in &events[2..4] {
        feed_grok_hook(&state, &mut processor, sid, event);
    }
    assert!(await_session(&state, sid, |s| s.awaiting_input).await);
    // A delayed completion of the old prompt must not retire the resumed wait.
    feed_grok_hook(&state, &mut processor, sid, &events[5]);
    assert!(
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .awaiting_input
    );
    feed_grok_hook(&state, &mut processor, sid, &events[4]);
    assert!(await_session(&state, sid, |s| !s.awaiting_input).await);
    let mut stop = events[5].clone();
    stop["promptId"] = "resumed-prompt".into();
    feed_grok_hook(&state, &mut processor, sid, &stop);
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
