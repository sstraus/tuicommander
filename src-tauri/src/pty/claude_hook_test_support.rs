fn recorded_claude_permissions(name: &str) -> Vec<serde_json::Value> {
    let text = match name {
        "accept" => include_str!("../fixtures/claude_hooks_2627/accept.json"),
        "deny" => include_str!("../fixtures/claude_hooks_2627/deny.json"),
        "escape" => include_str!("../fixtures/claude_hooks_2627/escape.json"),
        "notification" => include_str!("../fixtures/claude_hooks_2627/notification.json"),
        _ => panic!("unknown recording"),
    };
    serde_json::from_str(text).unwrap()
}

fn claude_permission_state(sid: &str) -> (Arc<AppState>, ChunkProcessor) {
    let state = accumulating_state(sid);
    state.config.write().progress_tracking = false;
    agent_session(&state, sid, SHELL_IDLE);
    {
        let mut session = state.session_maps.session_states.get_mut(sid).unwrap();
        session.agent_type = Some("claude".into());
        session.hook_instrumented = true;
    }
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(30, 100, 1000)));
    (
        state,
        ChunkProcessor::new(None, Some("claude-test-owner".into())),
    )
}

fn feed_claude_permission(
    state: &AppState,
    processor: &mut ChunkProcessor,
    sid: &str,
    event: &serde_json::Value,
) {
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    processor.process_chunk(&claude_hook_frames(event), &silence, sid, state);
}

/// Encode a receipt as the generated hook frames it: pieces, then an empty end.
fn claude_hook_frames(event: &serde_json::Value) -> String {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(event.to_string());
    let mut frames = String::new();
    for piece in encoded.as_bytes().chunks(768) {
        let piece = std::str::from_utf8(piece).unwrap();
        frames.push_str(&format!(
            "\x1b]7770;claude-hook=claude-test-owner:42:{piece}\x1b\\"
        ));
    }
    frames.push_str("\x1b]7770;claude-hook=claude-test-owner:42:\x1b\\");
    frames
}
