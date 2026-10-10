#[cfg(unix)]
#[test]
fn prefill_agent_input_refuses_unknown_and_plain_shell_sessions() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    assert!(prefill_agent_input(&state, "missing", "text").is_err());
    let bytes = insert_recording_session(&state, "shell-prefill");
    assert!(prefill_agent_input(&state, "shell-prefill", "text").is_err());
    assert!(bytes.lock().unwrap().is_empty());
}

/// Catches: a session that is not a PTY tab (the AI Chat ego over ACP has none)
/// matching the ego agent type through submit.
#[test]
fn test_submit_to_acp_hosted_ego_session_is_not_matched() {
    let state = crate::state::tests_support::make_test_app_state();
    assert_eq!(
        agent_submission_rejection(&state, "acp-ego-session", false),
        Some(("session_not_found", "unknown"))
    );
}

/// A mail wake can submit into Claude's detailed transcript view, where the
/// composer is hidden and the screen adapter returns Unknown. No later hook
/// busy or output means the submitted turn never started.
#[cfg(unix)]
#[test]
fn stale_busy_mail_wake_in_claude_transcript_returns_to_idle() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "stale-busy-transcript";
    let written = insert_recording_session(&state, sid);
    agent_session(&state, sid, SHELL_BUSY);
    transition_explicit_shell_state(&state, sid, SHELL_IDLE, "idle", true);
    assert_eq!(
        deliver_notice_to_pty(&state, sid, PEER_MAIL_WAKE),
        PtyDelivery::Typed
    );
    assert!(
        written
            .lock()
            .unwrap()
            .windows(PEER_MAIL_WAKE.len())
            .any(|part| part == PEER_MAIL_WAKE.as_bytes())
    );
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    assert_eq!(
        detect_claude_screen_activity(&["Showing detailed transcript · ctrl+o to toggle".into()]),
        AgentScreenActivity::Unknown
    );
    {
        let mut sl = silence.lock();
        sl.evidence.busy.as_mut().unwrap().at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
        sl.last_output_at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
    }
    state.session_maps.last_output_ms.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU64::new(
            now_epoch_ms() - PROTOCOL_STALE_TIMEOUT.as_millis() as u64,
        ),
    );
    let epoch = state
        .session_maps
        .session_states
        .get(sid)
        .unwrap()
        .turn_epoch;
    assert!(
        try_timer_idle_transition(
            &state,
            &silence,
            sid,
            AgentScreenActivity::Unknown,
            Some("claude"),
            Some(epoch)
        )
        .transitioned
    );
    let visible = state.session_state_with_shell(sid).unwrap();
    assert_eq!(visible.shell_state.as_deref(), Some("idle"));
    assert_eq!(visible.agent_state.as_deref(), Some("idle"));
}

#[test]
fn stale_busy_fresh_mail_wake_in_transcript_remains_working() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "stale-busy-fresh";
    agent_session(&state, sid, SHELL_BUSY);
    transition_explicit_shell_state(&state, sid, SHELL_IDLE, "idle", true);
    let claim = claim_idle_for_injection(&state, sid).unwrap();
    commit_injection_claim(&state, sid, claim);
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    assert!(
        !try_timer_idle_transition(
            &state,
            &silence,
            sid,
            AgentScreenActivity::Unknown,
            Some("claude"),
            Some(1)
        )
        .transitioned
    );
    assert_eq!(
        state
            .session_state_with_shell(sid)
            .unwrap()
            .agent_state
            .as_deref(),
        Some("working")
    );
}

#[test]
fn stale_busy_recent_output_keeps_claude_working_without_a_visible_composer() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "stale-busy-output";
    agent_session(&state, sid, SHELL_BUSY);
    transition_explicit_shell_state(&state, sid, SHELL_IDLE, "idle", true);
    let claim = claim_idle_for_injection(&state, sid).unwrap();
    commit_injection_claim(&state, sid, claim);
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    silence.lock().evidence.busy.as_mut().unwrap().at =
        std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
    assert!(
        !try_timer_idle_transition(
            &state,
            &silence,
            sid,
            AgentScreenActivity::Unknown,
            Some("claude"),
            Some(1)
        )
        .transitioned
    );
    assert_eq!(
        state
            .session_state_with_shell(sid)
            .unwrap()
            .agent_state
            .as_deref(),
        Some("working")
    );
}

#[test]
fn stale_busy_new_hook_busy_in_transcript_keeps_working() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "stale-busy-hook";
    agent_session(&state, sid, SHELL_BUSY);
    transition_explicit_shell_state(&state, sid, SHELL_IDLE, "idle", true);
    let claim = claim_idle_for_injection(&state, sid).unwrap();
    commit_injection_claim(&state, sid, claim);
    transition_explicit_shell_state(&state, sid, SHELL_BUSY, "busy", true);
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    {
        let mut sl = silence.lock();
        sl.evidence.busy.as_mut().unwrap().at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
        sl.last_output_at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
    }
    state.session_maps.last_output_ms.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU64::new(
            now_epoch_ms() - PROTOCOL_STALE_TIMEOUT.as_millis() as u64,
        ),
    );
    assert!(
        !try_timer_idle_transition(
            &state,
            &silence,
            sid,
            AgentScreenActivity::Unknown,
            Some("claude"),
            Some(1)
        )
        .transitioned
    );
    assert_eq!(
        state
            .session_state_with_shell(sid)
            .unwrap()
            .agent_state
            .as_deref(),
        Some("working")
    );
}

#[test]
fn stale_busy_uncertain_mail_write_does_not_hide_possible_work() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "stale-busy-uncertain";
    agent_session(&state, sid, SHELL_BUSY);
    transition_explicit_shell_state(&state, sid, SHELL_IDLE, "idle", true);
    let claim = claim_idle_for_injection(&state, sid).unwrap();
    commit_injection_claim(&state, sid, claim);
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    {
        let mut sl = silence.lock();
        sl.injection_delivery_uncertain = true;
        sl.evidence.busy.as_mut().unwrap().at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
        sl.last_output_at = std::time::Instant::now() - PROTOCOL_STALE_TIMEOUT;
    }
    assert!(
        !try_timer_idle_transition(
            &state,
            &silence,
            sid,
            AgentScreenActivity::Unknown,
            Some("claude"),
            Some(1)
        )
        .transitioned
    );
    assert_eq!(
        state
            .session_state_with_shell(sid)
            .unwrap()
            .agent_state
            .as_deref(),
        Some("working")
    );
}

#[test]
fn stale_busy_injection_claim_and_rollback_each_leave_one_transition_trace() {
    #[derive(Clone)]
    struct LogSink(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for LogSink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogSink {
        type Writer = LogSink;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    let state = crate::state::tests_support::make_test_app_state();
    let sid = "stale-busy-trace";
    agent_session(&state, sid, SHELL_IDLE);
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::fmt()
        .with_writer(LogSink(log.clone()))
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        let claim = claim_idle_for_injection(&state, sid).unwrap();
        assert!(rollback_injection_claim(&state, sid, claim));
    });
    let log = String::from_utf8(log.lock().unwrap().clone()).unwrap();
    assert_eq!(log.matches("Shell state → busy").count(), 1, "{log}");
    assert_eq!(log.matches("Shell state → idle").count(), 1, "{log}");
    assert_eq!(
        state
            .session_state_with_shell(sid)
            .unwrap()
            .shell_state
            .as_deref(),
        Some("idle")
    );
}

#[derive(Clone, Copy)]
enum SanitizedTraceStep {
    Submit,
    RealActivity,
    WorkingScreen,
    ReadyScreen,
    UnknownScreen,
}

#[test]
fn background_snapshot_ready_waits_for_newer_generation_and_repairs_working() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "background-ready-generation";
    let parent_id = "background-ready-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(child_id)
        .unwrap()
        .agent_type = Some("codex".into());
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .lock()
        .screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);

    let first_ready = try_timer_idle_transition(
        &state,
        &state
            .session_maps
            .silence_states
            .get(child_id)
            .unwrap()
            .clone(),
        child_id,
        AgentScreenActivity::Ready,
        Some("codex"),
        Some(0),
    );
    assert!(!first_ready.transitioned);
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());

    state.process_snapshot_cache.store(Some(vec![
        process(10, 1, "codex", "codex"),
        process(11, 10, "cargo", "cargo test --locked"),
    ]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    let snapshot = state.session_state_with_shell(child_id).unwrap();
    assert_eq!(snapshot.agent_state.as_deref(), Some("working"));
    assert!(snapshot.background_work);
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());

    let reconciled_ready = try_timer_idle_transition(
        &state,
        &state
            .session_maps
            .silence_states
            .get(child_id)
            .unwrap()
            .clone(),
        child_id,
        AgentScreenActivity::Ready,
        Some("codex"),
        Some(0),
    );
    assert!(reconciled_ready.transitioned);
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());

    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "idle");
}

#[test]
fn background_snapshot_child_absent_releases_declared_completion() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "background-ready-completed";
    let parent_id = "background-ready-completed-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(child_id)
        .unwrap()
        .agent_type = Some("codex".into());
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .lock()
        .mark_suggest_candidate(vec!["Review result".to_string()], 0);
    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .lock()
        .screen_ready_pending_since = Some(std::time::Instant::now() - AGENT_READY_CONFIRM);

    assert!(
        !try_timer_idle_transition(
            &state,
            &state
                .session_maps
                .silence_states
                .get(child_id)
                .unwrap()
                .clone(),
            child_id,
            AgentScreenActivity::Ready,
            Some("codex"),
            Some(0),
        )
        .transitioned
    );
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());

    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "codex", "codex")]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "codex",
        0,
        state.process_snapshot_cache.load(),
    ));
    assert!(
        try_timer_idle_transition(
            &state,
            &state
                .session_maps
                .silence_states
                .get(child_id)
                .unwrap()
                .clone(),
            child_id,
            AgentScreenActivity::Ready,
            Some("codex"),
            Some(0),
        )
        .transitioned
    );
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());
    let silence = state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .clone();
    assert!(emit_pending_suggest_if_idle(&state, &silence, child_id));
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "completed");
}

#[cfg(target_os = "macos")]
#[test]
fn claude_timed_caffeinate_does_not_delay_declared_completion() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "background-claude-caffeinate-completed";
    let parent_id = "background-claude-caffeinate-completed-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    state
        .session_maps
        .session_states
        .get_mut(child_id)
        .unwrap()
        .agent_type = Some("claude".into());
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .lock()
        .mark_suggest_candidate(vec!["Review result".to_string()], 0);
    state
        .process_snapshot_cache
        .store(Some(vec![process(10, 1, "claude", "claude")]));

    transition_explicit_shell_state_with_hook(&state, child_id, SHELL_IDLE, "idle", true, || {});
    assert!(state.agent_inbox.get(parent_id).unwrap().is_empty());

    state.process_snapshot_cache.store(Some(vec![
        process(10, 1, "claude", "claude"),
        process(11, 10, "mdkb", "mdkb mcp"),
        process(12, 10, "tuic-bridge", "tuic-bridge"),
        process(13, 10, "caffeinate", "caffeinate -i -t 300"),
    ]));
    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "claude",
        0,
        state.process_snapshot_cache.load(),
    ));

    let resolved = state.session_state_with_shell(child_id).unwrap();
    assert_eq!(resolved.shell_state.as_deref(), Some("idle"));
    assert_eq!(resolved.agent_state.as_deref(), Some("completed"));
    assert!(!resolved.background_work);
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1);
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "completed");
}

#[test]
fn explicit_agent_idle_child_absent_restores_api_state_and_notifies_once() {
    for (session_id, declare_completion, expected_state) in [
        ("background-explicit-idle", false, "idle"),
        ("background-explicit-completed", true, "completed"),
    ] {
        let state = crate::state::tests_support::make_test_app_state();
        let parent_id = format!("{session_id}-parent");
        agent_session(&state, session_id, SHELL_BUSY);
        state
            .session_maps
            .session_states
            .get_mut(session_id)
            .unwrap()
            .agent_type = Some("codex".into());
        state
            .session_maps
            .session_parent
            .insert(session_id.to_string(), parent_id.clone());
        state.agent_inbox.entry(parent_id.clone()).or_default();
        if declare_completion {
            state
                .session_maps
                .silence_states
                .get(session_id)
                .unwrap()
                .lock()
                .mark_suggest_candidate(vec!["Review result".to_string()], 0);
        }
        state
            .process_snapshot_cache
            .store(Some(vec![process(10, 1, "codex", "codex")]));

        transition_explicit_shell_state_with_hook(
            &state,
            session_id,
            SHELL_IDLE,
            "idle",
            true,
            || {},
        );
        assert!(state.agent_inbox.get(&parent_id).unwrap().is_empty());
        let pending = state.session_state_with_shell(session_id).unwrap();
        assert_eq!(pending.shell_state.as_deref(), Some("idle"));
        assert_eq!(pending.agent_state.as_deref(), Some("working"));
        assert!(pending.has_pending_background_probe());

        state
            .process_snapshot_cache
            .store(Some(vec![process(10, 1, "codex", "codex")]));
        assert!(refresh_background_work_from_cached_snapshot(
            &state,
            session_id,
            10,
            "codex",
            0,
            state.process_snapshot_cache.load(),
        ));
        let resolved = state.session_state_with_shell(session_id).unwrap();
        assert_eq!(resolved.agent_state.as_deref(), Some(expected_state));
        assert!(!resolved.has_pending_background_probe());
        let inbox = state.agent_inbox.get(&parent_id).unwrap();
        assert_eq!(inbox.len(), 1);
        let content: serde_json::Value =
            serde_json::from_str(&inbox.front().unwrap().content).unwrap();
        assert_eq!(content["state"], expected_state);
        drop(inbox);

        state
            .process_snapshot_cache
            .store(Some(vec![process(10, 1, "codex", "codex")]));
        assert!(!refresh_background_work_from_cached_snapshot(
            &state,
            session_id,
            10,
            "codex",
            0,
            state.process_snapshot_cache.load(),
        ));
        assert_eq!(state.agent_inbox.get(&parent_id).unwrap().len(), 1);
    }
}

// ---- Layer 3: state_change auto-notifications (#1164-2571) ----

#[test]
fn mark_session_exited_pushes_state_change_to_parent_inbox() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let child_id = "child-sess";
    let parent_id = "parent-sess";

    // Register parent-child relationship
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    // Pre-init parent inbox
    state.agent_inbox.entry(parent_id.to_string()).or_default();

    mark_session_exited(child_id, &state);

    let inbox = state
        .agent_inbox
        .get(parent_id)
        .expect("parent inbox must exist");
    assert!(
        !inbox.is_empty(),
        "parent inbox must have received state_change message"
    );
    let msg = inbox.front().unwrap();
    let content: serde_json::Value =
        serde_json::from_str(&msg.content).expect("content must be valid JSON");
    assert_eq!(content["type"], "state_change");
    assert_eq!(content["state"], "exited");
}

// ---- Self-acknowledging orchestrator lifecycle summary ----

fn lifecycle_inbox_message(
    id: &str,
    child: &str,
    timestamp: u64,
    content: serde_json::Value,
) -> crate::state::AgentMessage {
    crate::state::AgentMessage {
        id: id.to_string(),
        from_tuic_session: child.to_string(),
        from_name: "tuic".to_string(),
        content: content.to_string(),
        timestamp,
        delivered_via_channel: false,
    }
}

fn idle_payload(child: &str) -> serde_json::Value {
    serde_json::json!({"type": "state_change", "state": "idle", "session_id": child})
}

fn exited_payload(child: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "state_change",
        "state": "exited",
        "session_id": child,
        "exit_code": 0,
    })
}

#[test]
fn lifecycle_summary_carries_every_event_in_the_window() {
    let state = crate::state::tests_support::make_test_app_state();
    let parent = "parent-summary";
    state.push_agent_inbox(
        parent,
        lifecycle_inbox_message(
            "tuic-auto-idle",
            SUMMARY_CHILD,
            10,
            idle_payload(SUMMARY_CHILD),
        ),
    );
    state.push_agent_inbox(
        parent,
        lifecycle_inbox_message(
            "tuic-auto-exit",
            SUMMARY_CHILD_B,
            20,
            exited_payload(SUMMARY_CHILD_B),
        ),
    );

    let summary = summarize_lifecycle_group(
        &state,
        parent,
        crate::state::OrchestratorWakeGroup {
            observed_through: 0,
            wake_through: 20,
        },
    )
    .expect("a lifecycle-only window must summarize");

    assert!(
        summary.contains("child agent 8c261794 is now idle"),
        "{summary}"
    );
    assert!(
        summary.contains("child agent 9d3728a5 exited (exit 0)"),
        "{summary}"
    );
    assert!(
        !summary.contains("action=inbox"),
        "a self-acknowledging notice must not send the reader to the inbox: {summary}"
    );
    assert_eq!(summary.lines().count(), 1, "must stay one composer line");
}

#[test]
fn peer_payload_in_the_window_forces_the_generic_wake() {
    let state = crate::state::tests_support::make_test_app_state();
    let parent = "parent-mixed";
    state.push_agent_inbox(
        parent,
        lifecycle_inbox_message(
            "tuic-auto-idle",
            SUMMARY_CHILD,
            10,
            idle_payload(SUMMARY_CHILD),
        ),
    );
    state.push_agent_inbox(
        parent,
        crate::state::AgentMessage {
            id: "peer-1".to_string(),
            from_tuic_session: "peer".to_string(),
            from_name: "sender".to_string(),
            content: "secret peer payload".to_string(),
            timestamp: 20,
            delivered_via_channel: false,
        },
    );

    assert!(
        summarize_lifecycle_group(
            &state,
            parent,
            crate::state::OrchestratorWakeGroup {
                observed_through: 0,
                wake_through: 20,
            },
        )
        .is_none(),
        "one peer message must disqualify the whole group, not be skipped"
    );
}

#[test]
fn lifecycle_summary_ignores_messages_outside_the_reserved_window() {
    let state = crate::state::tests_support::make_test_app_state();
    let parent = "parent-window";
    state.push_agent_inbox(
        parent,
        lifecycle_inbox_message(
            "tuic-auto-old",
            SUMMARY_CHILD,
            10,
            idle_payload(SUMMARY_CHILD),
        ),
    );
    state.push_agent_inbox(
        parent,
        lifecycle_inbox_message(
            "tuic-auto-covered",
            SUMMARY_CHILD,
            20,
            exited_payload(SUMMARY_CHILD),
        ),
    );
    // Arrived after the reservation: neither described nor disqualifying.
    state.push_agent_inbox(
        parent,
        crate::state::AgentMessage {
            id: "peer-late".to_string(),
            from_tuic_session: "peer".to_string(),
            from_name: "sender".to_string(),
            content: "later peer payload".to_string(),
            timestamp: 30,
            delivered_via_channel: false,
        },
    );

    let summary = summarize_lifecycle_group(
        &state,
        parent,
        crate::state::OrchestratorWakeGroup {
            observed_through: 10,
            wake_through: 20,
        },
    )
    .expect("the reserved window is lifecycle-only");

    assert!(summary.contains("exited (exit 0)"), "{summary}");
    assert!(
        !summary.contains("is now idle"),
        "an already-observed message must not be repeated: {summary}"
    );
    assert!(!summary.contains("later peer payload"), "{summary}");
}

#[test]
fn oversize_lifecycle_summary_falls_back_to_the_generic_wake() {
    let state = crate::state::tests_support::make_test_app_state();
    let parent = "parent-oversize";
    for index in 0..12u64 {
        let child = format!("8c2617{index:02}-91e5-44a4-bf63-ec8afafd2adc");
        state.push_agent_inbox(
            parent,
            lifecycle_inbox_message(
                &format!("tuic-auto-{index}"),
                &child,
                index + 1,
                idle_payload(&child),
            ),
        );
    }

    assert!(
        summarize_lifecycle_group(
            &state,
            parent,
            crate::state::OrchestratorWakeGroup {
                observed_through: 0,
                wake_through: 12,
            },
        )
        .is_none(),
        "a burst too long to type must fall back to the generic wake"
    );
}

#[test]
fn try_shell_transition_busy_to_idle_pushes_state_change_to_parent_inbox() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-idle-sess";
    let parent_id = "parent-idle-sess";

    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    // Must have a session_state with agent_type to qualify for idle notification
    let ss = crate::state::SessionState {
        agent_type: Some("claude".to_string()),
        ..Default::default()
    };
    state
        .session_maps
        .session_states
        .insert(child_id.to_string(), ss);
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state.session_maps.silence_states.insert(
        child_id.to_string(),
        Arc::new(Mutex::new(SilenceState::new())),
    );

    let transitioned = try_shell_transition(&state, child_id, SHELL_BUSY, SHELL_IDLE, true);
    assert!(transitioned, "transition must succeed");

    let inbox = state
        .agent_inbox
        .get(parent_id)
        .expect("parent inbox must exist");
    assert!(
        !inbox.is_empty(),
        "parent inbox must have received state_change message"
    );
    let msg = inbox.front().unwrap();
    let content: serde_json::Value =
        serde_json::from_str(&msg.content).expect("content must be valid JSON");
    assert_eq!(content["type"], "state_change");
    assert_eq!(content["state"], "idle");
}

#[test]
fn background_work_defers_parent_idle_until_descendants_finish() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-background-sess";
    let parent_id = "parent-background-sess";
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    state.session_maps.silence_states.insert(
        child_id.to_string(),
        Arc::new(Mutex::new(SilenceState::new())),
    );

    assert!(try_shell_transition(
        &state, child_id, SHELL_BUSY, SHELL_IDLE, true
    ));
    assert!(
        state.agent_inbox.get(parent_id).unwrap().is_empty(),
        "a ready composer must not announce autonomous completion"
    );

    assert!(set_background_work_for_epoch(&state, child_id, 0, 1, false));
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1);
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "idle");
}

#[test]
fn background_work_defers_declared_completion_without_generic_idle() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-background-completed";
    let parent_id = "parent-background-completed";
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    let mut silence = SilenceState::new();
    silence.mark_suggest_candidate(vec!["Review result".to_string()], 0);
    let silence = Arc::new(Mutex::new(silence));
    state
        .session_maps
        .silence_states
        .insert(child_id.to_string(), silence.clone());

    assert!(!emit_pending_suggest_if_idle(&state, &silence, child_id));
    assert!(set_background_work_for_epoch(&state, child_id, 0, 1, false));
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1);
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "completed");
    assert!(!emit_pending_suggest_if_idle(&state, &silence, child_id));
}

#[cfg(unix)]
#[test]
fn background_probe_settlement_retries_pending_orchestrator_mail_wake() {
    let state = crate::state::tests_support::make_test_app_state();
    let parent_id = "parent-background-probe-mail";
    let bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        &state,
        parent_id,
        Box::new(RecordingWriter {
            bytes: Arc::clone(&bytes),
        }),
        TtyMode::Raw,
    );
    agent_session(&state, parent_id, SHELL_IDLE);
    {
        let mut session = state
            .session_maps
            .session_states
            .get_mut(parent_id)
            .expect("parent session state");
        session.agent_type = Some("codex".to_string());
        session.background_probe_turn_epoch = Some(0);
        session.background_probe_after_generation = Some(0);
    }
    state.orchestrator_peers.insert(parent_id.to_string());
    state.agent_inbox.insert(
        parent_id.to_string(),
        std::collections::VecDeque::from([crate::state::AgentMessage {
            id: "peer-result".to_string(),
            from_tuic_session: "child".to_string(),
            from_name: "worker".to_string(),
            content: "secret child result".to_string(),
            timestamp: 1,
            delivered_via_channel: false,
        }]),
    );

    assert_eq!(
        route_registered_orchestrator_mail(&state, parent_id, "peer-result", 1),
        Some(crate::state::OrchestratorDeliveryAssignment::InboxOnly),
        "the pending probe must keep the parent working and the payload inbox-only"
    );
    assert_eq!(state.orchestrator_wake_needed_through(parent_id), Some(1));

    assert!(set_background_work_for_epoch(
        &state, parent_id, 0, 1, false
    ));

    let written = String::from_utf8(bytes.lock().unwrap().clone()).expect("UTF-8 wake");
    assert!(
        written.contains("message available"),
        "wake was not submitted: {written:?}"
    );
    assert!(
        written.contains("agent action=inbox"),
        "wake omitted the inbox command: {written:?}"
    );
    assert!(
        !written.contains("secret child result"),
        "the child payload escaped the inbox: {written:?}"
    );
    assert_eq!(
        state.agent_inbox.get(parent_id).unwrap()[0].content,
        "secret child result"
    );
    assert_eq!(
        state.orchestrator_wake_needed_through(parent_id),
        Some(1),
        "the submitted generic notice remains pending until the parent reads the inbox"
    );
}

/// The canonical lifecycle can say idle while the composer still refuses an
/// injection — a draft, an open question, an unconfirmed idle. That first
/// attempt starts no write and burns the wake budget, and before the idle-edge
/// re-arm nothing ever announced the mail again: the notice was owed forever
/// and never typed.
#[cfg(unix)]
#[test]
fn a_composer_that_refused_the_first_wake_still_gets_one_at_the_next_idle() {
    let state = crate::state::tests_support::make_test_app_state();
    let parent_id = "parent-refused-first-wake";
    agent_session(&state, parent_id, SHELL_IDLE);
    let bytes = insert_recording_session(&state, parent_id);
    state.orchestrator_peers.insert(parent_id.to_string());
    state.agent_inbox.insert(
        parent_id.to_string(),
        std::collections::VecDeque::from([crate::state::AgentMessage {
            id: "peer-result".to_string(),
            from_tuic_session: "child".to_string(),
            from_name: "worker".to_string(),
            content: "secret child result".to_string(),
            timestamp: 1,
            delivered_via_channel: false,
        }]),
    );

    let mut buffer = InputLineBuffer::new();
    buffer.feed("Boss draft");
    state
        .session_maps
        .input_buffers
        .insert(parent_id.to_string(), parking_lot::Mutex::new(buffer));

    assert_eq!(
        route_registered_orchestrator_mail(&state, parent_id, "peer-result", 1),
        Some(crate::state::OrchestratorDeliveryAssignment::InboxOnly),
        "a draft in the composer must not be spliced into"
    );
    assert!(
        bytes.lock().unwrap().is_empty(),
        "a refused claim must write nothing"
    );
    assert_eq!(state.orchestrator_wake_needed_through(parent_id), Some(1));

    // The draft is gone and the turn settles — a real idle edge, not a repeat of
    // the lifecycle that refused the claim.
    state.session_maps.input_buffers.remove(parent_id);
    {
        let mut session = state
            .session_maps
            .session_states
            .get_mut(parent_id)
            .expect("parent session state");
        session.background_probe_turn_epoch = Some(0);
        session.background_probe_after_generation = Some(0);
    }
    assert!(set_background_work_for_epoch(
        &state, parent_id, 0, 1, false
    ));

    let written = String::from_utf8(bytes.lock().unwrap().clone()).expect("UTF-8 wake");
    assert!(
        written.contains("agent action=inbox"),
        "the idle edge owed the parent a wake: {written:?}"
    );
    assert!(
        !written.contains("secret child result"),
        "the child payload escaped the inbox: {written:?}"
    );
}

#[test]
fn cached_snapshot_detects_background_process_exit() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-background-exit";
    let parent_id = "parent-background-exit";
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            background_work: true,
            turn_epoch: 4,
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_IDLE),
    );
    state.session_maps.silence_states.insert(
        child_id.to_string(),
        Arc::new(Mutex::new(SilenceState::new())),
    );
    let exited = Arc::new(vec![process(10, 1, "codex", "codex")]);

    assert!(refresh_background_work_from_cached_snapshot(
        &state,
        child_id,
        10,
        "codex",
        4,
        Some((2, exited)),
    ));
    assert!(
        !state
            .session_maps
            .session_states
            .get(child_id)
            .unwrap()
            .background_work
    );
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "idle");
}

#[test]
fn declared_completion_does_not_emit_ambiguous_idle_lifecycle() {
    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-completed-sess";
    let parent_id = "parent-completed-sess";

    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );
    let mut silence = SilenceState::new();
    silence.mark_suggest_candidate(vec!["Review result".to_string()], 0);
    state
        .session_maps
        .silence_states
        .insert(child_id.to_string(), Arc::new(Mutex::new(silence)));

    assert!(try_shell_transition(
        &state, child_id, SHELL_BUSY, SHELL_IDLE, true
    ));

    assert!(
        state.agent_inbox.get(parent_id).unwrap().is_empty(),
        "the suggest drain must publish completed instead of an earlier idle"
    );
    let silence = state
        .session_maps
        .silence_states
        .get(child_id)
        .unwrap()
        .clone();
    assert!(emit_pending_suggest_if_idle(&state, &silence, child_id));
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1);
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["state"], "completed");
}

#[test]
fn pending_initial_prompt_timeout_notifies_parent_once() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let child_id = "child-prompt-timeout";
    let parent_id = "parent-prompt-timeout";
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.pending_initial_prompts.insert(
        child_id.to_string(),
        crate::state::PendingInitialPrompt::new("do the task"),
    );

    assert!(notify_initial_prompt_timeout_if_pending(&state, child_id));
    assert!(!notify_initial_prompt_timeout_if_pending(&state, child_id));

    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1, "timeout notification must be emitted once");
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["type"], "prompt_delivery_failed");
    assert_eq!(content["reason"], "timeout");
    assert_eq!(content["session_id"], child_id);
    // The regression: the watchdog used to report the failure AND drop the
    // prompt, so nothing retried and the child sat idle as if spawned with no
    // work. The prompt is preserved, and it rides along for re-delivery.
    assert_eq!(content["prompt"], "do the task");
    assert_eq!(content["retrying"], true);
    assert_eq!(
        state
            .pending_initial_prompts
            .get(child_id)
            .map(|pending| pending.prompt.clone()),
        Some("do the task".to_string()),
        "a reported timeout must not discard the child's task"
    );
}

/// Dialog detection, which the fixed 30s timeout had none of: a child parked on
/// "Do you trust the contents of this directory?" is not a child whose task is
/// void, and the parent is told which of the two it is.
#[test]
fn pending_initial_prompt_names_a_startup_dialog_as_the_cause() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let child_id = "child-prompt-dialog";
    let parent_id = "parent-prompt-dialog";
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.session_maps.session_states.insert(
        child_id.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            question_confident: true,
            ..Default::default()
        },
    );
    state.pending_initial_prompts.insert(
        child_id.to_string(),
        crate::state::PendingInitialPrompt::new("review the draft"),
    );

    assert!(notify_initial_prompt_timeout_if_pending(&state, child_id));

    let inbox = state.agent_inbox.get(parent_id).unwrap();
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(content["reason"], "startup_dialog");
    assert!(
        describe_lifecycle_payload(child_id, &content).contains("startup dialog"),
        "the typed one-liner must say what the parent is waiting on"
    );
}

/// The retry half: once the dialog is answered the child reaches a ready prompt,
/// the queued entry is typed, and the parent that was warned is told so.
#[cfg(unix)]
#[test]
fn a_prompt_that_lands_after_the_warning_closes_the_loop() {
    struct RespondingWriter(std::sync::mpsc::Sender<Vec<u8>>);

    impl std::io::Write for RespondingWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.send(bytes.to_vec()).expect("record PTY write");
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let state = crate::state::tests_support::make_test_app_state();
    let child_id = "child-prompt-late";
    let parent_id = "parent-prompt-late";
    agent_session(&state, child_id, SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut(child_id)
        .unwrap()
        .agent_type = Some("codex".into());
    state
        .grid
        .vt_log_buffers
        .insert(child_id.into(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
    state
        .session_maps
        .output_buffers
        .insert(child_id.into(), Mutex::new(OutputRingBuffer::new(1024)));
    let (writes, received) = std::sync::mpsc::channel();
    insert_session_with_writer(
        &state,
        child_id,
        Box::new(RespondingWriter(writes)),
        TtyMode::Raw,
    );
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    state.pending_initial_prompts.insert(
        child_id.to_string(),
        crate::state::PendingInitialPrompt {
            prompt: "review the draft".to_string(),
            notified: true,
        },
    );
    state
        .pending_injections
        .entry(child_id.to_string())
        .or_default()
        .push_back(crate::state::PendingInjection::initial_prompt(
            "review the draft",
        ));

    std::thread::scope(|scope| {
        scope.spawn(|| flush_pending_injections_blocking(&state, child_id));
        for expected in [b"\x15".as_slice(), b"review the draft", b"\r"] {
            assert_eq!(
                received
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap(),
                expected
            );
        }
        assert!(
            state.pending_initial_prompts.contains_key(child_id),
            "Enter write alone must not clear the warning"
        );
        let response = b"\xe2\x80\xa2 Working (1s \xe2\x80\xa2 esc to interrupt)\r\n\xe2\x80\xba Ask Codex to do anything";
        state
            .grid
            .vt_log_buffers
            .get(child_id)
            .unwrap()
            .lock()
            .process(response);
        state
            .session_maps
            .output_buffers
            .get(child_id)
            .unwrap()
            .lock()
            .write(response);
    });

    assert!(
        !state.pending_initial_prompts.contains_key(child_id),
        "delivery must clear the marker"
    );
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(
        content["type"], "prompt_delivered",
        "a parent told the prompt failed must not be left believing it"
    );
}

// ── PTY-injection message delivery (Step 2) ─────────────────────

fn flush_one_pending_as_submitted(state: &crate::state::AppState, sid: &str) {
    let claim = claim_idle_for_injection(state, sid).expect("idle claim");
    let injection = state
        .pending_injections
        .get_mut(sid)
        .and_then(|mut queue| queue.pop_front())
        .expect("pending message");
    apply_claimed_injection_outcome(
        state,
        sid,
        injection.text(),
        claim,
        InjectionOutcome::Submitted,
        ClaimedInjectionKind::Message,
    );
}

#[test]
fn submitted_input_lifecycle_peer_injection_starts_new_turn_and_clears_completion() {
    let state = crate::state::tests_support::make_test_app_state();
    completed_agent_session(&state, "completed");
    state.pending_injections.insert(
        "completed".to_string(),
        std::collections::VecDeque::from([crate::state::PendingInjection::notice("follow up")]),
    );

    flush_one_pending_as_submitted(&state, "completed");

    let snapshot = state.session_state_with_shell("completed").unwrap();
    assert_eq!(snapshot.shell_state.as_deref(), Some("busy"));
    assert_eq!(snapshot.agent_state.as_deref(), Some("working"));
    assert!(snapshot.suggested_actions.is_none());
    assert!(
        !state
            .session_maps
            .silence_states
            .get("completed")
            .unwrap()
            .lock()
            .completion_declared()
    );
}

#[test]
fn idle_parent_notification_finishes_before_new_turn_reservation() {
    use std::sync::atomic::Ordering;

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let child_id = "idle-race-child";
    let parent_id = "idle-race-parent";
    agent_session(&state, child_id, SHELL_BUSY);
    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();

    let (start_tx, start_rx) = std::sync::mpsc::channel();
    let (lock_held_tx, lock_held_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let submitter_state = Arc::clone(&state);
    let submitter = std::thread::spawn(move || {
        start_rx.recv().unwrap();
        let silence = submitter_state
            .session_maps
            .silence_states
            .get(child_id)
            .unwrap()
            .clone();
        lock_held_tx.send(silence.try_lock().is_none()).unwrap();
        note_submitted_input(&submitter_state, child_id);
        finished_tx.send(()).unwrap();
    });

    assert!(try_shell_transition_with_hook(
        &state,
        child_id,
        SHELL_BUSY,
        SHELL_IDLE,
        true,
        || {
            start_tx.send(()).unwrap();
            assert!(
                lock_held_rx.recv().unwrap(),
                "BUSY→IDLE must retain the lifecycle lock through parent enqueue"
            );
        },
    ));
    finished_rx.recv().unwrap();
    submitter.join().unwrap();

    assert_eq!(
        state
            .session_maps
            .shell_states
            .get(child_id)
            .unwrap()
            .load(Ordering::Acquire),
        SHELL_BUSY
    );
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(child_id)
            .unwrap()
            .turn_epoch,
        1
    );
    let inbox = state.agent_inbox.get(parent_id).unwrap();
    assert_eq!(inbox.len(), 1);
    let payload: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(payload["state"], "idle");
}

#[test]
fn submitted_input_lifecycle_ready_before_status_line_is_not_stale_completed() {
    let state = crate::state::tests_support::make_test_app_state();
    completed_agent_session(&state, "quick-turn");
    state.pending_injections.insert(
        "quick-turn".to_string(),
        std::collections::VecDeque::from([crate::state::PendingInjection::notice(
            "quick follow up",
        )]),
    );
    flush_one_pending_as_submitted(&state, "quick-turn");

    state
        .session_maps
        .silence_states
        .get("quick-turn")
        .unwrap()
        .lock()
        .confirm_idle();
    assert!(try_shell_transition(
        &state,
        "quick-turn",
        SHELL_BUSY,
        SHELL_IDLE,
        false,
    ));

    let snapshot = state.session_state_with_shell("quick-turn").unwrap();
    assert_eq!(snapshot.shell_state.as_deref(), Some("idle"));
    assert_eq!(snapshot.agent_state.as_deref(), Some("idle"));
    assert!(snapshot.suggested_actions.is_none());
}

#[test]
fn submitted_input_lifecycle_no_new_input_retains_completion() {
    let state = crate::state::tests_support::make_test_app_state();
    completed_agent_session(&state, "unchanged");

    let snapshot = state.session_state_with_shell("unchanged").unwrap();
    assert_eq!(snapshot.shell_state.as_deref(), Some("idle"));
    assert_eq!(snapshot.agent_state.as_deref(), Some("completed"));
    assert_eq!(
        snapshot.suggested_actions,
        Some(vec!["Review result".to_string()])
    );
    assert!(
        state
            .session_maps
            .silence_states
            .get("unchanged")
            .unwrap()
            .lock()
            .completion_declared()
    );
}

#[test]
fn injection_claim_rechecks_idle_atomically_after_delivery_decision() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "race-agent", SHELL_IDLE);
    assert!(should_inject_now(&state, "race-agent"));

    assert!(try_shell_transition(
        &state,
        "race-agent",
        SHELL_IDLE,
        SHELL_BUSY,
        false,
    ));

    assert!(
        claim_idle_for_injection(&state, "race-agent").is_none(),
        "a sender that observed idle before the agent became busy must queue instead of writing into the active composer"
    );
}

/// Typing into an idle agent must block injection outright — and a rejected
/// claim must leave the shell atom exactly as it found it, so the user's
/// half-typed line is never followed by a stray busy state. The post-CAS
/// re-check inside `claim_idle_for_injection` uses this same predicate for
/// the case where typing starts after the delivery decision.
#[test]
fn injection_claim_is_refused_while_the_user_is_typing() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "typing-agent", SHELL_IDLE);
    assert!(should_inject_now(&state, "typing-agent"));

    let mut buffer = InputLineBuffer::new();
    buffer.feed("half typed prompt");
    state
        .session_maps
        .input_buffers
        .insert("typing-agent".to_string(), parking_lot::Mutex::new(buffer));

    assert!(has_partial_user_input(&state, "typing-agent"));
    assert!(!should_inject_now(&state, "typing-agent"));
    assert!(
        claim_idle_for_injection(&state, "typing-agent").is_none(),
        "a partially typed composer must never be written into"
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get("typing-agent")
            .map(|a| a.load(std::sync::atomic::Ordering::Relaxed)),
        Some(SHELL_IDLE),
        "a refused claim must not leave the session marked busy"
    );
}

#[cfg(unix)]
#[test]
fn agent_submission_rejects_partial_composer_without_writing() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "submit-partial", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "submit-partial");
    let mut buffer = InputLineBuffer::new();
    buffer.feed("Boss draft");
    state.session_maps.input_buffers.insert(
        "submit-partial".to_string(),
        parking_lot::Mutex::new(buffer),
    );

    assert_eq!(
        write_agent_submission_to_pty(&state, "submit-partial", "new command"),
        AgentSubmissionWrite::Rejected {
            reason: "partial_composer",
            composer_state: "partial",
            pending: Vec::new(),
        }
    );
    assert!(bytes.lock().unwrap().is_empty());
    assert_eq!(
        state
            .session_maps
            .input_buffers
            .get("submit-partial")
            .unwrap()
            .lock()
            .content(),
        "Boss draft",
        "a receipt request must preserve the user's draft verbatim"
    );
}

#[cfg(unix)]
#[test]
fn a_human_reply_can_answer_a_confident_question_without_weakening_agent_injection() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "human-question", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "human-question");
    state
        .session_maps
        .session_states
        .get_mut("human-question")
        .unwrap()
        .question_confident = true;

    assert!(!should_inject_now(&state, "human-question"));
    assert!(matches!(
        write_agent_submission_to_pty(&state, "human-question", "yes"),
        AgentSubmissionWrite::Rejected {
            reason: "awaiting_input",
            ..
        }
    ));
    assert!(bytes.lock().unwrap().is_empty());

    assert!(matches!(
        write_human_reply_to_pty(&state, "human-question", "yes"),
        AgentSubmissionWrite::Complete { .. }
    ));
    let written = bytes.lock().unwrap();
    assert_eq!(
        written
            .iter()
            .map(|&byte| usize::from(byte == b'y'))
            .sum::<usize>(),
        1
    );
    assert_eq!(written.last(), Some(&b'\r'));
}

#[cfg(unix)]
#[test]
fn a_human_reply_cannot_overwrite_a_partial_composer() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "human-partial", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "human-partial");
    state
        .session_maps
        .session_states
        .get_mut("human-partial")
        .unwrap()
        .question_confident = true;
    let mut buffer = InputLineBuffer::new();
    buffer.feed("existing draft");
    state
        .session_maps
        .input_buffers
        .insert("human-partial".to_string(), parking_lot::Mutex::new(buffer));

    assert!(matches!(
        write_human_reply_to_pty(&state, "human-partial", "yes"),
        AgentSubmissionWrite::Rejected {
            reason: "partial_composer",
            ..
        }
    ));
    assert!(bytes.lock().unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn agent_submission_does_not_overtake_existing_queue() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "submit-queued", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "submit-queued");
    state
        .pending_injections
        .entry("submit-queued".to_string())
        .or_default()
        .push_back(crate::state::PendingInjection::notice("older notice"));

    // FIFO still holds: the submission does not jump the queue.
    let rejection = write_agent_submission_to_pty(&state, "submit-queued", "new command");
    assert!(
        matches!(&rejection, AgentSubmissionWrite::Rejected { .. }),
        "{rejection:?}"
    );
    let written = String::from_utf8_lossy(&bytes.lock().unwrap().clone()).to_string();
    assert!(
        !written.contains("new command"),
        "the submission must not overtake the parked entry: {written:?}"
    );
    // ...but the queue drains instead of standing still. A ready agent that is
    // already idle never sees another BUSY→IDLE edge, so the only thing that
    // could move this queue is the submit itself.
    assert!(
        written.contains("older notice"),
        "the parked entry must be typed, not left to block the composer forever: {written:?}"
    );
    assert!(
        state
            .pending_injections
            .get("submit-queued")
            .is_none_or(|queue| queue.is_empty()),
        "the queue must be empty once its entry reached the composer"
    );
}

/// The regression: `queued_commands_pending` was reported for a session that
/// could never drain, so the caller retried submit for minutes against a queue
/// that by construction could not move — and the reason named the symptom
/// rather than the cause.
#[cfg(unix)]
#[test]
fn a_queue_that_cannot_drain_reports_the_agent_not_the_queue() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "submit-unready", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "submit-unready");
    // Unconfirmed idle: exactly what a ready-screen agent looks like before its
    // adapter has proof. `flush_pending_injections` is gated on the same
    // predicate, so this queue cannot move until that changes.
    state.session_maps.silence_states.insert(
        "submit-unready".to_string(),
        Arc::new(Mutex::new(SilenceState::new())),
    );
    state
        .pending_injections
        .entry("submit-unready".to_string())
        .or_default()
        .push_back(crate::state::PendingInjection::notice(PEER_MAIL_WAKE));

    let rejection = write_agent_submission_to_pty(&state, "submit-unready", "new command");
    assert_eq!(
        rejection,
        AgentSubmissionWrite::Rejected {
            reason: "agent_not_ready",
            composer_state: "empty",
            pending: vec![crate::pty::PendingInjectionSummary {
                id: state.pending_injections.get("submit-unready").unwrap()[0].id(),
                kind: "notice",
                preview: PEER_MAIL_WAKE.to_string(),
            }],
        },
        "the blocker must name itself: which entry, of what kind"
    );
    assert!(bytes.lock().unwrap().is_empty());
}

/// Whatever is parked is listable, countable and deletable — of every kind.
/// A server entry that no surface reported is what made the stuck queue
/// undiagnosable: an empty composer, an empty Compose list, and submit
/// rejected anyway.
#[test]
fn every_parked_entry_is_observable_and_drainable() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "queue-visible", SHELL_BUSY);
    {
        let mut queue = state
            .pending_injections
            .entry("queue-visible".to_string())
            .or_default();
        queue.push_back(crate::state::PendingInjection::notice(PEER_MAIL_WAKE));
        queue.push_back(crate::state::PendingInjection::initial_prompt(
            "do the task",
        ));
        queue.push_back(crate::state::PendingInjection::user_command("git status"));
    }

    let listed = list_queued_commands(&state, "queue-visible");
    assert_eq!(
        listed.iter().map(|entry| entry.kind).collect::<Vec<_>>(),
        vec!["notice", "initial_prompt", "user_command"]
    );
    assert_eq!(queued_command_count(&state, "queue-visible"), 3);

    // Every kind deletes by id, not just the user's own.
    assert!(remove_queued_command(&state, "queue-visible", listed[0].id));
    assert!(remove_queued_command(&state, "queue-visible", listed[1].id));
    assert_eq!(queued_command_count(&state, "queue-visible"), 1);
    assert_eq!(clear_queued_commands(&state, "queue-visible"), 1);
    assert_eq!(queued_command_count(&state, "queue-visible"), 0);
}

#[cfg(unix)]
#[test]
fn agent_submission_claim_prevents_concurrent_peer_splicing() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    agent_session(&state, "submit-race", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "submit-race");
    let submit_state = Arc::clone(&state);
    let submit = std::thread::spawn(move || {
        write_agent_submission_to_pty(&submit_state, "submit-race", "atomic command")
    });

    for _ in 0..100 {
        if !bytes.lock().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let peer = deliver_notice_to_pty(&state, "submit-race", "peer command");
    let submitted = submit.join().unwrap();

    assert!(matches!(submitted, AgentSubmissionWrite::Complete { .. }));
    assert_eq!(peer, PtyDelivery::Queued);
    assert_eq!(
        String::from_utf8(bytes.lock().unwrap().clone()).unwrap(),
        "\u{15}atomic command\r",
        "the peer payload must not land between the split payload and Enter"
    );
    assert_eq!(
        state
            .pending_injections
            .get("submit-race")
            .unwrap()
            .front()
            .map(crate::state::PendingInjection::text),
        Some("peer command")
    );
}

#[cfg(unix)]
#[test]
fn agent_submission_writer_lock_prevents_raw_input_splicing() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    agent_session(&state, "submit-raw-race", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "submit-raw-race");
    let submit_state = Arc::clone(&state);
    let submit = std::thread::spawn(move || {
        write_agent_submission_to_pty(&submit_state, "submit-raw-race", "atomic command")
    });

    for _ in 0..100 {
        if !bytes.lock().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let raw_state = Arc::clone(&state);
    let raw = std::thread::spawn(move || {
        let writer = raw_state.pty_writer("submit-raw-race").unwrap();
        let mut writer = writer.lock();
        writer.write_all(b"raw input").unwrap();
        writer.flush().unwrap();
    });
    let submitted = submit.join().unwrap();
    raw.join().unwrap();

    assert!(matches!(submitted, AgentSubmissionWrite::Complete { .. }));
    assert_eq!(
        String::from_utf8(bytes.lock().unwrap().clone()).unwrap(),
        "\u{15}atomic command\rraw input",
        "a raw writer may follow the submission but cannot land before its Enter"
    );
}

/// `INJECT_ENTER_GAP` is 50 ms of REAL time that cannot be shortened, paid
/// twice per message (~100 ms: before the text and before the Enter), and it is
/// held under the session writer mutex on purpose — `agent_submission_writer_
/// lock_prevents_raw_input_splicing` pins that exact byte sequence. So the only
/// way a caller stops paying it is to stop being the thread that waits.
/// `flush_pending_injections` runs on the session-state accumulator and on the
/// silence timer, both tokio workers; it must hand the write to the injection
/// worker and return, while the queued message still reaches the composer whole.
#[cfg(unix)]
#[test]
fn flush_hands_the_enter_gap_to_the_injection_worker_not_the_caller() {
    use std::collections::VecDeque;
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    agent_session(&state, "detached-flush", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "detached-flush");
    let mut queue = VecDeque::new();
    queue.push_back(crate::state::PendingInjection::notice("wake up"));
    state
        .pending_injections
        .insert("detached-flush".to_string(), queue);

    let started = std::time::Instant::now();
    flush_pending_injections(&state, "detached-flush");
    let returned_in = started.elapsed();
    assert!(
        returned_in < INJECT_ENTER_GAP / 2,
        "the caller must not wait out the injection's Enter gap; returned in {returned_in:?}"
    );

    // Deferred, not dropped: the same framing must still land, payload then Enter.
    for _ in 0..300 {
        if bytes.lock().unwrap().ends_with(b"\r") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        String::from_utf8(bytes.lock().unwrap().clone()).unwrap(),
        "\u{15}wake up\r",
        "the deferred injection must be byte-identical to the inline one"
    );
    assert_eq!(
        state
            .pending_injections
            .get("detached-flush")
            .map(|queue| queue.len()),
        Some(0),
        "a delivered message must not stay queued"
    );
}

/// A silent first agent must not hold the shared injection worker while a
/// different agent receives its own queued wake.
#[cfg(unix)]
#[test]
fn queued_confirmation_wait_does_not_block_another_session() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    for sid in ["slow-confirmation", "other-session"] {
        agent_session(&state, sid, SHELL_IDLE);
        state
            .session_maps
            .session_states
            .get_mut(sid)
            .unwrap()
            .agent_type = Some("codex".into());
        state
            .grid
            .vt_log_buffers
            .insert(sid.into(), Mutex::new(VtLogBuffer::new(24, 80, 1000)));
        state
            .session_maps
            .output_buffers
            .insert(sid.into(), Mutex::new(OutputRingBuffer::new(1024)));
        state
            .pending_injections
            .entry(sid.into())
            .or_default()
            .push_back(crate::state::PendingInjection::notice("wake"));
    }
    let first_bytes = insert_recording_session(&state, "slow-confirmation");
    let second_bytes = insert_recording_session(&state, "other-session");
    flush_pending_injections(&state, "slow-confirmation");
    for _ in 0..200 {
        if first_bytes.lock().unwrap().ends_with(b"\r") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        first_bytes.lock().unwrap().ends_with(b"\r"),
        "first write reached Enter"
    );
    flush_pending_injections(&state, "other-session");
    for _ in 0..250 {
        if second_bytes.lock().unwrap().ends_with(b"\r")
            || state
                .session_maps
                .silence_states
                .get("slow-confirmation")
                .unwrap()
                .lock()
                .injection_delivery_uncertain
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        second_bytes.lock().unwrap().ends_with(b"\r"),
        "the second session must receive Enter before the first wait expires"
    );
}

#[test]
fn deliver_queues_pending_for_busy_agent() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "busy", SHELL_BUSY);
    let outcome = deliver_notice_to_pty(&state, "busy", "[TUIC message from lead] go");
    assert_eq!(
        outcome,
        PtyDelivery::Queued,
        "a busy composer parks the message; nothing reached the terminal"
    );
    let q = state.pending_injections.get("busy").expect("queued");
    assert_eq!(q.len(), 1);
    assert_eq!(
        q.front().map(crate::state::PendingInjection::text),
        Some("[TUIC message from lead] go")
    );
}

/// Compose enqueue on a busy agent: nothing may reach the composer, or the
/// user's queued note would steer the turn they deliberately did not interrupt.
#[cfg(unix)]
#[test]
fn enqueue_parks_command_while_agent_is_busy() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "busy", SHELL_BUSY);
    let bytes = insert_recording_session(&state, "busy");

    let first = enqueue_user_command(&state, "busy", "run the tests", None).expect("enqueued");
    assert_eq!((first.typed, first.queued), (false, 1));
    let second = enqueue_user_command(&state, "busy", "then push", None).expect("enqueued");
    assert_eq!((second.typed, second.queued), (false, 2));

    let queue = state.pending_injections.get("busy").expect("queue");
    assert_eq!(
        queue.iter().map(|entry| entry.text()).collect::<Vec<_>>(),
        vec!["run the tests", "then push"],
        "queued in the order the user composed them"
    );
    assert!(queue.iter().all(|entry| entry.kind() == "user_command"));
    assert_eq!(
        state
            .session_state_with_shell("busy")
            .expect("snapshot")
            .queued_commands,
        2,
        "queue depth is visible to the polling UI"
    );
    assert!(
        bytes.lock().unwrap().is_empty(),
        "a busy composer receives nothing"
    );
}

/// Regression: a silence-only idle edge leaves the queue parked; a later
/// stable Ready screen must wake it even when no second shell edge or PTY read
/// occurs. A callback writer observes the actual queue-to-PTY boundary.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn queued_codex_command_submits_when_ready_confirms_after_shell_idle() {
    struct WriteChannel(std::sync::mpsc::Sender<Vec<u8>>);

    impl std::io::Write for WriteChannel {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.send(bytes.to_vec()).expect("record PTY write");
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    tokio::time::pause();
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "codex-ready-after-idle";
    agent_session(&state, sid, SHELL_BUSY);
    {
        // The recording writer uses a shell child, not the captured agent.
        // The explicitly seeded identity models a configured launch preset.
        let mut session = state.session_maps.session_states.get_mut(sid).unwrap();
        session.agent_type = Some("codex".into());
        session.agent_type_from_run_config = true;
    }
    let (writes, received) = std::sync::mpsc::channel();
    insert_session_with_writer(&state, sid, Box::new(WriteChannel(writes)), TtyMode::Raw);

    let enqueued = enqueue_user_command(&state, sid, "resume queued work", None).unwrap();
    assert_eq!((enqueued.typed, enqueued.queued), (false, 1));
    assert!(
        received.try_recv().is_err(),
        "busy turn must receive no input"
    );

    // This state sequence is observed in :9876: shell idle with
    // idle_confirmed=false, then a queued flush is deferred. The reader's
    // cached Ready verdict subsequently matures without another output chunk.
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    assert!(try_shell_transition(
        &state, sid, SHELL_BUSY, SHELL_IDLE, true
    ));
    {
        let mut silence = silence.lock();
        let forty_minutes_ago = std::time::Instant::now() - std::time::Duration::from_secs(40 * 60);
        silence.force_idle_unconfirmed();
        silence.cached_screen_activity = AgentScreenActivity::Ready;
        silence.last_output_at = forty_minutes_ago;
        silence.last_chunk_at = forty_minutes_ago;
        silence.screen_ready_pending_since = Some(forty_minutes_ago);
    }
    assert!(!should_inject_now(&state, sid));
    flush_pending_injections_blocking(&state, sid);
    assert!(received.try_recv().is_err());
    assert_eq!(queued_command_count(&state, sid), 1);

    let running = Arc::new(AtomicBool::new(true));
    spawn_silence_timer(silence, running.clone(), sid.into(), state.clone());
    tokio::task::yield_now().await;
    tokio::time::advance(SILENCE_CHECK_INTERVAL + std::time::Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    running.store(false, Ordering::Release);

    for expected in [b"\x15".as_slice(), b"resume queued work", b"\r"] {
        let actual = received
            .recv_timeout(std::time::Duration::from_secs(15))
            .expect("ready confirmation must submit queued command without a new shell edge");
        assert_eq!(actual, expected);
    }
    assert!(
        received.try_recv().is_err(),
        "the command must be submitted once"
    );
    assert_eq!(queued_command_count(&state, sid), 0);

    // A later timer tick must not replay the command after the first drain.
    let running = Arc::new(AtomicBool::new(true));
    let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
    spawn_silence_timer(silence, running.clone(), sid.into(), state.clone());
    tokio::task::yield_now().await;
    tokio::time::advance(SILENCE_CHECK_INTERVAL + std::time::Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    running.store(false, Ordering::Release);
    assert!(received.try_recv().is_err());
    assert_eq!(queued_command_count(&state, sid), 0);
}

/// Gemini and Aider remove their bottom prompt while a turn runs. The child
/// repaint after Enter, rather than the master's successful write, settles it.
#[cfg(unix)]
#[test]
fn queued_prompt_disappearing_after_enter_confirms_gemini_and_aider() {
    struct RecordingChannel(std::sync::mpsc::Sender<Vec<u8>>);

    impl std::io::Write for RecordingChannel {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.send(bytes.to_vec()).expect("record PTY write");
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    for agent_type in ["gemini", "aider"] {
        let state = crate::state::tests_support::make_test_app_state();
        let sid = format!("prompt-gone-{agent_type}");
        agent_session(&state, &sid, SHELL_IDLE);
        state
            .session_maps
            .session_states
            .get_mut(&sid)
            .unwrap()
            .agent_type = Some(agent_type.into());
        let mut vt = VtLogBuffer::new(24, 80, 1000);
        vt.process(b"\x1b[24;1H> ");
        state
            .grid
            .vt_log_buffers
            .insert(sid.clone(), Mutex::new(vt));
        state
            .session_maps
            .output_buffers
            .insert(sid.clone(), Mutex::new(OutputRingBuffer::new(1024)));
        let (writes, received) = std::sync::mpsc::channel();
        insert_session_with_writer(
            &state,
            &sid,
            Box::new(RecordingChannel(writes)),
            TtyMode::Raw,
        );

        std::thread::scope(|scope| {
            scope.spawn(|| enqueue_user_command(&state, &sid, "check status", None).unwrap());
            for expected in [b"\x15".as_slice(), b"check status", b"\r"] {
                assert_eq!(
                    received
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .unwrap(),
                    expected
                );
            }
            let repaint = b"\x1b[2J\x1b[HProcessing request";
            state
                .grid
                .vt_log_buffers
                .get(&sid)
                .unwrap()
                .lock()
                .process(repaint);
            state
                .session_maps
                .output_buffers
                .get(&sid)
                .unwrap()
                .lock()
                .write(repaint);
        });

        assert!(
            !state
                .session_maps
                .silence_states
                .get(&sid)
                .unwrap()
                .lock()
                .injection_delivery_uncertain,
            "{agent_type} accepted the queued command after its prompt disappeared"
        );
    }
}

/// Boss's rule: a hands-free turn goes straight to the agent, even while it
/// works — the way a line typed by hand into a busy Claude Code does, which the
/// agent queues or takes mid-turn itself. The Compose queue is for something
/// else (one message, let the agent work, then the next), so the turn never
/// enters it. Parking it there until idle cost a median 103 s, max 594 s.
#[cfg(all(unix, feature = "dictation"))]
#[test]
fn a_voice_turn_to_a_busy_agent_is_written_immediately_and_never_queued() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "voice-now", SHELL_BUSY);
    let bytes = insert_recording_session(&state, "voice-now");

    assert_eq!(
        write_voice_turn(&state, "voice-now", "check the logs"),
        Ok(VoiceWrite::Written)
    );

    assert_eq!(
        String::from_utf8(bytes.lock().unwrap().clone()).unwrap(),
        "\u{15}check the logs\r",
        "the same framed write every delivery uses, never raw text"
    );
    assert_eq!(queued_command_count(&state, "voice-now"), 0);
    assert_eq!(
        shell_state_of(&state, "voice-now"),
        SHELL_BUSY,
        "the agent is still working; a mid-turn write must not report it idle"
    );
}

/// An idle agent takes it too, through the same claim the queue uses, and is
/// busy afterwards — the write started a turn.
#[cfg(all(unix, feature = "dictation"))]
#[test]
fn a_voice_turn_to_an_idle_agent_is_written_and_starts_a_turn() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "voice-idle", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "voice-idle");

    assert_eq!(
        write_voice_turn(&state, "voice-idle", "hello"),
        Ok(VoiceWrite::Written)
    );
    assert_eq!(
        String::from_utf8(bytes.lock().unwrap().clone()).unwrap(),
        "\u{15}hello\r"
    );
    assert_eq!(shell_state_of(&state, "voice-idle"), SHELL_BUSY);
}

/// A confident question owns the composer even mid-turn: speech aimed at the
/// agent must not answer a permission dialog.
#[cfg(all(unix, feature = "dictation"))]
#[test]
fn a_voice_turn_is_held_by_a_confident_question() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "voice-dialog", SHELL_BUSY);
    let bytes = insert_recording_session(&state, "voice-dialog");
    set_question_confident(&state, "voice-dialog", true);

    assert_eq!(
        write_voice_turn(&state, "voice-dialog", "yes do it"),
        Ok(VoiceWrite::Held(VoiceHold::Question))
    );
    assert!(bytes.lock().unwrap().is_empty());
    assert_eq!(
        queued_command_count(&state, "voice-dialog"),
        0,
        "held, not queued"
    );
}

/// A draft in the composer holds the turn: the Ctrl-U that opens every write
/// would erase what the user is typing.
#[cfg(all(unix, feature = "dictation"))]
#[test]
fn a_voice_turn_is_held_by_partial_input() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "voice-draft", SHELL_BUSY);
    let bytes = insert_recording_session(&state, "voice-draft");
    let mut buffer = InputLineBuffer::new();
    buffer.feed("half typed");
    state
        .session_maps
        .input_buffers
        .insert("voice-draft".to_string(), parking_lot::Mutex::new(buffer));

    assert_eq!(
        write_voice_turn(&state, "voice-draft", "spoken"),
        Ok(VoiceWrite::Held(VoiceHold::Draft))
    );
    assert!(bytes.lock().unwrap().is_empty());
    assert_eq!(shell_state_of(&state, "voice-draft"), SHELL_BUSY);
}

/// The Compose queue is not touched: a typed entry parked for the next idle
/// stays parked, in place, and a busy agent still receives nothing of it.
#[cfg(all(unix, feature = "dictation"))]
#[test]
fn a_voice_turn_leaves_the_compose_queue_alone() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "voice-compose", SHELL_BUSY);
    let bytes = insert_recording_session(&state, "voice-compose");
    let parked =
        enqueue_user_command(&state, "voice-compose", "run the tests", None).expect("enqueued");
    assert!(!parked.typed);

    assert_eq!(
        write_voice_turn(&state, "voice-compose", "spoken"),
        Ok(VoiceWrite::Written)
    );

    assert_eq!(
        String::from_utf8(bytes.lock().unwrap().clone()).unwrap(),
        "\u{15}spoken\r"
    );
    assert_eq!(
        list_queued_commands(&state, "voice-compose")
            .iter()
            .map(|entry| (entry.text.as_str(), entry.kind))
            .collect::<Vec<_>>(),
        vec![("run the tests", "user_command")]
    );
}

/// A write that never started is held, not lost, and leaves the agent busy:
/// releasing a claim that never took the idle atom must not invent an idle edge.
#[cfg(all(unix, feature = "dictation"))]
#[test]
fn a_mid_turn_voice_write_that_never_started_is_held_and_the_agent_stays_busy() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "voice-fail", SHELL_BUSY);
    insert_session_with_writer(&state, "voice-fail", Box::new(FailingWriter), TtyMode::Raw);

    assert!(matches!(
        write_voice_turn(&state, "voice-fail", "spoken"),
        Ok(VoiceWrite::Held(VoiceHold::WriteNotStarted))
    ));
    assert_eq!(shell_state_of(&state, "voice-fail"), SHELL_BUSY);
    assert!(
        !state
            .session_maps
            .silence_states
            .get("voice-fail")
            .expect("silence")
            .lock()
            .idle_confirmed(),
        "no idle evidence is restored for a turn that was never idle"
    );
    // And the claim was released: the next attempt is not refused as in flight.
    insert_recording_session(&state, "voice-fail");
    assert_eq!(
        write_voice_turn(&state, "voice-fail", "spoken"),
        Ok(VoiceWrite::Written)
    );
}

#[cfg(unix)]
#[test]
fn clear_queued_commands_preserves_peer_deliveries() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "busy", SHELL_BUSY);
    insert_recording_session(&state, "busy");
    enqueue_user_command(&state, "busy", "one", None).expect("enqueued");
    state.pending_injections.get_mut("busy").unwrap().push_back(
        crate::state::PendingInjection::notice("[TUIC message from lead] first peer"),
    );
    enqueue_user_command(&state, "busy", "two", None).expect("enqueued");
    state.pending_injections.get_mut("busy").unwrap().push_back(
        crate::state::PendingInjection::notice("[TUIC message from worker] second peer"),
    );

    // Clear empties the queue, server notices included. Leaving them behind is
    // what let "Clear" empty the visible list while the composer stayed blocked
    // on what was left — and a dropped wake costs nothing: the mail it points at
    // never left the inbox.
    assert_eq!(queued_command_count(&state, "busy"), 4);
    assert_eq!(clear_queued_commands(&state, "busy"), 4);
    assert_eq!(queued_command_count(&state, "busy"), 0);
    assert!(
        state
            .pending_injections
            .get("busy")
            .is_none_or(|queue| queue.is_empty())
    );
    assert_eq!(
        clear_queued_commands(&state, "busy"),
        0,
        "clearing an empty queue is a no-op, not an error"
    );
}

/// A recipient that cannot be woken must not accumulate one parked wake per
/// sender: the pointer covers the whole inbox, and every extra copy is another
/// idle window in which `submit` is rejected `queued_commands_pending`.
#[cfg(unix)]
#[test]
fn repeated_mail_to_a_busy_recipient_parks_a_single_wake() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "busy-mail", SHELL_BUSY);
    insert_recording_session(&state, "busy-mail");

    for _ in 0..3 {
        assert_eq!(
            deliver_notice_to_pty(&state, "busy-mail", PEER_MAIL_WAKE),
            PtyDelivery::Queued
        );
    }

    assert_eq!(
        state
            .pending_injections
            .get("busy-mail")
            .map(|queue| queue.len()),
        Some(1),
        "one pointer covers the whole inbox"
    );
}

/// The Compose panel lists what waits and deletes one entry — of every kind,
/// in delivery order. Server entries used to be invisible and untouchable here,
/// which is precisely why a single parked one could block `submit` with nothing
/// on screen to explain it.
#[cfg(unix)]
#[test]
fn list_and_remove_expose_every_parked_entry() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "busy", SHELL_BUSY);
    insert_recording_session(&state, "busy");
    enqueue_user_command(&state, "busy", "one", None).expect("enqueued");
    state
        .pending_injections
        .get_mut("busy")
        .unwrap()
        .push_back(crate::state::PendingInjection::notice(PEER_MAIL_WAKE));
    enqueue_user_command(&state, "busy", "two", None).expect("enqueued");

    let listed = list_queued_commands(&state, "busy");
    assert_eq!(
        listed.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
        vec!["one", PEER_MAIL_WAKE, "two"],
        "listed in delivery order, nothing hidden"
    );
    assert_eq!(
        listed.iter().map(|c| c.kind).collect::<Vec<_>>(),
        vec!["user_command", "notice", "user_command"],
        "kind is what tells a server entry from the user's own"
    );

    assert!(remove_queued_command(&state, "busy", listed[1].id));
    assert_eq!(
        list_queued_commands(&state, "busy")
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>(),
        vec!["one", "two"],
        "the blocking entry is deletable, and the rest keep their order"
    );
    assert!(
        !remove_queued_command(&state, "busy", listed[1].id),
        "removing an id that already drained is a no-op, not an error"
    );
}

/// The idle path, end to end against a real PTY: an idle agent gets the text
/// typed and submitted at once (Ctrl-U prefix, CR in a separate write), so
/// enqueueing costs nothing when there is no turn to protect.
#[cfg(unix)]
#[test]
fn enqueue_types_immediately_when_agent_is_idle() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "idle-now", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "idle-now");

    let outcome = enqueue_user_command(&state, "idle-now", "ship it", None).expect("enqueued");
    assert_eq!((outcome.typed, outcome.queued), (true, 0));
    assert_eq!(
        String::from_utf8(bytes.lock().unwrap().clone()).unwrap(),
        "\u{15}ship it\r"
    );
}

/// FIFO under the idle path: a user command enqueued behind a peer delivery
/// must not jump ahead of it. The flush types the shared head and leaves the
/// session busy, so the user command stays parked for the next idle window.
#[cfg(unix)]
#[test]
fn enqueue_never_overtakes_a_command_already_waiting() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "fifo", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "fifo");
    state
        .pending_injections
        .entry("fifo".to_string())
        .or_default()
        .push_back(crate::state::PendingInjection::notice("first"));

    let outcome = enqueue_user_command(&state, "fifo", "second", None).expect("enqueued");
    assert_eq!((outcome.typed, outcome.queued), (false, 1));
    assert_eq!(
        String::from_utf8(bytes.lock().unwrap().clone()).unwrap(),
        "\u{15}first\r",
        "the older command is the one that reached the composer"
    );
    assert_eq!(
        state
            .pending_injections
            .get("fifo")
            .expect("queue")
            .front()
            .map(crate::state::PendingInjection::text),
        Some("second")
    );
}

/// The defect this pair pins: `deliver_notice_to_managed_pty` used to return
/// `state.session_maps.sessions.contains_key(session_id)` — "the session exists", not "the
/// message was typed". Every call site read that as delivery and marked the
/// message `TerminalDispatched`, and the waiter filter hides Terminal-owned
/// messages, so a queued-but-never-typed message became invisible to
/// `agent wait` while still sitting unread in the inbox.
#[test]
fn queued_message_is_not_claimed_as_dispatched() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "busy-peer", SHELL_BUSY);
    let msg = "m-queued";
    assert_eq!(
        state.assign_agent_delivery("busy-peer", msg, true),
        crate::state::AgentDeliveryAssignment::Terminal
    );

    let outcome = deliver_notice_to_pty(&state, "busy-peer", "[TUIC message from lead] go");
    settle_terminal_delivery(&state, "busy-peer", msg, outcome);

    assert_eq!(outcome, PtyDelivery::Queued);
    assert_eq!(
        state.agent_delivery_owner("busy-peer", msg),
        Some(crate::state::AgentDeliveryOwner::TerminalPending),
        "still owned by the terminal — but pending, never dispatched"
    );
}

/// `settle_terminal_delivery` is the decision this fix introduced, so pin all
/// three branches directly. Driving `Typed` end-to-end would need a live PTY
/// (without one the claim path always reports `NotStarted` and requeues — see
/// `ready_prompt_delivery_attempts_and_requeues_when_pty_is_missing`), and a
/// fake PTY would only prove the fake.
#[test]
fn settle_maps_each_outcome_to_the_right_ownership() {
    let state = crate::state::tests_support::make_test_app_state();
    for (msg, outcome, expected) in [
        (
            "m-typed",
            PtyDelivery::Typed,
            Some(crate::state::AgentDeliveryOwner::TerminalDispatched),
        ),
        (
            "m-queued",
            PtyDelivery::Queued,
            Some(crate::state::AgentDeliveryOwner::TerminalPending),
        ),
        ("m-gone", PtyDelivery::Unavailable, None),
    ] {
        assert_eq!(
            state.assign_agent_delivery("peer", msg, true),
            crate::state::AgentDeliveryAssignment::Terminal
        );
        settle_terminal_delivery(&state, "peer", msg, outcome);
        assert_eq!(
            state.agent_delivery_owner("peer", msg),
            expected,
            "{outcome:?} must not claim more or less than it achieved"
        );
    }
}

#[test]
fn dead_session_reports_unavailable_and_releases_ownership() {
    let state = crate::state::tests_support::make_test_app_state();
    let msg = "m-dead";
    assert_eq!(
        state.assign_agent_delivery("ghost-peer", msg, true),
        crate::state::AgentDeliveryAssignment::Terminal
    );

    // No PTY was ever registered for this id.
    let outcome = deliver_notice_to_managed_pty(&state, "ghost-peer", "[TUIC] hi");
    settle_terminal_delivery(&state, "ghost-peer", msg, outcome);

    assert_eq!(outcome, PtyDelivery::Unavailable);
    assert_eq!(
        state.agent_delivery_owner("ghost-peer", msg),
        None,
        "ownership handed back so `agent wait` can still surface the inbox copy"
    );
}

#[test]
fn idle_flush_submits_only_one_queued_message_per_turn() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "idle", SHELL_IDLE);
    state.pending_injections.insert(
        "idle".to_string(),
        std::collections::VecDeque::from([
            crate::state::PendingInjection::notice("first"),
            crate::state::PendingInjection::notice("second"),
        ]),
    );

    flush_one_pending_as_submitted(&state, "idle");

    assert_eq!(
        state.pending_injections.get("idle").map(|queue| queue
            .iter()
            .map(|entry| entry.text().to_string())
            .collect::<Vec<_>>()),
        Some(vec!["second".to_string()]),
        "submitting the first message makes the agent busy; later messages must wait for its next idle transition"
    );
}

#[test]
fn deliver_queues_for_idle_agent_with_partial_user_input() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "typing", SHELL_IDLE);
    let mut input = crate::input_line_buffer::InputLineBuffer::new();
    input.feed("draft in progress");
    state
        .session_maps
        .input_buffers
        .insert("typing".to_string(), Mutex::new(input));

    assert!(
        !should_inject_now(&state, "typing"),
        "partial composer input must block terminal injection"
    );
    deliver_notice_to_pty(&state, "typing", "[TUIC message from child] done");
    let pending = state.pending_injections.get("typing").unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending.front().map(crate::state::PendingInjection::text),
        Some("[TUIC message from child] done")
    );
}

#[test]
fn delivery_gate_assigns_waiter_without_touching_terminal_queue() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "waiting", SHELL_IDLE);
    let lease = state.begin_agent_wait("waiting");
    state.push_agent_inbox(
        "waiting",
        crate::state::AgentMessage {
            id: "wait-owned".to_string(),
            from_tuic_session: "sender".to_string(),
            from_name: "sender".to_string(),
            content: "done".to_string(),
            timestamp: 1,
            delivered_via_channel: false,
        },
    );

    assert_eq!(
        state.assign_agent_delivery("waiting", "wait-owned", true),
        crate::state::AgentDeliveryAssignment::Waiter
    );

    assert!(
        !state.pending_injections.contains_key("waiting"),
        "active wait owns delivery; terminal injection must not be queued"
    );
    state.finish_agent_wait("waiting", lease, 0, true);
}

#[test]
fn managed_delivery_rejects_stale_agent_state_without_pty() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "vanished", SHELL_BUSY);

    assert_eq!(
        deliver_notice_to_managed_pty(&state, "vanished", "message"),
        PtyDelivery::Unavailable
    );
    assert!(!state.pending_injections.contains_key("vanished"));
}

#[test]
fn real_activity_invalidates_injection_rollback_ownership() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "active", SHELL_IDLE);
    let claim = claim_idle_for_injection(&state, "active").expect("claim");
    state
        .session_maps
        .silence_states
        .get("active")
        .unwrap()
        .lock()
        .note_real_activity();

    assert!(!rollback_injection_claim(&state, "active", claim));
    assert!(
        state
            .session_maps
            .shell_states
            .get("active")
            .is_some_and(|value| value.load(Ordering::Acquire) == SHELL_BUSY),
        "rollback must not erase genuine post-claim activity"
    );
}

/// Ctrl-U types nothing, and a claim requires an empty composer, so a text
/// write that fails before its first byte left the composer as it was: the
/// submission is retry-safe. Counting the text's progress from the Ctrl-U byte
/// would silently turn this into `Uncertain` and forbid the retry.
#[cfg(unix)]
#[test]
fn text_write_failing_after_ctrl_u_is_a_clean_failure_and_releases_the_claim() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "ctrl-u-then-fail", SHELL_IDLE);
    insert_session_with_writer(
        &state,
        "ctrl-u-then-fail",
        Box::new(FailsAtWrite {
            calls: 0,
            fail_at: 1,
            partial: false,
        }),
        TtyMode::Raw,
    );

    let outcome = write_agent_submission_to_pty(&state, "ctrl-u-then-fail", "retry me");

    assert!(
        matches!(outcome, AgentSubmissionWrite::Failed(ref e) if e.contains("injected PTY failure")),
        "{outcome:?}"
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get("ctrl-u-then-fail")
            .map(|value| value.load(Ordering::Acquire)),
        Some(SHELL_IDLE),
        "a clean failure must hand the idle composer back for the retry"
    );
}

/// Part of the text reached the composer: a retry would type it twice.
#[cfg(unix)]
#[test]
fn partial_text_write_after_ctrl_u_is_uncertain() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "ctrl-u-then-partial", SHELL_IDLE);
    insert_session_with_writer(
        &state,
        "ctrl-u-then-partial",
        Box::new(FailsAtWrite {
            calls: 0,
            fail_at: 1,
            partial: true,
        }),
        TtyMode::Raw,
    );

    let outcome = write_agent_submission_to_pty(&state, "ctrl-u-then-partial", "retry me");

    assert!(
        matches!(outcome, AgentSubmissionWrite::Uncertain(_)),
        "{outcome:?}"
    );
    assert_eq!(
        state
            .session_maps
            .shell_states
            .get("ctrl-u-then-partial")
            .map(|value| value.load(Ordering::Acquire)),
        Some(SHELL_BUSY),
        "an uncertain delivery keeps the claim"
    );
}

#[test]
fn uncertain_injection_preserves_busy_and_surfaces_status_flag() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "uncertain", SHELL_IDLE);
    let claim = claim_idle_for_injection(&state, "uncertain").expect("claim");
    mark_injection_uncertain(&state, "uncertain", claim);

    assert!(
        state
            .session_maps
            .shell_states
            .get("uncertain")
            .is_some_and(|value| value.load(Ordering::Acquire) == SHELL_BUSY)
    );
    assert!(
        state
            .session_maps
            .silence_states
            .get("uncertain")
            .unwrap()
            .lock()
            .injection_delivery_uncertain
    );
    assert!(!state.pending_injections.contains_key("uncertain"));
}

#[test]
fn injection_payload_single_line_is_the_text_alone() {
    // Single-line: no paste wrapper, and no Ctrl-U — that travels in its own write.
    assert_eq!(injection_payload("hello"), "hello");
}

/// Claude Code (verified live on v2.1.280) treats a long input chunk as a
/// paste. A Ctrl-U inside it is stripped as an invisible character, and Claude
/// then refuses the Enter that follows ("review and press Enter to send") —
/// a 584-char submission stayed unsent even with a 500ms Enter gap. Ctrl-U
/// must reach the child in its own read, so it goes out a real gap before the
/// text, and the text a real gap before the Enter.
///
/// Codex is the other half of the same defect (story 1163): it ingests a long
/// plain write as a paste burst and swallows an Enter that lands inside it
/// (live, 0.159.0: 1000 chars at a 200ms gap stayed in the composer). The long
/// single-line wake is therefore framed as one bracketed paste, so no gap has to
/// outlast a length-dependent ingestion time.
#[cfg(unix)]
#[test]
fn agent_submission_keeps_ctrl_u_gap_and_brackets_a_long_single_line_codex_wake() {
    let state = crate::state::tests_support::make_test_app_state();
    let writes = Arc::new(std::sync::Mutex::new(Vec::new()));
    insert_session_with_writer(
        &state,
        "timed-submit",
        Box::new(TimedWriter {
            writes: Arc::clone(&writes),
        }),
        TtyMode::Raw,
    );
    agent_session(&state, "timed-submit", SHELL_IDLE);
    state
        .session_maps
        .session_states
        .get_mut("timed-submit")
        .unwrap()
        .agent_type = Some("codex".into());
    let text = "dictated text ".repeat(50);
    let text = text.trim();
    assert!(!text.contains('\n'), "the bug needs a single-line payload");
    let framed = format!("\x1b[200~{text}\x1b[201~");

    write_agent_command_to_pty(&state, "timed-submit", text).unwrap();

    let writes = writes.lock().unwrap();
    let chunks: Vec<&[u8]> = writes.iter().map(|(_, bytes)| bytes.as_slice()).collect();
    assert_eq!(
        chunks,
        vec![b"\x15".as_slice(), framed.as_bytes(), b"\r".as_slice()]
    );
    assert!(
        writes[1].0 - writes[0].0 >= std::time::Duration::from_millis(45),
        "Ctrl-U and the text must not share a read"
    );
    assert!(
        writes[2].0 - writes[1].0 >= std::time::Duration::from_millis(45),
        "the CR must not share a read with the paste"
    );

    assert_eq!(
        injection_enter_gap(Some("codex")),
        std::time::Duration::from_millis(50)
    );
    assert_eq!(
        injection_enter_gap(Some("claude")),
        std::time::Duration::from_millis(50)
    );
}

/// Critic (1163): only Codex is framed, and only a verified agent gets the
/// short gap. Catches: the Codex branch leaking to another agent, or an agent
/// losing its >=50ms gap (Claude swallows a CR that shares the payload's read).
#[test]
fn long_single_line_payload_is_framed_for_codex_only_and_every_known_gap_is_at_least_50ms() {
    let long = "x".repeat(1000);
    for agent in [
        "claude",
        "gemini",
        "opencode",
        "aider",
        "goose",
        "grok",
        "pi",
        "amp",
        "cursor",
        "droid",
        "future-agent",
    ] {
        let profile = agent_submit_profile(Some(agent));
        assert_eq!((profile.payload)(&long), long, "{agent} must stay plain");
        assert!(
            profile.enter_gap >= std::time::Duration::from_millis(50),
            "{agent} gap"
        );
    }
    assert_eq!((agent_submit_profile(None).payload)(&long), long);
    assert!(agent_submit_profile(None).enter_gap >= std::time::Duration::from_millis(50));
    let codex = agent_submit_profile(Some("codex"));
    assert_eq!((codex.payload)(&long), format!("\x1b[200~{long}\x1b[201~"));
    // Short text, a slash command and a one-key answer stay plain keystrokes.
    for short in ["y", "/status", &"x".repeat(500)] {
        assert_eq!((codex.payload)(short), short);
    }
    assert!(codex.enter_gap >= std::time::Duration::from_millis(50));
}

/// Critic (1163): a human answer to a Codex question is a keypress, not prose.
/// Codex's approval/choice overlays take key events and ignore a paste event, so
/// a bracketed `y` is dropped and the following CR selects the default option.
/// Catches: `write_human_reply_to_pty` framing a short reply as a bracketed paste.
#[cfg(unix)]
#[test]
fn a_short_human_reply_to_a_codex_question_is_typed_not_pasted() {
    let state = crate::state::tests_support::make_test_app_state();
    agent_session(&state, "codex-human-question", SHELL_IDLE);
    let bytes = insert_recording_session(&state, "codex-human-question");
    {
        let mut session = state
            .session_maps
            .session_states
            .get_mut("codex-human-question")
            .unwrap();
        session.agent_type = Some("codex".into());
    }

    assert!(matches!(
        write_human_reply_to_pty(&state, "codex-human-question", "y"),
        AgentSubmissionWrite::Complete { .. }
    ));
    let written = bytes.lock().unwrap();
    assert!(
        !written.windows(4).any(|w| w == b"\x1b[20"),
        "a one-key answer must not be framed as a paste: {:?}",
        String::from_utf8_lossy(&written)
    );
}

#[test]
fn injection_payload_multiline_bracketed_paste() {
    // Multiline MUST ride in a bracketed paste — raw newlines prefill an
    // Ink/codex TUI without submitting; the paste-end marker makes the
    // separately-written CR a genuine Enter (story 091, verified live).
    assert_eq!(
        injection_payload("line1\nline2"),
        "\x1b[200~line1\nline2\x1b[201~"
    );
}

#[test]
fn flush_noop_while_busy() {
    // flush_pending_injections is self-guarded: a direct call against a busy
    // agent (e.g. the user-input unblock path firing while the agent already
    // went back to work) must leave the queue untouched.
    use std::collections::VecDeque;
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    agent_session(&state, "busy", SHELL_BUSY);
    let mut q = VecDeque::new();
    q.push_back(crate::state::PendingInjection::notice("later"));
    state.pending_injections.insert("busy".to_string(), q);

    flush_pending_injections(&state, "busy");
    wait_for_injection_queue();
    assert_eq!(
        state.pending_injections.get("busy").map(|q| q.len()),
        Some(1),
        "busy agent → flush must be a no-op"
    );
}

#[test]
fn mark_session_exited_sends_single_exited_notification() {
    // F1/DATA-1: only one state_change("exited") must reach parent inbox on exit.
    // The BUSY→IDLE transition in the exit path uses notify_parent=false, so the
    // orchestrator must never see a spurious "idle" before "exited".
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let child_id = "child-exit-dedup";
    let parent_id = "parent-exit-dedup";

    state
        .session_maps
        .session_parent
        .insert(child_id.to_string(), parent_id.to_string());
    state.agent_inbox.entry(parent_id.to_string()).or_default();
    let ss = crate::state::SessionState {
        agent_type: Some("claude".to_string()),
        ..Default::default()
    };
    state
        .session_maps
        .session_states
        .insert(child_id.to_string(), ss);
    state.session_maps.shell_states.insert(
        child_id.to_string(),
        std::sync::atomic::AtomicU8::new(SHELL_BUSY),
    );

    // Simulate exit path: transition (notify_parent=false) + mark_session_exited.
    try_shell_transition(&state, child_id, SHELL_BUSY, SHELL_IDLE, false);
    // mark_session_exited needs a sessions entry to attempt exit-code capture
    // (it's OK if there's none — it just skips the exit code).
    push_state_change_to_parent(
        &state,
        child_id,
        serde_json::json!({
            "type": "state_change",
            "state": "exited",
            "session_id": child_id,
            "exit_code": null,
        }),
    );

    let inbox = state
        .agent_inbox
        .get(parent_id)
        .expect("parent inbox must exist");
    assert_eq!(inbox.len(), 1, "inbox must have exactly one message");
    let content: serde_json::Value = serde_json::from_str(&inbox.front().unwrap().content).unwrap();
    assert_eq!(
        content["state"], "exited",
        "the single message must be 'exited'"
    );
}

/// Nothing to retract must stay silent — an idle session emitting a clear
/// on every silence tick would flood the bus and every WS client with it.
#[test]
fn retraction_is_silent_when_the_session_is_not_awaiting() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    state
        .session_maps
        .session_states
        .insert("s1".to_string(), crate::state::SessionState::default());

    let mut rx = state.event_bus.subscribe();
    emit_question_cleared_if_stale(&state, "s1");
    assert!(rx.try_recv().is_err(), "idle session must emit nothing");
}

/// Catches: clearing an armed run-config identity during shell startup blocks
/// the configured agent before it has ever reached the foreground.
#[cfg(unix)]
#[test]
fn configured_agent_is_submittable_during_shell_startup_before_first_observation() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "preset-shell-startup";
    let probe =
        crate::test_support::ForegroundIdentityProbe::shell_root(state.clone(), sid, "bash");
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .seed_configured_agent(Some("claude".into()));
    assert_eq!(refresh_session_agent(&state, sid), None);
    assert!(matches!(
        write_agent_submission_to_pty(&state, sid, "start task"),
        AgentSubmissionWrite::Complete { .. }
    ));
    assert_eq!(*probe.bytes.lock().unwrap(), b"\x15start task\r");
}

/// Catches: a configured preset stays permanently armed after its agent was
/// observed, letting unattended submit/mail type into the shell after exit.
#[cfg(unix)]
#[test]
fn configured_agent_seen_then_exited_to_shell_is_not_submittable_or_wakeable() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "preset-agent-exit";
    let agent =
        crate::test_support::ForegroundIdentityProbe::shell_parent(state.clone(), sid, "claude");
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .seed_configured_agent(Some("claude".into()));
    assert_eq!(
        refresh_session_agent(&state, sid).as_deref(),
        Some("claude")
    );
    let seen = state.session_maps.session_states.get(sid).unwrap().clone();
    assert!(seen.agent_foreground_observed);
    drop(agent);
    let shell =
        crate::test_support::ForegroundIdentityProbe::shell_root(state.clone(), sid, "bash");
    // Replace only the test PTY child; carry the same production identity state.
    state.session_maps.session_states.insert(sid.into(), seen);
    assert_eq!(refresh_session_agent(&state, sid), None);
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .agent_type,
        None
    );
    assert!(matches!(
        write_agent_submission_to_pty(&state, sid, "unsafe shell command"),
        AgentSubmissionWrite::Rejected {
            reason: "not_managed_agent",
            ..
        }
    ));
    assert!(!should_inject_now(&state, sid));
    assert!(shell.bytes.lock().unwrap().is_empty());
}

/// Catches: treating an unclassified startup helper as unseen leaves the preset
/// armed forever, allowing unattended submission into its returned shell.
/// Early disarming is the intentional safe trade-off for direnv/nvm hooks.
#[cfg(unix)]
#[test]
fn non_shell_startup_helper_disarms_preset_and_refuses_submit_after_shell_return() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "preset-startup-helper";
    let helper =
        crate::test_support::ForegroundIdentityProbe::shell_parent(state.clone(), sid, "direnv");
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .seed_configured_agent(Some("claude".into()));
    assert_eq!(
        refresh_session_agent(&state, sid).as_deref(),
        Some("claude")
    );
    let seen = state.session_maps.session_states.get(sid).unwrap().clone();
    assert!(seen.agent_foreground_observed);
    drop(helper);
    let shell =
        crate::test_support::ForegroundIdentityProbe::shell_root(state.clone(), sid, "bash");
    state.session_maps.session_states.insert(sid.into(), seen);
    assert_eq!(refresh_session_agent(&state, sid), None);
    assert!(matches!(
        write_agent_submission_to_pty(&state, sid, "unsafe shell command"),
        AgentSubmissionWrite::Rejected {
            reason: "not_managed_agent",
            ..
        }
    ));
    assert!(!should_inject_now(&state, sid));
    assert!(shell.bytes.lock().unwrap().is_empty());
}

/// Catches: a shell absent from a basename allowlist retains a discovered agent
/// forever. The root process identity must revoke for every shell spelling.
#[cfg(unix)]
#[test]
fn shell_root_identity_revokes_agent_for_ash_and_renamed_shell() {
    // Linux /proc/comm exposes at most 15 bytes. Keep the real executable's
    // arbitrary name within that limit so setup waits for an observable name.
    for name in ["ash", "renamed-shell"] {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sid = "unlisted-root-shell";
        let probe =
            crate::test_support::ForegroundIdentityProbe::shell_root(state.clone(), sid, name);
        state
            .session_maps
            .session_states
            .get_mut(sid)
            .unwrap()
            .agent_type = Some("claude".into());
        assert_eq!(refresh_session_agent(&state, sid), None);
        assert!(!should_inject_now(&state, sid));
        assert!(matches!(
            write_agent_submission_to_pty(&state, sid, "unsafe"),
            AgentSubmissionWrite::Rejected {
                reason: "not_managed_agent",
                ..
            }
        ));
        assert!(probe.bytes.lock().unwrap().is_empty());
    }
}

/// Catches: a nested shell opened by a direct agent revokes its identity, or
/// receives an unattended task/mail wake intended for the parent agent.
#[cfg(unix)]
#[test]
fn direct_agent_nested_subshell_holds_input_without_revoking_identity() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "direct-agent-nested-shell";
    let mut probe = crate::test_support::ForegroundIdentityProbe::bash_wrapper(
        state.clone(),
        sid,
        crate::state::SpawnRootRole::DirectProgram,
    );
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .seed_configured_agent(Some("claude".into()));
    assert_eq!(
        refresh_session_agent(&state, sid).as_deref(),
        Some("claude")
    );
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .agent_type
            .as_deref(),
        Some("claude")
    );
    assert_eq!(
        state
            .session_state_with_shell(sid)
            .unwrap()
            .agent_state
            .as_deref(),
        Some("working")
    );
    assert!(!should_inject_now(&state, sid));
    assert!(matches!(
        write_agent_submission_to_pty(&state, sid, "unsafe child input"),
        AgentSubmissionWrite::Rejected {
            reason: "agent_not_ready",
            ..
        }
    ));
    assert!(probe.bytes.lock().unwrap().is_empty());
    probe.return_to_root();
    assert_eq!(
        refresh_session_agent(&state, sid).as_deref(),
        Some("claude")
    );
    assert!(should_inject_now(&state, sid));
}

/// Catches: a mirrored/legacy session with no authoritative spawn role permits
/// unattended input using a configured agent identity alone.
#[cfg(unix)]
#[test]
fn unknown_spawn_root_role_refuses_submit_and_mail_wake() {
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "missing-root-role";
    let _probe = crate::test_support::ForegroundIdentityProbe::new(state.clone(), sid, "claude");
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .agent_type = Some("claude".into());
    state
        .session_maps
        .session_states
        .get_mut(sid)
        .unwrap()
        .spawn_root_role = crate::state::SpawnRootRole::Unknown;
    assert!(!should_inject_now(&state, sid));
    assert!(matches!(
        write_agent_submission_to_pty(&state, sid, "unsafe"),
        AgentSubmissionWrite::Rejected {
            reason: "agent_not_ready",
            ..
        }
    ));
}

/// Catches: an agent exec'd into the shell root leaves live agent identity and
/// a mail/submit target behind after its process exits instead of a shell return.
#[cfg(unix)]
#[tokio::test]
async fn exec_root_agent_exit_clears_identity_and_refuses_submit_and_mail() {
    use std::io::Write;
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let sid = "exec-root-agent-exit";
    let _probe =
        crate::test_support::ForegroundIdentityProbe::shell_root(state.clone(), sid, "claude");
    assert_eq!(
        refresh_session_agent(&state, sid).as_deref(),
        Some("claude")
    );
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(sid)
            .unwrap()
            .agent_type
            .as_deref(),
        Some("claude")
    );
    assert!(
        should_inject_now(&state, sid),
        "the live exec'd agent must receive input"
    );
    crate::state::AppState::spawn_session_state_accumulator(state.clone());
    _probe.start_reader();
    // The probe's actual cat image exits normally on terminal EOF. The recorded
    // writer remains untouched; use its native master solely to end our process.
    {
        let entry = state.session_maps.sessions.get(sid).unwrap();
        let session = entry.lock();
        session
            .master
            .take_writer()
            .unwrap()
            .write_all(b"\x04")
            .unwrap();
    }
    // Do not wait for the child while holding the session mutex: the native
    // reader needs that lock before it can drain macOS PTY output and see EOF.
    // Native reader EOF now owns lifecycle publication and process cleanup.
    while state.session_maps.session_states.contains_key(sid)
        || state.session_maps.sessions.contains_key(sid)
    {
        tokio::task::yield_now().await;
    }
    assert_eq!(refresh_session_agent(&state, sid), None);
    assert!(!should_inject_now(&state, sid));
    assert!(matches!(
        write_agent_submission_to_pty(&state, sid, "unsafe"),
        AgentSubmissionWrite::Rejected {
            reason: "session_not_found",
            ..
        }
    ));
}

/// Catches: concurrent retries both append, or identical text with a new key is discarded.
#[cfg(unix)]
#[test]
fn queue_idempotency_concurrent_retries_preserve_distinct_jobs_and_sessions() {
    let state = crate::state::tests_support::make_test_app_state();
    for sid in ["keyed-one", "keyed-two"] {
        agent_session(&state, sid, SHELL_BUSY);
        insert_recording_session(&state, sid);
    }
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let state = &state;
            scope.spawn(move || {
                let receipt =
                    enqueue_user_command(state, "keyed-one", "wake", Some("job-1")).unwrap();
                assert!(receipt.accepted);
            });
        }
    });
    enqueue_user_command(&state, "keyed-one", "wake", Some("job-2")).unwrap();
    enqueue_user_command(&state, "keyed-two", "wake", Some("job-1")).unwrap();
    enqueue_user_command(&state, "keyed-one", "wake", None).unwrap();
    enqueue_user_command(&state, "keyed-one", "wake", None).unwrap();
    assert_eq!(list_queued_commands(&state, "keyed-one").len(), 4);
    assert_eq!(list_queued_commands(&state, "keyed-two").len(), 1);
}

/// Catches: invalid keys reserve acceptance, and old keys remain forever rather than bounded.
#[cfg(unix)]
#[test]
fn queue_idempotency_validates_keys_and_bounds_recent_acceptance() {
    let state = crate::state::tests_support::make_test_app_state();
    let sid = "key-bounds";
    agent_session(&state, sid, SHELL_BUSY);
    insert_recording_session(&state, sid);
    for key in [String::new(), "x".repeat(129)] {
        assert!(enqueue_user_command(&state, sid, "wake", Some(&key)).is_err());
    }
    assert!(list_queued_commands(&state, sid).is_empty());
    for n in 0..129 {
        enqueue_user_command(&state, sid, "wake", Some(&format!("job-{n}"))).unwrap();
        clear_queued_commands(&state, sid);
    }
    // Most recent acceptance remains known even after cancellation.
    enqueue_user_command(&state, sid, "wake", Some("job-128")).unwrap();
    assert!(list_queued_commands(&state, sid).is_empty());
    // The oldest falls outside the documented 128-key window.
    enqueue_user_command(&state, sid, "wake", Some("job-0")).unwrap();
    assert_eq!(list_queued_commands(&state, sid).len(), 1);
}

#[cfg(unix)]
mod submit_confirmation {
    //! Submit-confirmation gaps found by the 1423 path audit. Each test names the
    //! plausible bug it catches; none depends on how the fix is built, only on what a
    //! caller or the user observes (toast, uncertainty flag, number of Enters).
    use super::*;
    use crate::state::VtLogBuffer;
    use crate::test_support::{agent_session, insert_recording_session};
    use parking_lot::Mutex;

    const CODEX_READY: &[u8] = b"\x1b[22;1H\xe2\x80\xba Ask Codex to do anything";

    /// An idle agent session with a recording PTY, a VT screen holding `screen`
    /// and an output ring, as the queued-delivery tests build it.
    fn idle_agent(
        agent_type: &str,
        sid: &str,
        screen: &[u8],
    ) -> (
        std::sync::Arc<AppState>,
        std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    ) {
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        agent_session(&state, sid, SHELL_IDLE);
        state
            .session_maps
            .session_states
            .get_mut(sid)
            .unwrap()
            .agent_type = Some(agent_type.into());
        let mut vt = VtLogBuffer::new(24, 80, 1000);
        vt.process(screen);
        state.grid.vt_log_buffers.insert(sid.into(), Mutex::new(vt));
        state
            .session_maps
            .output_buffers
            .insert(sid.into(), Mutex::new(OutputRingBuffer::new(1 << 16)));
        let bytes = insert_recording_session(&state, sid);
        (state, bytes)
    }

    /// Block until the injection's final Enter has reached the PTY.
    fn wait_for_enter(bytes: &std::sync::Mutex<Vec<u8>>) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while bytes.lock().unwrap().last() != Some(&b'\r') {
            assert!(
                std::time::Instant::now() < deadline,
                "injection never wrote Enter"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn confirmation_toasts(
        alerts: &mut tokio::sync::broadcast::Receiver<crate::state::AppEvent>,
    ) -> usize {
        std::iter::from_fn(|| alerts.try_recv().ok())
            .filter(|event| {
                matches!(event, crate::state::AppEvent::McpToast { title, .. }
                    if title == "Agent input was not confirmed")
            })
            .count()
    }

    fn enters(bytes: &std::sync::Mutex<Vec<u8>>) -> usize {
        bytes
            .lock()
            .unwrap()
            .iter()
            .map(|&b| usize::from(b == b'\r'))
            .sum()
    }

    /// Catches: confirmation that reads only the CURRENT screen. Codex accepts the
    /// lifecycle wake, shows Working and finishes before the poller looks (fast turn,
    /// or a Mac busy with a build): the screen is Ready again and a real submission
    /// is reported as "not confirmed".
    #[cfg(unix)]
    #[test]
    fn queued_codex_turn_that_already_finished_when_polled_is_confirmed() {
        let sid = "critic-codex-finished-turn";
        let (state, bytes) = idle_agent("codex", sid, CODEX_READY);
        let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
        let mut alerts = state.event_bus.subscribe();

        std::thread::scope(|scope| {
            scope.spawn(|| enqueue_user_command(&state, sid, "wake the agent", None).unwrap());
            wait_for_enter(&bytes);
            let mut reader = ChunkProcessor::new(None, None);
            reader.process_chunk(
                "\x1b[21;1H\u{2022} Working (1s \u{2022} esc to interrupt)",
                &silence,
                sid,
                &state,
            );
            reader.process_chunk(
                "\x1b[21;1H\x1b[2K\x1b[22;1H\x1b[2K\u{203a} Ask Codex to do anything",
                &silence,
                sid,
                &state,
            );
        });

        assert_eq!(confirmation_toasts(&mut alerts), 0, "false failure toast");
        assert!(!silence.lock().injection_delivery_uncertain);
        assert_eq!(
            enters(&bytes),
            1,
            "a turn that ran must not get a second Enter"
        );
    }

    /// Catches: a Claude hook busy that is gone again (Stop hook ran) before the
    /// poller reads `busy_source_is("hook-busy")`. The pair arrives in one chunk so
    /// the test does not race the poller. The idle hook is the recorded busy hook
    /// with only its state word changed; the framing is the recorded one.
    #[cfg(unix)]
    #[test]
    fn queued_claude_turn_whose_hook_busy_already_ended_is_confirmed() {
        let sid = "critic-claude-finished-turn";
        let (state, bytes) = idle_agent("claude", sid, b"");
        let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
        let mut alerts = state.event_bus.subscribe();
        let capture = String::from_utf8(
            std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/fixtures/agent_prompts/claude-hooked-missing-question-20260921.raw"
            ))
            .expect("recorded Claude hook stream"),
        )
        .expect("UTF-8");
        let busy = capture.find("state=busy").expect("captured busy hook");
        let start = capture[..busy].rfind('\x1b').expect("hook start");
        let end = busy + capture[busy..].find("\x1b\\").expect("hook end") + 2;
        let busy_hook = &capture[start..end];
        let idle_hook = busy_hook.replacen("state=busy", "state=idle", 1);
        let both = format!("{busy_hook}{idle_hook}");

        std::thread::scope(|scope| {
            scope.spawn(|| enqueue_user_command(&state, sid, "wake the agent", None).unwrap());
            wait_for_enter(&bytes);
            let mut reader = ChunkProcessor::new(None, None);
            reader.process_chunk(&both, &silence, sid, &state);
        });

        assert_eq!(confirmation_toasts(&mut alerts), 0, "false failure toast");
        assert!(!silence.lock().injection_delivery_uncertain);
    }

    /// Boss decision A (2026-10-04): lifecycle notices, mail wakes and voice are
    /// write-only; the inbox copy covers a lost Enter. Replaces the round-1 critic
    /// test `notice_to_idle_codex_that_never_reacts_is_uncertain`.
    ///
    /// Catches: a notice to a silent agent holding the shared injection worker for
    /// the confirmation wait, then flagging Uncertain and raising the false
    /// "Agent input was not confirmed" toast.
    #[cfg(unix)]
    #[test]
    fn notice_to_silent_agent_is_written_once_without_waiting_for_confirmation() {
        let sid = "critic-notice-silent-codex";
        let (state, bytes) = idle_agent("codex", sid, CODEX_READY);
        let silence = state.session_maps.silence_states.get(sid).unwrap().clone();
        let mut alerts = state.event_bus.subscribe();

        deliver_notice_to_managed_pty(&state, sid, "[TUIC] child agent 5d0dbc39 is now idle");

        assert_eq!(enters(&bytes), 1, "written once, no retry Enter");
        assert!(
            !silence.lock().injection_delivery_uncertain,
            "a notice is write-only: silence is not an uncertain delivery"
        );
        assert_eq!(confirmation_toasts(&mut alerts), 0, "no toast for a notice");
    }

    /// Catches: `composer_retains_text` matching the submitted text anywhere on the
    /// screen. Once Codex has accepted the turn its transcript echoes `› <text>`
    /// above an EMPTY composer; reading that echo as "still in the composer" sends a
    /// second bare Enter into a live agent, the duplicate submission the toast text
    /// warns about.
    #[cfg(unix)]
    #[test]
    fn codex_transcript_echo_above_empty_composer_gets_no_second_enter() {
        let sid = "critic-codex-transcript-echo";
        let (state, bytes) = idle_agent("codex", sid, CODEX_READY);
        let silence = state.session_maps.silence_states.get(sid).unwrap().clone();

        std::thread::scope(|scope| {
            scope.spawn(|| enqueue_user_command(&state, sid, "wake the agent", None).unwrap());
            wait_for_enter(&bytes);
            let mut reader = ChunkProcessor::new(None, None);
            reader.process_chunk(
                "\x1b[2J\x1b[10;1H\u{203a} wake the agent\x1b[22;1H\u{203a} Ask Codex to do anything",
                &silence,
                sid,
                &state,
            );
        });

        assert_eq!(
            enters(&bytes),
            1,
            "text in the transcript is not text in the composer"
        );
    }
}

#[cfg(unix)]
mod injection_fairness {
    //! Round-2 critic cases for 1423: what the shared injection worker costs other
    //! sessions now that notices wait for submit confirmation.
    use super::*;
    use crate::state::VtLogBuffer;
    use crate::test_support::{agent_session, insert_recording_session};
    use parking_lot::Mutex;

    const CODEX_READY: &[u8] = b"\x1b[22;1H\xe2\x80\xba Ask Codex to do anything";

    /// Catches: a lifecycle/mail notice to a SILENT agent parking the single FIFO
    /// `tuic-injection` thread for the whole confirmation window (6 s for Codex and
    /// Claude). Every other session's notice, queued behind it on the same worker,
    /// is then late by that window; `flush_pending_injections` already moved to a
    /// worker per session for this reason.
    #[cfg(unix)]
    #[test]
    fn silent_agent_notice_does_not_hold_the_shared_injection_worker() {
        let sid = "critic2-silent-codex-parent";
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        agent_session(&state, sid, SHELL_IDLE);
        state
            .session_maps
            .session_states
            .get_mut(sid)
            .unwrap()
            .agent_type = Some("codex".into());
        let mut vt = VtLogBuffer::new(24, 80, 1000);
        vt.process(CODEX_READY);
        state.grid.vt_log_buffers.insert(sid.into(), Mutex::new(vt));
        state
            .session_maps
            .output_buffers
            .insert(sid.into(), Mutex::new(OutputRingBuffer::new(1 << 16)));
        let bytes = insert_recording_session(&state, sid);

        let worker_state = std::sync::Arc::clone(&state);
        spawn_injection_job(move || {
            let _ = deliver_notice_to_managed_pty(&worker_state, sid, "[TUIC] child is now idle");
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while bytes.lock().unwrap().last() != Some(&b'\r') {
            assert!(
                std::time::Instant::now() < deadline,
                "notice never wrote Enter"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let started = std::time::Instant::now();
        wait_for_injection_queue();
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "another session's job waited {:?} behind a silent agent's confirmation window",
            started.elapsed()
        );
    }
}
