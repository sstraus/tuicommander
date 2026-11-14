struct ReportSession {
    state: Arc<AppState>,
    id: String,
}
impl ReportSession {
    fn new() -> Self {
        let state = test_state();
        #[cfg(unix)]
        let shell = "/bin/sh";
        #[cfg(windows)]
        let shell = "cmd.exe";
        let created = handle_session(
            &state,
            &serde_json::json!({"action":"create","shell":shell}),
            None,
        );
        let id = created["session_id"]
            .as_str()
            .expect("real PTY creation")
            .to_owned();
        register_peer(&state, &id, "reporter", "stage-sender");
        register_peer(&state, TEST_UUID_B, "coordinator", "stage-reader");
        Self { state, id }
    }
    fn send(&self, report: serde_json::Value) {
        let sent = handle_messaging(
            &self.state,
            &serde_json::json!({"action":"send","to":TEST_UUID_B,"message":"Report","report":report}),
            Some("stage-sender"),
        );
        assert!(sent.get("error").is_none(), "{sent}");
    }
    fn row(&self) -> serde_json::Value {
        let rows = super::super::session::local_session_rows(&self.state);
        serde_json::to_value(
            rows.into_iter()
                .find(|row| row.session_id == self.id)
                .unwrap(),
        )
        .unwrap()
    }
}
impl Drop for ReportSession {
    fn drop(&mut self) {
        handle_session(
            &self.state,
            &serde_json::json!({"action":"kill","session_id":self.id}),
            None,
        );
    }
}

// Catches: health loses a typed failure or undelivered input while an idle peer
// is incorrectly classified as stalled merely for being quiet.
#[tokio::test]
async fn boss_attention_health_uses_failure_and_delivery_facts() {
    let session = ReportSession::new();
    let attention = || {
        let mut result = crate::boss_attention::BossAttention::default();
        crate::boss_attention::add_health(
            &mut result,
            super::super::session::local_session_rows(&session.state),
        );
        result
    };
    session.send(serde_json::json!({"state":"failed"}));
    assert_eq!(attention().health[0].kind, "Failed");
    session
        .state
        .session_maps
        .exit_codes
        .insert(session.id.clone(), 0);
    assert_eq!(
        attention().health[0].kind,
        "Failed",
        "a current failure remains visible after the owned process stops"
    );
    session.state.session_maps.exit_codes.remove(&session.id);
    session.send(serde_json::json!({"state":"progress"}));
    session.state.pending_initial_prompts.insert(
        session.id.clone(),
        crate::state::PendingInitialPrompt::new("task"),
    );
    assert_eq!(attention().health[0].kind, "Input stalled");
    session.state.pending_initial_prompts.remove(&session.id);
    assert!(
        !attention()
            .health
            .iter()
            .any(|item| item.kind == "Input stalled")
    );
}

// Catches: legacy mail/ACK/eviction erase report facts or replace host time with inbox cursor.
#[tokio::test]
async fn typed_report_stage_survives_legacy_mail_ack_and_eviction() {
    let session = ReportSession::new();
    session.send(serde_json::json!({"state":"ready"}));
    let accepted = session.row();
    assert!(accepted["report_at"].as_u64().is_some_and(|at| at > 0));
    let id = accepted["report_message_id"].clone();
    for _ in 0..=crate::state::AGENT_INBOX_CAPACITY {
        handle_messaging(
            &session.state,
            &serde_json::json!({"action":"send","to":TEST_UUID_B,"message":"BLOCKED: legacy text"}),
            Some("stage-sender"),
        );
    }
    let read = handle_messaging(
        &session.state,
        &serde_json::json!({"action":"inbox"}),
        Some("stage-reader"),
    );
    assert!(
        read["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m.get("report").is_none())
    );
    let row = session.row();
    assert_eq!(row["stage"], "waiting_on_review");
    assert_eq!(row["report_message_id"], id);
    assert_eq!(row["report_at"], accepted["report_at"]);
    let inventory = handle_session(
        &session.state,
        &serde_json::json!({"action":"list","session_id":session.id,"compact":true}),
        None,
    );
    let rows = inventory.as_array().unwrap();
    assert_eq!(rows[0]["stage"], "waiting_on_review");
    assert_eq!(rows[0]["report_at"], accepted["report_at"]);
}

// Catches: report priority inverts, progress invents a stage, or new input retains ready.
#[tokio::test]
async fn typed_report_stage_uses_ordered_facts_and_new_input_epoch() {
    let session = ReportSession::new();
    for (report, expected) in [
        (
            serde_json::json!({"state":"waiting","wait_for":"boss"}),
            "waiting_on_boss",
        ),
        (
            serde_json::json!({"state":"waiting","wait_for":"review"}),
            "waiting_on_review",
        ),
        (
            serde_json::json!({"state":"waiting","wait_for":"dependency"}),
            "blocked",
        ),
        (serde_json::json!({"state":"blocked"}), "blocked"),
        (serde_json::json!({"state":"failed"}), "blocked"),
        (serde_json::json!({"state":"dropped"}), "blocked"),
    ] {
        session.send(report);
        assert_eq!(session.row()["stage"], expected);
    }
    session.send(serde_json::json!({"state":"progress"}));
    assert!(
        session.row()["stage"].is_null(),
        "unknown agent lifecycle must remain unknown"
    );
    session.send(serde_json::json!({"state":"ready"}));
    session.state.pending_initial_prompts.insert(
        session.id.clone(),
        crate::state::PendingInitialPrompt::new("task"),
    );
    assert_eq!(session.row()["stage"], "undelivered_input");
    session.state.pending_initial_prompts.remove(&session.id);
    // The real input-bookkeeping boundary owns turn advancement. Agent identity
    // is normally set by native detection; this rule case supplies that fact.
    session
        .state
        .session_maps
        .session_states
        .entry(session.id.clone())
        .or_default()
        .agent_type = Some("codex".into());
    super::super::session::apply_input_bookkeeping(&session.state, &session.id, "next task\r");
    let row = session.row();
    assert!(
        row.get("report").is_none(),
        "prior-turn report must be invalidated: {row}"
    );
    assert_ne!(row["stage"], "waiting_on_review");
}

// Catches: inventories lose native question, queued-command or background facts.
#[cfg(unix)]
#[tokio::test]
async fn typed_report_stage_prioritizes_native_question_queue_and_background() {
    let state = test_state();
    // Existing direct-agent harness installs the real session maps and an idle
    // composer. It replaces only external PTY writes, not inventory snapshots.
    let bytes = install_atomic_submit_test_session(&state, TEST_UUID_A);
    // HTTP session creation installs this identity binding; the direct-agent
    // composer fixture predates inventory identity fields and leaves it to callers.
    state
        .session_maps
        .live_pty_by_tuic_session
        .insert(TEST_UUID_A.into(), TEST_UUID_A.into());
    register_peer(&state, TEST_UUID_A, "reporter", "stage-sender");
    register_peer(&state, TEST_UUID_B, "coordinator", "stage-reader");
    let session = ReportSession {
        state,
        id: TEST_UUID_A.into(),
    };
    let assert_stage = |expected: Option<&str>| {
        let http_ipc = session.row();
        assert_eq!(http_ipc["stage"].as_str(), expected, "HTTP/IPC: {http_ipc}");
        let mcp = handle_session(
            &session.state,
            &serde_json::json!({"action":"list","session_id":session.id}),
            None,
        );
        assert_eq!(mcp[0]["stage"].as_str(), expected, "MCP list: {mcp}");
        let status = handle_session(
            &session.state,
            &serde_json::json!({"action":"status","session_id":session.id}),
            None,
        );
        assert_eq!(status["stage"].as_str(), expected, "MCP status: {status}");
    };
    assert_stage(Some("idle"));
    // The process reconciler owns this stored fact in production. Keep it in
    // AppState so the real snapshot builder, rather than this test, derives working.
    session
        .state
        .session_maps
        .session_states
        .get_mut(&session.id)
        .unwrap()
        .background_work = true;
    assert_stage(Some("working"));
    super::super::session::apply_input_bookkeeping(&session.state, &session.id, "draft");
    let queued =
        crate::pty::enqueue_user_command(&session.state, &session.id, "next task", None).unwrap();
    assert_eq!(
        queued.queued, 1,
        "partial composer must retain the real queue item"
    );
    assert_stage(Some("queued"));
    session.send(serde_json::json!({"state":"ready"}));
    assert_stage(Some("waiting_on_review"));
    // Feed the production accumulator, as native question producers do.
    crate::state::tests_support::apply_replay_event(
        &session.state,
        &crate::state::AppEvent::PtyParsed {
            session_id: session.id.clone(),
            parsed: serde_json::json!({"type":"question","confident":true,
                "source":"progress-blocked","prompt_text":"Which scope?"})
            .into(),
        },
    );
    assert_stage(Some("waiting_on_boss"));
    session
        .state
        .session_maps
        .exit_codes
        .insert(session.id.clone(), 0);
    assert_stage(None);
    assert!(
        bytes.lock().unwrap().is_empty(),
        "queue must not type into the draft"
    );
}
