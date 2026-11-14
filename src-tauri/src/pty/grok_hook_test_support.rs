fn recorded_grok_hooks(name: &str) -> Vec<serde_json::Value> {
    let bytes: &[u8] = match name {
        "interrupt" => include_bytes!("../fixtures/grok_hooks_2624/interrupt.json"),
        "allow" => include_bytes!("../fixtures/grok_hooks_2624/allow.json"),
        "ask" => include_bytes!("../fixtures/grok_hooks_2624/ask.json"),
        "deny" => include_bytes!("../fixtures/grok_hooks_2624/deny.json"),
        _ => panic!("unknown recording"),
    };
    serde_json::from_slice(bytes).unwrap()
}

fn grok_hook_frame(event: &serde_json::Value) -> String {
    use base64::Engine;
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(event).unwrap());
    format!("\x1b]7770;grok-hook=grok-test-owner:{encoded}\x1b\\")
}

fn grok_hook_state(sid: &str) -> (Arc<AppState>, ChunkProcessor) {
    let state = accumulating_state(sid);
    agent_session(&state, sid, SHELL_IDLE);
    {
        let mut session = state.session_maps.session_states.get_mut(sid).unwrap();
        session.agent_type = Some("grok".into());
        session.hook_instrumented = true;
    }
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), Mutex::new(VtLogBuffer::new(35, 120, 1000)));
    let processor = ChunkProcessor::new(None, Some("grok-test-owner".into()));
    (state, processor)
}

fn feed_grok_hook(
    state: &AppState,
    processor: &mut ChunkProcessor,
    sid: &str,
    event: &serde_json::Value,
) {
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    processor.process_chunk(&grok_hook_frame(event), &silence, sid, state);
}
