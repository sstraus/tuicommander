// Catches: a cancelled or rejected permission dialog leaves the tab working, because Claude fires no Stop hook on interrupt.
#[tokio::test]
async fn claude_permission_cancel_or_reject_returns_to_idle_without_stop() {
    for (name, bytes) in [
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
        let sid = format!("claude-interrupt-idle-{name}");
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
        for record in capture.records {
            if record.direction == crate::pty_capture::CaptureDirection::Input {
                crate::mcp_http::session::apply_input_bookkeeping(
                    &state,
                    &sid,
                    std::str::from_utf8(&record.data).unwrap(),
                );
            } else {
                let data = escapes.push(&utf8.push(&record.data));
                let (clean, _) = crate::state::strip_kitty_sequences(&data);
                processor.process_chunk(&clean, &silence, &sid, &state);
                tokio::task::yield_now().await;
            }
        }
        let shell = |state: &AppState| {
            state
                .session_maps
                .shell_states
                .get(&sid)
                .unwrap()
                .load(Ordering::Acquire)
        };
        assert_eq!(
            shell(&state),
            SHELL_BUSY,
            "{name}: the prompt hook holds busy"
        );
        let activity = silence.lock().cached_screen_activity;
        // The first tick arms the foreground probe; the process snapshot lands
        // before the next one. The interrupt claim must outlive the first tick.
        try_timer_idle_transition(&state, &silence, &sid, activity, Some("claude"), None);
        assert_eq!(shell(&state), SHELL_BUSY, "{name}: probe still pending");
        state
            .session_maps
            .session_states
            .get_mut(&sid)
            .unwrap()
            .background_probe_satisfied_turn_epoch = Some(0);
        try_timer_idle_transition(&state, &silence, &sid, activity, Some("claude"), None);
        assert_eq!(
            shell(&state),
            SHELL_IDLE,
            "{name}: interrupted dialog must end the turn"
        );
    }
}
