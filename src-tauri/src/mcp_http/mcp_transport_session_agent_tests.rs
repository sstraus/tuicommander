// Catches: Codex option values named like subcommands turn submitted tasks into unsent prefill.
#[cfg(unix)]
#[tokio::test]
async fn critic_codex_profile_named_review_keeps_task_submission() {
    use std::os::unix::fs::PermissionsExt;
    for profile in ["review", "exec", "e"] {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let output = root.path().join("argv");
        let binary = root.path().join("codex");
        std::fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$ARGV_OUTPUT\"\nread -r release\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let _config = crate::config::set_config_dir_override(root.path().join("config"));
        let cfg = serde_json::from_value(serde_json::json!({"agents": {"codex": {
            "codex_bypass_migrated": true, "prevent_alt_screen": false,
            "skip_trust_dialog": false, "native_status_signals": false,
            "run_configs": [{"name": "Default", "command": binary,
                "args": ["--profile", profile], "is_default": true,
                "env": {"ARGV_OUTPUT": output}}]
        }}}))
        .unwrap();
        crate::config::save_agents_config(crate::config::AgentsConfig::default(), cfg).unwrap();
        let state = test_state();
        let spawned = handle_agent(
            &state,
            "127.0.0.1:1".parse().unwrap(),
            &serde_json::json!({"action": "spawn", "agent_type": "codex",
                    "prompt": "perform the task", "cwd": root.path()}),
            None,
        );
        assert!(spawned.get("error").is_none(), "{spawned}");
        let actual = wait_for_file_content_async(&output, std::time::Duration::from_secs(60)).await;
        assert!(
            !actual.lines().any(|arg| arg == "perform the task"),
            "profile {profile} is an option value, not a subcommand; argv={actual:?}"
        );
        let session = spawned["session_id"].as_str().unwrap();
        assert_eq!(
            state
                .pending_injections
                .get(session)
                .unwrap()
                .front()
                .unwrap()
                .text(),
            "perform the task"
        );
        // Keep the argv recorder alive while inspecting its live input queue.
        // Close only this test's PTY after the assertions finish.
        handle_session(
            &state,
            &serde_json::json!({"action": "kill", "session_id": session}),
            None,
        );
    }
}

// Catches: prefill-only agent identity steals a named wrapper's positional task.
#[cfg(unix)]
#[tokio::test]
async fn critic_named_wrappers_deliver_positional_task_for_every_agent() {
    use std::os::unix::fs::PermissionsExt;
    for agent in [
        "claude", "codex", "gemini", "grok", "opencode", "aider", "amp", "cursor", "goose",
        "droid", "pi", "ego",
    ] {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let output = root.path().join("argv");
        let binary = root.path().join("wrapper");
        std::fs::write(&binary, "#!/bin/sh\nif [ \"${1-}\" = --version ]; then exit 0; fi\nprintf '%s\\n' \"$@\" > \"$ARGV_OUTPUT\"\n").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let _config = crate::config::set_config_dir_override(root.path().join("config"));
        let cfg = serde_json::from_value(serde_json::json!({"agents": {(agent): {
            "codex_bypass_migrated": true, "prevent_alt_screen": false,
            "skip_trust_dialog": false, "native_status_signals": false,
            "run_configs": [{"name": "My wrapper", "command": binary,
                "args": ["run"], "is_default": true, "env": {"ARGV_OUTPUT": output}}]
        }}}))
        .unwrap();
        crate::config::save_agents_config(crate::config::AgentsConfig::default(), cfg).unwrap();
        let state = test_state();
        let spawned = handle_agent(
            &state,
            "127.0.0.1:1".parse().unwrap(),
            &serde_json::json!({"action": "spawn", "agent_type": "My wrapper",
                    "prompt": "perform the task", "cwd": root.path()}),
            None,
        );
        assert!(spawned.get("error").is_none(), "{agent}: {spawned}");
        let actual = wait_for_file_content_async(&output, std::time::Duration::from_secs(60)).await;
        assert!(
            actual.ends_with("run\nperform the task\n"),
            "agent {agent}: wrapper subcommand and task must remain positional; argv={actual:?}"
        );
        assert!(
            !state
                .pending_injections
                .contains_key(spawned["session_id"].as_str().unwrap())
        );
    }
}

// Needs this module's private helpers, so it is textually included.
// Submit-confirmation gap found by the 1423 path audit (MCP submit receipt).
/// Catches: `acknowledged` meaning "any byte after Enter". Codex re-renders its
/// composer while it ingests a long paste; when the Enter was swallowed that
/// repaint still carries the text, and reporting `acknowledged: true,
/// composer_state: cleared` tells the caller a turn started that never did.
#[cfg(unix)]
#[tokio::test]
async fn session_submit_repaint_that_keeps_text_in_the_composer_is_not_acknowledged() {
    let state = test_state();
    let session_id = "submit-swallowed-enter";
    let bytes = install_atomic_submit_test_session(&state, session_id);
    let vt = crate::state::VtLogBuffer::new(24, 80, 1000);
    state
        .grid
        .vt_log_buffers
        .insert(session_id.to_string(), parking_lot::Mutex::new(vt));
    let call_state = Arc::clone(&state);
    let args = serde_json::json!({
        "action": "submit",
        "session_id": session_id,
        "input": "inspect the repository",
        "timeout_ms": 1_000,
    });
    let call = tokio::spawn(async move {
        handle_mcp_tool_call(&call_state, loopback_addr(), "session", &args, None).await
    });
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
    while bytes.lock().unwrap().last() != Some(&b'\r') {
        assert!(
            tokio::time::Instant::now() < deadline,
            "framed write missing"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let repaint = "\x1b[22;1H\u{203a} inspect the repository";
    state
        .grid
        .vt_log_buffers
        .get(session_id)
        .unwrap()
        .lock()
        .process(repaint.as_bytes());
    state
        .session_maps
        .output_buffers
        .get(session_id)
        .unwrap()
        .lock()
        .write(repaint.as_bytes());

    let response = call.await.unwrap();

    assert_eq!(response["acknowledged"], false, "{response}");
}

#[test]
fn pty_description_field_is_optional_replacement_or_clear() {
    assert!(matches!(
        parse_pty_description(&serde_json::json!({})),
        Ok(PtyDescriptionUpdate::Unchanged)
    ));
    assert_eq!(
        parse_pty_description(&serde_json::json!({"pty_description": "  Run checks  "})),
        Ok(PtyDescriptionUpdate::Set(Some("Run checks".to_string())))
    );
    assert_eq!(
        parse_pty_description(&serde_json::json!({"pty_description": ""})),
        Ok(PtyDescriptionUpdate::Set(None))
    );
    assert_eq!(
        parse_pty_description(&serde_json::json!({"pty_description": null})),
        Ok(PtyDescriptionUpdate::Set(None))
    );
    assert!(parse_pty_description(&serde_json::json!({"pty_description": 7})).is_err());
}

#[test]
fn spawn_description_preserves_explicit_value_or_clear() {
    assert_eq!(
        resolve_spawn_pty_description(
            PtyDescriptionUpdate::Set(Some("Focused task".to_string())),
            "Longer prompt that must not replace the explicit description",
        ),
        Some("Focused task".to_string())
    );
    assert_eq!(
        resolve_spawn_pty_description(
            PtyDescriptionUpdate::Set(None),
            "Prompt must not override an explicit clear",
        ),
        None
    );
}

#[test]
fn spawn_description_falls_back_to_compact_prompt_metadata() {
    assert_eq!(
        resolve_spawn_pty_description(
            PtyDescriptionUpdate::Unchanged,
            "  Audit all Cargo manifests\n\twithout changing files.  ",
        ),
        Some("Audit all Cargo manifests without changing files.".to_string())
    );

    let long_prompt = "è".repeat(INFERRED_PTY_DESCRIPTION_MAX_CHARS + 20);
    let inferred = resolve_spawn_pty_description(PtyDescriptionUpdate::Unchanged, &long_prompt)
        .expect("non-empty prompt produces a description");
    assert_eq!(inferred.chars().count(), INFERRED_PTY_DESCRIPTION_MAX_CHARS);
    assert!(inferred.ends_with('…'));
    assert_eq!(
        resolve_spawn_pty_description(PtyDescriptionUpdate::Unchanged, " \n\t "),
        None
    );
}

#[tokio::test]
async fn session_create_emits_event_bus_session_created() {
    let state = test_state();
    let mut rx = state.event_bus.subscribe();

    let args = serde_json::json!({"action": "create"});
    let result = handle_session(&state, &args, None);

    // Skip if PTY cannot be opened (sandbox/CI without /dev/ptmx access)
    if result.get("error").is_some() {
        eprintln!("Skipping: PTY not available in this environment");
        return;
    }

    // Session should have been created successfully
    assert!(
        result.get("session_id").is_some(),
        "Expected session_id in result: {result}"
    );

    // event_bus should have received SessionCreated
    let event = rx
        .try_recv()
        .expect("Expected SessionCreated event on event_bus");
    match event {
        crate::state::AppEvent::SessionCreated { session_id, .. } => {
            assert_eq!(session_id, result["session_id"].as_str().unwrap());
        }
        other => panic!("Expected SessionCreated, got: {other:?}"),
    }
}

#[tokio::test]
async fn session_list_omits_absent_optional_fields() {
    let state = test_state();
    let created = handle_session(&state, &serde_json::json!({"action": "create"}), None);
    if created.get("error").is_some() {
        eprintln!("Skipping: PTY not available in this environment");
        return;
    }
    let session_id = created["session_id"].as_str().unwrap();
    let listed = handle_session(&state, &serde_json::json!({"action": "list"}), None);
    let entry = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["session_id"] == session_id)
        .unwrap()
        .as_object()
        .unwrap();

    for absent in [
        "display_name",
        "cwd",
        "worktree_path",
        "worktree_branch",
        "agent_state",
        // False for nearly every session, so they follow the same rule as
        // every other optional field rather than being spelled out as false.
        "background_work",
        "standby",
    ] {
        assert!(!entry.contains_key(absent), "{absent} must be omitted");
    }
    assert!(entry.contains_key("is_caller"));

    let _ = handle_session(
        &state,
        &serde_json::json!({"action": "kill", "session_id": session_id}),
        None,
    );
}

#[tokio::test]
async fn session_create_registers_vt_log_and_last_output() {
    let state = test_state();
    let args = serde_json::json!({"action": "create"});
    let result = handle_session(&state, &args, None);

    // Skip if PTY cannot be opened (sandbox/CI without /dev/ptmx access)
    if result.get("error").is_some() {
        eprintln!("Skipping: PTY not available in this environment");
        return;
    }

    let sid = result["session_id"].as_str().unwrap();

    assert!(
        state.grid.vt_log_buffers.contains_key(sid),
        "vt_log_buffers should contain session"
    );
    assert!(
        state.session_maps.last_output_ms.contains_key(sid),
        "last_output_ms should contain session"
    );
    assert!(
        state.session_maps.output_buffers.contains_key(sid),
        "output_buffers should contain session"
    );
}

#[tokio::test]
async fn session_input_updates_same_input_state_as_http_write() {
    let state = test_state();
    let result = handle_session(&state, &serde_json::json!({"action": "create"}), None);
    if result.get("error").is_some() {
        eprintln!("Skipping: PTY not available in this environment");
        return;
    }
    let sid = result["session_id"].as_str().unwrap();

    let input = handle_session(
        &state,
        &serde_json::json!({"action": "input", "session_id": sid, "input": "/"}),
        None,
    );

    assert!(input.get("error").is_none(), "unexpected error: {input}");
    assert!(
        state
            .session_maps
            .last_input_ms
            .get(sid)
            .is_some_and(|stamp| stamp.load(std::sync::atomic::Ordering::Relaxed) > 0),
        "MCP session(input) must stamp last_input_ms"
    );
    assert!(
        state
            .session_maps
            .slash_mode
            .get(sid)
            .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)),
        "MCP session(input) must feed InputLineBuffer and enter slash mode for '/'"
    );
}

// Each row is a turn, question or command a suspend would cut silently.
#[test]
fn suspend_refusal_names_the_work_a_suspend_would_cut() {
    let working = suspend_candidate(Some("claude"), Some("working"), Some("busy"));
    assert_eq!(suspend_refusal(&working), Some("agent working"));
    let starting = suspend_candidate(Some("claude"), Some("starting"), Some("busy"));
    assert_eq!(suspend_refusal(&starting), Some("agent working"));
    let mut background = suspend_candidate(Some("claude"), Some("idle"), Some("busy"));
    background.background_work = true;
    assert_eq!(suspend_refusal(&background), Some("agent working"));
    let mut asking = suspend_candidate(Some("claude"), Some("awaiting_input"), Some("busy"));
    asking.awaiting_input = true;
    assert_eq!(suspend_refusal(&asking), Some("waiting for input"));
    let mut queued = suspend_candidate(Some("claude"), Some("idle"), Some("busy"));
    queued.queued_commands = 1;
    assert_eq!(suspend_refusal(&queued), Some("queued commands pending"));
    let running = suspend_candidate(None, None, Some("busy"));
    assert_eq!(suspend_refusal(&running), Some("command running"));
}

// An agent TUI keeps the shell "busy" for its whole life, so the shell state must not
// veto an idle agent; otherwise no agent could ever be suspended.
#[test]
fn suspend_refusal_allows_an_idle_agent_and_an_idle_shell() {
    let idle_agent = suspend_candidate(Some("claude"), Some("idle"), Some("busy"));
    assert_eq!(suspend_refusal(&idle_agent), None);
    let idle_shell = suspend_candidate(None, None, Some("idle"));
    assert_eq!(suspend_refusal(&idle_shell), None);
}

async fn suspend(state: &Arc<AppState>, sid: &str) -> serde_json::Value {
    handle_session_suspend(
        state,
        &serde_json::json!({"action": "suspend", "session_id": sid}),
        None,
    )
    .await
}

/// Run `suspend` against an attached UI that answers the request with `verdict`.
async fn suspend_answered_with(verdict: Option<(bool, Option<String>)>) -> serde_json::Value {
    let state = test_state();
    let idle = "550e8400-e29b-41d4-a716-446655440c04";
    idle_suspend_candidate(&state, idle);
    state
        .sse_client_count
        .store(1, std::sync::atomic::Ordering::Relaxed);
    let mut events = state.event_bus.subscribe();
    let call = tokio::spawn({
        let state = state.clone();
        async move { suspend(&state, idle).await }
    });
    let request_id = loop {
        if let Ok(crate::state::AppEvent::SessionSuspendRequested {
            session_id,
            request_id,
        }) = events.recv().await
        {
            assert_eq!(session_id, idle);
            break request_id;
        }
    };
    if let Some((ok, reason)) = verdict {
        crate::mcp_http::mcp_transport::resolve_session_suspend(&state, &request_id, ok, reason);
    }
    call.await.expect("suspend task")
}

#[tokio::test]
async fn session_rename_sets_display_name_and_defaults_to_sticky() {
    let state = test_state();
    let created = handle_session(&state, &serde_json::json!({"action": "create"}), None);
    if created.get("error").is_some() {
        eprintln!("Skipping: PTY not available in this environment");
        return;
    }
    let sid = created["session_id"].as_str().unwrap();
    let mut events = state.event_bus.subscribe();

    let result = handle_session(
        &state,
        &serde_json::json!({"action": "rename", "session_id": sid, "name": "Story 857"}),
        None,
    );
    assert!(result.get("error").is_none(), "unexpected error: {result}");

    let listed = handle_session(&state, &serde_json::json!({"action": "list"}), None);
    let entry = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["session_id"] == sid)
        .unwrap();
    assert_eq!(entry["display_name"], "Story 857");
    // The rename starts in the backend: only this push tells the UI (#869-e5da).
    let renamed = std::iter::from_fn(|| events.try_recv().ok()).find_map(|event| match event {
        crate::state::AppEvent::SessionRenamed {
            session_id,
            name,
            is_custom,
        } => Some((session_id, name, is_custom)),
        _ => None,
    });
    assert_eq!(
        renamed,
        Some((sid.to_string(), "Story 857".to_string(), true))
    );
    assert!(
        state
            .session_maps
            .sessions
            .get(sid)
            .unwrap()
            .lock()
            .display_name_is_custom,
        "an MCP rename must be sticky by default, like a manual user rename"
    );

    let _ = handle_session(
        &state,
        &serde_json::json!({"action": "kill", "session_id": sid}),
        None,
    );
}

#[tokio::test]
async fn session_rename_is_custom_false_allows_later_refinement() {
    let state = test_state();
    let created = handle_session(&state, &serde_json::json!({"action": "create"}), None);
    if created.get("error").is_some() {
        eprintln!("Skipping: PTY not available in this environment");
        return;
    }
    let sid = created["session_id"].as_str().unwrap();

    let result = handle_session(
        &state,
        &serde_json::json!({"action": "rename", "session_id": sid, "name": "draft", "is_custom": false}),
        None,
    );
    assert!(result.get("error").is_none(), "unexpected error: {result}");
    assert!(
        !state
            .session_maps
            .sessions
            .get(sid)
            .unwrap()
            .lock()
            .display_name_is_custom,
        "is_custom=false must not lock the name against a later OSC/intent title"
    );

    let _ = handle_session(
        &state,
        &serde_json::json!({"action": "kill", "session_id": sid}),
        None,
    );
}

#[tokio::test]
async fn session_rename_reports_an_unknown_session() {
    let state = test_state();
    let result = handle_session(
        &state,
        &serde_json::json!({"action": "rename", "session_id": "no-such-session", "name": "Foo"}),
        None,
    );
    assert!(
        result.get("error").is_some(),
        "unknown session must error, got: {result}"
    );
}

#[tokio::test]
async fn session_rename_rejects_blank_or_missing_name() {
    let state = test_state();
    let created = handle_session(&state, &serde_json::json!({"action": "create"}), None);
    if created.get("error").is_some() {
        eprintln!("Skipping: PTY not available in this environment");
        return;
    }
    let sid = created["session_id"].as_str().unwrap();

    for args in [
        serde_json::json!({"action": "rename", "session_id": sid}),
        serde_json::json!({"action": "rename", "session_id": sid, "name": "   "}),
        // A tab title is one line of text: an escape sequence or a newline
        // would reach every UI and every peer that lists sessions.
        serde_json::json!({"action": "rename", "session_id": sid, "name": "evil\u{1b}]0;x\u{7}"}),
        serde_json::json!({"action": "rename", "session_id": sid, "name": "two\nlines"}),
        serde_json::json!({"action": "rename", "session_id": sid, "name": "x".repeat(257)}),
    ] {
        let result = handle_session(&state, &args, None);
        assert!(
            result.get("error").is_some(),
            "rename without a non-empty name must error, got: {result}"
        );
    }
    assert!(
        state
            .session_maps
            .sessions
            .get(sid)
            .unwrap()
            .lock()
            .display_name
            .is_none(),
        "a rejected rename must not touch the existing display name"
    );

    let _ = handle_session(
        &state,
        &serde_json::json!({"action": "kill", "session_id": sid}),
        None,
    );
}

// Catches: persisting the pre-preamble workflow/user brief instead of the
// final managed argv, or rebuilding system instructions from later settings.
#[cfg(unix)]
#[tokio::test]
async fn prompt_receipt_records_final_managed_preamble_and_explicit_instruction_args() {
    let state = test_state();
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().join("config"));
    state
        .mcp
        .to_session
        .insert("receipt-parent-protocol".into(), TEST_UUID_B.into());
    // A real OS shell waits on input. This probes TUIC's spawn argv/metadata,
    // not the behavior of any third-party model CLI.
    let spawned = handle_agent(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        &serde_json::json!({
            "action":"spawn", "agent_type":"claude", "binary_path":"/bin/sh",
            "name":"receipt probe", "prompt":"Inspect this launch receipt",
            "cwd":root.path(), "args":["-c", "read receipt_input", "{prompt}", "--append-system-prompt", "é🦀"]
        }),
        Some("receipt-parent-protocol"),
    );
    let session_id = spawned["session_id"]
        .as_str()
        .unwrap_or_else(|| panic!("spawn failed: {spawned}"));
    let captured = crate::prompt_receipt::read_receipt(&state, session_id);
    let stopped = handle_session(
        &state,
        &serde_json::json!({"action":"kill", "session_id":session_id}),
        None,
    );
    assert!(
        stopped.get("error").is_none(),
        "probe cleanup failed: {stopped}"
    );
    let receipt = captured.unwrap();
    assert!(
        receipt.sections[0]
            .text
            .starts_with("## TUICommander Multi-Agent Context")
    );
    assert!(receipt.sections[0].text.contains(TEST_UUID_B));
    assert!(
        receipt.sections[0]
            .text
            .ends_with("Inspect this launch receipt")
    );
    assert_eq!(
        receipt.sections[0].source,
        "TUIC managed spawn / build_spawn_prompt"
    );
    assert_eq!(receipt.sections[1].text, "é🦀");
    assert_eq!(receipt.sections[1].bytes, Some(6));
}

#[cfg(unix)]
#[tokio::test]
async fn mobile_http_reply_answers_confident_question_with_automated_mail_queued() {
    use tower::ServiceExt;

    let state = test_state();
    let session_id = "queued-phone-question";
    let bytes = install_atomic_submit_test_session(&state, session_id);
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .question_confident = true;
    state
        .pending_injections
        .entry(session_id.to_string())
        .or_default()
        .push_back(crate::state::PendingInjection::notice("automated mail"));

    let automated = handle_session_submit(
        &state,
        &serde_json::json!({"session_id": session_id, "input": "agent answer"}),
        false,
    )
    .await;
    assert_eq!(automated["reason"], "awaiting_input");
    assert!(bytes.lock().unwrap().is_empty());

    let mut request = axum::http::Request::post(format!("/sessions/{session_id}/submit"))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(r#"{"input":"Boss answer"}"#))
        .unwrap();
    request
        .extensions_mut()
        .insert(axum::extract::ConnectInfo(loopback_addr()));
    let app = super::super::build_router(Arc::clone(&state), false, true);
    let call = tokio::spawn(async move { app.oneshot(request).await.unwrap() });

    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            if bytes.lock().unwrap().last() == Some(&b'\r') {
                break;
            }
            assert!(!call.is_finished(), "reply returned before writing");
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("phone answer did not reach the PTY");
    state
        .session_maps
        .output_buffers
        .get(session_id)
        .unwrap()
        .lock()
        .write(b"child moved");

    let response = call.await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let receipt: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(receipt["submitted"], true);
    assert_eq!(receipt["acknowledged"], true);
    assert_eq!(bytes.lock().unwrap().as_slice(), b"\x15Boss answer\r");
    assert_eq!(state.pending_injections.get(session_id).unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn session_input_remains_a_raw_write_only_compatibility_surface() {
    let state = test_state();
    let session_id = "input-compat";
    let bytes = install_atomic_submit_test_session(&state, session_id);

    let response = handle_session(
        &state,
        &serde_json::json!({
            "action": "input",
            "session_id": session_id,
            "input": "literal draft",
        }),
        None,
    );

    assert_eq!(response, serde_json::json!({"ok": true}));
    assert_eq!(bytes.lock().unwrap().as_slice(), b"literal draft");
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .turn_epoch,
        0
    );
    assert_eq!(
        state
            .session_maps
            .input_buffers
            .get(session_id)
            .unwrap()
            .lock()
            .content(),
        "literal draft"
    );
}

#[cfg(unix)]
#[test]
fn session_input_claude_text_and_enter_arrive_in_separate_reads() {
    let state = test_state();
    let session_id = "claude-input-gap";
    install_atomic_submit_test_session(&state, session_id);
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .agent_type = Some("claude".into());
    let writes = Arc::new(std::sync::Mutex::new(Vec::new()));
    state
        .session_maps
        .sessions
        .get(session_id)
        .unwrap()
        .lock()
        .writer = Arc::new(parking_lot::Mutex::new(Box::new(InputTimedWriter {
        writes: Arc::clone(&writes),
    })));

    let response = handle_session(
        &state,
        &serde_json::json!({"action":"input", "session_id":session_id, "input":"review this", "special_key":"enter"}),
        None,
    );

    assert_eq!(response, serde_json::json!({"ok":true}));
    let writes = writes.lock().unwrap();
    assert_eq!(
        writes
            .iter()
            .map(|(_, bytes)| bytes.as_slice())
            .collect::<Vec<_>>(),
        vec![b"review this".as_slice(), b"\r".as_slice()]
    );
    assert!(
        writes[1].0 - writes[0].0 >= std::time::Duration::from_millis(45),
        "raw-mode agent must consume text before Enter"
    );
}

#[cfg(unix)]
#[test]
fn session_input_shell_text_and_enter_keep_raw_bytes() {
    let state = test_state();
    let session_id = "shell-input-pair";
    let bytes = install_atomic_submit_test_session(&state, session_id);
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .agent_type = None;

    let response = handle_session(
        &state,
        &serde_json::json!({"action":"input", "session_id":session_id, "input":"echo ready", "special_key":"enter"}),
        None,
    );

    assert_eq!(response, serde_json::json!({"ok":true}));
    assert_eq!(bytes.lock().unwrap().as_slice(), b"echo ready\r");
}

#[cfg(unix)]
#[test]
fn session_input_claude_non_enter_key_keeps_raw_bytes() {
    let state = test_state();
    let session_id = "claude-tab-pair";
    let bytes = install_atomic_submit_test_session(&state, session_id);
    state
        .session_maps
        .session_states
        .get_mut(session_id)
        .unwrap()
        .agent_type = Some("claude".into());

    let response = handle_session(
        &state,
        &serde_json::json!({"action":"input", "session_id":session_id, "input":"choice", "special_key":"tab"}),
        None,
    );

    assert_eq!(response, serde_json::json!({"ok":true}));
    assert_eq!(bytes.lock().unwrap().as_slice(), b"choice\t");
}

/// The loopback guard runs before anything is allocated — a rejected caller
/// must not leave a task behind for someone else to poll.
#[test]
fn a_rejected_remote_spawn_creates_no_task() {
    let state = test_state();
    let rejected = handle_agent(
        &state,
        "8.8.8.8:1234".parse().unwrap(),
        &serde_json::json!({
            "action": "spawn",
            "prompt": "remote caller",
            "binary_path": SHORT_LIVED_TEST_BINARY,
        }),
        Some("mcp-remote"),
    );

    assert!(
        rejected["error"]
            .as_str()
            .is_some_and(|e| e.contains("restricted to localhost")),
        "a remote spawn must be refused: {rejected}"
    );
    assert!(rejected.get("task_id").is_none());
    assert_eq!(state.tasks.len(), 0, "no task may outlive a refused spawn");
}

#[tokio::test]
async fn agent_spawn_rejects_removed_screen_override() {
    let state = test_state();
    for field in ["allow_alt_screen", "allowAltScreen"] {
        let mut request = serde_json::json!({
            "action": "spawn",
            "prompt": "work",
            "binary_path": "/nonexistent/definitely-not-an-agent",
        });
        request[field] = serde_json::json!(true);
        let result = handle_agent(&state, "127.0.0.1:1".parse().unwrap(), &request, None);
        assert!(
            result["error"]
                .as_str()
                .is_some_and(|message| message.contains(field)),
            "removed {field} must be rejected explicitly: {result}"
        );
    }
    assert_eq!(state.tasks.len(), 0);
}

#[cfg(unix)]
#[tokio::test]
async fn agent_spawn_layers_caller_env_and_protects_peer_identity() {
    let root = tempfile::Builder::new()
        .prefix("mcp-spawn-env-")
        .tempdir_in(crate::test_support::test_temp_root())
        .unwrap();
    let output = root.path().join("child-env");
    let command = format!(
        "printf '%s|%s|%s|%s|%s|%s' \"$CLAUDE_CONFIG_DIR\" \"$TUIC_SESSION\" \"$TUIC_PARENT\" \"$LAYER\" \"$ONLY_RUN\" \"$CARGO_INCREMENTAL\" > '{}'",
        output.display()
    );
    let _config = crate::config::set_config_dir_override(root.path().join("tuic-config"));
    let config: crate::config::AgentsConfig = serde_json::from_value(serde_json::json!({
            "agents": {"aider": {"run_configs": [{
                "name": "Env Profile", "command": "/bin/sh", "args": ["-c", command],
                "env": {"CLAUDE_CONFIG_DIR": "/run", "LAYER": "run", "ONLY_RUN": "present", "CARGO_INCREMENTAL": "1",
                        "TUIC_SESSION": "run-spoof", "TUIC_PARENT": "run-spoof"}
            }]}}
        }))
        .unwrap();
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), config).unwrap();
    let state = test_state();
    state
        .mcp
        .to_session
        .insert("mcp-env-test".into(), "parent-peer".into());
    let spawned = handle_agent(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        &serde_json::json!({
            "action": "spawn", "agent_type": "Env Profile", "prompt": "inspect env",
            "env": {"CLAUDE_CONFIG_DIR": "/caller", "LAYER": "caller", "CARGO_INCREMENTAL": "2",
                    "TUIC_SESSION": "spoofed", "TUIC_PARENT": "spoofed"}
        }),
        Some("mcp-env-test"),
    );
    assert!(spawned.get("error").is_none(), "spawn failed: {spawned}");
    let session_id = spawned["session_id"].as_str().unwrap();
    let actual = wait_for_file_content_async(&output, std::time::Duration::from_secs(5)).await;
    assert_eq!(
        actual,
        format!("/caller|{session_id}|parent-peer|caller|present|2")
    );
    std::fs::remove_file(&output).unwrap();
    let unparented = handle_agent(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        &serde_json::json!({
            "action": "spawn", "agent_type": "Env Profile", "prompt": "inspect env",
            "env": {"TUIC_PARENT": "spoofed"}
        }),
        None,
    );
    assert!(
        unparented.get("error").is_none(),
        "spawn failed: {unparented}"
    );
    let unparented_id = unparented["session_id"].as_str().unwrap();
    assert_eq!(
        wait_for_file_content_async(&output, std::time::Duration::from_secs(5)).await,
        format!("/run|{unparented_id}||run|present|1")
    );
}

// Catches: managed Claude background output falls back to the system temp directory.
#[test]
fn managed_claude_tmpdir_creates_default_before_spawn() {
    let home = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let mut cmd = CommandBuilder::new("claude");
    cmd.env_clear();
    cmd.env("HOME", home.path());
    apply_managed_claude_tmpdir(&mut cmd, Some("claude")).unwrap();
    let expected = home.path().join("Gits/.tmp/claude");
    assert_eq!(
        cmd.get_env("CLAUDE_CODE_TMPDIR"),
        Some(expected.as_os_str())
    );
    assert!(
        expected.is_dir(),
        "the harness needs an existing output directory"
    );
}

// Catches: TUIC overwrites an explicit inherited/config/caller harness setting.
#[test]
fn managed_claude_tmpdir_preserves_explicit_environment() {
    let home = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let custom = home.path().join("custom");
    let mut cmd = CommandBuilder::new("claude");
    cmd.env_clear();
    cmd.env("HOME", home.path());
    cmd.env("CLAUDE_CODE_TMPDIR", custom.as_os_str());
    apply_managed_claude_tmpdir(&mut cmd, Some("claude")).unwrap();
    assert_eq!(cmd.get_env("CLAUDE_CODE_TMPDIR"), Some(custom.as_os_str()));
    assert!(!home.path().join("Gits").exists());
}

// Catches: a Claude-only default changes other agents or anonymous executable spawns.
#[test]
fn managed_claude_tmpdir_leaves_other_agents_unchanged() {
    let home = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    for agent in [Some("codex"), Some("goose"), None] {
        let mut cmd = CommandBuilder::new("agent");
        cmd.env_clear();
        cmd.env("HOME", home.path());
        apply_managed_claude_tmpdir(&mut cmd, agent).unwrap();
        assert!(cmd.get_env("CLAUDE_CODE_TMPDIR").is_none());
    }
    assert!(!home.path().join("Gits").exists());
}

// Catches: a failed default-directory creation silently launches Claude into system temp.
#[test]
fn managed_claude_tmpdir_rejects_unusable_default_directory() {
    let home = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    std::fs::write(home.path().join("Gits"), "not a directory").unwrap();
    let mut cmd = CommandBuilder::new("claude");
    cmd.env_clear();
    cmd.env("HOME", home.path());
    let error = apply_managed_claude_tmpdir(&mut cmd, Some("claude")).unwrap_err();
    assert!(error.contains("Cannot create Claude background output directory"));
    assert!(cmd.get_env("CLAUDE_CODE_TMPDIR").is_none());
}

// Catches: spawn assembly drops the default or overwrites run-config/caller overrides.
#[cfg(unix)]
#[tokio::test]
async fn managed_claude_spawn_passes_tmpdir_to_child_with_env_precedence() {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().join("config"));
    let output = root.path().join("child-env");
    let command = format!(
        "printf '%s' \"$CLAUDE_CODE_TMPDIR\" > '{}'",
        output.display()
    );
    let configured = root.path().join("configured");
    let caller = root.path().join("caller");
    let config: crate::config::AgentsConfig = serde_json::from_value(serde_json::json!({
        "agents": {"claude": {"run_configs": [
            {"name": "Default Tmp", "command": "/bin/sh", "args": ["-c", command, "{prompt}"]},
            {"name": "Configured Tmp", "command": "/bin/sh", "args": ["-c", command, "{prompt}"],
             "env": {"CLAUDE_CODE_TMPDIR": configured}}
        ]}}
    }))
    .unwrap();
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), config).unwrap();
    let state = test_state();
    let default = std::env::var_os("CLAUDE_CODE_TMPDIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.path().join("Gits/.tmp/claude"));
    for (profile, override_path, expected) in [
        ("Default Tmp", None, &default),
        ("Configured Tmp", None, &configured),
        ("Configured Tmp", Some(&caller), &caller),
    ] {
        let mut request = serde_json::json!({
            "action": "spawn", "agent_type": profile, "prompt": "inspect env",
            "env": {"HOME": root.path()}
        });
        if let Some(path) = override_path {
            request["env"]["CLAUDE_CODE_TMPDIR"] = serde_json::json!(path);
        }
        let spawned = handle_agent(&state, "127.0.0.1:1".parse().unwrap(), &request, None);
        assert!(spawned.get("error").is_none(), "spawn failed: {spawned}");
        assert_eq!(
            wait_for_file_content_async(&output, std::time::Duration::from_secs(30)).await,
            expected.to_string_lossy()
        );
        std::fs::remove_file(&output).unwrap();
    }
}

#[test]
fn agent_spawn_rejects_non_string_caller_env_before_creating_a_session() {
    let state = test_state();
    let rejected = handle_agent(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        &serde_json::json!({
            "action": "spawn", "prompt": "task", "binary_path": "/bin/sh",
            "env": {"CLAUDE_CONFIG_DIR": 42}
        }),
        None,
    );
    assert_eq!(
        rejected["error"],
        "Action 'spawn' requires 'env' to be a map of string values"
    );
    assert!(state.session_maps.sessions.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn agent_spawn_run_config_model_is_overridable_and_legacy_args_keep_conflict() {
    let root = tempfile::Builder::new()
        .prefix("mcp-spawn-model-")
        .tempdir_in(crate::test_support::test_temp_root())
        .unwrap();
    let _config = crate::config::set_config_dir_override(root.path().join("tuic-config"));
    let output = root.path().join("argv");
    let command = format!("printf '%s|%s' \"$0\" \"$1\" > '{}'", output.display());
    let config: crate::config::AgentsConfig = serde_json::from_value(serde_json::json!({
            "agents": {"aider": {"run_configs": [
                {"name": "Structured", "command": "/bin/sh", "args": ["-c", command], "model": "sonnet"},
                {"name": "Legacy", "command": "/bin/sh", "args": ["-c", "exit 0", "--model", "opus"]}
            ]}}
        })).unwrap();
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), config).unwrap();
    let state = test_state();
    for (requested_model, expected) in [(None, "--model|sonnet"), (Some("opus"), "--model|opus")] {
        let mut request =
            serde_json::json!({"action": "spawn", "agent_type": "Structured", "prompt": "task"});
        if let Some(model) = requested_model {
            request["model"] = serde_json::json!(model);
        }
        let spawned = handle_agent(&state, "127.0.0.1:1".parse().unwrap(), &request, None);
        assert!(spawned.get("error").is_none(), "spawn failed: {spawned}");
        let content = wait_for_file_content(&output, std::time::Duration::from_secs(5));
        assert_eq!(content, expected);
        std::fs::remove_file(&output).unwrap();
    }
    let legacy = handle_agent(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        &serde_json::json!({"action": "spawn", "agent_type": "Legacy", "prompt": "task"}),
        None,
    );
    assert!(
        legacy.get("error").is_none(),
        "legacy config must still spawn: {legacy}"
    );
    let legacy = handle_agent(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        &serde_json::json!({"action": "spawn", "agent_type": "Legacy", "prompt": "task", "model": "sonnet"}),
        None,
    );
    assert!(
        legacy["error"]
            .as_str()
            .is_some_and(|error| error.contains("Conflict: run config already contains --model")),
        "legacy args must remain authoritative: {legacy}"
    );
}

/// A spawn that never reaches the PTY (bad binary) must also leave no task.
#[test]
fn a_failed_spawn_creates_no_task() {
    let state = test_state();
    let failed = handle_agent(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        &serde_json::json!({
            "action": "spawn",
            "prompt": "bad binary",
            "binary_path": "/nonexistent/definitely-not-a-binary",
        }),
        Some("mcp-classic"),
    );

    assert!(failed["error"].is_string(), "spawn must fail: {failed}");
    assert_eq!(state.tasks.len(), 0, "no task may outlive a failed spawn");
}

// --- task tool ---

fn task_call(
    state: &Arc<AppState>,
    addr: &str,
    args: serde_json::Value,
    mcp_sid: Option<&str>,
) -> serde_json::Value {
    handle_task(state, addr.parse().unwrap(), &args, mcp_sid)
}

// ── blocking wait tests (Step 3) ────────────────────────────────

#[test]
fn clamp_wait_timeout_defaults_and_caps() {
    assert_eq!(
        clamp_wait_timeout(None),
        WAIT_DEFAULT_MS,
        "absent → default"
    );
    assert_eq!(
        clamp_wait_timeout(Some(0)),
        WAIT_DEFAULT_MS,
        "zero → default"
    );
    assert_eq!(clamp_wait_timeout(Some(1_000)), 1_000, "in-range preserved");
    assert_eq!(
        clamp_wait_timeout(Some(300_001)),
        WAIT_EFFECTIVE_MAX_MS,
        "over-cap clamped"
    );
    assert_eq!(WAIT_DEFAULT_MS, 60_000);
    assert_eq!(WAIT_MAX_MS, 300_000);
}

/// A caller that asks for the advertised maximum must get an answer, not a
/// client-side abort. Codex ends a tools/call at exactly 300s, so a wait that
/// runs the full 300000 ms loses that race every time.
#[test]
fn the_advertised_maximum_wait_answers_before_a_client_deadline() {
    assert_eq!(
        clamp_wait_timeout(Some(WAIT_MAX_MS)),
        WAIT_EFFECTIVE_MAX_MS,
        "asking for the documented maximum must not run to the client's ceiling"
    );
    const {
        assert!(
            WAIT_EFFECTIVE_MAX_MS < WAIT_MAX_MS,
            "the effective cap has to leave the client room to receive the reply"
        )
    };
    const {
        assert!(
            WAIT_MAX_MS - WAIT_EFFECTIVE_MAX_MS >= 5_000,
            "margin must at least match the bridge's own response margin"
        )
    };
    assert_eq!(
        clamp_wait_timeout(Some(WAIT_EFFECTIVE_MAX_MS - 1)),
        WAIT_EFFECTIVE_MAX_MS - 1,
        "a request below the effective cap is untouched"
    );
}

#[test]
fn session_wait_met_idle_and_exited() {
    use std::sync::atomic::AtomicU8;
    let state = test_state();
    state
        .session_maps
        .shell_states
        .insert("s1".to_string(), AtomicU8::new(crate::pty::SHELL_BUSY));
    assert!(!session_wait_met(&state, "s1", "idle"), "busy → not met");
    state
        .session_maps
        .shell_states
        .insert("s1".to_string(), AtomicU8::new(crate::pty::SHELL_IDLE));
    assert!(session_wait_met(&state, "s1", "idle"), "idle → met");

    assert!(
        !session_wait_met(&state, "s2", "exited"),
        "unknown session → not exited (avoid false immediate met)"
    );
    state.session_maps.exit_codes.insert("s3".to_string(), 0);
    assert!(
        session_wait_met(&state, "s3", "exited"),
        "exit code recorded → exited"
    );
}

/// `wait` now requires a real session (see below), so these tests register one.
/// `#[cfg(unix)]` follows `insert_managed_test_session`, which opens a real PTY.
#[cfg(unix)]
#[tokio::test]
async fn session_wait_returns_immediately_when_already_idle() {
    use std::sync::atomic::AtomicU8;
    let state = test_state();
    insert_managed_test_session(&state, "s", "/tmp");
    state
        .session_maps
        .shell_states
        .insert("s".to_string(), AtomicU8::new(crate::pty::SHELL_IDLE));
    let r = handle_session_wait(
        &state,
        &serde_json::json!({"action":"wait","session_id":"s","until":"idle"}),
    )
    .await;
    assert_eq!(r["met"], true);
    assert_eq!(r["timed_out"], false);
}

#[cfg(unix)]
#[tokio::test]
async fn mcp_delivery_regression_session_wait_times_out_with_flag() {
    use std::sync::atomic::AtomicU8;
    let state = test_state();
    insert_managed_test_session(&state, "s", "/tmp");
    state
        .session_maps
        .shell_states
        .insert("s".to_string(), AtomicU8::new(crate::pty::SHELL_BUSY));
    let r = handle_session_wait(
        &state,
        &serde_json::json!({"action":"wait","session_id":"s","until":"idle","timeout_ms":200}),
    )
    .await;
    assert_eq!(r["met"], false);
    assert_eq!(r["timed_out"], true);
}

/// `subscribe_pty_events` CREATES the channel for whatever id it is handed,
/// and teardown only reaps ids that were real sessions — so an unvalidated
/// `wait` let a caller grow `pty_event_channels` with entries nothing would
/// ever remove, one 256-slot broadcast channel per made-up id. It also blocked
/// the caller for the full timeout on a session that cannot exist.
#[tokio::test]
async fn session_wait_rejects_an_unknown_session_without_subscribing() {
    let state = test_state();
    let started = std::time::Instant::now();

    let r = handle_session_wait(
            &state,
            &serde_json::json!({"action":"wait","session_id":"never-existed","until":"idle","timeout_ms":5_000}),
        )
        .await;

    assert!(
        r["error"]
            .as_str()
            .unwrap_or_default()
            .contains("Unknown session"),
        "unexpected response: {r}"
    );
    assert!(
        state.session_maps.pty_event_channels.is_empty(),
        "an unknown id must not leave a broadcast channel behind"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "it must fail fast, not block for the full timeout"
    );
}

/// The state `until=exited` exists to report is the state the unknown-session
/// guard rejected. `mark_session_exited` records the exit code and THEN drops
/// the `sessions` entry, so from the instant the child dies the wait answered
/// `Unknown session "<id>"` — an error naming the very session the caller had
/// just been handed, for the one outcome it was waiting on.
///
/// The guard itself stays: `subscribe_pty_events` builds a channel for any id
/// it is handed. What had to move is the already-met check, which reads
/// `exit_codes` and subscribes to nothing.
///
/// Both halves are driven for real — `mark_session_exited` builds the
/// tombstone, the wait reads it — so a reorder that stopped recording the exit
/// code before dropping the session would fail here too. No reader thread is
/// started, so nothing races this test for the child's status.
#[cfg(unix)]
#[tokio::test]
async fn session_wait_until_exited_reports_a_just_reaped_session() {
    let state = test_state();
    insert_managed_test_session(&state, "reaped", "/tmp");
    let pid = state
        .session_maps
        .sessions
        .get("reaped")
        .and_then(|entry| entry.value().lock()._child.process_id())
        .expect("test child reports a pid");
    await_child_exit_without_reaping(pid);

    crate::pty::mark_session_exited("reaped", &state);
    assert_eq!(
        state
            .session_maps
            .exit_codes
            .get("reaped")
            .map(|e| *e.value()),
        Some(0),
        "mark_session_exited must record the exit code"
    );
    assert!(
        !state.session_maps.sessions.contains_key("reaped"),
        "…and then drop the session entry — that pairing is the whole defect"
    );

    let channels_before = state.session_maps.pty_event_channels.len();
    let r = handle_session_wait(
        &state,
        &serde_json::json!({
            "action": "wait",
            "session_id": "reaped",
            "until": "exited",
            "timeout_ms": 5_000,
        }),
    )
    .await;

    assert!(
        r.get("error").is_none(),
        "a reaped session must not read as unknown: {r}"
    );
    assert_eq!(r["met"], true, "{r}");
    assert_eq!(r["timed_out"], false, "{r}");
    assert_eq!(
        r["exit_code"], 0,
        "the recorded exit code must come back, not be omitted: {r}"
    );
    assert_eq!(
        state.session_maps.pty_event_channels.len(),
        channels_before,
        "an already-met wait must not subscribe to anything"
    );
}

/// A wait on an id that was never a session still fails fast, and still
/// without leaving a broadcast channel behind — the tombstone fast path above
/// runs first, so it must not weaken that guard.
///
/// The elapsed bound IS the subject here: "fails fast" is the claim, and the
/// 5 s request it must not honour gives it a 5x margin.
#[tokio::test]
async fn session_wait_until_exited_still_rejects_an_id_that_never_existed() {
    let state = test_state();
    let started = std::time::Instant::now();

    let r = handle_session_wait(
        &state,
        &serde_json::json!({
            "action": "wait",
            "session_id": "never-existed",
            "until": "exited",
            "timeout_ms": 5_000,
        }),
    )
    .await;

    assert!(
        r["error"]
            .as_str()
            .unwrap_or_default()
            .contains("Unknown session"),
        "unexpected response: {r}"
    );
    assert!(
        state.session_maps.pty_event_channels.is_empty(),
        "an unknown id must not leave a broadcast channel behind"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "it must fail fast, not block for the full timeout"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn mcp_delivery_regression_session_wait_wakes_from_pty_event_without_polling() {
    use std::sync::atomic::{AtomicU8, Ordering};

    let state = test_state();
    insert_managed_test_session(&state, "event-session", "/tmp");
    state.session_maps.shell_states.insert(
        "event-session".to_string(),
        AtomicU8::new(crate::pty::SHELL_BUSY),
    );
    let waiting_state = Arc::clone(&state);
    let waiter = tokio::spawn(async move {
        handle_session_wait(
            &waiting_state,
            &serde_json::json!({
                "action": "wait",
                "session_id": "event-session",
                "until": "idle",
                "timeout_ms": 1_000,
            }),
        )
        .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    state
        .session_maps
        .shell_states
        .get("event-session")
        .unwrap()
        .store(crate::pty::SHELL_IDLE, Ordering::Release);
    state.emit_pty_event(crate::state::AppEvent::PtyParsed {
        session_id: "event-session".to_string(),
        parsed: serde_json::json!({"type": "shell-state", "state": "idle"}).into(),
    });

    let response = waiter.await.unwrap();
    assert_eq!(response["met"], true);
    assert_eq!(response["timed_out"], false);
}

#[test]
fn messaging_rejects_non_loopback_caller() {
    // A non-loopback caller (remote/LAN, even if it cleared auth via lan_auth_bypass)
    // must not reach any messaging action — it could otherwise register an identity
    // or inject a message into a local agent's context.
    let state = test_state();
    let lan: SocketAddr = "192.168.1.50:4000".parse().unwrap();
    let args = serde_json::json!({
        "action": "register", "tuic_session": "550e8400-e29b-41d4-a716-446655440a01"
    });
    let rejected = handle_agent_unified(&state, lan, &args, Some("mcp-lan"));
    assert!(
        rejected["error"]
            .as_str()
            .unwrap_or("")
            .contains("localhost"),
        "expected localhost-only rejection, got {rejected}"
    );
    assert_eq!(
        state.peer_agents.len(),
        0,
        "LAN register must create no peer"
    );

    // Loopback caller passes the gate and registers normally.
    let loop_addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
    let ok = handle_agent_unified(&state, loop_addr, &args, Some("mcp-local"));
    assert_eq!(ok["ok"], true);
    assert_eq!(state.peer_agents.len(), 1);
}

#[tokio::test]
async fn call_tool_dispatches_to_native_handler_propagating_args() {
    // session with a missing action should surface handle_session's guidance
    // error — this proves the args went through the dispatch layer.
    let state = test_state();
    let r = handle_call_tool(
        &state,
        loopback_addr(),
        &serde_json::json!({ "tool_name": "session", "arguments": {} }),
        None,
        None,
    )
    .await;
    let err = r["error"].as_str().unwrap();
    assert!(
        err.contains("action"),
        "expected handle_session's 'action' guidance error: {err}"
    );
}

/// Characterization for SIMP-1: when a session has registered HTML tabs and is
/// closed via the MCP `session(close)` action, the entry MUST be drained from
/// `session_html_tabs` (the same shared helper is used by `session(kill)`).
#[test]
fn session_close_drains_session_html_tabs_entry() {
    let target = "550e8400-e29b-41d4-a716-446655440d01";
    let state = test_state();
    state
        .session_maps
        .session_html_tabs
        .insert(target.to_string(), vec!["html-tab-1".to_string()]);

    use crate::state::VtLogBuffer;
    state.grid.vt_log_buffers.insert(
        target.to_string(),
        parking_lot::Mutex::new(VtLogBuffer::new(24, 220, 500)),
    );
    handle_session(
        &state,
        &serde_json::json!({"action": "close", "session_id": target}),
        None,
    );
    assert!(
        state.session_maps.session_html_tabs.get(target).is_none(),
        "html tabs entry must be removed after close (drives SIMP-1 helper)"
    );
}

// -------- Tombstone / post-mortem output regression tests --------

/// Simulate a process-exited session (tombstone) by inserting buffers and
/// an exit code without a `sessions` entry. The `output` action must serve
/// the last output with `exited: true` and the captured `exit_code` — NOT
/// return "Session not found".
#[test]
fn tombstoned_session_output_returns_last_buffer_and_exit_code() {
    use crate::OutputRingBuffer;
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = test_state();
    let sid = "tombstone-test-1".to_string();

    // Pre-populate buffers with sample output.
    let mut ring = OutputRingBuffer::new(4096);
    ring.write(b"hello from the crypt\n");
    state
        .session_maps
        .output_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(ring));

    let mut vt = VtLogBuffer::new(24, 80, 100);
    vt.process(b"hello from the crypt\r\n");
    state
        .grid
        .vt_log_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(vt));

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    state
        .session_maps
        .last_output_ms
        .insert(sid.clone(), AtomicU64::new(now_ms));
    state.session_maps.exit_codes.insert(sid.clone(), 42);

    // Sanity: session entry is absent (this IS the tombstone).
    assert!(!state.session_maps.sessions.contains_key(&sid));

    // Raw format path.
    let raw_res = handle_session(
        &state,
        &serde_json::json!({"action": "output", "session_id": sid, "format": "raw"}),
        None,
    );
    assert!(
        raw_res.get("error").is_none(),
        "Unexpected error: {raw_res}"
    );
    assert_eq!(raw_res["exited"], serde_json::json!(true));
    assert_eq!(raw_res["exit_code"], serde_json::json!(42));
    assert!(
        raw_res["data"]
            .as_str()
            .unwrap()
            .contains("hello from the crypt"),
        "Expected tombstoned output in raw response: {raw_res}"
    );

    // Default (VT-clean) format path.
    let clean_res = handle_session(
        &state,
        &serde_json::json!({"action": "output", "session_id": sid}),
        None,
    );
    assert!(
        clean_res.get("error").is_none(),
        "Unexpected error: {clean_res}"
    );
    assert_eq!(clean_res["exited"], serde_json::json!(true));
    assert_eq!(clean_res["exit_code"], serde_json::json!(42));
    assert!(
        clean_res["data"]
            .as_str()
            .unwrap()
            .contains("hello from the crypt"),
        "Expected tombstoned output in clean response: {clean_res}"
    );
}

/// Criterion 3 of story 789-f6ed: secret redaction survives the deletion of
/// the `ai_terminal_*` family.
///
/// `ai_terminal_read_screen` redacted; `session action=output` did not, and
/// it is the screen read every MCP client uses — Claude Code included. So
/// this is not a protection being preserved, it is one being extended to
/// the path that never had it. Both formats are asserted: `raw` exists to
/// keep ANSI, not to opt out of redaction.
#[test]
fn session_output_redacts_a_secret_on_the_terminal() {
    use crate::OutputRingBuffer;
    use crate::state::VtLogBuffer;

    let state = test_state();
    let sid = "redaction-session".to_string();
    let secret = "ghp_0123456789abcdefghijklmnopqrstuvwxyzAB";
    let line = format!("export GITHUB_TOKEN={secret}");

    let mut ring = OutputRingBuffer::new(4096);
    ring.write(format!("{line}\n").as_bytes());
    state
        .session_maps
        .output_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(ring));

    let mut vt = VtLogBuffer::new(24, 80, 100);
    vt.process(format!("{line}\r\n").as_bytes());
    state
        .grid
        .vt_log_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(vt));

    for format in ["clean", "raw"] {
        let mut args = serde_json::json!({ "action": "output", "session_id": sid });
        if format == "raw" {
            args["format"] = serde_json::json!("raw");
        }
        let response = handle_session(&state, &args, None);
        let data = response["data"]
            .as_str()
            .unwrap_or_else(|| panic!("{format}: no data in {response}"));
        assert!(
            !data.contains(secret),
            "{format} output leaked the token: {data}"
        );
        assert!(
            data.contains("[REDACTED]"),
            "{format} output must say it redacted something: {data}"
        );
    }
}

/// Read `session action=output` in every shape a caller can ask for and
/// assert that no secret fragment survives.
fn assert_no_fragment_in_reads(vt: crate::state::VtLogBuffer, label: &str) {
    let state = test_state();
    let sid = "wrap-session".to_string();
    state
        .grid
        .vt_log_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(vt));
    let reads = [
        serde_json::json!({ "action": "output", "session_id": sid }),
        serde_json::json!({ "action": "output", "session_id": sid, "since_cursor": 0 }),
        serde_json::json!({ "action": "output", "session_id": sid, "from_line": 1 }),
        serde_json::json!({ "action": "output", "session_id": sid, "limit": 2 }),
    ];
    for args in reads {
        let response = handle_session(&state, &args, None);
        let data = response["data"]
            .as_str()
            .unwrap_or_else(|| panic!("{label}: no data in {response}"));
        if let Some(gram) = leaks_fragment(data, WRAP_SECRET) {
            panic!("{label}: {args} leaked {gram:?} of the token: {data:?}");
        }
    }
}

/// Every response of every small-window read, individually: a caller that
/// polls with a tiny `limit` sees one response at a time.
fn assert_windows_never_leak(vt: crate::state::VtLogBuffer, label: &str) {
    let state = test_state();
    let sid = "window-session".to_string();
    let total = vt.total_lines();
    state
        .grid
        .vt_log_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(vt));
    for start in 0..=total {
        for limit in 1..=3 {
            for key in ["since_cursor", "from_line"] {
                let args = serde_json::json!({
                    "action": "output", "session_id": sid, key: start, "limit": limit
                });
                let response = handle_session(&state, &args, None);
                let data = response["data"].as_str().unwrap_or("");
                if let Some(gram) = leaks_fragment(data, WRAP_SECRET) {
                    panic!("{label}: {args} leaked {gram:?} of the token: {data:?}");
                }
            }
        }
    }
}

/// RED (review finding 2): a redraw that moves the cursor after every
/// character leaves no 5-char run in the byte stream.
#[test]
fn session_output_raw_redacts_a_secret_redrawn_one_character_at_a_time() {
    use crate::OutputRingBuffer;
    use crate::state::VtLogBuffer;

    for with_grid in [false, true] {
        let state = test_state();
        let sid = "per-char-session".to_string();
        let mut raw = String::from("echo ");
        for c in WRAP_SECRET.chars() {
            raw.push_str(&format!("{c}\x1b[C\x1b[D"));
        }
        raw.push_str("\r\n");
        let mut ring = OutputRingBuffer::new(4096);
        ring.write(raw.as_bytes());
        state
            .session_maps
            .output_buffers
            .insert(sid.clone(), parking_lot::Mutex::new(ring));
        if with_grid {
            let mut vt = VtLogBuffer::new(24, 80, 100);
            vt.process(format!("echo {WRAP_SECRET}\r\n").as_bytes());
            state
                .grid
                .vt_log_buffers
                .insert(sid.clone(), parking_lot::Mutex::new(vt));
        }
        let response = handle_session(
            &state,
            &serde_json::json!({ "action": "output", "session_id": sid, "format": "raw" }),
            None,
        );
        let data = response["data"].as_str().expect("raw data");
        assert_eq!(
            leaks_fragment(&visible_text(data), WRAP_SECRET),
            None,
            "with_grid={with_grid}: {data:?}"
        );
    }
}

/// Cost of the reads agents poll: a full 10k-line log and a full 2 MB ring
/// with a secret every 50 lines. Run with `--run-ignored only --no-capture`.
#[test]
#[ignore = "timing measurement, not an assertion"]
fn session_output_read_cost_on_a_full_log() {
    use crate::OutputRingBuffer;
    use crate::state::VtLogBuffer;

    let state = test_state();
    let sid = "cost-session".to_string();
    let mut ring = OutputRingBuffer::new(crate::state::OUTPUT_RING_BUFFER_CAPACITY);
    let mut vt = VtLogBuffer::new(24, 80, 10_000);
    let mut n = 0;
    while ring.total_written() < 2 * crate::state::OUTPUT_RING_BUFFER_CAPACITY as u64 {
        let line = if n % 50 == 0 {
            format!("export GITHUB_TOKEN={WRAP_SECRET} # {n}")
        } else {
            format!(
                "\x1b[32m   Compiling\x1b[0m crate-{n} v1.{n}.0 (/Users/dev/project/crates/crate-{n}) done in 0.{n}s"
            )
        };
        ring.write(format!("{line}\r\n").as_bytes());
        if n < 12_000 {
            vt.process(format!("{line}\r\n").as_bytes());
        }
        n += 1;
    }
    eprintln!("log lines={} ring lines={n}", vt.total_lines());
    state
        .session_maps
        .output_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(ring));
    state
        .grid
        .vt_log_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(vt));
    for (name, args) in [
        (
            "tail",
            serde_json::json!({ "action": "output", "session_id": sid }),
        ),
        (
            "raw",
            serde_json::json!({ "action": "output", "session_id": sid, "format": "raw" }),
        ),
        (
            "raw limit=100000",
            serde_json::json!({ "action": "output", "session_id": sid, "format": "raw", "limit": 100000 }),
        ),
    ] {
        let mut times = Vec::new();
        for _ in 0..15 {
            let start = std::time::Instant::now();
            let response = handle_session(&state, &args, None);
            times.push(start.elapsed());
            assert!(response["data"].is_string());
        }
        times.sort();
        eprintln!("COST {name}: median={:?} max={:?}", times[7], times[14]);
    }
}

/// `format=raw` keeps ANSI, so it cannot be cleaned by re-reading the grid.
/// These are the bytes zsh 5.9 really wrote to an 80-column pty for the
/// story's command: the typed echo is redrawn around the wrap (` \r`,
/// `ESC[K`, an overwritten `t`), then the command's own output follows.
#[test]
fn session_output_raw_redacts_a_secret_the_line_editor_wrapped() {
    use crate::OutputRingBuffer;

    let state = test_state();
    let sid = "wrap-raw-session".to_string();
    let raw = "e\x08echo GITHUB_TOKEN=ghp_abcdefghijklmnopqrs \r\x1b[Kt\rtuvwxyz0123456789\x1b[?2004l\r\r\n\
                   GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789\r\n";
    let secret = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let mut ring = OutputRingBuffer::new(4096);
    ring.write(raw.as_bytes());
    state
        .session_maps
        .output_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(ring));

    let response = handle_session(
        &state,
        &serde_json::json!({ "action": "output", "session_id": sid, "format": "raw" }),
        None,
    );
    let data = response["data"].as_str().expect("raw data");
    assert_eq!(
        leaks_fragment(&visible_text(data), secret),
        None,
        "raw leaked: {data:?}"
    );
    assert!(data.contains("[REDACTED]"), "{data:?}");
}

/// A token typed with no echo of it elsewhere (`read`-less `export`) is
/// known to the terminal grid only; the raw read must still scrub the
/// fragments the redraw left in the byte stream.
#[test]
fn session_output_raw_redacts_a_wrapped_secret_seen_only_by_the_grid() {
    use crate::OutputRingBuffer;
    use crate::state::VtLogBuffer;

    let state = test_state();
    let sid = "wrap-raw-grid-session".to_string();
    let raw = format!(
        "export GITHUB_TOKEN={} \r\x1b[K{}\r\n",
        &WRAP_SECRET[..20],
        &WRAP_SECRET[20..]
    );
    let mut ring = OutputRingBuffer::new(4096);
    ring.write(raw.as_bytes());
    state
        .session_maps
        .output_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(ring));
    let mut vt = VtLogBuffer::new(24, 40, 100);
    vt.process(format!("export GITHUB_TOKEN={WRAP_SECRET}\r\n").as_bytes());
    state
        .grid
        .vt_log_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(vt));

    let response = handle_session(
        &state,
        &serde_json::json!({ "action": "output", "session_id": sid, "format": "raw" }),
        None,
    );
    let data = response["data"].as_str().expect("raw data");
    assert_eq!(
        leaks_fragment(&visible_text(data), WRAP_SECRET),
        None,
        "raw leaked: {data:?}"
    );
}

/// The tail read is what an orchestrator pays for on every check of a child:
/// the empty input box and the user's status line under it carry nothing it
/// can use, so they are cut. `format=raw` stays the unfiltered escape hatch.
#[test]
fn session_output_tail_omits_empty_input_box_and_status_line() {
    use crate::OutputRingBuffer;
    use crate::state::VtLogBuffer;

    let state = test_state();
    let sid = "chrome-trim-session".to_string();
    let separator = "─".repeat(40);
    let screen = format!(
        "  the agent's answer\r\n\r\n{separator}\r\n❯ \r\n{separator}\r\n  [Opus 5 | Team] ██░░ 22% | $1.07\r\n  ⏵⏵ auto mode on"
    );

    let mut ring = OutputRingBuffer::new(4096);
    ring.write(screen.as_bytes());
    state
        .session_maps
        .output_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(ring));
    let mut vt = VtLogBuffer::new(24, 80, 100);
    vt.process(screen.as_bytes());
    state
        .grid
        .vt_log_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(vt));

    let clean = handle_session(
        &state,
        &serde_json::json!({"action": "output", "session_id": sid}),
        None,
    );
    let data = clean["data"].as_str().expect("clean data");
    assert!(data.contains("the agent's answer"), "{data}");
    for chrome in ["❯", "Opus 5", "auto mode", "────"] {
        assert!(
            !data.contains(chrome),
            "tail kept chrome {chrome:?}: {data}"
        );
    }

    let raw = handle_session(
        &state,
        // The raw ring reads `limit` bytes, not lines.
        &serde_json::json!({"action": "output", "session_id": sid, "format": "raw", "limit": 4096}),
        None,
    );
    assert!(
        raw["data"].as_str().expect("raw data").contains("Opus 5"),
        "format=raw must keep the status line: {raw}"
    );
}

#[test]
fn session_output_omits_unknown_exit_code() {
    use crate::OutputRingBuffer;

    let state = test_state();
    let sid = "tombstone-without-exit-code";
    let mut ring = OutputRingBuffer::new(4096);
    ring.write(b"final output\n");
    state
        .session_maps
        .output_buffers
        .insert(sid.to_string(), parking_lot::Mutex::new(ring));

    let response = handle_session(
        &state,
        &serde_json::json!({"action": "output", "session_id": sid, "format": "raw"}),
        None,
    );

    assert_eq!(response["exited"], true);
    assert!(
        response.get("exit_code").is_none(),
        "unknown optional exit_code must be omitted: {response}"
    );
}

/// A timeout response must not carry a redundant `hint`. This used to force
/// the timeout with a nonexistent session id — that path now returns an error
/// instead of blocking, so the timeout is forced with a real busy session,
/// which is what a timeout actually means.
#[cfg(unix)]
#[tokio::test]
async fn session_wait_timeout_has_no_redundant_hint() {
    use std::sync::atomic::AtomicU8;
    let state = test_state();
    insert_managed_test_session(&state, "busy-session", "/tmp");
    state.session_maps.shell_states.insert(
        "busy-session".to_string(),
        AtomicU8::new(crate::pty::SHELL_BUSY),
    );
    let response = handle_session_wait(
        &state,
        &serde_json::json!({
            "action": "wait",
            "session_id": "busy-session",
            "timeout_ms": 1,
        }),
    )
    .await;

    assert_eq!(response["met"], false);
    assert_eq!(response["timed_out"], true);
    assert!(response.get("hint").is_none());
}

// Catches: delta pages expose PEM body rows because only the page, not
// the retained multiline key, is considered during secret discovery.
#[test]
fn session_output_delta_pages_do_not_leak_multiline_private_key_body() {
    let state = test_state();
    let sid = "critic-pem-pages";
    let body = "QWxwaGFCZXRhR2FtbWFEZWx0YUVwc2lsb25aZXRh";
    let mut vt = crate::state::VtLogBuffer::new(2, 120, 100);
    vt.process(
            format!(
                "-----BEGIN OPENSSH PRIVATE KEY-----\r\n{body}\r\n-----END OPENSSH PRIVATE KEY-----\r\nafter\r\nflush\r\n"
            )
            .as_bytes(),
        );
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), parking_lot::Mutex::new(vt));
    let mut cursor = 0;
    let mut observed = String::new();
    loop {
        let page = session_output(
            &state,
            &serde_json::json!({
                "action": "output", "session_id": sid,
                "since_cursor": cursor, "limit": 1
            }),
        );
        observed.push_str(page["data"].as_str().unwrap());
        if page["has_more"] == false {
            break;
        }
        let next = page["next_cursor"].as_u64().unwrap();
        assert!(next > cursor, "paging must advance: {page}");
        cursor = next;
    }
    assert!(
        observed.contains("after"),
        "must reach output after the key"
    );
    assert!(
        !observed.contains(body),
        "delta paging leaked a private key body: {observed:?}"
    );
}

// Catches: splitting secret discovery at the history/screen boundary exposes
// a PEM body even though the complete key remains in the terminal.
#[test]
fn session_output_pages_do_not_leak_private_key_crossing_history_screen() {
    let state = test_state();
    let sid = "critic-pem-screen-boundary";
    let body = "QWxwaGFCZXRhR2FtbWFEZWx0YUVwc2lsb25aZXRh";
    let text = format!(
        "-----BEGIN OPENSSH PRIVATE KEY-----\r\n{body}\r\n-----END OPENSSH PRIVATE KEY-----\r\n"
    );
    let mut vt = crate::state::VtLogBuffer::new(2, 120, 100);
    vt.process(text.as_bytes());
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), parking_lot::Mutex::new(vt));
    let mut ring = crate::OutputRingBuffer::new(4096);
    ring.write(text.as_bytes());
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), parking_lot::Mutex::new(ring));
    let mut leaked = Vec::new();
    for request in [
        serde_json::json!({"from_line":1,"limit":1}),
        serde_json::json!({"since_cursor":1,"limit":1}),
        serde_json::json!({"limit":1}),
        serde_json::json!({"format":"raw","from_byte":0,"limit":4096}),
        serde_json::json!({"format":"raw","since_cursor":0,"limit":4096}),
    ] {
        let mut args = request.clone();
        args["action"] = "output".into();
        args["session_id"] = sid.into();
        let page = session_output(&state, &args);
        let data = page["data"].as_str().expect("terminal output must exist");
        if data.contains(body) {
            leaked.push(request);
        }
    }
    assert!(
        leaked.is_empty(),
        "retained PEM body leaked across history/screen boundary: {leaked:?}"
    );
}

// Catches: reusing the absolute window's total cursor skips all later pages.
#[test]
fn session_output_pages_clean_history_without_skipping_or_repeating_lines() {
    let defs = native_tool_definitions();
    let session = defs
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "session")
        .unwrap();
    assert_eq!(
        session["inputSchema"]["properties"]["from_byte"]["minimum"],
        0
    );
    let state = test_state();
    let sid = "paged-clean";
    let mut vt = crate::state::VtLogBuffer::new(2, 12, 100);
    for line in [
        "first",
        "a long line wrapping across several rows",
        "日本語",
        "fourth",
        "screen",
        "",
    ] {
        vt.process(format!("{line}\r\n").as_bytes());
    }
    let (expected, _, total) = vt.lines_since_logical(0, usize::MAX);
    let expected =
        crate::redaction::join_wrapped_rows(expected.iter().map(|l| (l.text(), l.wrapped)));
    state
        .grid
        .vt_log_buffers
        .insert(sid.into(), parking_lot::Mutex::new(vt));
    let mut position = 0;
    let mut pages = Vec::new();
    loop {
        let page = session_output(
            &state,
            &serde_json::json!({
                "action":"output", "session_id":sid, "from_line":position, "limit":2
            }),
        );
        assert_eq!(page["cursor"], total); // Existing snapshot/delta contract.
        pages.push(page["data"].as_str().unwrap().to_owned());
        if !page["has_more"].as_bool().unwrap() {
            assert!(page["next_cursor"].is_null());
            break;
        }
        assert_eq!(page["truncated"], true);
        assert!(page["continuation"].as_str().unwrap().contains("from_line"));
        let next = page["next_cursor"].as_u64().unwrap();
        assert!(next > position && next < total as u64);
        position = next;
    }
    assert_eq!(pages.join("\n"), expected);
    let delta = session_output(
        &state,
        &serde_json::json!({
            "action":"output", "session_id":sid, "since_cursor":0, "limit":1
        }),
    );
    assert_eq!(delta["cursor"], delta["next_cursor"]);
    assert_eq!(delta["has_more"], true);
    let tail = session_output(
        &state,
        &serde_json::json!({
            "action":"output", "session_id":sid, "limit":1
        }),
    );
    assert_eq!(tail["truncated"], true);
    assert!(
        tail["continuation"]
            .as_str()
            .unwrap()
            .contains("older output")
    );
}

// Catches: source-byte windows split Unicode or keep returning the tail.
#[test]
fn session_output_pages_raw_unicode_and_ansi_without_reexecuting() {
    let state = test_state();
    let sid = "paged-raw";
    let expected = "\x1b[31mfirst café 日本語🙂\x1b[0m\r\nlast";
    let mut ring = crate::OutputRingBuffer::new(4096);
    ring.write(expected.as_bytes());
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), parking_lot::Mutex::new(ring));
    for limit in [1, 2, 7, 50] {
        let mut position = 0;
        let mut actual = String::new();
        loop {
            let page = session_output(
                &state,
                &serde_json::json!({
                    "action":"output", "session_id":sid, "format":"raw", "since_cursor":position, "limit":limit
                }),
            );
            actual.push_str(page["data"].as_str().unwrap());
            assert_eq!(page["start_offset"], position);
            assert_eq!(page["total_written"], expected.len());
            let next = page["cursor"].as_u64().unwrap();
            assert!(next > position);
            if !page["has_more"].as_bool().unwrap() {
                assert_eq!(next, expected.len() as u64);
                break;
            }
            assert_eq!(page["next_cursor"], next);
            assert!(page["continuation"].as_str().unwrap().contains("from_byte"));
            position = next;
        }
        assert_eq!(actual, expected, "limit={limit}");
    }
    let tail = session_output(
        &state,
        &serde_json::json!({
            "action":"output", "session_id":sid, "format":"raw", "limit":3
        }),
    );
    assert_eq!(tail["data"], "ast");
    assert_eq!(tail["truncated"], true);
    assert!(
        tail["continuation"]
            .as_str()
            .unwrap()
            .contains("older output")
    );
}

// Catches: eviction is silently presented as complete output, or a future
// offset underflows and panics instead of producing an exhausted page.
#[test]
fn session_output_raw_pages_report_eviction_and_exhausted_offsets() {
    let state = test_state();
    let sid = "paged-eviction";
    let mut ring = crate::OutputRingBuffer::new(8);
    ring.write(b"0123456789abcdef");
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), parking_lot::Mutex::new(ring));
    let stale = session_output(
        &state,
        &serde_json::json!({
            "action":"output", "session_id":sid, "format":"raw", "from_byte":0, "limit":3
        }),
    );
    assert_eq!(stale["data"], "89a");
    assert_eq!(stale["oldest_offset"], 8);
    assert_eq!(stale["missed_count"], 8);
    assert_eq!(stale["next_cursor"], 11);
    for offset in [16, 100, u64::MAX] {
        let empty = session_output(
            &state,
            &serde_json::json!({
                "action":"output", "session_id":sid, "format":"raw", "from_byte":offset
            }),
        );
        assert_eq!(empty["data"], "");
        assert_eq!(empty["cursor"], 16);
        assert_eq!(empty["has_more"], false);
        assert_eq!(empty["truncated"], false);
    }
    let entry = state.session_maps.output_buffers.get(sid).unwrap();
    let mut ring = entry.lock();
    *ring = crate::OutputRingBuffer::new(8);
    drop(ring);
    drop(entry);
    let empty = session_output(
        &state,
        &serde_json::json!({
            "action":"output", "session_id":sid, "format":"raw", "from_byte":0
        }),
    );
    assert_eq!(empty["data"], "");
    assert_eq!(empty["cursor"], 0);
    assert_eq!(empty["has_more"], false);
}

// Catches: tiny raw pages leak fragments that reconstruct a full token,
// including registered values whose byte length changes during masking.
#[test]
fn session_output_raw_tiny_pages_cannot_reconstruct_secrets() {
    let state = test_state();
    let sid = "paged-secret";
    let registered = "private-value日本語";
    let form = crate::secrets::Form::request(
        vec![crate::secrets::Field {
            name: "TOKEN".into(),
            kind: crate::secrets::FieldKind::Password,
            display: None,
        }],
        "test".into(),
    )
    .unwrap();
    let opened = state.secrets.open(form).unwrap();
    state
        .secrets
        .submit(
            &opened.nonce,
            serde_json::from_value(serde_json::json!({
                "nonce":opened.nonce, "status":"stored", "values":{"TOKEN":registered}
            }))
            .unwrap(),
        )
        .unwrap();
    let raw = format!("before {WRAP_SECRET} after {registered} end");
    let mut ring = crate::OutputRingBuffer::new(4096);
    ring.write(raw.as_bytes());
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), parking_lot::Mutex::new(ring));
    let mut actual = String::new();
    let mut position = 0;
    loop {
        let page = session_output(
            &state,
            &serde_json::json!({
                "action":"output", "session_id":sid, "format":"raw", "from_byte":position, "limit":1
            }),
        );
        actual.push_str(page["data"].as_str().unwrap());
        let next = page["cursor"].as_u64().unwrap();
        assert!(next > position);
        if page["has_more"] == false {
            break;
        }
        position = next;
    }
    assert_eq!(
        actual,
        format!(
            "before {} after {} end",
            "*".repeat(WRAP_SECRET.len()),
            "*".repeat(registered.len())
        )
    );
}

// Catches: lossy decoding expands malformed PTY bytes and makes the next
// source-byte cursor index into a different position or panic.
#[test]
fn session_output_raw_pages_keep_source_offsets_for_invalid_utf8() {
    let state = test_state();
    let sid = "paged-invalid";
    let bytes = b"a\xff\xc3\xa9\xfez";
    let mut ring = crate::OutputRingBuffer::new(64);
    ring.write(bytes);
    state
        .session_maps
        .output_buffers
        .insert(sid.into(), parking_lot::Mutex::new(ring));
    let mut output = String::new();
    let mut cursor = 0;
    loop {
        let page = session_output(
            &state,
            &serde_json::json!({
                "action":"output", "session_id":sid, "format":"raw", "from_byte":cursor, "limit":1
            }),
        );
        output.push_str(page["data"].as_str().unwrap());
        let next = page["cursor"].as_u64().unwrap();
        assert!(next > cursor);
        cursor = next;
        if page["has_more"] == false {
            break;
        }
    }
    assert_eq!(cursor, bytes.len() as u64);
    assert_eq!(output, String::from_utf8_lossy(bytes));
}

/// `session output` response includes `cursor` field (== total VtLog lines)
/// and `total_written` remains present for backwards compat.
#[test]
fn session_output_includes_cursor_field() {
    use crate::OutputRingBuffer;
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = test_state();
    let sid = "cursor-field-test".to_string();

    let mut ring = OutputRingBuffer::new(4096);
    ring.write(b"line one\n");
    state
        .session_maps
        .output_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(ring));

    let mut vt = VtLogBuffer::new(24, 80, 200);
    // Feed >24 lines so some scroll into log (total_pushed > 0).
    for i in 0..30 {
        vt.process(format!("line {i}\r\n").as_bytes());
    }
    state
        .grid
        .vt_log_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(vt));

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    state
        .session_maps
        .last_output_ms
        .insert(sid.clone(), AtomicU64::new(now_ms));
    state.session_maps.exit_codes.insert(sid.clone(), 0);

    let res = handle_session(
        &state,
        &serde_json::json!({"action": "output", "session_id": sid}),
        None,
    );
    assert!(res.get("error").is_none(), "Unexpected error: {res}");
    assert!(res.get("cursor").is_some(), "cursor field missing: {res}");
    assert!(
        res.get("total_written").is_some(),
        "total_written missing (backwards compat): {res}"
    );
    let cursor = res["cursor"].as_u64().expect("cursor must be u64");
    assert!(cursor > 0, "cursor should be > 0 after scrollback: {res}");
    assert_eq!(
        res["cursor"], res["total_written"],
        "cursor and total_written must match"
    );
}

/// `since_cursor` returns only new lines since the given position.
#[test]
fn session_output_since_cursor_returns_delta() {
    use crate::OutputRingBuffer;
    use crate::state::VtLogBuffer;
    use std::sync::atomic::AtomicU64;

    let state = test_state();
    let sid = "since-cursor-test".to_string();

    state.session_maps.output_buffers.insert(
        sid.clone(),
        parking_lot::Mutex::new(OutputRingBuffer::new(4096)),
    );

    let mut vt = VtLogBuffer::new(24, 80, 200);
    // Feed >24 lines so total_pushed > 0.
    for i in 0..30 {
        vt.process(format!("old line {i}\r\n").as_bytes());
    }
    let cursor_after_old = vt.total_lines();
    assert!(cursor_after_old > 0, "scrollback must have lines");

    // Feed >24 new lines so they overflow the viewport into scrollback.
    for i in 0..30 {
        vt.process(format!("new line {i}\r\n").as_bytes());
    }
    state
        .grid
        .vt_log_buffers
        .insert(sid.clone(), parking_lot::Mutex::new(vt));

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    state
        .session_maps
        .last_output_ms
        .insert(sid.clone(), AtomicU64::new(now_ms));
    state.session_maps.exit_codes.insert(sid.clone(), 0);

    let res = handle_session(
        &state,
        &serde_json::json!({"action": "output", "session_id": sid, "since_cursor": cursor_after_old}),
        None,
    );
    assert!(res.get("error").is_none(), "Unexpected error: {res}");
    let data = res["data"].as_str().expect("data field");
    // Delta includes lines scrolled in since cursor — includes new lines.
    assert!(
        data.contains("new line"),
        "expected new lines in delta: {res}"
    );
    let new_cursor = res["cursor"].as_u64().expect("cursor must be u64");
    assert!(
        new_cursor > cursor_after_old as u64,
        "cursor must advance: {res}"
    );
}

/// A session with no trace (never existed or fully reaped) must return a
/// structured error with `reason: session_not_found_or_reaped` — not the
/// bare "Session not found" the pre-fix code returned.
#[test]
fn unknown_session_id_returns_structured_error() {
    let state = test_state();

    let res = handle_session(
        &state,
        &serde_json::json!({"action": "output", "session_id": "does-not-exist-at-all"}),
        None,
    );

    assert_eq!(
        res["error"].as_str(),
        Some("Session not found"),
        "Should surface error: {res}"
    );
    assert_eq!(
        res["reason"].as_str(),
        Some("session_not_found_or_reaped"),
        "Unknown session should report session_not_found_or_reaped: {res}"
    );
}

// --- build_spawn_prompt ---

#[test]
fn build_spawn_prompt_no_parent_returns_original() {
    let result = build_spawn_prompt("do the task", None, "child-123", "worker");
    assert_eq!(result, "do the task");
}

#[test]
fn build_spawn_prompt_with_parent_prepends_preamble() {
    let result = build_spawn_prompt("do the task", Some("parent-456"), "child-123", "worker");
    assert!(
        result.contains("parent-456"),
        "preamble must mention parent"
    );
    assert!(
        result.contains("do the task"),
        "original prompt must be preserved"
    );
    let preamble_end = result.find("do the task").unwrap();
    assert!(preamble_end > 0, "preamble must precede prompt");
    assert!(
        result.contains("register"),
        "preamble must include the reconnect registration fallback"
    );
    assert!(result.contains("pre-registered as peer `worker`"));
}

#[test]
fn build_spawn_prompt_with_parent_includes_send_instruction() {
    let result = build_spawn_prompt("my task", Some("orch-789"), "child-abc", "worker");
    assert!(
        result.contains("orch-789"),
        "preamble must include parent session for send target"
    );
    assert!(
        result.contains("send"),
        "preamble must instruct send on completion"
    );
}

// --- spawn auto-registration + inbox pre-init ---

#[cfg(unix)]
#[tokio::test]
async fn spawn_auto_registers_child_in_peer_list() {
    let state = test_state();
    let addr = "127.0.0.1:0".parse().unwrap();
    register_peer(
        &state,
        "550e8400-e29b-41d4-a716-446655440b01",
        "orchestrator",
        "mcp-orch",
    );

    let result = handle_agent(
        &state,
        addr,
        &serde_json::json!({
            "action": "spawn",
            "prompt": "hello",
            "binary_path": LONG_LIVED_TEST_BINARY,
            "cwd": TEST_SPAWN_CWD,
        }),
        Some("mcp-orch"),
    );
    // Skip if PTY cannot be opened (sandbox/CI without /dev/ptmx access)
    if result
        .get("error")
        .and_then(|e| e.as_str())
        .is_some_and(|e| e.contains("Failed to open PTY"))
    {
        eprintln!("Skipping: PTY not available in this environment");
        return;
    }
    assert!(result.get("error").is_none(), "spawn failed: {result}");
    let session_id = result["session_id"].as_str().unwrap();

    let peers = handle_messaging(
        &state,
        &serde_json::json!({"action": "list_peers"}),
        Some("mcp-orch"),
    );
    let sessions: Vec<&str> = peers["peers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["tuic_session"].as_str().unwrap())
        .collect();
    assert!(
        sessions.contains(&session_id),
        "child {session_id} not in list_peers: {sessions:?}"
    );

    handle_session(
        &state,
        &serde_json::json!({"action": "kill", "session_id": session_id}),
        None,
    );
}

#[cfg(unix)]
#[tokio::test]
async fn spawn_name_is_applied_to_peer_session_and_response() {
    let state = test_state();
    let addr = "127.0.0.1:0".parse().unwrap();
    register_peer(
        &state,
        "550e8400-e29b-41d4-a716-446655440b01",
        "orchestrator",
        "mcp-orch",
    );
    let mut events = state.event_bus.subscribe();

    let result = handle_agent(
        &state,
        addr,
        &serde_json::json!({
            "action": "spawn",
            "name": "linux-primary",
            "prompt": "hello",
            "binary_path": LONG_LIVED_TEST_BINARY,
            "cwd": TEST_SPAWN_CWD,
        }),
        Some("mcp-orch"),
    );
    if result
        .get("error")
        .and_then(|e| e.as_str())
        .is_some_and(|e| e.contains("Failed to open PTY"))
    {
        eprintln!("Skipping: PTY not available in this environment");
        return;
    }

    assert!(result.get("error").is_none(), "spawn failed: {result}");
    assert_eq!(result["name"], "linux-primary");
    let session_id = result["session_id"].as_str().unwrap();
    // Catches: MCP direct spawn loses its root role, allowing shell-return
    // revocation to misclassify the agent's own root process.
    assert_eq!(
        state
            .session_maps
            .session_states
            .get(session_id)
            .unwrap()
            .spawn_root_role,
        crate::state::SpawnRootRole::DirectProgram
    );
    assert!(
        state
            .session_maps
            .sessions
            .get(session_id)
            .unwrap()
            .lock()
            ._child
            .process_id()
            .is_some()
    );
    let created = events
        .try_recv()
        .expect("named spawn must emit session-created");
    match created {
        crate::state::AppEvent::SessionCreated {
            session_id: event_session_id,
            display_name,
            parent_session,
            ..
        } => {
            assert_eq!(event_session_id, session_id);
            assert_eq!(display_name.as_deref(), Some("linux-primary"));
            // The UI tags the tab with its parent from this event alone; the
            // parent map is filled later and is never pushed to the frontend.
            assert_eq!(
                parent_session.as_deref(),
                Some("550e8400-e29b-41d4-a716-446655440b01")
            );
        }
        other => panic!("expected session-created, got {other:?}"),
    }
    assert_eq!(
        result["parent_session_id"],
        "550e8400-e29b-41d4-a716-446655440b01"
    );
    for dropped in ["peer_registered", "communication_ready", "send_to"] {
        assert!(
            result.get(dropped).is_none(),
            "{dropped} was constant or a second copy of session_id: {result}"
        );
    }
    assert_eq!(
        state.peer_agents.get(session_id).unwrap().name,
        "linux-primary"
    );
    assert_eq!(
        state
            .session_maps
            .sessions
            .get(session_id)
            .unwrap()
            .lock()
            .display_name
            .as_deref(),
        Some("linux-primary")
    );

    // A WebView reload rebuilds the tab from the session row alone: the row
    // must say the name came from the spawn (so the agent's OSC title cannot
    // replace it) and which agent spawned it (so the sub-agent tag survives).
    let row = super::super::session::local_session_rows(&state)
        .into_iter()
        .find(|row| row.session_id == session_id)
        .expect("the spawned session is listed");
    assert!(row.display_name_from_spawn, "{row:?}");
    assert_eq!(
        row.parent_session.as_deref(),
        Some("550e8400-e29b-41d4-a716-446655440b01")
    );

    let listed = handle_session(&state, &serde_json::json!({"action": "list"}), None);
    let listed_session = listed
        .as_array()
        .and_then(|sessions| {
            sessions
                .iter()
                .find(|session| session["session_id"] == session_id)
        })
        .expect("named spawned session must appear in session action=list");
    assert_eq!(listed_session["display_name"], "linux-primary");
    assert_eq!(listed_session["is_caller"], false);
    assert!(
        listed_session["alias"].as_str().is_some(),
        "independent short alias must remain available"
    );
    assert_ne!(listed_session["alias"], listed_session["display_name"]);

    apply_initialize_identity(&state, "child-mcp", Some(session_id));
    assert_eq!(
        state.peer_agents.get(session_id).unwrap().name,
        "linux-primary",
        "child auto-bind must preserve the parent-assigned name"
    );
    let caller_list = handle_session(
        &state,
        &serde_json::json!({"action": "list"}),
        Some("child-mcp"),
    );
    let caller = caller_list
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["session_id"] == session_id)
        .unwrap();
    assert_eq!(
        caller["is_caller"], true,
        "the caller's managed PTY must be explicit so it is never self-closed"
    );

    handle_session(
        &state,
        &serde_json::json!({"action": "kill", "session_id": session_id}),
        None,
    );
}

#[test]
fn spawn_rejects_empty_name_before_opening_pty() {
    let state = test_state();
    let result = handle_agent(
        &state,
        "127.0.0.1:0".parse().unwrap(),
        &serde_json::json!({
            "action": "spawn",
            "name": "   ",
            "prompt": "hello",
            "binary_path": SHORT_LIVED_TEST_BINARY,
        }),
        None,
    );

    assert_eq!(
        result["error"],
        "Action 'spawn' requires 'name' to be a non-empty string when provided"
    );
    assert!(state.session_maps.sessions.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn spawn_pre_initializes_child_inbox() {
    let state = test_state();
    let addr = "127.0.0.1:0".parse().unwrap();
    register_peer(
        &state,
        "550e8400-e29b-41d4-a716-446655440b01",
        "orchestrator",
        "mcp-orch",
    );

    let result = handle_agent(
        &state,
        addr,
        &serde_json::json!({
            "action": "spawn",
            "prompt": "hello",
            "binary_path": LONG_LIVED_TEST_BINARY,
            "cwd": TEST_SPAWN_CWD,
        }),
        Some("mcp-orch"),
    );
    // Skip if PTY cannot be opened (sandbox/CI without /dev/ptmx access)
    if result
        .get("error")
        .and_then(|e| e.as_str())
        .is_some_and(|e| e.contains("Failed to open PTY"))
    {
        eprintln!("Skipping: PTY not available in this environment");
        return;
    }
    assert!(result.get("error").is_none(), "spawn failed: {result}");
    let session_id = result["session_id"].as_str().unwrap();

    assert!(
        state.agent_inbox.contains_key(session_id),
        "child inbox must be pre-initialized after spawn"
    );

    handle_session(
        &state,
        &serde_json::json!({"action": "kill", "session_id": session_id}),
        None,
    );
}

#[cfg(unix)]
#[tokio::test]
async fn unregistered_headerless_spawn_has_no_parent() {
    // A registered headerless caller is a parent (see
    // parent_registration_after_spawn_links_existing_child); only a caller
    // that never registered has no identity for the child to answer.
    let state = test_state();
    let addr = "127.0.0.1:0".parse().unwrap();
    let result = handle_agent(
        &state,
        addr,
        &serde_json::json!({
            "action": "spawn",
            "prompt": "hello",
            "binary_path": SHORT_LIVED_TEST_BINARY,
            "cwd": TEST_SPAWN_CWD,
        }),
        Some("mcp-anon"),
    );
    // Skip if PTY cannot be opened (sandbox/CI without /dev/ptmx access)
    if result
        .get("error")
        .and_then(|e| e.as_str())
        .is_some_and(|e| e.contains("Failed to open PTY"))
    {
        eprintln!("Skipping: PTY not available in this environment");
        return;
    }
    assert!(
        result.get("error").is_none(),
        "unregistered caller spawn must succeed: {result}"
    );
    assert!(result["session_id"].as_str().is_some());
    assert!(
        result.get("parent_session_id").is_none(),
        "an unregistered caller has no identity for the child to answer"
    );
    assert!(
        result["communication_warning"]
            .as_str()
            .is_some_and(|warning| warning.contains("Child has no parent")),
        "the spawn response must state the child has no parent: {result}"
    );
}

// ---- Layer 2: session(status) enrichment + spawn response (#1163-7599) ----

#[test]
fn session_status_unknown_session_returns_structured_error() {
    let state = test_state();
    let result = handle_session(
        &state,
        &serde_json::json!({"action": "status", "session_id": "nonexistent"}),
        None,
    );
    let err = result["error"].as_str().unwrap_or("");
    assert!(
        err.contains("not found"),
        "expected 'not found' error, got: {result}"
    );
}

#[test]
fn session_status_omits_absent_optional_fields() {
    let state = test_state();
    let session_id = "status-compact";
    state.session_maps.session_states.insert(
        session_id.to_string(),
        crate::state::SessionState::default(),
    );

    let response = handle_session(
        &state,
        &serde_json::json!({"action": "status", "session_id": session_id}),
        None,
    );
    let object = response.as_object().unwrap();
    for absent in [
        "shell_state",
        "agent_state",
        "agent_type",
        "exit_code",
        "idle_since_ms",
        "busy_duration_ms",
    ] {
        assert!(!object.contains_key(absent), "{absent} must be omitted");
    }
    assert!(object.contains_key("background_work"));
    assert!(object.contains_key("awaiting_input"));
}

#[test]
fn session_status_includes_exit_code_when_exited() {
    let state = test_state();
    let sid = "s-exit-test";
    state
        .session_maps
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(crate::pty::SHELL_IDLE),
    );
    state.session_maps.exit_codes.insert(sid.to_string(), 42);

    let result = handle_session(
        &state,
        &serde_json::json!({"action": "status", "session_id": sid}),
        None,
    );
    assert!(result.get("error").is_none(), "unexpected error: {result}");
    assert_eq!(
        result["exit_code"],
        serde_json::json!(42),
        "exit_code missing: {result}"
    );
}

#[test]
fn session_status_includes_idle_since_ms_when_idle() {
    let state = test_state();
    let sid = "s-idle-test";
    state
        .session_maps
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(crate::pty::SHELL_IDLE),
    );
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
        - 500;
    state
        .session_maps
        .shell_state_since_ms
        .insert(sid.to_string(), std::sync::atomic::AtomicU64::new(since));

    let result = handle_session(
        &state,
        &serde_json::json!({"action": "status", "session_id": sid}),
        None,
    );
    assert!(result.get("error").is_none(), "unexpected error: {result}");
    let idle_ms = result["idle_since_ms"].as_u64();
    assert!(
        idle_ms.is_some(),
        "idle_since_ms must be present when idle: {result}"
    );
    assert!(
        idle_ms.unwrap() >= 400,
        "idle_since_ms must reflect elapsed time: {result}"
    );
    assert!(
        result["busy_duration_ms"].is_null(),
        "busy_duration_ms must be absent when idle: {result}"
    );
}

#[test]
fn session_status_includes_busy_duration_ms_when_busy() {
    let state = test_state();
    let sid = "s-busy-test";
    state
        .session_maps
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(crate::pty::SHELL_BUSY),
    );
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
        - 300;
    state
        .session_maps
        .shell_state_since_ms
        .insert(sid.to_string(), std::sync::atomic::AtomicU64::new(since));

    let result = handle_session(
        &state,
        &serde_json::json!({"action": "status", "session_id": sid}),
        None,
    );
    assert!(result.get("error").is_none(), "unexpected error: {result}");
    let busy_ms = result["busy_duration_ms"].as_u64();
    assert!(
        busy_ms.is_some(),
        "busy_duration_ms must be present when busy: {result}"
    );
    assert!(
        busy_ms.unwrap() >= 200,
        "busy_duration_ms must reflect elapsed time: {result}"
    );
    assert!(
        result["idle_since_ms"].is_null(),
        "idle_since_ms must be absent when busy: {result}"
    );
}

#[test]
fn session_list_includes_shell_state_per_entry() {
    let state = test_state();
    // Without real PTY sessions we can't test list output (sessions DashMap requires live PTY).
    // This test verifies the field would appear if a session entry exists.
    // Integration coverage via manual QA — list with running session must show shell_state.
    // Here we just verify the status handler path we control returns shell_state.
    let sid = "s-list-test";
    state
        .session_maps
        .session_states
        .insert(sid.to_string(), crate::state::SessionState::default());
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(crate::pty::SHELL_IDLE),
    );

    let result = handle_session(
        &state,
        &serde_json::json!({"action": "status", "session_id": sid}),
        None,
    );
    assert!(
        result["shell_state"].as_str().is_some(),
        "shell_state must be in status response: {result}"
    );
}

#[test]
fn session_status_distinguishes_declared_completion_from_idle() {
    let state = test_state();
    let sid = "s-completed-test";
    state.session_maps.session_states.insert(
        sid.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            suggested_actions: Some(vec!["Review result".to_string()]),
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        sid.to_string(),
        std::sync::atomic::AtomicU8::new(crate::pty::SHELL_IDLE),
    );

    let result = handle_session(
        &state,
        &serde_json::json!({"action": "status", "session_id": sid}),
        None,
    );

    assert_eq!(result["shell_state"], "idle");
    assert_eq!(
        result["agent_state"], "completed",
        "an explicit suggest marker is task completion, not generic idle: {result}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn spawn_response_includes_enrichment_fields() {
    let state = test_state();
    let addr = "127.0.0.1:0".parse().unwrap();
    let result = handle_agent(
        &state,
        addr,
        &serde_json::json!({
            "action": "spawn",
            "prompt": "hello",
            "binary_path": LONG_LIVED_TEST_BINARY,
            "cwd": TEST_SPAWN_CWD,
        }),
        Some("mcp-orch"),
    );

    if result.get("error").is_some() {
        eprintln!("Skipping: PTY not available in this environment");
        return;
    }

    assert!(
        result["session_id"].as_str().is_some(),
        "session_id missing: {result}"
    );
    assert!(
        result["server_ts"].as_u64().is_some(),
        "server_ts missing: {result}"
    );
    // Call templates restated ids already in the response; they cost ~600
    // chars per spawn and must not come back.
    for template in [
        "monitor_with",
        "status_with",
        "wait_with",
        "peer_monitor_with",
        "peer_wait_with",
    ] {
        assert!(
            result.get(template).is_none(),
            "{template} must not be in the spawn response: {result}"
        );
    }
    assert!(result["communication_warning"].as_str().is_some());

    let session_id = result["session_id"].as_str().unwrap();
    assert!(
        state.peer_agents.contains_key(session_id),
        "every managed child must be registered even when the caller has no peer identity"
    );
    assert!(
        state.agent_inbox.contains_key(session_id),
        "every managed child must have an inbox immediately after spawn"
    );

    handle_session(
        &state,
        &serde_json::json!({"action": "kill", "session_id": session_id}),
        None,
    );
}

#[cfg(unix)]
#[tokio::test]
async fn parent_registration_after_spawn_links_existing_child() {
    let state = test_state();
    let addr = "127.0.0.1:0".parse().unwrap();
    let parent_mcp = "mcp-late-parent";
    let mut events = state.event_bus.subscribe();

    let spawned = handle_agent(
        &state,
        addr,
        &serde_json::json!({
            "action": "spawn",
            "prompt": "hello",
            "binary_path": LONG_LIVED_TEST_BINARY,
            "cwd": TEST_SPAWN_CWD,
        }),
        Some(parent_mcp),
    );
    // THE single PTY-availability decision for this test. Everything past it
    // runs, or nothing does — see the release before the second spawn for why
    // that is now a safe thing to promise. Carry the underlying error rather
    // than a fixed sentence: "PTY not available" reads identically on a machine
    // with no PTY support and on one that merely ran out of descriptors, and
    // that ambiguity is what made the original flake so hard to place.
    if let Some(error) = spawned.get("error") {
        eprintln!("Skipping: cannot open a PTY here — {error}");
        return;
    }
    let child = spawned["session_id"].as_str().unwrap();
    assert!(
        spawned.get("parent_session_id").is_none(),
        "an unregistered parent has no identity for the child to answer: {spawned}"
    );
    // The placeholder is a routing key, not a session any tab can match: sent
    // to the UI it pinned the tab's tag to "sub" and nothing ever corrected it.
    let parent_row = |state: &AppState| {
        super::super::session::local_session_rows(state)
            .into_iter()
            .find(|row| row.session_id == child)
            .expect("the spawned child is listed")
            .parent_session
    };
    let created = std::iter::from_fn(|| events.try_recv().ok())
        .find_map(|event| match event {
            crate::state::AppEvent::SessionCreated {
                session_id,
                parent_session,
                ..
            } if session_id == child => Some(parent_session),
            _ => None,
        })
        .expect("the spawn publishes session-created");
    assert_eq!(created, None, "a pending placeholder must not be published");
    assert_eq!(parent_row(&state), None);
    let pending_parent = pending_parent_id(parent_mcp);
    state.push_agent_inbox(
        &pending_parent,
        crate::state::AgentMessage {
            id: "tuic-auto-before-parent-registration".to_string(),
            from_tuic_session: child.to_string(),
            from_name: "tuic".to_string(),
            content: r#"{"type":"state_change","state":"idle"}"#.to_string(),
            timestamp: 1,
            delivered_via_channel: false,
        },
    );

    state.mcp.sessions.insert(
        parent_mcp.to_string(),
        crate::state::McpSessionMeta {
            prompt_instructions: None,
            last_activity: std::time::Instant::now(),
            is_claude_code: false,
            requires_meta_tools: false,
            has_sse_stream: false,
            sse_generation: 0,
            repo_path: None,
        },
    );
    let registered = handle_messaging(
        &state,
        &serde_json::json!({
            "action": "register",
            "name": "orchestrator",
        }),
        Some(parent_mcp),
    );

    assert_eq!(registered["ok"], true, "registration failed: {registered}");
    assert_eq!(registered["identity_generated"], true);
    let parent_tuic = registered["tuic_session"].as_str().unwrap();
    assert!(is_valid_uuid(parent_tuic));
    assert_eq!(registered["linked_children"], 1);
    assert_eq!(
        parent_row(&state).as_deref(),
        Some(parent_tuic),
        "once the parent registers, the row names it for the next reload"
    );
    assert_eq!(
        state
            .session_maps
            .session_parent
            .get(child)
            .map(|entry| entry.value().clone()),
        Some(parent_tuic.to_string()),
        "late parent registration must restore child lifecycle/message routing"
    );
    assert_eq!(
        state
            .agent_inbox
            .get(parent_tuic)
            .map(|messages| messages.len()),
        Some(1),
        "lifecycle mail emitted before parent registration must be preserved"
    );

    // Release the first child BEFORE spawning the second. Its master fd, its
    // cloned reader fd and its writer are three descriptors this test has no
    // further use for — every assertion about `child` is already above — and
    // holding them made the second spawn strictly harder than the first.
    //
    // That asymmetry was the bug, and it is three descriptors wide. Measured
    // by bisecting `ulimit -n` against this binary: this test fully passed at
    // n>=19, HALF-RAN at n=18/17/16 (first spawn fine, second dying on
    // `Failed to spawn shell: Too many open files (os error 24)`, then
    // `dup of fd 13 failed`), and skipped cleanly at n<=15. The single-spawn
    // `spawn_response_includes_enrichment_fields` passed all the way down to
    // n=16 and skipped at n<=15. Same floor, three fds of daylight — the
    // pressure was self-inflicted, not ambient, so it is fixed by giving the
    // descriptors back instead of by tolerating the failure.
    handle_session(
        &state,
        &serde_json::json!({"action": "kill", "session_id": child}),
        None,
    );

    let ready_child = handle_agent(
        &state,
        addr,
        &serde_json::json!({
            "action": "spawn",
            "prompt": "report with agent send",
            "binary_path": LONG_LIVED_TEST_BINARY,
            "cwd": TEST_SPAWN_CWD,
        }),
        Some(parent_mcp),
    );
    // Hard assert, deliberately: PTY availability was decided at the top and
    // the descriptors the first child held are back, so this spawn has the
    // headroom the first one had. Skipping here instead would silently drop
    // the three assertions below, which are the subject of the test.
    assert!(
        ready_child.get("error").is_none(),
        "registered external parent spawn failed: {ready_child}"
    );
    assert_eq!(ready_child["parent_session_id"], parent_tuic);
    let ready_child_id = ready_child["session_id"].as_str().unwrap();
    assert!(state.agent_inbox.contains_key(ready_child_id));

    handle_session(
        &state,
        &serde_json::json!({"action": "kill", "session_id": ready_child_id}),
        None,
    );
}

#[cfg(unix)]
#[tokio::test]
async fn spawn_response_reports_parent_for_registered_caller() {
    // A registered caller learns that child-to-parent mail works from
    // parent_session_id alone; how to wait for it is in the tool description.
    let state = test_state();
    let addr = "127.0.0.1:0".parse().unwrap();
    let tuic = "550e8400-e29b-41d4-a716-446655440aa2";
    let mcp = "mcp-arch1-orch";
    register_peer(&state, tuic, "orchestrator", mcp);

    let result = handle_agent(
        &state,
        addr,
        &serde_json::json!({
            "action": "spawn",
            "prompt": "hello",
            "binary_path": SHORT_LIVED_TEST_BINARY,
            "cwd": TEST_SPAWN_CWD,
        }),
        Some(mcp),
    );
    if result.get("error").is_some() {
        eprintln!("Skipping: PTY not available in this environment");
        return;
    }
    assert_eq!(result["parent_session_id"], tuic, "{result}");
    assert!(result.get("communication_warning").is_none(), "{result}");
    assert!(result.get("peer_monitor_with").is_none(), "{result}");
    assert!(result.get("peer_wait_with").is_none(), "{result}");
}

// ── session(kill) self-kill guard ────────────────────────────────────────

#[test]
fn session_kill_rejects_own_session() {
    let state = test_state();
    let mcp_sid = "mcp-kill-guard-test";
    let tuic_sid = "550e8400-e29b-41d4-a716-446655440001";
    state
        .mcp
        .to_session
        .insert(mcp_sid.to_string(), tuic_sid.to_string());

    let result = handle_session(
        &state,
        &serde_json::json!({"action": "kill", "session_id": tuic_sid}),
        Some(mcp_sid),
    );
    assert!(
        result["error"].as_str().is_some(),
        "kill own session must return error: {result}"
    );
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("Cannot kill own session"),
        "error message must mention 'Cannot kill own session': {result}"
    );
}

#[test]
fn session_close_rejects_own_session() {
    // Mirror of the kill guard for `close`. With Story 074 auto-identity the
    // caller is in mcp_to_session even without an explicit register, so this
    // guard now fires for the common orchestrator-closes-itself mistake.
    let state = test_state();
    let mcp_sid = "mcp-close-guard-test";
    let tuic_sid = "550e8400-e29b-41d4-a716-446655440009";
    state
        .mcp
        .to_session
        .insert(mcp_sid.to_string(), tuic_sid.to_string());

    let result = handle_session(
        &state,
        &serde_json::json!({"action": "close", "session_id": tuic_sid}),
        Some(mcp_sid),
    );
    assert!(
        result["error"]
            .as_str()
            .map(|e| e.contains("Cannot close own session"))
            .unwrap_or(false),
        "close own session must be rejected with a hint: {result}"
    );
}

#[test]
fn session_kill_allows_other_session() {
    let state = test_state();
    let mcp_sid = "mcp-kill-other-test";
    let own_tuic = "550e8400-e29b-41d4-a716-446655440002";
    let other_tuic = "550e8400-e29b-41d4-a716-446655440003";
    state
        .mcp
        .to_session
        .insert(mcp_sid.to_string(), own_tuic.to_string());

    // Killing a different session — should NOT be blocked by self-kill guard.
    // It will return "Session not found" (no real PTY), not the self-kill error.
    let result = handle_session(
        &state,
        &serde_json::json!({"action": "kill", "session_id": other_tuic}),
        Some(mcp_sid),
    );
    let err = result["error"].as_str().unwrap_or("");
    assert!(
        !err.contains("Cannot kill own session"),
        "self-kill guard must NOT block killing other sessions: {result}"
    );
}

#[tokio::test]
async fn agent_send_includes_managed_recipient_shell_and_agent_state_only() {
    let state = test_state();
    #[cfg(unix)]
    let stable_shell = "/bin/cat";
    #[cfg(windows)]
    let stable_shell = "cmd.exe";
    let created = handle_session(
        &state,
        &serde_json::json!({"action": "create", "shell": stable_shell}),
        None,
    );
    if created.get("error").is_some() {
        eprintln!("Skipping: PTY not available in this environment");
        return;
    }
    let recipient = created["session_id"].as_str().unwrap();
    state.session_maps.session_states.insert(
        recipient.to_string(),
        crate::state::SessionState {
            agent_type: Some("codex".to_string()),
            awaiting_input: true,
            ..Default::default()
        },
    );
    state.session_maps.shell_states.insert(
        recipient.to_string(),
        std::sync::atomic::AtomicU8::new(crate::pty::SHELL_BUSY),
    );
    register_peer(&state, TEST_UUID_A, "sender", "mcp-managed-sender");
    register_peer(&state, recipient, "recipient", "mcp-managed-recipient");

    let response = handle_messaging(
        &state,
        &serde_json::json!({
            "action": "send",
            "to": recipient,
            "message": "state summary",
        }),
        Some("mcp-managed-sender"),
    );

    let recipient_state = response["recipient_state"].as_object().unwrap();
    assert_eq!(recipient_state.len(), 2);
    assert!(recipient_state["shell_state"].as_str().is_some());
    assert_eq!(recipient_state["agent_state"], "awaiting_input");

    let _ = handle_session(
        &state,
        &serde_json::json!({"action": "kill", "session_id": recipient}),
        None,
    );
}

// -----------------------------------------------------------------------
// resolve_run_config tests
// -----------------------------------------------------------------------

/// A direct Codex child must trust only its launch cwd. Its normal config
/// stays untouched; the fake executable records argv as the external oracle.
#[cfg(unix)]
#[tokio::test]
async fn managed_codex_spawn_trusts_its_new_cwd_without_writing_user_config() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::Builder::new()
        .prefix("managed-codex-trust-")
        .tempdir_in(crate::test_support::test_temp_root())
        .unwrap();
    let cwd = root.path().join("never-trusted");
    std::fs::create_dir(&cwd).unwrap();
    let script = root.path().join("codex");
    let argv = root.path().join("argv");
    let submitted = root.path().join("submitted");
    std::fs::write(&script, format!("#!/bin/sh\nif [ \"$1\" = --version ]; then exit 0; fi\nprintf '%s\\n' \"$@\" > '{}'\nstty -echo\nprintf 'Starting Codex\\n'\nsleep 0.1\nprintf '› \\n'\nIFS= read -r text\nprintf '%s' \"$text\" > '{}'\nexec cat >/dev/null\n", argv.display(), submitted.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        std::process::Command::new(&script)
            .arg("--version")
            .status()
            .unwrap()
            .success()
    );
    let _config = crate::config::set_config_dir_override(root.path().join("tuic-config"));
    let state = test_state();
    // The production boot runs this reconciler before a Codex ready screen can release BUSY.
    crate::pty::spawn_process_snapshot_refresher(state.clone());
    let spawned = handle_agent(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        &serde_json::json!({
            "action": "spawn", "agent_type": "codex", "binary_path": script,
            "cwd": cwd, "prompt": "say READY",
            "args": ["--no-alt-screen"],
        }),
        None,
    );
    assert!(spawned.get("error").is_none(), "spawn failed: {spawned}");
    let sid = spawned["session_id"].as_str().unwrap();
    let row = super::super::session::local_session_rows(&state)
        .into_iter()
        .find(|row| row.session_id == sid)
        .unwrap();
    assert_eq!(row.tuic_session.as_deref(), Some(sid));
    // `submitted` is written only by the invocation that reaches the
    // trust-confirmed branch, after `argv` in the same script run — wait
    // for it first so `argv` cannot still hold an earlier probe's argv.
    let prompt = wait_for_file_content_async(&submitted, std::time::Duration::from_secs(15)).await;
    let actual = wait_for_file_content_async(&argv, std::time::Duration::from_secs(15)).await;
    let output = handle_session(
        &state,
        &serde_json::json!({"action":"output", "session_id":sid, "limit":50}),
        None,
    );
    let queued = state.pending_initial_prompts.contains_key(sid);
    let shell = state
        .session_maps
        .shell_states
        .get(sid)
        .map(|value| value.load(std::sync::atomic::Ordering::Relaxed));
    let idle_confirmed = state
        .session_maps
        .silence_states
        .get(sid)
        .map(|value| value.lock().idle_confirmed());
    let blocked = crate::pty::blocked_on_confident_question(&state, sid);
    handle_session(
        &state,
        &serde_json::json!({"action":"kill", "session_id":sid}),
        None,
    );
    assert!(!actual.is_empty(), "agent must record launch argv");
    let expected = format!(
        "projects={{{}={{trust_level=\"trusted\"}}}}",
        serde_json::to_string(&cwd.to_string_lossy()).unwrap()
    );
    assert!(
        actual
            .lines()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|pair| pair == ["-c", expected.as_str()]),
        "launch must scope trust to the new cwd: {actual}"
    );
    // Catches: a managed Codex child blocked on the Update now / Skip prompt
    // (1373).
    assert!(
        actual
            .lines()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|pair| pair == ["-c", "check_for_update_on_startup=false"]),
        "managed Codex child must skip the update check: {actual}"
    );
    assert!(
        prompt.contains("say READY"),
        "spawn prompt must be submitted: prompt={prompt:?}; queued={queued}; shell={shell:?}; idle_confirmed={idle_confirmed:?}; blocked={blocked}; output={output}"
    );
    assert!(!root.path().join("codex-config/config.toml").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn managed_codex_trust_opt_out_preserves_normal_launch() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::Builder::new()
        .prefix("managed-codex-opt-out-")
        .tempdir_in(crate::test_support::test_temp_root())
        .unwrap();
    let cwd = root.path().join("never-trusted");
    std::fs::create_dir(&cwd).unwrap();
    let script = root.path().join("codex");
    let argv = root.path().join("argv");
    std::fs::write(&script, format!("#!/bin/sh\nif [ \"$1\" = --version ]; then exit 0; fi\nprintf '%s\\n' \"$@\" > '{}'\nexec cat >/dev/null\n", argv.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        std::process::Command::new(&script)
            .arg("--version")
            .status()
            .unwrap()
            .success()
    );
    let _config = crate::config::set_config_dir_override(root.path().join("tuic-config"));
    let mut agents = crate::config::AgentsConfig::default();
    agents.agents.insert(
        "codex".into(),
        crate::config::AgentSettings {
            skip_trust_dialog: Some(false),
            ..Default::default()
        },
    );
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), agents).unwrap();
    let state = test_state();
    let spawned = handle_agent(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        &serde_json::json!({"action":"spawn", "agent_type":"codex", "binary_path":script, "cwd":cwd, "prompt":"say READY"}),
        None,
    );
    assert!(spawned.get("error").is_none(), "spawn failed: {spawned}");
    let sid = spawned["session_id"].as_str().unwrap();
    let actual = wait_for_file_content_async(&argv, std::time::Duration::from_secs(5)).await;
    handle_session(
        &state,
        &serde_json::json!({"action":"kill", "session_id":sid}),
        None,
    );
    assert!(!actual.is_empty(), "agent must record launch argv");
    assert!(
        !actual.contains("trust_level"),
        "opt-out must preserve Codex trust behavior: {actual}"
    );
}

/// A configured Codex launcher must pass the launch-only trust setting to
/// the child it starts. The child records its own argv, not TUIC's builder.
#[cfg(unix)]
#[tokio::test]
async fn managed_codex_wrapper_trusts_only_its_spawn_cwd_and_submits_prompt() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::Builder::new()
        .prefix("managed-codex-wrapper-")
        .tempdir_in(crate::test_support::test_temp_root())
        .unwrap();
    let cwd = root.path().join("never-trusted");
    std::fs::create_dir(&cwd).unwrap();
    let other = root.path().join("other-project");
    std::fs::create_dir(&other).unwrap();
    let wrapper = root.path().join("custom-launcher");
    let child = root.path().join("codex");
    let argv = root.path().join("child-argv");
    let submitted = root.path().join("submitted");
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then exit 0; fi\nexec '{}' \"$@\"\n",
            child.display()
        ),
    )
    .unwrap();
    std::fs::write(&child, format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\ncase \"$*\" in\n  *'trust_level=\"trusted\"'*) sleep 0.1; printf 'Starting Codex\\n› \\n' ;;\n  *) printf 'Do you trust this directory?\\n'; exec cat >/dev/null ;;\nesac\nstty -echo\nIFS= read -r text\nprintf '%s' \"$text\" > '{}'\nexec cat >/dev/null\n", argv.display(), submitted.display())).unwrap();
    for path in [&wrapper, &child] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let _config = crate::config::set_config_dir_override(root.path().join("tuic-config"));
    let mut agents = crate::config::AgentsConfig::default();
    agents.agents.insert(
        "codex".into(),
        crate::config::AgentSettings {
            run_configs: vec![crate::config::AgentRunConfig {
                name: "Custom Codex".into(),
                command: wrapper.to_string_lossy().into_owned(),
                args: vec![],
                model: None,
                env: Default::default(),
                is_default: true,
            }],
            ..Default::default()
        },
    );
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), agents).unwrap();
    let state = test_state();
    crate::pty::spawn_process_snapshot_refresher(state.clone());
    let spawned = handle_agent(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        &serde_json::json!({"action":"spawn", "agent_type":"Custom Codex", "cwd":cwd, "prompt":"say READY", "args":["--no-alt-screen"]}),
        None,
    );
    assert!(spawned.get("error").is_none(), "spawn failed: {spawned}");
    let sid = spawned["session_id"].as_str().unwrap();
    // `submitted` is written only by the invocation that reaches the
    // trust-confirmed branch, after `argv` in the same script run — wait
    // for it first so `argv` cannot still hold an earlier probe's argv.
    let prompt = wait_for_file_content_async(&submitted, std::time::Duration::from_secs(15)).await;
    let actual = wait_for_file_content_async(&argv, std::time::Duration::from_secs(15)).await;
    handle_session(
        &state,
        &serde_json::json!({"action":"kill", "session_id":sid}),
        None,
    );
    let expected = format!(
        "projects={{{}={{trust_level=\"trusted\"}}}}",
        serde_json::to_string(&cwd.to_string_lossy()).unwrap()
    );
    assert!(
        actual
            .lines()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|pair| pair == ["-c", expected.as_str()]),
        "child must receive cwd-scoped trust: {actual}"
    );
    assert!(
        !actual.contains(&other.to_string_lossy().to_string()),
        "other cwd must not be trusted: {actual}"
    );
    assert!(
        prompt.contains("say READY"),
        "wrapper child must receive initial task: {prompt:?}; argv={actual}"
    );
    assert!(!root.path().join("codex-config/config.toml").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn managed_codex_wrapper_opt_out_leaves_trust_prompt() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::Builder::new()
        .prefix("managed-codex-wrapper-opt-out-")
        .tempdir_in(crate::test_support::test_temp_root())
        .unwrap();
    let cwd = root.path().join("never-trusted");
    std::fs::create_dir(&cwd).unwrap();
    let wrapper = root.path().join("custom-launcher");
    let argv = root.path().join("child-argv");
    std::fs::write(&wrapper, format!("#!/bin/sh\nif [ \"$1\" = --version ]; then exit 0; fi\nprintf '%s\\n' \"$@\" > '{}'\nprintf 'Do you trust this directory?\\n'\nexec cat >/dev/null\n", argv.display())).unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    let _config = crate::config::set_config_dir_override(root.path().join("tuic-config"));
    let mut agents = crate::config::AgentsConfig::default();
    agents.agents.insert(
        "codex".into(),
        crate::config::AgentSettings {
            skip_trust_dialog: Some(false),
            run_configs: vec![crate::config::AgentRunConfig {
                name: "Custom Codex".into(),
                command: wrapper.to_string_lossy().into_owned(),
                args: vec![],
                model: None,
                env: Default::default(),
                is_default: true,
            }],
            ..Default::default()
        },
    );
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), agents).unwrap();
    let state = test_state();
    let spawned = handle_agent(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        &serde_json::json!({"action":"spawn", "agent_type":"Custom Codex", "cwd":cwd, "prompt":"say READY", "args":["--no-alt-screen"]}),
        None,
    );
    assert!(spawned.get("error").is_none(), "spawn failed: {spawned}");
    let sid = spawned["session_id"].as_str().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let actual = wait_for_file_content_async(&argv, std::time::Duration::from_secs(5)).await;
    let output = loop {
        let output = handle_session(
            &state,
            &serde_json::json!({"action":"output", "session_id":sid, "limit":50}),
            None,
        );
        if output.to_string().contains("Do you trust this directory?")
            || std::time::Instant::now() >= deadline
        {
            break output;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };
    handle_session(
        &state,
        &serde_json::json!({"action":"kill", "session_id":sid}),
        None,
    );
    assert!(
        !actual.contains("trust_level"),
        "opt-out must preserve wrapper argv: {actual}"
    );
    assert!(
        output.to_string().contains("Do you trust this directory?"),
        "normal trust prompt must remain: {output}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn managed_claude_spawn_accepts_only_its_startup_trust_dialog() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::Builder::new()
        .prefix("managed-claude-trust-")
        .tempdir_in(crate::test_support::test_temp_root())
        .unwrap();
    let cwd = root.path().join("never-trusted");
    std::fs::create_dir(&cwd).unwrap();
    let script = root.path().join("claude");
    let accepted = root.path().join("accepted");
    let observed_keys = root.path().join("keys");
    std::fs::write(&script, format!("#!/bin/sh\nif [ \"$1\" = --version ]; then exit 0; fi\nstty -echo -icanon -icrnl min 1 time 0\nprintf 'Quick safety check: Is this a project you created or one you trust?\\n  Yes, I trust this folder\\n❯ No, exit\\n'\nkeys=$(dd bs=1 count=4 2>/dev/null | od -An -tx1 | tr -d ' \\n')\nprintf '%s' \"$keys\" > '{}'\nif [ \"$keys\" = 1b5b410d ]; then printf '%s' \"$1\" > '{}'; fi\nexec cat >/dev/null\n", observed_keys.display(), accepted.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        std::process::Command::new(&script)
            .arg("--version")
            .status()
            .unwrap()
            .success()
    );
    let _config = crate::config::set_config_dir_override(root.path().join("tuic-config"));
    let state = test_state();
    let spawned = handle_agent(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        &serde_json::json!({
            "action": "spawn", "agent_type": "claude", "binary_path": script,
            "cwd": cwd, "prompt": "say READY", "cols": 120,
        }),
        None,
    );
    assert!(spawned.get("error").is_none(), "spawn failed: {spawned}");
    let sid = spawned["session_id"].as_str().unwrap();
    let task = wait_for_file_content_async(&accepted, std::time::Duration::from_secs(5)).await;
    let output = handle_session(
        &state,
        &serde_json::json!({"action":"output", "session_id":sid, "format":"raw", "limit": 4096}),
        None,
    );
    let armed = state.managed_trust_dialogs.contains(sid);
    handle_session(
        &state,
        &serde_json::json!({"action":"kill", "session_id":sid}),
        None,
    );
    assert!(
        !task.is_empty(),
        "managed child must pass trust dialog without manual input: armed={armed}; keys={:?}; output={output}",
        std::fs::read_to_string(&observed_keys)
    );
    assert!(
        task.contains("say READY"),
        "spawn prompt must remain submitted: {task}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn managed_claude_trust_opt_out_leaves_dialog_waiting() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::Builder::new()
        .prefix("managed-claude-opt-out-")
        .tempdir_in(crate::test_support::test_temp_root())
        .unwrap();
    let cwd = root.path().join("never-trusted");
    std::fs::create_dir(&cwd).unwrap();
    let script = root.path().join("claude");
    let observed_keys = root.path().join("keys");
    let help = root.path().join("claude-help.txt");
    std::fs::write(
        &help,
        include_str!("../../tests/fixtures/agent-help/claude-2026-10-04.txt"),
    )
    .unwrap();
    std::fs::write(&script, format!("#!/bin/sh\nif [ \"$1\" = --version ]; then exit 0; fi\nif [ \"$1\" = --help ]; then cat '{}'; exit 0; fi\nstty -echo -icanon -icrnl min 0 time 10\nprintf 'Quick safety check: Is this a project you created or one you trust?\\n  Yes, I trust this folder\\n❯ No, exit\\n'\nkeys=$(dd bs=1 count=1 2>/dev/null | od -An -tx1 | tr -d ' \\n')\nprintf '%s' \"$keys\" > '{}'\nexec cat >/dev/null\n", help.display(), observed_keys.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        std::process::Command::new(&script)
            .arg("--version")
            .status()
            .unwrap()
            .success()
    );
    let _config = crate::config::set_config_dir_override(root.path().join("tuic-config"));
    let mut agents = crate::config::AgentsConfig::default();
    agents.agents.insert(
        "claude".into(),
        crate::config::AgentSettings {
            skip_trust_dialog: Some(false),
            ..Default::default()
        },
    );
    crate::config::save_agents_config(crate::config::AgentsConfig::default(), agents).unwrap();
    let state = test_state();
    let spawned = handle_agent(
        &state,
        "127.0.0.1:1".parse().unwrap(),
        &serde_json::json!({"action":"spawn", "agent_type":"claude", "binary_path":script, "cwd":cwd, "prompt":"say READY", "cols":120}),
        None,
    );
    assert!(spawned.get("error").is_none(), "spawn failed: {spawned}");
    let sid = spawned["session_id"].as_str().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !observed_keys.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let keys = std::fs::read_to_string(&observed_keys);
    let output = handle_session(
        &state,
        &serde_json::json!({"action":"output", "session_id":sid, "limit":50}),
        None,
    );
    handle_session(
        &state,
        &serde_json::json!({"action":"kill", "session_id":sid}),
        None,
    );
    assert_eq!(
        keys.expect("agent must sample trust-dialog input"),
        "",
        "opt-out must not answer Claude's trust question"
    );
    assert!(
        output["data"]
            .as_str()
            .unwrap_or_default()
            .contains("No, exit"),
        "trust dialog must remain visible: {output}"
    );
}

#[test]
fn resolve_run_config_matches_by_name_case_insensitive() {
    let cfg = make_agents_config();
    let resolved = resolve_run_config("Claude Qwen3.5", &cfg);
    assert_eq!(resolved.agent_type, "claude");
    assert_eq!(resolved.command.as_deref(), Some("ollama"));
    assert!(
        resolved
            .args
            .as_ref()
            .unwrap()
            .contains(&"qwen3.5".to_string())
    );
    assert_eq!(
        resolved.env.get("OLLAMA_HOST").map(|s| s.as_str()),
        Some("localhost:11434")
    );
}

// Catches: literal Codex rewrites a configured wrapper task into an undeliverable PTY injection.
#[test]
fn literal_codex_wrapper_default_keeps_positional_task_delivery() {
    let cfg: crate::config::AgentsConfig = serde_json::from_value(serde_json::json!({
        "agents": {"codex": {"run_configs": [
            {"name": "Wrapper", "command": "codex-wrapper", "args": ["run"], "is_default": true}
        ]}}
    }))
    .unwrap();
    let resolved = resolve_run_config("codex", &cfg);
    let (argv, deferred) = compose_mcp_spawn_args(McpSpawnArgs {
        agent_type: &resolved.agent_type,
        args: resolved.args.as_ref().unwrap(),
        prompt: "perform the task",
        model: None,
        print_mode: false,
        output_format: None,
        default_template: false,
    })
    .unwrap();
    assert_eq!(
        argv,
        vec!["run", "perform the task"],
        "default wrapper argv must keep the run-config positional prompt contract"
    );
    assert!(
        deferred.is_none(),
        "wrapper commands must not receive a deferred PTY task"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn literal_codex_default_does_not_ignore_settings_or_restore_removed_bypass() {
    // Catches: public spawn discards Settings args, restores bypass, or loses the task.
    use std::os::unix::fs::PermissionsExt;
    for args in [vec!["--dangerously-bypass-approvals-and-sandbox"], vec![]] {
        let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let output = root.path().join("argv");
        let binary = root.path().join("codex");
        std::fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$ARGV_OUTPUT\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let _config = crate::config::set_config_dir_override(root.path().join("config"));
        let cfg: crate::config::AgentsConfig = serde_json::from_value(serde_json::json!({
            "agents": {"codex": {
                "codex_bypass_migrated": true,
                "prevent_alt_screen": false, "skip_trust_dialog": false,
                "native_status_signals": false,
                "run_configs": [
                    {"name": "Custom", "command": binary, "args": ["--search"]},
                    {"name": "Default", "command": binary, "args": args, "is_default": true,
                     "env": {"ARGV_OUTPUT": output}}
                ]
            }}
        }))
        .unwrap();
        crate::config::save_agents_config(crate::config::AgentsConfig::default(), cfg).unwrap();
        let state = test_state();
        let spawned = handle_agent(
            &state,
            "127.0.0.1:1".parse().unwrap(),
            &serde_json::json!({"action": "spawn", "agent_type": "CODEX",
                                   "prompt": "perform the task", "cwd": root.path()}),
            None,
        );
        assert!(spawned.get("error").is_none(), "{spawned}");
        let session = spawned["session_id"].as_str().unwrap();
        assert_eq!(
            state
                .pending_injections
                .get(session)
                .unwrap()
                .front()
                .unwrap()
                .text(),
            "perform the task"
        );
        let actual = wait_for_file_content_async(&output, std::time::Duration::from_secs(60)).await;
        let mut expected = vec!["-c", "check_for_update_on_startup=false"];
        expected.extend(args);
        assert_eq!(actual, expected.join("\n") + "\n");
    }
}

#[test]
fn resolve_run_config_falls_back_to_agent_type() {
    let cfg = make_agents_config();
    let resolved = resolve_run_config("gemini", &cfg);
    assert_eq!(resolved.agent_type, "gemini");
    assert!(resolved.command.is_none());
    assert!(resolved.args.is_none());
    assert!(resolved.env.is_empty());
}

#[test]
fn resolve_run_config_cross_agent_match() {
    let cfg = make_agents_config();
    let resolved = resolve_run_config("codex-fast", &cfg);
    assert_eq!(resolved.agent_type, "codex");
    assert_eq!(resolved.command.as_deref(), Some("codex"));
}

// -----------------------------------------------------------------------
// substitute_prompt_in_args tests
// -----------------------------------------------------------------------

#[test]
fn substitute_prompt_placeholder_present() {
    let args = vec![
        "-p".to_string(),
        "{prompt}".to_string(),
        "--no-input".to_string(),
    ];
    let result = substitute_prompt_in_args(&args, "fix the bug");
    assert_eq!(result, vec!["-p", "fix the bug", "--no-input"]);
}

#[test]
fn substitute_prompt_placeholder_absent_appends() {
    let args = vec!["--fast".to_string()];
    let result = substitute_prompt_in_args(&args, "fix the bug");
    assert_eq!(result, vec!["--fast", "fix the bug"]);
}

#[test]
fn substitute_prompt_multiple_placeholders() {
    let args = vec![
        "{prompt}".to_string(),
        "--echo".to_string(),
        "{prompt}".to_string(),
    ];
    let result = substitute_prompt_in_args(&args, "hello");
    assert_eq!(result, vec!["hello", "--echo", "hello"]);
}

// -----------------------------------------------------------------------
// finalize_spawn_args tests (story 091 — prefill-only TUIs)
// -----------------------------------------------------------------------

#[test]
fn finalize_codex_withholds_prompt_from_argv() {
    // codex's positional prompt only prefills its TUI (never submits):
    // the placeholder must be dropped and the task deferred for injection.
    let merged = vec!["{prompt}".to_string()];
    let (argv, deferred) = finalize_spawn_args("codex", &merged, "say pong");
    assert!(argv.is_empty(), "codex argv must not carry the task");
    assert_eq!(deferred.as_deref(), Some("say pong"));
}

#[test]
fn finalize_codex_keeps_flags_drops_prompt() {
    let merged = vec![
        "{prompt}".to_string(),
        "--model".to_string(),
        "o4".to_string(),
    ];
    let (argv, deferred) = finalize_spawn_args("codex", &merged, "task");
    assert_eq!(argv, vec!["--model", "o4"]);
    assert_eq!(deferred.as_deref(), Some("task"));
}

#[test]
fn finalize_codex_defers_even_without_placeholder() {
    // Run-config args with no {prompt}: substitute would APPEND the prompt,
    // which for codex still only prefills — defer it instead.
    let merged = vec!["--fast".to_string()];
    let (argv, deferred) = finalize_spawn_args("codex", &merged, "task");
    assert_eq!(argv, vec!["--fast"]);
    assert_eq!(deferred.as_deref(), Some("task"));
}

#[test]
fn explicit_codex_flags_keep_flags_and_defer_prompt() {
    let explicit = vec!["--dangerously-bypass-approvals-and-sandbox".to_string()];
    let (argv, deferred) = finalize_explicit_spawn_args("codex", &explicit, "perform the task");

    assert_eq!(argv, explicit);
    assert_eq!(deferred.as_deref(), Some("perform the task"));
}

#[test]
fn codex_spawn_composition_preserves_model_with_explicit_args() {
    let explicit = vec!["--dangerously-bypass-approvals-and-sandbox".to_string()];
    let (argv, deferred) = compose_mcp_spawn_args(McpSpawnArgs {
        agent_type: "codex",
        args: &explicit,
        prompt: "perform the task",
        model: Some("gpt-5.6-terra"),
        print_mode: false,
        output_format: None,
        default_template: false,
    })
    .unwrap();

    assert_eq!(
        argv,
        vec![
            "--dangerously-bypass-approvals-and-sandbox",
            "--model",
            "gpt-5.6-terra"
        ]
    );
    assert_eq!(deferred.as_deref(), Some("perform the task"));
}

#[test]
fn direct_codex_explicit_args_without_agent_type_use_codex_semantics() {
    let explicit = vec!["--search".to_string()];
    let agent_type = resolve_spawn_agent_type("/usr/local/bin/codex", None).unwrap();
    let (argv, deferred) = compose_mcp_spawn_args(McpSpawnArgs {
        agent_type: &agent_type,
        args: &explicit,
        prompt: "perform the task",
        model: None,
        print_mode: false,
        output_format: None,
        default_template: false,
    })
    .unwrap();

    assert_eq!(argv, vec!["--search"]);
    assert_eq!(deferred.as_deref(), Some("perform the task"));
}

#[test]
fn direct_codex_binary_path_without_args_defers_prompt() {
    let agent_type = resolve_spawn_agent_type("/usr/local/bin/codex", None).unwrap();
    let template = crate::agent::default_prompt_args("codex").unwrap();
    let (argv, deferred) = compose_mcp_spawn_args(McpSpawnArgs {
        agent_type: &agent_type,
        args: &template,
        prompt: "perform the task",
        model: Some("gpt-5.6-luna"),
        print_mode: false,
        output_format: None,
        default_template: true,
    })
    .unwrap();

    assert_eq!(argv, vec!["--model", "gpt-5.6-luna"]);
    assert_eq!(deferred.as_deref(), Some("perform the task"));
}

#[test]
fn explicit_codex_placeholder_remains_authoritative() {
    let explicit = vec!["exec".to_string(), "{prompt}".to_string()];
    let (argv, deferred) = finalize_explicit_spawn_args("codex", &explicit, "perform the task");

    assert_eq!(argv, vec!["exec", "perform the task"]);
    assert!(deferred.is_none());
}

#[test]
fn explicit_non_prefill_flags_append_prompt() {
    let explicit = vec!["--verbose".to_string()];
    let (argv, deferred) = finalize_explicit_spawn_args("claude", &explicit, "perform the task");

    assert_eq!(argv, vec!["--verbose", "perform the task"]);
    assert!(deferred.is_none());
}

#[test]
fn explicit_claude_placeholder_remains_authoritative() {
    let explicit = vec![
        "--model".to_string(),
        "opus".to_string(),
        "{prompt}".to_string(),
    ];
    let (argv, deferred) = finalize_explicit_spawn_args("claude", &explicit, "perform the task");

    assert_eq!(argv, vec!["--model", "opus", "perform the task"]);
    assert!(deferred.is_none());
}

#[test]
fn finalize_other_agents_substitute_as_before() {
    let merged = vec!["session".to_string(), "{prompt}".to_string()];
    let (argv, deferred) = finalize_spawn_args("goose", &merged, "do it");
    assert_eq!(argv, vec!["session", "do it"]);
    assert!(deferred.is_none(), "non-prefill agents keep argv delivery");
}

#[test]
fn agent_enter_uses_command_injection_but_other_inputs_stay_raw() {
    assert!(uses_agent_command_injection(Some("codex"), Some("\r")));
    assert!(uses_agent_command_injection(Some("opencode"), Some("\r")));
    assert!(!uses_agent_command_injection(Some("claude"), Some("\r")));
    assert!(!uses_agent_command_injection(None, Some("\r")));
    assert!(!uses_agent_command_injection(Some("codex"), Some("\t")));
    assert!(!uses_agent_command_injection(Some("codex"), None));
}

#[test]
fn claude_template_argv_byte_identical_to_retired_branch() {
    // Story 092: claude folded into the default_prompt_args table. The row +
    // merge's claude flags-first rule must reproduce the retired dedicated
    // spawn branch's argv EXACTLY (element-for-element) for representative
    // spawns: prompt only; prompt+model; prompt+print_mode+output_format.
    let old_branch = |prompt: &str,
                      model: Option<&str>,
                      print_mode: bool,
                      output_format: Option<&str>|
     -> Vec<String> {
        // Verbatim ordering of the retired branch:
        // --print, --output-format F, --model M, <prompt>.
        let mut argv: Vec<String> = Vec::new();
        if print_mode {
            argv.push("--print".to_string());
        }
        if let Some(f) = output_format {
            argv.push("--output-format".to_string());
            argv.push(f.to_string());
        }
        if let Some(m) = model {
            argv.push("--model".to_string());
            argv.push(m.to_string());
        }
        argv.push(prompt.to_string());
        argv
    };
    let new_path = |prompt: &str,
                    model: Option<&str>,
                    print_mode: bool,
                    output_format: Option<&str>|
     -> Vec<String> {
        let template = crate::agent::default_prompt_args("claude").expect("claude row");
        let merged =
            merge_mcp_params_into_args("claude", &template, model, print_mode, output_format, true)
                .expect("no conflicts");
        let (argv, deferred) = finalize_spawn_args("claude", &merged, prompt);
        assert!(deferred.is_none(), "claude keeps argv prompt delivery");
        argv
    };
    for (model, print_mode, output_format) in [
        (None, false, None),               // prompt only
        (Some("opus"), false, None),       // prompt + model
        (None, true, Some("stream-json")), // prompt + print + format
    ] {
        assert_eq!(
            new_path("fix the bug", model, print_mode, output_format),
            old_branch("fix the bug", model, print_mode, output_format),
            "argv drift for model={model:?} print={print_mode} format={output_format:?}"
        );
    }
}

// -----------------------------------------------------------------------
// merge_mcp_params_into_args tests
// -----------------------------------------------------------------------

#[test]
fn merge_params_model_no_conflict() {
    let args = vec!["--fast".to_string()];
    let result =
        merge_mcp_params_into_args("claude", &args, Some("gpt-4"), false, None, false).unwrap();
    assert!(result.contains(&"--model".to_string()));
    assert!(result.contains(&"gpt-4".to_string()));
}

#[test]
fn merge_params_model_conflict() {
    let args = vec!["--model".to_string(), "sonnet".to_string()];
    let result = merge_mcp_params_into_args("claude", &args, Some("gpt-4"), false, None, false);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Conflict"));
}

#[test]
fn merge_params_print_mode_appended() {
    let args = vec![];
    let result = merge_mcp_params_into_args("claude", &args, None, true, None, false).unwrap();
    assert!(result.contains(&"--print".to_string()));
}

#[test]
fn merge_params_print_mode_already_present() {
    let args = vec!["--print".to_string()];
    let result = merge_mcp_params_into_args("claude", &args, None, true, None, false).unwrap();
    // Should not duplicate
    assert_eq!(result.iter().filter(|a| *a == "--print").count(), 1);
}

#[test]
fn merge_params_output_format_conflict() {
    let args = vec!["--output-format".to_string(), "json".to_string()];
    let result = merge_mcp_params_into_args("claude", &args, None, false, Some("text"), false);
    assert!(result.is_err());
}

#[test]
fn merge_params_output_format_no_conflict() {
    let args = vec![];
    let result =
        merge_mcp_params_into_args("claude", &args, None, false, Some("json"), false).unwrap();
    assert!(result.contains(&"--output-format".to_string()));
    assert!(result.contains(&"json".to_string()));
}

// Claude-only-param guard (todo.md O5): --print / --output-format must be
// dropped for non-claude agents (codex/gemini/goose die with clap error 2).

#[test]
fn merge_params_codex_drops_print_mode() {
    let args = vec!["{prompt}".to_string()];
    let result = merge_mcp_params_into_args("codex", &args, None, true, None, false).unwrap();
    assert!(
        !result.contains(&"--print".to_string()),
        "codex must not receive --print"
    );
    assert_eq!(result, vec!["{prompt}".to_string()]);
}

#[test]
fn merge_params_codex_drops_output_format() {
    let args = vec!["{prompt}".to_string()];
    let result =
        merge_mcp_params_into_args("codex", &args, None, false, Some("json"), false).unwrap();
    assert!(
        !result.contains(&"--output-format".to_string()),
        "codex must not receive --output-format"
    );
    assert!(!result.contains(&"json".to_string()));
}

#[test]
fn merge_params_codex_drops_both() {
    let args = vec!["{prompt}".to_string()];
    let result =
        merge_mcp_params_into_args("codex", &args, None, true, Some("json"), false).unwrap();
    assert!(!result.contains(&"--print".to_string()));
    assert!(!result.contains(&"--output-format".to_string()));
    assert_eq!(result, vec!["{prompt}".to_string()]);
}

#[test]
fn direct_codex_executable_identity_is_exact_and_cross_platform() {
    assert!(is_direct_codex_executable("/usr/local/bin/codex"));
    assert!(is_direct_codex_executable("C:\\tools\\codex.exe"));
    assert!(is_direct_codex_executable("C:\\tools\\codex.cmd"));
    assert!(is_direct_codex_executable("/opt/tools/CODEX"));
    assert!(!is_direct_codex_executable(
        "/opt/company/bin/codex-wrapper"
    ));
}

#[test]
fn direct_codex_executable_overrides_mismatched_bucket_semantics() {
    let agent_type = resolve_spawn_agent_type("/usr/local/bin/codex", Some("claude")).unwrap();
    assert_eq!(agent_type, "codex");

    let (argv, deferred) = compose_mcp_spawn_args(McpSpawnArgs {
        agent_type: &agent_type,
        args: &["--search".to_string()],
        prompt: "perform the task",
        model: None,
        print_mode: false,
        output_format: None,
        default_template: false,
    })
    .unwrap();
    assert_eq!(argv, vec!["--search"]);
    assert_eq!(deferred.as_deref(), Some("perform the task"));
}

#[test]
fn merge_params_codex_keeps_model() {
    // --model is generic (codex accepts it) — only print/output-format are gated.
    let args = vec!["{prompt}".to_string()];
    let result =
        merge_mcp_params_into_args("codex", &args, Some("gpt-5"), true, Some("json"), false)
            .unwrap();
    assert!(result.contains(&"--model".to_string()));
    assert!(result.contains(&"gpt-5".to_string()));
    assert!(!result.contains(&"--print".to_string()));
    assert!(!result.contains(&"--output-format".to_string()));
}

#[test]
fn merge_params_claude_keeps_both() {
    // Regression: claude behavior unchanged — both flags still injected.
    let args: Vec<String> = vec![];
    let result =
        merge_mcp_params_into_args("claude", &args, None, true, Some("json"), false).unwrap();
    assert!(result.contains(&"--print".to_string()));
    assert!(result.contains(&"--output-format".to_string()));
    assert!(result.contains(&"json".to_string()));
}

#[test]
fn merge_params_run_config_claude_keeps_appended_order() {
    // Regression (codex review, story 092): the claude flags-first rule is
    // scoped to the default template. A user run config may wrap claude in a
    // launcher subcommand ("launch claude {prompt}") — prepending flags
    // before it would feed them to the wrapper. Run-config args keep the
    // legacy appended placement.
    let args = vec!["launch".to_string(), "{prompt}".to_string()];
    let result =
        merge_mcp_params_into_args("claude", &args, Some("opus"), false, None, false).unwrap();
    assert_eq!(result, vec!["launch", "{prompt}", "--model", "opus"]);
}

// ── session alias as the universal agent address (#737-2150) ────────────

/// Create a live PTY for a test, or report that this environment has none.
fn create_test_session(state: &Arc<AppState>) -> Option<String> {
    let created = handle_session(state, &serde_json::json!({"action": "create"}), None);
    if created.get("error").is_some() {
        eprintln!("Skipping: PTY not available in this environment");
        return None;
    }
    Some(created["session_id"].as_str().unwrap().to_string())
}

fn kill_test_session(state: &Arc<AppState>, session_id: &str) {
    let _ = handle_session(
        state,
        &serde_json::json!({"action": "kill", "session_id": session_id}),
        None,
    );
}

fn listed_entry(
    state: &Arc<AppState>,
    session_id: &str,
    mcp_session_id: Option<&str>,
) -> serde_json::Map<String, serde_json::Value> {
    handle_session(
        state,
        &serde_json::json!({"action": "list"}),
        mcp_session_id,
    )
    .as_array()
    .expect("session list is an array")
    .iter()
    .find(|entry| entry["session_id"] == session_id)
    .unwrap_or_else(|| panic!("{session_id} missing from session list"))
    .as_object()
    .expect("session list entry is an object")
    .clone()
}

/// One address book: whatever name the caller holds for a terminal reaches it.
// Needs a runtime: creating a PTY arms the reader thread through tokio.
#[tokio::test]
async fn the_session_tool_addresses_a_session_by_alias_or_tuic_session() {
    let state = test_state();
    let Some(session_id) = create_test_session(&state) else {
        return;
    };
    let tab_uuid = "550e8400-e29b-41d4-a716-4466554417a1";
    state.bind_live_pty(tab_uuid, &session_id);
    state
        .session_maps
        .session_states
        .insert(session_id.clone(), crate::state::SessionState::default());
    let alias = state
        .session_maps
        .term_aliases
        .get(&session_id)
        .map(|entry| entry.value().clone())
        .expect("a spawned session has an alias");

    for reference in [session_id.as_str(), tab_uuid, alias.as_str()] {
        let status = handle_session(
            &state,
            &serde_json::json!({"action": "status", "session_id": reference}),
            None,
        );
        assert_eq!(
            status["session_id"], session_id,
            "'{reference}' must address the same terminal: {status}"
        );
    }

    let missing = handle_session(&state, &serde_json::json!({"action": "status"}), None);
    let error = missing["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("alias"),
        "the guidance must name the alias as an accepted address, got: {error}"
    );
    kill_test_session(&state, &session_id);
}

// Catches: a verdict for another request id (a different session, or a made-up id)
// resolving this pending call.
#[tokio::test]
async fn crit1358_a_verdict_for_another_request_does_not_resolve_this_one() {
    let sid = "550e8400-e29b-41d4-a716-446655440d03";
    let state = crit1358_attached_state(&[sid]);
    let mut events = state.event_bus.subscribe();
    let call = tokio::spawn({
        let state = state.clone();
        async move { suspend(&state, sid).await }
    });
    let (_, request_id) = crit1358_next_request(&mut events).await;

    resolve_session_suspend(&state, "not-the-request-id", true, None);
    tokio::task::yield_now().await;
    assert!(!call.is_finished(), "a foreign id resolved the call");
    assert!(state.suspend_responses.contains_key(&request_id));

    resolve_session_suspend(&state, &request_id, false, None);
    let result = call.await.unwrap();
    assert_eq!(result["error"], "Cannot suspend: refused");
}

// Catches: two concurrent suspends sharing one request id (or one slot), so one tab's
// refusal is delivered to the other session's caller.
#[tokio::test]
async fn crit1358_concurrent_suspends_each_get_their_own_verdict() {
    let a = "550e8400-e29b-41d4-a716-446655440d04";
    let b = "550e8400-e29b-41d4-a716-446655440d05";
    let state = crit1358_attached_state(&[a, b]);
    let mut events = state.event_bus.subscribe();
    let call_a = tokio::spawn({
        let state = state.clone();
        async move { suspend(&state, a).await }
    });
    let call_b = tokio::spawn({
        let state = state.clone();
        async move { suspend(&state, b).await }
    });
    let mut ids = std::collections::HashMap::new();
    for _ in 0..2 {
        let (sid, rid) = crit1358_next_request(&mut events).await;
        ids.insert(sid, rid);
    }
    assert_ne!(ids[a], ids[b], "request ids must be unique per call");

    resolve_session_suspend(&state, &ids[b], false, Some("agent working".into()));
    resolve_session_suspend(&state, &ids[a], true, None);

    assert_eq!(call_a.await.unwrap(), serde_json::json!({"ok": true}));
    assert_eq!(
        call_b.await.unwrap()["error"],
        "Cannot suspend: agent working"
    );
    assert!(state.suspend_responses.is_empty());
}

// Catches: a second client answering the same request (two UIs own the tab) overriding
// or panicking after the first verdict, or leaving the slot behind.
#[tokio::test]
async fn crit1358_the_first_verdict_wins_and_a_duplicate_is_ignored() {
    let sid = "550e8400-e29b-41d4-a716-446655440d06";
    let state = crit1358_attached_state(&[sid]);
    let mut events = state.event_bus.subscribe();
    let call = tokio::spawn({
        let state = state.clone();
        async move { suspend(&state, sid).await }
    });
    let (_, request_id) = crit1358_next_request(&mut events).await;

    resolve_session_suspend(&state, &request_id, true, None);
    resolve_session_suspend(&state, &request_id, false, Some("late".into()));

    assert_eq!(call.await.unwrap(), serde_json::json!({"ok": true}));
    assert!(state.suspend_responses.is_empty());
}

// Catches: a verdict arriving after the 20 s timeout re-creating or leaking state, or the
// timed-out slot blocking a later suspend of the same session.
#[tokio::test(start_paused = true)]
async fn crit1358_a_late_verdict_after_the_timeout_is_ignored_and_a_retry_works() {
    let sid = "550e8400-e29b-41d4-a716-446655440d07";
    let state = crit1358_attached_state(&[sid]);
    let mut events = state.event_bus.subscribe();
    let first = tokio::spawn({
        let state = state.clone();
        async move { suspend(&state, sid).await }
    });
    let (_, stale_id) = crit1358_next_request(&mut events).await;
    assert_eq!(
        first.await.unwrap()["error"],
        "Cannot suspend: no tab answered the request"
    );
    assert!(state.suspend_responses.is_empty());

    resolve_session_suspend(&state, &stale_id, true, None);
    assert!(state.suspend_responses.is_empty());

    let retry = tokio::spawn({
        let state = state.clone();
        async move { suspend(&state, sid).await }
    });
    let (_, fresh_id) = crit1358_next_request(&mut events).await;
    assert_ne!(fresh_id, stale_id);
    resolve_session_suspend(&state, &stale_id, false, Some("stale".into()));
    resolve_session_suspend(&state, &fresh_id, true, None);
    assert_eq!(retry.await.unwrap(), serde_json::json!({"ok": true}));
}

// Catches: self-suspend allowed once the arm moved out of handle_session (the guard
// dropped in the move), ending the caller's own session.
#[tokio::test]
async fn crit1358_an_agent_cannot_suspend_its_own_session() {
    let sid = "550e8400-e29b-41d4-a716-446655440d0a";
    let state = crit1358_attached_state(&[sid]);
    state
        .mcp
        .to_session
        .insert("mcp-own".to_string(), sid.to_string());
    let mut events = state.event_bus.subscribe();

    let result = handle_session_suspend(
        &state,
        &serde_json::json!({"action": "suspend", "session_id": sid}),
        Some("mcp-own"),
    )
    .await;

    assert_eq!(result["error"], "Cannot suspend own session.");
    assert!(events.try_recv().is_err());
}

// Catches: `suspend` dropped from handle_session's dispatcher leaving a stale sync path
// that answers success without a tab (a direct handle_session call must not succeed).
#[test]
fn crit1358_handle_session_has_no_fire_and_forget_suspend() {
    let sid = "550e8400-e29b-41d4-a716-446655440d0b";
    let state = crit1358_attached_state(&[sid]);
    let result = handle_session(
        &state,
        &serde_json::json!({"action": "suspend", "session_id": sid}),
        None,
    );
    assert!(result.get("ok").is_none(), "got {result}");
}
