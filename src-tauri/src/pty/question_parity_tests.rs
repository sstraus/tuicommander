/// Replay real PTY input as well as output; snapshots are the accumulator state
/// consumed by both desktop and mobile, not merely emitted parser events.
fn replay_native_question(
    fixture: &str,
    agent: &str,
    mut observe: impl FnMut(&tuic_terminal::pty_capture::CaptureRecord, &crate::state::SessionState),
) {
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(fixture)).unwrap();
    let (rows, cols) = capture.geometry.expect("real recorded geometry");
    let sid = "question-parity";
    let (state, silence) = chunk_trace_state(sid);
    state.config.write().progress_tracking = false;
    state
        .grid
        .vt_log_buffers
        .get(sid)
        .unwrap()
        .lock()
        .resize(rows, cols);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some(agent.into());
    let mut rx = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);
    let mut utf8 = Utf8ReadBuffer::new();
    let mut escapes = EscapeAwareBuffer::new();
    for record in capture.records {
        // Observe immediately before input too: the visible question is what
        // the human actually answered, even if its paint spans several chunks.
        if record.direction == crate::pty_capture::CaptureDirection::Input {
            observe(
                &record,
                &state.session_maps.session_states.get(sid).unwrap(),
            );
            crate::mcp_http::session::apply_input_bookkeeping(
                &state,
                sid,
                std::str::from_utf8(&record.data).unwrap(),
            );
        } else {
            let data = escapes.push(&utf8.push(&record.data));
            let (clean, _) = crate::state::strip_kitty_sequences(&data);
            processor.process_chunk(&clean, &silence, sid, &state);
        }
        while let Ok(event) = rx.try_recv() {
            crate::state::tests_support::apply_replay_event(&state, &event);
        }
        if record.direction == crate::pty_capture::CaptureDirection::Output {
            observe(
                &record,
                &state.session_maps.session_states.get(sid).unwrap(),
            );
        }
    }
}

/// Catches: retained Ink footer bytes re-arm awaiting after Claude exits to zsh.
#[tokio::test]
async fn question_not_rearmed_from_scrollback_after_agent_exit() {
    let capture =
        crate::pty_capture::decode_capture(&agent_prompt_fixture("claude-question-exit-2091.tcap"))
            .unwrap();
    let (rows, cols) = capture.geometry.unwrap();
    let sid = "question-exit-2091";
    let (state, silence) = chunk_trace_state(sid);
    state.config.write().progress_tracking = false;
    state
        .grid
        .vt_log_buffers
        .get(sid)
        .unwrap()
        .lock()
        .resize(rows, cols);
    apply_foreground_agent_observation(
        &state,
        sid,
        1,
        Some("claude".into()),
        false,
        false,
        "claude".into(),
    );
    let mut rx = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);
    let mut utf8 = Utf8ReadBuffer::new();
    let mut escapes = EscapeAwareBuffer::new();
    let mut saw_question = false;
    let mut shell_command = false;
    let mut shell_result = false;
    for record in capture.records {
        if record.direction == crate::pty_capture::CaptureDirection::Input {
            let input = std::str::from_utf8(&record.data).unwrap();
            if input.contains("printf 'POST_EXIT_2091") {
                assert!(saw_question, "the real question must precede agent exit");
                // The recorded API snapshot proves the foreground is now zsh.
                // TCAP records terminal I/O, not OS process observations.
                apply_foreground_agent_observation(&state, sid, 2, None, true, false, "zsh".into());
                shell_command = true;
            }
            crate::mcp_http::session::apply_input_bookkeeping(&state, sid, input);
        } else {
            let data = escapes.push(&utf8.push(&record.data));
            let (clean, _) = crate::state::strip_kitty_sequences(&data);
            processor.process_chunk(&clean, &silence, sid, &state);
            shell_result |= shell_command && clean.contains("POST_EXIT_2091");
        }
        while let Ok(event) = rx.try_recv() {
            crate::state::tests_support::apply_replay_event(&state, &event);
        }
        let session = state.session_maps.session_states.get(sid).unwrap();
        saw_question |= session.awaiting_input && session.choice_prompt.is_some();
    }
    assert!(
        shell_command && shell_result,
        "captured shell command must run"
    );
    let screen = state
        .grid
        .vt_log_buffers
        .get(sid)
        .unwrap()
        .lock()
        .screen_rows();
    assert!(
        crate::output_parser::ink_dialog_footer(&screen).is_some(),
        "stale footer remains visible above the shell"
    );
    let session = state.session_maps.session_states.get(sid).unwrap();
    assert!(
        session.agent_type.is_none(),
        "foreground observation returned to shell"
    );
    assert!(
        !session.awaiting_input,
        "exited agent's retained footer must not re-arm awaiting: {:?}",
        session.question_text
    );
    assert!(session.question_text.is_none());
    assert!(session.choice_prompt.is_none());
    drop(session);

    // The live post-exit check also caught the silence timer resurrecting the
    // retained title after the chunk recovery was fixed. Exercise that producer
    // against this same recorded screen, not a hand-written question fixture.
    {
        let mut sl = silence.lock();
        sl.startup_settled = true;
        sl.note_explicit_state(SHELL_IDLE, true);
        sl.last_output_at = std::time::Instant::now() - SILENCE_QUESTION_THRESHOLD;
        assert!(sl.is_silent(), "the real quiet-shell timer path must run");
    }
    tokio::time::pause();
    let running = Arc::new(AtomicBool::new(true));
    spawn_silence_timer(silence, running.clone(), sid.into(), state.clone());
    tokio::time::sleep(SILENCE_CHECK_INTERVAL * 2).await;
    running.store(false, Ordering::Release);
    while let Ok(event) = rx.try_recv() {
        crate::state::tests_support::apply_replay_event(&state, &event);
    }
    let session = state.session_maps.session_states.get(sid).unwrap();
    assert!(
        !session.awaiting_input,
        "silence must not revive the exited agent's title: {:?}",
        session.question_text
    );
    assert!(session.question_text.is_none());
    assert!(session.choice_prompt.is_none());
}

/// Catches: foreground shell return drops agent identity but leaves its confident
/// question latched until another input happens to clear it.
#[tokio::test]
async fn foreground_agent_exit_clears_captured_question_before_shell_input() {
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "claude-await-exit-before-input-1985.tcap",
    ))
    .unwrap();
    let (rows, cols) = capture.geometry.expect("recorded native geometry");
    let sid = "awaiting-agent-exit-before-input";
    let (state, silence) = chunk_trace_state(sid);
    state.config.write().progress_tracking = false;
    state
        .grid
        .vt_log_buffers
        .get(sid)
        .unwrap()
        .lock()
        .resize(rows, cols);
    apply_foreground_agent_observation(
        &state,
        sid,
        1,
        Some("claude".into()),
        false,
        false,
        "claude".into(),
    );
    let mut rx = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);
    let mut utf8 = Utf8ReadBuffer::new();
    let mut escapes = EscapeAwareBuffer::new();
    let mut saw_question = false;
    for record in capture.records {
        if record.direction == crate::pty_capture::CaptureDirection::Input {
            crate::mcp_http::session::apply_input_bookkeeping(
                &state,
                sid,
                std::str::from_utf8(&record.data).unwrap(),
            );
        } else {
            let data = escapes.push(&utf8.push(&record.data));
            let (clean, _) = crate::state::strip_kitty_sequences(&data);
            processor.process_chunk(&clean, &silence, sid, &state);
        }
        while let Ok(event) = rx.try_recv() {
            crate::state::tests_support::apply_replay_event(&state, &event);
        }
        let session = state.session_maps.session_states.get(sid).unwrap();
        saw_question |= session.awaiting_input && session.question_confident;
    }
    assert!(
        saw_question,
        "the real captured question must reach session state"
    );
    assert!(
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .awaiting_input
    );

    // Non-agent helpers are not exit evidence and must preserve the question.
    apply_foreground_agent_observation(&state, sid, 2, None, false, true, "sleep".into());
    assert!(
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .awaiting_input
    );

    // Recorded post-exit API/OS observations prove the shell owns foreground.
    // TCAP records I/O, not process snapshots; no new user input follows exit.
    apply_foreground_agent_observation(&state, sid, 3, None, true, false, "bash".into());
    while let Ok(event) = rx.try_recv() {
        crate::state::tests_support::apply_replay_event(&state, &event);
    }
    let session = state.session_maps.session_states.get(sid).unwrap();
    assert!(session.agent_type.is_none());
    assert!(
        !session.awaiting_input,
        "returned shell retains exited agent question: {:?}",
        session.question_text
    );
    assert!(session.question_text.is_none());
    assert!(!session.question_confident);
    assert!(session.choice_prompt.is_none());
    drop(session);
    assert!(
        silence.lock().awaiting_rank().is_none(),
        "exited agent still owns ranked awaiting evidence"
    );
}

/// Catches: a reader holding the old identity re-arms a confident dialog after
/// the exit withdrawal has already reached the authoritative session snapshot.
#[tokio::test]
async fn late_captured_repaint_cannot_rearm_question_after_agent_exit() {
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "claude-await-exit-before-input-1985.tcap",
    ))
    .unwrap();
    let (rows, cols) = capture.geometry.expect("recorded native geometry");
    let sid = "late-question-after-agent-exit";
    let (state, silence) = chunk_trace_state(sid);
    state.config.write().progress_tracking = false;
    state
        .grid
        .vt_log_buffers
        .get(sid)
        .unwrap()
        .lock()
        .resize(rows, cols);
    apply_foreground_agent_observation(
        &state,
        sid,
        1,
        Some("claude".into()),
        false,
        false,
        "claude".into(),
    );
    let mut rx = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);
    let mut utf8 = Utf8ReadBuffer::new();
    let mut escapes = EscapeAwareBuffer::new();
    let mut saw_question = false;
    let mut open_dialog = false;
    for record in capture.records {
        if record.direction == crate::pty_capture::CaptureDirection::Input {
            crate::mcp_http::session::apply_input_bookkeeping(
                &state,
                sid,
                std::str::from_utf8(&record.data).unwrap(),
            );
        } else {
            let data = escapes.push(&utf8.push(&record.data));
            let (clean, _) = crate::state::strip_kitty_sequences(&data);
            processor.process_chunk(&clean, &silence, sid, &state);
        }
        while let Ok(event) = rx.try_recv() {
            crate::state::tests_support::apply_replay_event(&state, &event);
        }
        let session = state.session_maps.session_states.get(sid).unwrap();
        saw_question |= session.awaiting_input && session.question_confident;
        drop(session);
        let screen = state
            .grid
            .vt_log_buffers
            .get(sid)
            .unwrap()
            .lock()
            .screen_rows();
        if saw_question && rearm_awaiting_for_open_dialog(&screen, false, false, false).is_some() {
            open_dialog = true;
            break;
        }
    }
    assert!(
        saw_question,
        "the real captured question must reach session state"
    );
    assert!(
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .awaiting_input
    );

    assert!(open_dialog, "capture must reach an open native dialog");

    // Pause this reader immediately after it samples Claude. Commit and apply
    // shell return before allowing the retained captured dialog to be examined.
    let exit_state = Arc::clone(&state);
    processor.after_identity_sample = Some(Box::new(move || {
        let mut exit_rx = exit_state.event_bus.subscribe();
        apply_foreground_agent_observation(&exit_state, sid, 2, None, true, false, "bash".into());
        while let Ok(event) = exit_rx.try_recv() {
            crate::state::tests_support::apply_replay_event(&exit_state, &event);
        }
        assert!(
            !exit_state
                .session_maps
                .session_states
                .get(sid)
                .unwrap()
                .awaiting_input
        );
    }));
    // Erase the captured title during a partial exit repaint, retaining its
    // footer. An unchanged choice would be swallowed by choice-prompt dedup.
    processor.process_chunk("\x1b[?2026l\x1b[3;1H\x1b[2K", &silence, sid, &state);
    let screen = state
        .grid
        .vt_log_buffers
        .get(sid)
        .unwrap()
        .lock()
        .screen_rows();
    assert!(
        rearm_awaiting_for_open_dialog(&screen, false, false, false).is_some(),
        "retained dialog: {screen:?}"
    );

    while let Ok(event) = rx.try_recv() {
        crate::state::tests_support::apply_replay_event(&state, &event);
    }
    let session = state.session_maps.session_states.get(sid).unwrap();
    assert!(session.agent_type.is_none());
    assert!(
        !session.awaiting_input,
        "returned shell retains exited agent question: {:?}",
        session.question_text
    );
    assert!(session.question_text.is_none());
    assert!(!session.question_confident);
    assert!(session.choice_prompt.is_none());
    drop(session);
    assert!(
        silence.lock().awaiting_rank().is_none(),
        "exited agent still owns ranked awaiting evidence"
    );
}

/// Catches: Codex 0.162 queued/open questions missing the shared answer controls.
#[tokio::test]
async fn captured_codex_0162_question_reaches_answer_controls() {
    let mut opened = 0;
    let mut answered = Vec::new();
    let mut final_awaiting = false;
    replay_native_question(
        "codex-native-question-2026.tcap",
        "codex",
        |record, session| {
            if record.direction == crate::pty_capture::CaptureDirection::Input {
                if record.data == b"\x1b[1;2D" {
                    assert!(
                        session.awaiting_input && session.question_confident,
                        "queued question must enable the mobile opening control"
                    );
                    assert!(session.choice_prompt.is_none());
                    opened += 1;
                } else if record.data == b"3" || record.data == b"2" {
                    let prompt = session
                        .choice_prompt
                        .as_ref()
                        .expect("open question options");
                    assert!(session.awaiting_input);
                    assert_eq!(prompt.selection_mode, None, "numbers answer immediately");
                    let labels: Vec<_> = prompt
                        .options
                        .iter()
                        .map(|option| option.label.as_str())
                        .collect();
                    if record.data == b"3" {
                        assert_eq!(prompt.title, "Pick a color.");
                        assert_eq!(labels, ["Red", "Green", "Blue", "Other"]);
                    } else {
                        assert_eq!(prompt.title, "May I run `ls`?");
                        assert_eq!(labels, ["Yes", "No", "Other"]);
                    }
                    answered.push(record.data.clone());
                }
            }
            final_awaiting = session.awaiting_input;
        },
    );
    assert_eq!(opened, 2, "both real queued questions were opened");
    assert_eq!(
        answered,
        [b"3".to_vec(), b"2".to_vec()],
        "one answer per question"
    );
    assert!(!final_awaiting, "answered question must retract");
}
