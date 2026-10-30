use super::*;

fn fixture() -> (tempfile::TempDir, Arc<AppState>, Value) {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let repo = root.path().join("workspace");
    std::fs::create_dir(&repo).unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "wf04@example.test"],
        vec!["config", "user.name", "WF04"],
        vec!["commit", "--allow-empty", "-qm", "fixture"],
    ] {
        crate::git_cli::git_cmd(&repo).args(args).run().unwrap();
    }
    // Compose existing captured config and prompt exchanges. No agent response
    // fields are invented or changed: both source sessions use the same ID.
    let captured = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/acp");
    let config = std::fs::read_to_string(captured.join("config-set.jsonl")).unwrap();
    let prompt = std::fs::read_to_string(captured.join("prompt-turn.jsonl")).unwrap();
    let mut scenario = config
        .lines()
        .filter(|line| !line.contains("await_stdin_eof"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    scenario.extend(
        prompt
            .lines()
            .skip_while(|line| !line.contains("\"session/prompt\""))
            .map(str::to_owned),
    );
    std::fs::write(repo.join("scenario.jsonl"), scenario.join("\n") + "\n").unwrap();
    std::fs::copy(
        captured.join("ego-initialize.json"),
        repo.join("ego-initialize.json"),
    )
    .unwrap();
    let executable = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(format!(
            "tuic-acp-fixture-agent{}",
            std::env::consts::EXE_SUFFIX
        ));
    assert!(
        executable.is_file(),
        "nextest fixture-bins setup must build {}",
        executable.display()
    );
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    let request = json!({
        "idempotency_key":"acp-attempt", "parent_session_id":"workflow:acp-fixture",
        "name":"ACP worker", "keep_open":true, "workspace":{"existing_path":repo},
        "spawn":{"prompt":"Implement the bound task", "acp":{
            "executable":executable,"profile":"fixture","machine_profile":"",
            "model":"opus","effort":"high",
        }},
    });
    (root, state, request)
}

// Catches: a response-lost daemon ACP launch starts twice, fabricates a PTY,
// delivers before binding, or treats missing restart inventory as a dead child.
#[tokio::test]
async fn lost_spawn_response_does_not_spawn_again() {
    let (_root, state, request) = fixture();
    assert!(state.peer_agents.is_empty());
    assert!(
        execute_scoped(
            &state,
            "127.0.0.1:0".parse().unwrap(),
            &request,
            Fault::AfterChild,
            false
        )
        .await
        .unwrap_err()
        .contains("lost ACP spawn response")
    );
    let first = execute_owned(&state, "127.0.0.1:0".parse().unwrap(), &request).await;
    assert_eq!(first["stage"], "acp_not_delivered", "{first}");
    assert_eq!(first["prompt_delivery"], "not_delivered");
    assert_eq!(first["session_id"], Value::Null);
    assert!(
        state.session_maps.sessions.is_empty(),
        "ACP peers have no PTY"
    );
    assert!(
        state.session_maps.session_parent.is_empty(),
        "run authority is not a terminal parent"
    );
    let peer = first["peer_id"].as_str().unwrap();
    let connection: crate::acp::AcpConnectionId =
        serde_json::from_value(first["connection_id"].clone()).unwrap();
    let before = state.acp.peer_snapshot(peer).unwrap();
    assert_eq!(before.attachments.len(), 1);
    assert!(
        before.attachments[0].active_turn.is_none(),
        "no prompt before binding releases delivery"
    );
    assert_eq!(
        first["conversation_id"],
        "01932d5e-0000-7000-8000-0000000000aa"
    );
    assert_ne!(first["peer_id"], first["conversation_id"]);
    assert_eq!(first["child"]["acp"]["generation"], before.generation);
    let repeated = execute_owned(&state, "127.0.0.1:0".parse().unwrap(), &request).await;
    assert_eq!(repeated["connection_id"], first["connection_id"]);
    assert_eq!(repeated["launch_id"], first["launch_id"]);
    assert_eq!(repeated["conversation_id"], first["conversation_id"]);
    assert!(
        state.acp.peer_snapshot(peer).unwrap().attachments[0]
            .active_turn
            .is_none()
    );
    let mut restarted = crate::state::tests_support::make_test_app_state();
    restarted.data_dir = state.data_dir.clone();
    let restarted = Arc::new(restarted);
    let unknown = execute_owned(&restarted, "127.0.0.1:0".parse().unwrap(), &request).await;
    assert_eq!(unknown["inventory"], "unknown");
    assert_eq!(unknown["prompt_delivery"], "not_delivered");
    assert_eq!(unknown["launch_id"], first["launch_id"]);
    assert!(restarted.acp.peer_snapshot(peer).is_none());

    let mut conflict = request.clone();
    conflict["spawn"]["acp"]["model"] = json!("unapproved-other-model");
    let refused = execute_owned(&state, "127.0.0.1:0".parse().unwrap(), &conflict).await;
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .contains("idempotency_conflict")
    );
    let mut stream = state.acp.subscribe(connection, 1).unwrap();
    let delivered = deliver_acp_owned(&state, first["launch_id"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(delivered["prompt_delivery"], "delivered", "{delivered}");
    let again = deliver_acp_owned(&state, first["launch_id"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(again["turn_id"], delivered["turn_id"]);
    let observed = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let mut prompt_count = 0;
        let mut text = String::new();
        while let Some(event) = stream.recv().await {
            let event = event.unwrap();
            match event.event {
                crate::acp::AcpClientEvent::PromptSent { text: sent } => {
                    prompt_count += 1;
                    text = sent;
                }
                crate::acp::AcpClientEvent::TurnSettled { .. } => return (prompt_count, text),
                _ => {}
            }
        }
        panic!("recorded ACP turn ended without its protocol settlement");
    })
    .await;
    state.acp.disconnect(connection).await.unwrap();
    assert_eq!(
        observed.expect("recorded ACP turn did not settle"),
        (1, "Implement the bound task".into())
    );
    assert!(state.session_maps.sessions.is_empty());
    assert_eq!(
        delivered["ready"], false,
        "protocol handoff is not attempt success"
    );
}

// Catches: another host scope can request workflow-only ACP authority.
#[tokio::test]
async fn acp_launch_refuses_other_authority_before_any_reservation() {
    let (_root, state, mut request) = fixture();
    let public = execute(&state, "127.0.0.1:0".parse().unwrap(), &request).await;
    assert!(
        public["error"]
            .as_str()
            .unwrap()
            .contains("daemon workflow authority")
    );
    request["parent_session_id"] = json!("automation:not-a-workflow");
    let other_owner = execute_owned(&state, "127.0.0.1:0".parse().unwrap(), &request).await;
    assert!(
        other_owner["error"]
            .as_str()
            .unwrap()
            .contains("daemon workflow authority")
    );
    assert!(!state.data_dir.join("managed-launches.sqlite").exists());
    assert!(state.session_maps.sessions.is_empty());
}
