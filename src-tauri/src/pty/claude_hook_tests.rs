// Catches: idle notices, foreign sessions, or retired requests re-open human attention.
#[tokio::test]
async fn claude_permission_notifications_require_live_owned_tool() {
    let sid = "claude-notification-filter";
    let (state, mut processor) = claude_permission_state(sid);
    let events = recorded_claude_permissions("notification");
    for event in &events[..3] {
        feed_claude_permission(&state, &mut processor, sid, event);
    }
    for kind in ["idle_prompt", "auth_success", "task_complete"] {
        let mut notice = events[4].clone();
        notice["notification_type"] = kind.into();
        feed_claude_permission(&state, &mut processor, sid, &notice);
    }
    let mut foreign = events[3].clone();
    foreign["session_id"] = "another-native-session".into();
    feed_claude_permission(&state, &mut processor, sid, &foreign);
    assert!(
        !state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .awaiting_input
    );
    feed_claude_permission(&state, &mut processor, sid, &events[4]);
    assert!(await_session(&state, sid, |s| s.awaiting_input).await);
    let mut done = events[2].clone();
    done["hook_event_name"] = "PostToolUse".into();
    feed_claude_permission(&state, &mut processor, sid, &done);
    assert!(await_session(&state, sid, |s| !s.awaiting_input).await);
    for event in &events[2..] {
        feed_claude_permission(&state, &mut processor, sid, event);
    }
    assert!(
        !state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .awaiting_input
    );
}

// Catches: a parallel tool's busy/completion retracts another tool's permission.
#[tokio::test]
async fn claude_permission_ignores_unrelated_tool_and_clears_matching_failure() {
    let sid = "claude-parallel-permission";
    let (state, mut processor) = claude_permission_state(sid);
    let events = recorded_claude_permissions("accept");
    for event in &events[..4] {
        feed_claude_permission(&state, &mut processor, sid, event);
    }
    assert!(await_session(&state, sid, |s| s.awaiting_input).await);
    let mut unrelated = events[2].clone();
    unrelated["tool_use_id"] = "parallel-tool".into();
    feed_claude_permission(&state, &mut processor, sid, &unrelated);
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    processor.process_chunk("\x1b]7770;state=busy\x07", &silence, sid, &state);
    unrelated["hook_event_name"] = "PostToolUse".into();
    feed_claude_permission(&state, &mut processor, sid, &unrelated);
    tokio::task::yield_now().await;
    assert!(
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .awaiting_input
    );
    let mut failed = events[4].clone();
    failed["hook_event_name"] = "PostToolUseFailure".into();
    feed_claude_permission(&state, &mut processor, sid, &failed);
    assert!(await_session(&state, sid, |s| !s.awaiting_input).await);
}

// Catches: permission waiting survives an accepted next prompt, stop, failure or session end.
#[tokio::test]
async fn claude_permission_retracts_on_next_prompt_or_terminal_completion() {
    for completion in ["prompt", "idle"] {
        let sid = format!("claude-permission-clear-{completion}");
        let (state, mut processor) = claude_permission_state(&sid);
        let events = recorded_claude_permissions("accept");
        for event in &events[..4] {
            feed_claude_permission(&state, &mut processor, &sid, event);
        }
        assert!(await_session(&state, &sid, |s| s.awaiting_input).await);
        if completion == "prompt" {
            let mut next = events[1].clone();
            next["prompt_id"] = "next-prompt".into();
            feed_claude_permission(&state, &mut processor, &sid, &next);
        }
        let silence = state.session_maps.silence_states.get(&sid).unwrap().clone();
        processor.process_chunk(
            &format!("\x1b]7770;state={completion}\x07"),
            &silence,
            &sid,
            &state,
        );
        assert!(await_session(&state, &sid, |s| !s.awaiting_input).await);
        feed_claude_permission(&state, &mut processor, &sid, &events[3]);
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

// Catches: native answers latch a confident wait when bare Enter has no text or cancellation has no PostToolUse.
#[tokio::test]
async fn claude_permission_recorded_answers_retract_on_native_outcome() {
    for (name, bytes) in [
        (
            "accept",
            include_bytes!("../fixtures/agent_prompts/claude-2627-permission-accept.tcap")
                .as_slice(),
        ),
        (
            "deny",
            include_bytes!("../fixtures/agent_prompts/claude-2627-permission-deny.tcap").as_slice(),
        ),
        (
            "escape",
            include_bytes!("../fixtures/agent_prompts/claude-2627-permission-escape.tcap")
                .as_slice(),
        ),
    ] {
        let sid = format!("claude-no-post-{name}");
        let (state, mut processor) = claude_permission_state(&sid);
        let silence = state.session_maps.silence_states.get(&sid).unwrap().clone();
        let capture = crate::pty_capture::decode_capture(bytes).unwrap();
        let (rows, cols) = capture.geometry.unwrap();
        state
            .grid
            .vt_log_buffers
            .get(&sid)
            .unwrap()
            .lock()
            .resize(rows, cols);
        let mut utf8 = Utf8ReadBuffer::new();
        let mut escapes = EscapeAwareBuffer::new();
        for event in recorded_claude_permissions(name).iter().take(4) {
            feed_claude_permission(&state, &mut processor, &sid, event);
        }
        let mut answered = false;
        for record in capture.records {
            if record.direction == crate::pty_capture::CaptureDirection::Input {
                assert!(
                    await_session(&state, &sid, |s| s.awaiting_input
                        && s.choice_prompt.is_some())
                    .await,
                    "{name}: real native permission controls"
                );
                crate::mcp_http::session::apply_input_bookkeeping(
                    &state,
                    &sid,
                    std::str::from_utf8(&record.data).unwrap(),
                );
                answered = true;
                assert!(
                    !state
                        .session_maps
                        .session_states
                        .get(&sid)
                        .unwrap()
                        .awaiting_input,
                    "{name}: native answer must clear attention"
                );
            } else {
                let data = escapes.push(&utf8.push(&record.data));
                let (clean, _) = crate::state::strip_kitty_sequences(&data);
                processor.process_chunk(&clean, &silence, &sid, &state);
                tokio::task::yield_now().await;
            }
        }
        assert!(answered);
        if name == "accept" {
            // The accepted command emits PostToolUse; only deny/Escape have no post-tool.
            for event in recorded_claude_permissions(name).iter().skip(4) {
                feed_claude_permission(&state, &mut processor, &sid, event);
            }
        }
        assert!(
            await_session(&state, &sid, |s| !s.awaiting_input).await,
            "{name}"
        );
    }
}

// Catches: broad tool receipts erase an open wizard or elicitation before its own result.
#[tokio::test]
async fn claude_permission_tool_noise_preserves_question_and_elicitation_owners() {
    for modal in ["AskUserQuestion", "ExitPlanMode", "Elicitation"] {
        let sid = format!("claude-modal-owner-{modal}");
        let (state, mut processor) = claude_permission_state(&sid);
        let events = recorded_claude_permissions("accept");
        feed_claude_permission(&state, &mut processor, &sid, &events[1]);
        let mut open = events[2].clone();
        if modal == "Elicitation" {
            // Event-name perturbation checks the existing coarse Elicitation/result contract.
            open["hook_event_name"] = "Elicitation".into();
            open.as_object_mut().unwrap().remove("prompt_id");
        } else {
            open["tool_name"] = modal.into();
        }
        feed_claude_permission(&state, &mut processor, &sid, &open);
        assert!(await_session(&state, &sid, |s| s.awaiting_input).await);
        let mut noise = events[2].clone();
        noise["tool_use_id"] = "background".into();
        feed_claude_permission(&state, &mut processor, &sid, &noise);
        let silence = state.session_maps.silence_states.get(&sid).unwrap().clone();
        processor.process_chunk("\x1b]7770;state=busy\x07", &silence, &sid, &state);
        noise["hook_event_name"] = "PostToolUse".into();
        feed_claude_permission(&state, &mut processor, &sid, &noise);
        tokio::task::yield_now().await;
        assert!(
            state
                .session_maps
                .session_states
                .get(&sid)
                .unwrap()
                .awaiting_input,
            "{modal}"
        );
        open["hook_event_name"] = if modal == "Elicitation" {
            "ElicitationResult"
        } else {
            "PostToolUse"
        }
        .into();
        feed_claude_permission(&state, &mut processor, &sid, &open);
        assert!(
            await_session(&state, &sid, |s| !s.awaiting_input).await,
            "{modal}"
        );
    }
}

// Catches: wrong-tab receipts request attention, or fragmented/coalesced native receipts lose ordered clears.
#[tokio::test]
async fn claude_permission_owner_and_chunk_boundaries_preserve_ordered_state() {
    for size in [1, 7, 65536] {
        let sid = format!("claude-permission-fragments-{size}");
        let (state, mut processor) = claude_permission_state(&sid);
        let silence = state.session_maps.silence_states.get(&sid).unwrap().clone();
        let mut rx = state.event_bus.subscribe();
        let events = recorded_claude_permissions("accept");
        let mut frames = String::new();
        for event in &events[1..5] {
            frames.push_str(&claude_hook_frames(event));
        }
        processor.process_chunk(
            &frames.replace("claude-test-owner:", "different-tab:"),
            &silence,
            &sid,
            &state,
        );
        assert!(rx.try_recv().is_err());
        for chunk in frames.as_bytes().chunks(size) {
            processor.process_chunk(std::str::from_utf8(chunk).unwrap(), &silence, &sid, &state);
        }
        let mut questions = 0;
        let mut answered = false;
        while let Ok(event) = rx.try_recv() {
            if let crate::state::AppEvent::PtyParsed { parsed, .. } = event {
                if parsed["type"] == "question" {
                    questions += 1;
                }
                if questions > 0 && parsed["type"] == "user-input" {
                    answered = true;
                }
            }
        }
        assert_eq!(questions, 1, "{size}");
        assert!(
            answered,
            "{size}: matching answer must follow request on the bus"
        );
        assert!(await_session(&state, &sid, |s| !s.awaiting_input).await);
    }
}

/// Run the generated hook for `event` and return the bytes it writes to its tty.
#[cfg(unix)]
fn run_claude_hook(event: &serde_json::Value) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let scratch = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let output = scratch.path().join("tty");
    let command = crate::agent_hook::claude_hook_map()
        .into_iter()
        .find(|(e, m, _)| *e == event["hook_event_name"].as_str().unwrap() && m.is_empty())
        .unwrap()
        .2
        .replace(
            crate::agent_hook::tty_resolve(),
            &format!("__t='{}'", output.display()),
        );
    let mut child = Command::new("sh")
        .args(["-c", &command])
        .env("TUIC_SESSION", "claude-test-owner")
        // The tty fallback: never post to the TUIC that runs this test.
        .env_remove("TUIC_SOCKET")
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(event.to_string().as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
    std::fs::read_to_string(&output).unwrap()
}

/// Deliver `stream` the way a tty shared with a redrawing TUI does. Measured on
/// macOS on 2026-11-03 with a real PTY and a concurrent spinner writer: `sh`
/// printf writes in 1024-byte pieces, and the spinner bytes landed only between
/// those pieces (a 53 KB frame split at 11264, 12288, 22528 and 36864 bytes).
#[cfg(unix)]
fn deliver_through_busy_tty(stream: &str) -> String {
    const WRITE: usize = 1024;
    const REDRAW: &str = "\r\x1b[2K✻ Working";
    let starts: Vec<usize> = stream
        .match_indices("\x1b]")
        .map(|(index, _)| index)
        .chain([stream.len()])
        .collect();
    let mut delivered = String::new();
    for frame in starts.windows(2) {
        for piece in stream.as_bytes()[frame[0]..frame[1]].chunks(WRITE) {
            delivered.push_str(REDRAW);
            delivered.push_str(std::str::from_utf8(piece).unwrap());
        }
    }
    delivered
}

// Catches: a hook frame longer than one tty write is cut by an interleaved TUI
// redraw, so its base64 tail prints over the Claude Code screen (#2747-636f).
#[cfg(unix)]
#[tokio::test]
async fn claude_hook_frames_survive_tui_redraw_between_tty_writes() {
    let sid = "claude-hook-interleaved-redraw";
    let (state, mut processor) = claude_permission_state(sid);
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let events = recorded_claude_permissions("accept");
    // A Bash command long enough to need many frames, yet inside the 16 KB cap.
    let command = format!("cat <<'EOF'\n{}\nEOF", "y".repeat(10 * 1024));
    let mut pre = events[2].clone();
    let mut ask = events[3].clone();
    pre["tool_input"]["command"] = command.clone().into();
    ask["tool_input"]["command"] = command.into();
    let mut large = events[2].clone();
    large["tool_use_id"] = "toolu_large_write".into();
    large["tool_name"] = "Write".into();
    large["tool_input"] =
        serde_json::json!({"file_path": "/tmp/large.txt", "content": "x".repeat(20 * 1024)});
    for event in [&events[1], &large, &pre, &ask] {
        let delivered = deliver_through_busy_tty(&run_claude_hook(event));
        processor.process_chunk(&delivered, &silence, sid, &state);
    }
    let rows = state
        .grid
        .vt_log_buffers
        .get(sid)
        .unwrap()
        .lock()
        .screen_rows();
    assert!(
        rows.iter()
            .map(|row| row.trim())
            .all(|row| row.is_empty() || row == "✻ Working"),
        "hook payload reached the screen: {rows:#?}"
    );
    assert!(await_session(&state, sid, |s| s.awaiting_input).await);
}

/// Deliver `stream` the way a macOS tty does once its output queue backs up: a
/// writer blocked at the high-water mark inside one write() resumes after a
/// concurrent writer, so a redraw lands inside a single frame at an OBUFSIZ
/// (100-byte) boundary. Measured on macOS on 2026-11-03 with a real PTY, a
/// fast reader and an 80 ms spinner: a 350-frame burst of 768-byte pieces had a
/// split frame in 13 of 20 runs; a 28-frame burst in 0 of 20.
#[cfg(unix)]
fn deliver_through_backpressured_tty(stream: &str) -> String {
    const REDRAW: &str = "\r\x1b[2K✻ Working";
    let mut delivered = String::new();
    for (index, piece) in stream.as_bytes().chunks(100).enumerate() {
        if index % 64 == 63 {
            delivered.push_str(REDRAW);
        }
        delivered.push_str(std::str::from_utf8(piece).unwrap());
    }
    delivered
}

// Catches: the hook streams a receipt the reader cannot use (a PostToolUse whose
// tool_response is a large Read result) as hundreds of frames, and a
// backpressured macOS tty splits one of them, so base64 still prints over the
// Claude Code screen (#2747-636f).
#[cfg(unix)]
#[tokio::test]
async fn claude_hook_large_receipt_leaves_no_text_through_backpressured_tty() {
    let sid = "claude-hook-backpressured-tty";
    let (state, mut processor) = claude_permission_state(sid);
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let mut post = recorded_claude_permissions("accept")[4].clone();
    post["tool_name"] = "Read".into();
    post["tool_response"] = serde_json::json!({"file": {"content": "x".repeat(200 * 1024)}});
    let delivered = deliver_through_backpressured_tty(&run_claude_hook(&post));
    processor.process_chunk(&delivered, &silence, sid, &state);
    let rows = state
        .grid
        .vt_log_buffers
        .get(sid)
        .unwrap()
        .lock()
        .screen_rows();
    assert!(
        rows.iter()
            .map(|row| row.trim())
            .all(|row| row.is_empty() || row == "✻ Working"),
        "hook payload reached the screen: {rows:#?}"
    );
}

#[cfg(unix)]
type StopHookReopenReplay = (
    Arc<AppState>,
    Arc<Mutex<SilenceState>>,
    Arc<std::sync::Mutex<Vec<u8>>>,
);

/// Replay one recorded Claude 2.1.316 turn (`claude-stop-hook-reopen-2837.tcap`) through the
/// reader chunk path, with TUIC's hook markers where the live session received them: the
/// prompt hook with the submitted line, the Stop hook right before the first chunk after the
/// last spinner frame. That chunk only retitles the window, so the screen still shows the
/// spinner row and reopens the hook-closed turn. The replay ends on a composer that holds text
/// TUIC did not type, which the Ready adapter cannot read.
#[cfg(unix)]
fn replay_claude_stop_hook_reopen_2837(sid: &str) -> StopHookReopenReplay {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    agent_session(&state, sid, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .hook_instrumented = true;
    let mut processor = ChunkProcessor::new(None, Some("claude-test-owner".into()));
    let bytes = insert_recording_session(&state, sid);
    let capture = crate::pty_capture::decode_capture(&agent_prompt_fixture(
        "claude-stop-hook-reopen-2837.tcap",
    ))
    .unwrap();
    let (rows, cols) = capture.geometry.unwrap();
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(rows, cols, 2000)));
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    let mut carry = Vec::new();
    for (index, record) in capture.records.iter().enumerate() {
        match index {
            322 => processor.process_chunk("\x1b]7770;state=prompt\x07", &silence, sid, &state),
            447 => processor.process_chunk("\x1b]7770;state=idle\x07", &silence, sid, &state),
            _ => false,
        };
        if record.direction != crate::pty_capture::CaptureDirection::Output {
            continue;
        }
        carry.extend_from_slice(&record.data);
        let valid = match std::str::from_utf8(&carry) {
            Ok(text) => text.len(),
            Err(error) => error.valid_up_to(),
        };
        let text = std::str::from_utf8(&carry[..valid]).unwrap().to_owned();
        carry.drain(..valid);
        processor.process_chunk(&text, &silence, sid, &state);
    }
    (state, silence, bytes)
}

/// One silence-timer tick as `spawn_silence_timer` runs it for a non-Working screen.
#[cfg(unix)]
fn claude_timer_tick(state: &AppState, silence: &Arc<Mutex<SilenceState>>, sid: &str) -> bool {
    let activity = silence.lock().cached_screen_activity;
    assert_ne!(activity, AgentScreenActivity::Working);
    let epoch = state
        .session_maps
        .session_states
        .get(sid)
        .unwrap()
        .turn_epoch;
    try_timer_idle_transition(state, silence, sid, activity, Some("claude"), Some(epoch))
        .transitioned
}

/// The process snapshot a pending probe waits for: the agent owns no running descendant.
/// Like `reconcile_process_snapshot_demand`, it lands only when a probe is armed.
#[cfg(unix)]
fn reconcile_quiet_claude_process(state: &AppState, sid: &str) {
    let epoch = state
        .session_maps
        .session_states
        .get(sid)
        .unwrap()
        .turn_epoch;
    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "claude", "claude")]));
    refresh_background_work_from_cached_snapshot(
        state,
        sid,
        10,
        "claude",
        epoch,
        state.process_snapshot_cache.load(),
    );
}

// Catches: the spinner row Claude keeps on screen while its Stop hooks run reopens the turn
// that TUIC's Stop hook closed and wipes that Protocol idle; with a composer the Ready adapter
// cannot read, idle stays unconfirmed for good, so the queued mail notice is never typed (#2837-df6a).
#[cfg(unix)]
#[test]
fn claude_native_stop_keeps_idle_and_types_queued_mail_notice() {
    let sid = "claude-stop-hook-reopen-mail";
    let (state, silence, bytes) = replay_claude_stop_hook_reopen_2837(sid);
    assert_eq!(
        silence.lock().cached_screen_activity,
        AgentScreenActivity::Unknown,
        "the replay must end on a composer the Ready adapter cannot read"
    );
    let first = claude_timer_tick(&state, &silence, sid);
    reconcile_quiet_claude_process(&state, sid);
    let transitioned = first || claude_timer_tick(&state, &silence, sid);

    assert!(
        !transitioned,
        "native Stop is already idle; no screen recovery is needed"
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_IDLE
    );
    assert!(silence.lock().idle_confirmed());
    assert!(matches!(
        deliver_notice_to_pty(&state, sid, PEER_MAIL_WAKE),
        PtyDelivery::Typed
    ));
    flush_pending_injections_blocking(&state, sid);
    let written = String::from_utf8_lossy(&bytes.lock().unwrap()).into_owned();
    assert_eq!(written.matches(PEER_MAIL_WAKE).count(), 1);
    assert!(
        state
            .pending_injections
            .get(sid)
            .is_none_or(|queue| queue.is_empty())
    );
}

// Catches: `session submit` refuses the same idle worker as `agent_not_ready` (#2837-df6a).
#[cfg(unix)]
#[test]
fn claude_native_stop_accepts_session_submit_without_screen_recovery() {
    let sid = "claude-stop-hook-reopen-submit";
    let (state, silence, bytes) = replay_claude_stop_hook_reopen_2837(sid);
    let first = claude_timer_tick(&state, &silence, sid);
    reconcile_quiet_claude_process(&state, sid);
    assert!(!first && !claude_timer_tick(&state, &silence, sid));
    // The replay answers the terminal's own queries; the readiness check must add nothing.
    let written_before = bytes.lock().unwrap().len();
    assert_eq!(agent_submission_rejection(&state, sid, false), None);
    assert_eq!(bytes.lock().unwrap().len(), written_before);
}

// Catches: the restore gives a reopened turn its hook idle back although Claude went on with
// a tool (PreToolUse `state=busy` after the Stop hook), so mail is typed into a working agent.
#[cfg(unix)]
#[test]
fn claude_stop_hook_reopen_stays_busy_when_a_tool_continuation_follows() {
    let sid = "claude-stop-hook-reopen-tool";
    let (state, silence, bytes) = replay_claude_stop_hook_reopen_2837(sid);
    let mut processor = ChunkProcessor::new(None, Some("claude-test-owner".into()));
    processor.process_chunk("\x1b]7770;state=busy\x07", &silence, sid, &state);
    let written_before = bytes.lock().unwrap().len();
    assert!(matches!(
        deliver_notice_to_pty(&state, sid, PEER_MAIL_WAKE),
        PtyDelivery::Queued
    ));

    let first = claude_timer_tick(&state, &silence, sid);
    reconcile_quiet_claude_process(&state, sid);
    let second = claude_timer_tick(&state, &silence, sid);

    assert!(
        !first && !second,
        "a tool continuation must keep the turn open"
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(sid)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
    assert!(!silence.lock().idle_confirmed());
    flush_pending_injections_blocking(&state, sid);
    assert_eq!(bytes.lock().unwrap().len(), written_before);
    assert_eq!(
        state.pending_injections.get(sid).map(|queue| queue.len()),
        Some(1)
    );
}

// Catches: a real managed worker's completed-turn composer cannot release queued mail or
// accept a submission after the Stop-hook spinner reopens its turn (#2837-df6a).
#[cfg(unix)]
#[test]
fn claude_stop_hook_reopen_managed_idle_screen_delivers_mail_and_accepts_submit() {
    for mail in [true, false] {
        let sid = if mail {
            "claude-stop-hook-reopen-managed-mail"
        } else {
            "claude-stop-hook-reopen-managed-submit"
        };
        let (state, silence, bytes) = replay_claude_stop_hook_reopen_2837(sid);
        let screen: serde_json::Value =
            serde_json::from_slice(&agent_prompt_fixture("claude-managed-idle-2605-2837.json"))
                .unwrap();
        let rows = screen["rows"].as_u64().unwrap() as u16;
        let cols = screen["cols"].as_u64().unwrap() as u16;
        state
            .grid
            .vt_log_buffers
            .insert(sid.into(), Mutex::new(VtLogBuffer::new(rows, cols, 2000)));
        // A screen snapshot has no original repaint boundaries. Position each captured row
        // explicitly, without inventing a managed-worker stream or scrolling its last row.
        let mut repaint = String::from("\x1b[2J");
        for (row, text) in screen["lines"].as_array().unwrap().iter().enumerate() {
            repaint.push_str(&format!("\x1b[{};1H{}", row + 1, text.as_str().unwrap()));
        }
        let mut processor = ChunkProcessor::new(None, Some("claude-test-owner".into()));
        processor.process_chunk(&repaint, &silence, sid, &state);
        assert_eq!(
            silence.lock().cached_screen_activity,
            AgentScreenActivity::Ready,
            "the recorded managed worker must have a ready composer"
        );

        claude_timer_tick(&state, &silence, sid);
        reconcile_quiet_claude_process(&state, sid);
        claude_timer_tick(&state, &silence, sid);
        assert_eq!(
            state
                .session_maps
                .shell_states
                .get(sid)
                .unwrap()
                .load(Ordering::Acquire),
            SHELL_IDLE
        );
        assert!(silence.lock().idle_confirmed());
        // A queued notice starts a new turn, so submission is a separate replay.
        if !mail {
            let written_before = bytes.lock().unwrap().len();
            assert_eq!(agent_submission_rejection(&state, sid, false), None);
            assert_eq!(bytes.lock().unwrap().len(), written_before);
            continue;
        }
        assert!(matches!(
            deliver_notice_to_pty(&state, sid, PEER_MAIL_WAKE),
            PtyDelivery::Typed
        ));
        flush_pending_injections_blocking(&state, sid);
        let written = String::from_utf8_lossy(&bytes.lock().unwrap()).into_owned();
        assert_eq!(written.matches(PEER_MAIL_WAKE).count(), 1);
        assert!(
            state
                .pending_injections
                .get(sid)
                .is_none_or(|queue| queue.is_empty())
        );
    }
}
