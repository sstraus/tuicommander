/// Catches: a hard-wrapped approval footer losing mobile controls or latching after answer.
#[tokio::test]
async fn captured_ego_wrapped_approval_reaches_mobile_answer_controls() {
    let mut answered = false;
    let mut resumed = false;
    replay_native_question(
        "ego-wrapped-approval-2102.tcap",
        "ego",
        |record, session| {
            if record.direction == crate::pty_capture::CaptureDirection::Input
                && record.data == b"y"
            {
                assert!(session.awaiting_input && session.question_confident);
                let prompt = session
                    .choice_prompt
                    .as_ref()
                    .expect("wrapped native approval controls");
                assert_eq!(prompt.title, "Allow run printf 'PARITY_P_OK\\n'?");
                assert_eq!(
                    session.question_text.as_deref(),
                    Some(prompt.title.as_str())
                );
                assert_eq!(
                    prompt
                        .options
                        .iter()
                        .map(|option| option.key.as_str())
                        .collect::<Vec<_>>(),
                    ["y", "a", "n", "e", "d"]
                );
                assert_eq!(prompt.options[1].label, "yes, remember rule (details: d)");
                answered = true;
            }
            if answered
                && record.direction == crate::pty_capture::CaptureDirection::Output
                && record
                    .data
                    .windows(b"7770;state=busy".len())
                    .any(|bytes| bytes == b"7770;state=busy")
            {
                assert!(!session.awaiting_input && session.choice_prompt.is_none());
                resumed = true;
            }
        },
    );
    assert!(
        answered && resumed,
        "mobile approval must reach the native CLI and resume it"
    );
}

/// Catches: a successful tool receipt leaking into the shared native selector title.
#[tokio::test]
async fn captured_ego_selector_title_excludes_tool_receipts() {
    let mut saw_selector = false;
    replay_native_question("ego-mcp-questions-2088-2089.tcap", "ego", |_, session| {
        let Some(prompt) = session.choice_prompt.as_ref() else {
            return;
        };
        if prompt
            .options
            .iter()
            .map(|option| option.label.as_str())
            .collect::<Vec<_>>()
            != ["Red", "Green", "Blue"]
        {
            return;
        }
        saw_selector = true;
        assert_eq!(prompt.title, "Pick a colour?");
        assert_eq!(session.question_text.as_deref(), Some("Pick a colour?"));
        assert!(session.awaiting_input && session.question_confident);
        assert_eq!(
            prompt.selection_mode,
            Some(crate::output_parser::ChoiceSelectionMode::NavigateEnter)
        );
    });
    assert!(
        saw_selector,
        "recorded selector must reach shared client state"
    );
}

/// Catches: real ego diagnostics/submitted history contaminating selector titles.
#[tokio::test]
async fn captured_ego_selector_title_excludes_diagnostics() {
    let mut saw_color = false;
    let mut saw_permission = false;
    let mut final_awaiting = false;
    replay_native_question(
        "ego-native-questions-2028.tcap",
        "ego",
        |record, session| {
            if let Some(prompt) = session.choice_prompt.as_ref() {
                let labels: Vec<_> = prompt
                    .options
                    .iter()
                    .map(|option| option.label.as_str())
                    .collect();
                if labels == ["Red", "Green", "Blue"] {
                    assert_eq!(prompt.title, "Pick a color: Red, Green, or Blue.");
                    saw_color = true;
                } else if labels == ["Yes", "No"] {
                    assert_eq!(prompt.title, "May I run ls?");
                    saw_permission = true;
                }
                assert!(!prompt.title.contains("εgo:"));
                assert_eq!(
                    session.question_text.as_deref(),
                    Some(prompt.title.as_str())
                );
                assert_eq!(
                    prompt.selection_mode,
                    Some(crate::output_parser::ChoiceSelectionMode::NavigateEnter)
                );
            }
            if record.direction == crate::pty_capture::CaptureDirection::Output {
                final_awaiting = session.awaiting_input;
            }
        },
    );
    assert!(
        saw_color && saw_permission,
        "both recorded selectors must reach clients"
    );
    assert!(!final_awaiting, "answer retracts the native selector");
}

/// Catches: a scroll-only newline resurrecting answered ego choices after busy.
#[tokio::test]
async fn captured_ego_answered_selector_stays_cleared_after_busy_hook() {
    let mut resumed = false;
    let mut saw_answer = false;
    replay_native_question(
        "ego-selector-retirement-2028.tcap",
        "ego",
        |record, session| {
            if record.direction == crate::pty_capture::CaptureDirection::Input {
                saw_answer |= record.data == b"1\r";
                return;
            }
            resumed |= saw_answer
                && record
                    .data
                    .windows(b"7770;state=busy".len())
                    .any(|bytes| bytes == b"7770;state=busy");
            if resumed {
                assert!(
                    !session.awaiting_input && session.choice_prompt.is_none(),
                    "answered selector re-armed after protocol busy: {:?}",
                    record.data,
                );
            }
        },
    );
    assert!(resumed, "capture must include the answer and busy hook");
}

/// Catches: fixing retirement by permanently swallowing an identical next question.
#[test]
fn ego_question_same_selector_recovers_after_fresh_native_wait() {
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "ego-selector-retirement-2028.tcap",
    ))
    .unwrap();
    let (rows, cols) = capture.geometry.unwrap();
    let sid = "ego-next-identical-question";
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
        .agent_type = Some("ego".into());
    let mut rx = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);
    let mut utf8 = Utf8ReadBuffer::new();
    let mut escapes = EscapeAwareBuffer::new();
    // Replay through busy, before the old selector disappears. Then replay the
    // captured native awaiting + selector paint as a controlled next-question
    // transition. Every external byte comes from the real recording.
    for index in (0..=30).chain([23, 26]) {
        let record = &capture.records[index];
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
        if index == 30 {
            let session = state.session_maps.session_states.get(sid).unwrap();
            assert!(!session.awaiting_input && session.choice_prompt.is_none());
        }
    }
    let session = state.session_maps.session_states.get(sid).unwrap();
    assert!(session.awaiting_input);
    assert_eq!(
        session.choice_prompt.as_ref().unwrap().title,
        "Which release checks should we run?"
    );
}

/// Catches: enforced ego approvals missing their native keys or losing the refusal wait.
#[tokio::test]
async fn captured_ego_enforced_approval_reaches_answer_controls() {
    let mut refused = false;
    let mut settled = false;
    let mut final_awaiting = false;
    replay_native_question(
        "ego-enforced-approval-2028.tcap",
        "ego",
        |record, session| {
            if record.direction == crate::pty_capture::CaptureDirection::Input {
                if record.data == b"n" {
                    assert!(session.awaiting_input && session.question_confident);
                    let prompt = session
                        .choice_prompt
                        .as_ref()
                        .expect("enforced ego approval keys");
                    assert_eq!(prompt.title, "Allow run python3 -c 'print(987654321)'?");
                    assert_eq!(
                        prompt
                            .options
                            .iter()
                            .map(|option| option.key.as_str())
                            .collect::<Vec<_>>(),
                        ["y", "a", "n", "e", "d"]
                    );
                    assert_eq!(prompt.selection_mode, None);
                    refused = true;
                } else if refused && record.data == b"\r" {
                    assert!(
                        session.awaiting_input && session.question_confident,
                        "refusal explanation is still waiting"
                    );
                    assert!(session.choice_prompt.is_none());
                    assert_eq!(
                        session.question_text.as_deref(),
                        Some("What should it do instead? (Enter alone just says no)")
                    );
                    settled = true;
                }
            }
            final_awaiting = session.awaiting_input;
        },
    );
    assert!(refused && settled);
    assert!(!final_awaiting);
}

/// Catches: empty native awaiting dedup swallowing the second MCP approval.
#[tokio::test]
async fn captured_ego_second_mcp_approval_rearms_awaiting() {
    let mut approvals = 0;
    let mut first_controls = false;
    replay_native_question(
        "ego-mcp-questions-2088-2089.tcap",
        "ego",
        |record, session| {
            if record.direction == crate::pty_capture::CaptureDirection::Input
                && matches!(record.data.as_slice(), b"y" | b"y\r")
            {
                approvals += 1;
                assert!(
                    session.awaiting_input,
                    "approval {approvals} must await an answer"
                );
                // The first approval's OSC already worked before this regression.
                // Check the second first so RED identifies the lost wait, not the
                // separate hard-wrapped approval-title parsing failure.
                if approvals == 1 {
                    first_controls = session.choice_prompt.is_some();
                }
                if approvals >= 2 {
                    let prompt = session
                        .choice_prompt
                        .as_ref()
                        .expect("native approval controls");
                    let target = if approvals == 2 {
                        "call_tool; call tuic/session; run"
                    } else {
                        "progress; run"
                    };
                    assert!(
                        prompt
                            .title
                            .starts_with(&format!("Allow call tuic/{target} /"))
                    );
                    assert!(
                        prompt
                            .title
                            .ends_with("/src-tauri/binaries/tuic-bridge-aarch64-app?")
                    );
                    assert!(session.question_confident);
                    assert_eq!(
                        session.question_text.as_deref(),
                        Some(prompt.title.as_str())
                    );
                    assert_eq!(
                        prompt
                            .options
                            .iter()
                            .map(|o| o.key.as_str())
                            .collect::<Vec<_>>(),
                        ["y", "a", "n", "e", "d"]
                    );
                }
            }
            if record.direction == crate::pty_capture::CaptureDirection::Output
                && record
                    .data
                    .windows(b"7770;state=awaiting".len())
                    .any(|s| s == b"7770;state=awaiting")
            {
                assert!(
                    session.awaiting_input,
                    "fresh native awaiting must not depend on a parsed title"
                );
            }
            if record.direction == crate::pty_capture::CaptureDirection::Output
                && record
                    .data
                    .windows(b"7770;state=busy".len())
                    .any(|s| s == b"7770;state=busy")
            {
                assert!(
                    !session.awaiting_input && session.choice_prompt.is_none(),
                    "busy must retire answered approval"
                );
            }
        },
    );
    assert_eq!(approvals, 3, "all recorded approvals must be exercised");
    assert!(
        first_controls,
        "the first hard-wrapped approval also needs native controls"
    );
}

/// Catches: a final native provider failure is dropped in startup grace or erased by idle.
#[tokio::test]
async fn captured_ego_provider_failure_2335_survives_idle() {
    let mut saw_final = false;
    replay_native_question(
        "ego-provider-failure-2335.tcap",
        "ego",
        |record, session| {
            if record.direction == crate::pty_capture::CaptureDirection::Output
                && record
                    .data
                    .windows(b"send a new prompt to continue".len())
                    .any(|bytes| bytes == b"send a new prompt to continue")
            {
                saw_final = true;
                assert!(
                    session
                        .last_error
                        .as_deref()
                        .is_some_and(|text| text.contains("HTTP response body failed"))
                );
                assert!(
                    !session.awaiting_input,
                    "a provider failure is not a human question"
                );
            }
            if saw_final {
                assert!(
                    session.last_error.is_some(),
                    "idle must not erase the final failure"
                );
            }
        },
    );
    assert!(
        saw_final,
        "recorded native termination must reach the real accumulator"
    );
}

/// Catches: ego has no spinner StatusLine, so a successful later turn stays red.
#[tokio::test]
async fn captured_ego_provider_recovery_2335_clears_error_on_native_work() {
    let mut failed = false;
    let mut answered = false;
    let mut recovered = false;
    let mut completed = false;
    replay_native_question(
        "ego-provider-recovered-2335.tcap",
        "ego",
        |record, session| {
            let bytes = &record.data;
            if record.direction == crate::pty_capture::CaptureDirection::Input {
                return;
            }
            // This capture records output only. The native composer repaint is
            // the recorded evidence of the follow-up, before its busy marker.
            if bytes
                .windows(b"> Please complete the response successfully.".len())
                .any(|part| part == b"> Please complete the response successfully.")
            {
                assert!(
                    failed && session.last_error.is_some(),
                    "the follow-up composer alone must not clear failure"
                );
                answered = true;
            }
            if bytes
                .windows(b"send a new prompt to continue".len())
                .any(|part| part == b"send a new prompt to continue")
            {
                assert!(session.last_error.is_some());
                failed = true;
            }
            if answered
                && bytes
                    .windows(b"7770;state=busy".len())
                    .any(|part| part == b"7770;state=busy")
            {
                assert!(
                    session.last_error.is_none(),
                    "native work must clear the prior failure"
                );
                recovered = true;
            }
            if recovered {
                assert!(session.last_error.is_none());
                completed |= bytes.windows(b"done ".len()).any(|part| part == b"done ");
            }
        },
    );
    assert!(
        failed && answered && recovered && completed,
        "exercise the recorded failure and successful subsequent turn"
    );
}

/// Catches: ego's pre-634 provider-failure emitter latching a completed turn as awaiting.
#[tokio::test]
async fn ego_2317_captured_provider_failure_releases_composer() {
    let mut completions = 0;
    let mut stopped_receipts = 0;
    replay_native_question(
        "ego-provider-failed-idle-2317.tcap",
        "ego",
        |record, session| {
            if record.direction != crate::pty_capture::CaptureDirection::Output {
                return;
            }
            let text = String::from_utf8_lossy(&record.data);
            if text.contains("7770;state=idle") {
                completions += 1;
                assert!(
                    !session.awaiting_input && session.choice_prompt.is_none(),
                    "a failed turn is complete, not an approval: {:?}",
                    session.question_text,
                );
            }
            if text.contains("stopped 11:08 PM") {
                stopped_receipts += 1;
                assert!(!session.awaiting_input);
            }
        },
    );
    assert_eq!(
        completions, 3,
        "initial failure, submitted retry and mail wake"
    );
    assert_eq!(
        stopped_receipts, 3,
        "each recorded failed turn has its receipt"
    );
}

/// Catches: a stopped-turn workaround swallowing the same native awaiting marker approvals use.
#[tokio::test]
async fn ego_2317_old_emitter_awaiting_remains_authoritative() {
    let sid = "ego-2317-old-emitter";
    let (state, silence) = chunk_trace_state(sid);
    state.config.write().progress_tracking = false;
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("ego".into());
    let mut rx = state.event_bus.subscribe();
    let mut processor = ChunkProcessor::new(None, None);
    // Exact old Announce::signal TurnCompleted state sequence from ego 870b20c85e.
    // The failing producer, not TUIC's handling of this authoritative marker, changed in 634-bcab.
    processor.process_chunk(
        "\x1b]7770;state=busy\x07\x1b]7770;state=awaiting\x07",
        &silence,
        sid,
        &state,
    );
    while let Ok(event) = rx.try_recv() {
        crate::state::tests_support::apply_replay_event(&state, &event);
    }
    let session = state.session_maps.session_states.get(sid).unwrap();
    assert!(session.awaiting_input && session.question_confident);
}

/// Catches: releasing failed turns also releasing a real native exec permission prompt.
#[tokio::test]
async fn ego_2317_captured_approval_still_owns_composer() {
    let mut approval = false;
    replay_native_question(
        "ego-provider-failure-approval-2317.tcap",
        "ego",
        |_, session| {
            if let Some(prompt) = &session.choice_prompt {
                assert_eq!(prompt.title, "Allow run sh -c 'printf APPROVAL_2317'?");
                assert!(session.awaiting_input && session.question_confident);
                assert_eq!(
                    prompt
                        .options
                        .iter()
                        .map(|option| option.key.as_str())
                        .collect::<Vec<_>>(),
                    ["y", "a", "n", "e", "d"],
                );
                approval = true;
            }
        },
    );
    assert!(
        approval,
        "the real default-mode approval must reach session state"
    );
}

/// Catches: a matched inventory failure is lost between PTY bytes and client state,
/// or a native intermediate retry is promoted to a terminal session error.
#[tokio::test]
async fn api_error_2335_inventory_reaches_session_error() {
    // Source-format cases, not captured transcripts: literals and formatter
    // parameter instances from docs/backend/output-parser.md's 2335 inventory.
    // The VT renders physical wrapping; the test never injects parsed events.
    let mut cases: Vec<(&str, String, bool)> = Vec::new();
    for line in [
        "API Error: Connection lost mid-response. The response above may be incomplete.",
        "API Error: Connection lost before a response was produced. Try again.",
        "API Error: The response stopped arriving. The response above may be incomplete.",
        "API Error: The response stalled before a response was produced. Try again.",
        "API Error: Server error mid-response. The response above may be incomplete.",
        "API Error: Part of the response never arrived. The response above may be incomplete.",
        "API Error: Part of the response never arrived and no response was produced. Try again.",
        "API Error: The response stream was malformed. The response above may be incomplete.",
        "API Error: The response stream was malformed and no response was produced. Try again.",
        "API Error: Request timed out",
        "API Error: Request timed out. Check your internet connection and proxy settings",
        "API Error: Connection dropped (ECONNRESET)",
        "API Error: Connection refused — a firewall or proxy may be blocking it (ECONNREFUSED)",
        "API Error: Unable to connect to API. Check your internet connection",
        "API Error: 500 Internal server error. This is a server-side issue, usually temporary — try again in a moment. If it persists, check https://status.claude.com.",
        r#"API Error: 500 {"type":"error","error":{"type":"api_error","message":"Internal server error"},"request_id":"req_011CYV92oEFMbcz45mjVYssM"}"#,
        "Invalid API key · Fix external API key",
        "Invalid auth token · Fix external auth token",
        "Authentication required · Sign in again to continue",
        r#"API Error: 401 {"type":"error","error":{"type":"authentication_error","message":"Invalid bearer token"}}"#,
    ] {
        cases.push(("claude", format!("{line}\r\n"), true));
    }
    for line in [
        "■ stream disconnected before completion: HTTP response body failed",
        "■ request timed out",
        "■ exceeded retry limit, last status: 503 Service Unavailable",
        "■ unexpected status 500 Internal Server Error: upstream unavailable",
        "■ unexpected status 401 Unauthorized: invalid credential",
        "■ unexpected status 403 Forbidden: invalid credential",
        "■ exceeded retry limit, last status: 401 Unauthorized",
        "■ exceeded retry limit, last status: 403 Forbidden",
        "stream error: exceeded retry limit, last status: 500 Internal Server Error",
    ] {
        cases.push(("codex", format!("{line}\r\n"), true));
    }
    for status in [401, 403] {
        cases.push((
            "ego",
            format!("εgo: the provider returned HTTP {status}: authentication refusal\r\n"),
            true,
        ));
    }
    // Parameter instances of the source-backed final receipt; unlike the two
    // real captures below, these are not evidence of live timeout/HTTP failures.
    for cause in [
        "the response stream failed: HTTP response body failed",
        "request timed out",
        "the provider returned HTTP 500: Internal Server Error",
        "the provider returned HTTP 503: Service Unavailable",
        "the provider returned HTTP 401: Unauthorized",
        "the provider returned HTTP 403: Forbidden",
    ] {
        cases.push((
            "ego",
            format!("\x1b]777;notify;εgo;{cause}; review the output, then send a new prompt to continue\x07"),
            true,
        ));
    }
    cases.push((
        "codex",
        "Reconnecting... 1/5 (stream disconnected before completion: request timed out)\r\n".into(),
        false,
    ));
    for frame in [
        "\x1b]7770;ego-recovery={\"phase\":\"waiting\",\"next_attempt_ms\":1792100000000,\"attempt\":3}\x07",
        "\x1b]7770;ego-recovery={\"phase\":\"retrying\",\"next_attempt_ms\":null,\"attempt\":4}\x07",
        "\x1b]7770;ego-recovery=clear\x07",
    ] {
        cases.push(("ego", frame.into(), false));
    }
    let mut replays = Vec::new();
    for (agent, bytes, final_failure) in cases {
        for cols in [36, 40, 128] {
            replays.push((
                agent,
                41,
                cols,
                format!("source-format {bytes:?} at {cols} columns"),
                vec![bytes.as_bytes().to_vec()],
                final_failure,
            ));
        }
    }
    // Preserve real bytes, original chunk boundaries and recorded geometry.
    for (agent, fixture) in [
        ("claude", "claude-wrapped-provider-failure-2335.tcap"),
        ("ego", "ego-provider-failure-2335.tcap"),
    ] {
        let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(fixture)).unwrap();
        let (rows, cols) = capture.geometry.unwrap();
        let chunks = capture
            .records
            .into_iter()
            .filter(|record| record.direction == crate::pty_capture::CaptureDirection::Output)
            .map(|record| record.data)
            .collect();
        replays.push((agent, rows, cols, fixture.into(), chunks, true));
    }
    let mut failures = Vec::new();
    for (agent, rows, cols, label, chunks, final_failure) in replays {
        let sid = "api-error-inventory-2335";
        let (state, silence) = chunk_trace_state(sid);
        // Text inventory models output in an established agent session. Startup
        // suppression is a separate contract covered by the real ego idle test.
        silence.lock().startup_settled = true;
        state.config.write().progress_tracking = false;
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
        let mut rx = state.event_bus.subscribe();
        let mut processor = ChunkProcessor::new(None, None);
        let mut utf8 = Utf8ReadBuffer::new();
        let mut escapes = EscapeAwareBuffer::new();
        // The native idle receipt must not erase a final error or invent one.
        for bytes in chunks
            .into_iter()
            .chain([b"\x1b]7770;state=idle\x07".to_vec()])
        {
            let data = escapes.push(&utf8.push(&bytes));
            let (clean, _) = crate::state::strip_kitty_sequences(&data);
            processor.process_chunk(&clean, &silence, sid, &state);
            loop {
                match rx.try_recv() {
                    Ok(event) => crate::state::tests_support::apply_replay_event(&state, &event),
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(error) => panic!("{label}: replay lost events: {error}"),
                }
            }
            if !final_failure
                && state
                    .session_state_with_shell(sid)
                    .unwrap()
                    .last_error
                    .is_some()
            {
                failures.push(format!("intermediate retry became Error: {label}"));
            }
        }
        let snapshot = state.session_state_with_shell(sid).unwrap();
        if snapshot.last_error.is_some() != final_failure {
            failures.push(format!(
                "expected last_error={final_failure}, got {:?}: {label}",
                snapshot.last_error
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
