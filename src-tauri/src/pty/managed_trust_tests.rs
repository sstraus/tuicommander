// Recorded in an owned Python PTY with Claude Code 2.1.286, 41x80, on
// 2026-10-17. See claude-mobile-startup-1987.md for provenance and SHA-256.
fn recorded_claude_startup_trust() -> tuic_terminal::pty_capture::DecodedCapture {
    crate::pty_capture::decode_capture(&agent_prompt_fixture("claude-mobile-startup-1987.tcap"))
        .unwrap()
}

fn recorded_default_no_trust_screen() -> Vec<String> {
    let capture = recorded_claude_startup_trust();
    let (rows, cols) = capture.geometry.unwrap();
    let mut vt = VtLogBuffer::new(rows, cols, 2000);
    for record in capture.records {
        if record.direction == crate::pty_capture::CaptureDirection::Input {
            break;
        }
        vt.process(&record.data);
    }
    let screen = vt.screen_rows();
    assert!(screen.iter().any(|row| row.contains("Quick safety check:")));
    assert!(
        screen
            .iter()
            .any(|row| row.trim_start().starts_with('❯') && row.contains("No, exit"))
    );
    assert!(
        screen
            .iter()
            .any(|row| row.contains("Yes, I trust this folder"))
    );
    screen
}

// Catches: assuming Yes precedes the selected No rejects the real startup dialog.
#[test]
fn recorded_claude_default_no_trust_is_recognized() {
    assert!(managed_claude_trust_dialog(
        &recorded_default_no_trust_screen()
    ));
}

// Catches: accepting either identity marker alone can answer another dialog.
#[test]
fn recorded_claude_trust_requires_both_identity_markers() {
    let screen = recorded_default_no_trust_screen();
    let mut accepted = Vec::new();
    for marker in [
        "Quick safety check:",
        "Is this a project you created or one you trust?",
    ] {
        assert!(screen.iter().any(|row| row.contains(marker)));
        let incomplete: Vec<_> = screen.iter().map(|row| row.replace(marker, "")).collect();
        if managed_claude_trust_dialog(&incomplete) {
            accepted.push(marker);
        }
    }
    assert!(
        accepted.is_empty(),
        "accepted with missing markers: {accepted:?}"
    );
}

#[cfg(unix)]
fn replay_recorded_managed_trust(agent: &str, managed: bool) -> Vec<u8> {
    let capture = recorded_claude_startup_trust();
    let (rows, cols) = capture.geometry.unwrap();
    let sid = "recorded-managed-trust";
    let (state, silence) = chunk_trace_state(sid);
    state.config.write().progress_tracking = false;
    let written = insert_recording_session(&state, sid);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some(agent.into());
    state
        .grid
        .vt_log_buffers
        .get(sid)
        .unwrap()
        .lock()
        .resize(rows, cols);
    if managed {
        state.managed_first_run.insert(
            sid.into(),
            crate::state::ManagedFirstRun {
                answer_trust: true,
                ..Default::default()
            },
        );
    }
    let mut processor = ChunkProcessor::new(None, None);
    let mut utf8 = Utf8ReadBuffer::new();
    let mut escapes = EscapeAwareBuffer::new();
    // The second repaint is still the same open dialog, not a second allowance.
    for _ in 0..2 {
        for record in &capture.records {
            if record.direction == crate::pty_capture::CaptureDirection::Input {
                break;
            }
            let data = escapes.push(&utf8.push(&record.data));
            let (clean, _) = crate::state::strip_kitty_sequences(&data);
            processor.process_chunk(&clean, &silence, sid, &state);
        }
    }
    let result = written.lock().unwrap().clone();
    close_pty_core(&state, sid, false);
    result
}

// Catches: missing the real dialog, selecting the wrong row, or answering a repaint twice.
#[cfg(unix)]
#[test]
fn recorded_claude_default_no_trust_receives_one_recorded_answer() {
    let recorded_answer = recorded_claude_startup_trust()
        .records
        .into_iter()
        .find(|record| record.direction == crate::pty_capture::CaptureDirection::Input)
        .unwrap()
        .data;
    assert_eq!(recorded_answer, b"\x1b[B\r");
    assert_eq!(
        replay_recorded_managed_trust("claude", true),
        recorded_answer
    );
}

// Catches: managed trust acceptance leaking into user-opened or other-agent terminals.
#[cfg(unix)]
#[test]
fn recorded_claude_trust_is_not_answered_without_managed_claude_identity() {
    let unexpected: Vec<_> = [("claude", false), ("codex", true), ("", true)]
        .into_iter()
        .filter_map(|(agent, managed)| {
            let written = replay_recorded_managed_trust(agent, managed);
            (!written.is_empty()).then_some((agent, managed, written))
        })
        .collect();
    assert!(
        unexpected.is_empty(),
        "unexpected trust writes: {unexpected:?}"
    );
}

// Recorded with Claude Code 2.1.322; see claude-first-run-3017.md.
#[cfg(unix)]
fn recorded_claude_first_run() -> tuic_terminal::pty_capture::DecodedCapture {
    crate::pty_capture::decode_capture(&agent_prompt_fixture("claude-first-run-3017.tcap")).unwrap()
}

/// Replay the OUTPUT records of `capture` through the production chunk
/// processor of a managed Claude child; `until_input` stops at the first
/// recorded answer. Returns the bytes TUIC wrote to the child.
#[cfg(unix)]
fn replay_managed_first_run(
    state: &Arc<AppState>,
    silence: &Arc<Mutex<SilenceState>>,
    sid: &str,
    passes: usize,
    until_input: bool,
) -> Vec<u8> {
    let capture = recorded_claude_first_run();
    let (rows, cols) = capture.geometry.unwrap();
    state.config.write().progress_tracking = false;
    let written = insert_recording_session(state, sid);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("claude".into());
    state
        .grid
        .vt_log_buffers
        .get(sid)
        .unwrap()
        .lock()
        .resize(rows, cols);
    let mut processor = ChunkProcessor::new(None, None);
    let mut utf8 = Utf8ReadBuffer::new();
    let mut escapes = EscapeAwareBuffer::new();
    for _ in 0..passes {
        for record in &capture.records {
            if record.direction == crate::pty_capture::CaptureDirection::Input {
                if until_input {
                    break;
                }
                continue;
            }
            let data = escapes.push(&utf8.push(&record.data));
            let (clean, _) = crate::state::strip_kitty_sequences(&data);
            processor.process_chunk(&clean, silence, sid, state);
        }
    }
    // Drop the emulator's own answers to the capture's terminal queries
    // (`ESC[?6c` for `ESC[c`): they are not dialog answers.
    let written = written.lock().unwrap().clone();
    let mut answers = Vec::new();
    let mut rest = written.as_slice();
    while !rest.is_empty() {
        if rest.starts_with(b"\x1b[?")
            && let Some(end) = rest.iter().position(|byte| *byte == b'c')
        {
            rest = &rest[end + 1..];
            continue;
        }
        answers.push(rest[0]);
        rest = &rest[1..];
    }
    answers
}

// Catches: the external CLAUDE.md imports dialog stays unanswered, so a managed
// (or workflow-launched) child sits on it until a person presses Enter, and a
// repaint of either dialog sends a second answer.
#[cfg(unix)]
#[test]
fn recorded_first_run_answers_trust_then_external_imports_once() {
    let recorded: Vec<u8> = recorded_claude_first_run()
        .records
        .into_iter()
        .filter(|record| record.direction == crate::pty_capture::CaptureDirection::Input)
        .flat_map(|record| record.data)
        .collect();
    assert_eq!(recorded, b"\x1b[B\r\r");
    let sid = "recorded-first-run";
    let (state, silence) = chunk_trace_state(sid);
    state.managed_first_run.insert(
        sid.into(),
        crate::state::ManagedFirstRun {
            answer_trust: true,
            ..Default::default()
        },
    );
    // The second pass repaints both open dialogs: no second answer.
    let written = replay_managed_first_run(&state, &silence, sid, 2, false);
    close_pty_core(&state, sid, false);
    assert_eq!(written, recorded);
}

// Catches: a first-run dialog TUIC does not answer (here trust with the
// opt-out) raises no wait, so the parent never learns its child is blocked.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn unanswered_first_run_dialog_reaches_the_parent_with_its_text() {
    let sid = "unanswered-first-run";
    let (state, silence) = chunk_trace_state(sid);
    crate::state::AppState::spawn_session_state_accumulator(state.clone());
    state
        .session_maps
        .session_parent
        .insert(sid.into(), "parent-1".into());
    state.agent_inbox.entry("parent-1".into()).or_default();
    // A managed child whose trust answer is opted out (`skip_trust_dialog=false`).
    state
        .managed_first_run
        .insert(sid.into(), crate::state::ManagedFirstRun::default());
    let written = replay_managed_first_run(&state, &silence, sid, 1, true);
    let mut notices = Vec::new();
    for _ in 0..200 {
        notices = state
            .agent_inbox
            .get("parent-1")
            .map(|inbox| {
                inbox
                    .iter()
                    .filter_map(|m| serde_json::from_str::<serde_json::Value>(&m.content).ok())
                    .filter(|m| m["state"] == "awaiting_input")
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !notices.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    close_pty_core(&state, sid, false);
    assert!(written.is_empty(), "an unanswered dialog gets no keys");
    assert_eq!(
        notices.len(),
        1,
        "the parent must be told once: {notices:?}"
    );
    let prompt = notices[0]["prompt"].as_str().unwrap_or_default();
    for text in [
        "Quick safety check:",
        "No, exit",
        "Yes, I trust this folder",
    ] {
        assert!(
            prompt.contains(text),
            "dialog text {text:?} missing: {prompt:?}"
        );
    }
}
