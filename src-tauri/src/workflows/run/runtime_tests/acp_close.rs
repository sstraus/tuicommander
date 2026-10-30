//! Genuine ACP process and close journal, using the recorded fixture agent.
use super::*;

// Catches: a live detached background receipt authorizing worker reap, an ACP
// disconnect being called without a durable stop intent, or a valid idle owned
// worker being impossible to close after its final report. Never contacts a model.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_background_receipt_prevents_idle_reap() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    let (config, project, _plan, story, _template, _guard) = fixture();
    let captured = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/acp");
    let options = std::fs::read_to_string(captured.join("config-set.jsonl")).unwrap();
    let turn = std::fs::read_to_string(captured.join("prompt-turn.jsonl")).unwrap();
    let mut scenario = options
        .lines()
        .filter(|line| !line.contains("await_stdin_eof"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    scenario.extend(
        turn.lines()
            .skip_while(|line| !line.contains("\"session/prompt\""))
            .map(str::to_owned),
    );
    std::fs::write(
        project.path().join("scenario.jsonl"),
        scenario.join("\n") + "\n",
    )
    .unwrap();
    std::fs::write(project.path().join("expected-profile.txt"), "fixture").unwrap();
    std::fs::copy(
        captured.join("ego-initialize.json"),
        project.path().join("ego-initialize.json"),
    )
    .unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.name", "WF04"],
        vec!["config", "user.email", "wf04@example.test"],
        vec!["add", "."],
        vec!["commit", "-qm", "recorded ACP fixture"],
    ] {
        crate::git_cli::git_cmd(project.path())
            .args(args)
            .run()
            .unwrap();
    }
    let workspace = config.path().join("acp-workspace");
    crate::git_cli::git_cmd(project.path())
        .args([
            "worktree",
            "add",
            "-qb",
            "acp-worker",
            workspace.to_str().unwrap(),
        ])
        .run()
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
        "nextest fixture-bins must build {}",
        executable.display()
    );
    WorkflowStore::open()
        .unwrap()
        .import_agent_policy(
            0,
            crate::workflows::AgentPolicyInput {
                source: "recorded ACP close fixture".into(),
                source_revision: "1".into(),
                routes: vec![crate::workflows::RoleRoute {
                    role: AgentRole::Implementer,
                    execution: crate::workflows::AgentExecution::Acp {
                        profile: "fixture".into(),
                        model: "opus".into(),
                        effort: "high".into(),
                    },
                    acp_runtime: Some(crate::workflows::AcpRuntimeRoute {
                        executable,
                        machine_profile: String::new(),
                    }),
                    model: "opus".into(),
                    effort: "high".into(),
                    capabilities: vec!["story_read".into(), "story_report".into()],
                    scope: crate::workflows::AgentScope::IsolatedWorkspace,
                    output_contract_version: 1,
                    default_for_role: true,
                }],
            },
        )
        .unwrap();
    let state = state(config.path());
    WorkflowRuntime::spawn(&state);
    owner_ready(&state).await;
    let published = definition(project.path().to_str().unwrap(), false, true);
    let mut hints = state.event_bus.subscribe();
    let run = state
        .workflow_runtime
        .start_graph_in_worktree(
            &state,
            &request(
                project.path().to_str().unwrap(),
                &story,
                &published,
                "acp-owned-close",
            ),
            Some((&story, workspace.to_str().unwrap())),
        )
        .unwrap();
    let store = RunStore::open().unwrap();
    let bound = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        loop {
            let current = store.snapshot(&run.id).unwrap();
            if current.attempts.first().is_some_and(|a| a.agent.is_some()) {
                break current;
            }
            assert_eq!(
                current.status,
                RunStatus::Running,
                "ACP launch setup stopped: {:?}",
                current.pause_cause
            );
            hints.recv().await.unwrap();
        }
    })
    .await
    .expect("ACP fixture binding setup did not finish");
    let attempt = &bound.attempts[0];
    let binding = attempt.agent.as_ref().unwrap();
    let acp = binding.acp.as_ref().expect("genuine ACP binding");
    let mut stream = state.acp.subscribe(acp.connection_id, 1).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        while let Some(frame) = stream.recv().await {
            if matches!(
                frame.unwrap().event,
                crate::acp::AcpClientEvent::TurnSettled { .. }
            ) {
                return;
            }
        }
        panic!("ACP fixture ended before its recorded turn settled");
    })
    .await
    .expect("ACP fixture turn setup did not finish");
    store
        .report_acp_agent(
            AttemptReport {
                contract_version: 1,
                run_id: run.id.clone(),
                story_id: story.clone(),
                story_revision: StoryStore::open()
                    .unwrap()
                    .get_story(&story)
                    .unwrap()
                    .revision,
                attempt_id: attempt.id.clone(),
                generation: attempt.generation,
                outcome: AttemptOutcome::Completed,
                summary: "Fixture turn settled; typed result is separate from process close".into(),
                criterion_results: vec![],
                evidence: vec![],
                input_request: None,
                review: None,
            },
            &binding.session_id,
            acp.clone(),
        )
        .unwrap();
    let cancelled = store
        .command(&run.id, "cancel-acp-fixture", RunCommand::Cancel)
        .unwrap()
        .snapshot;
    // The OS, not a hand-written process snapshot, supplies this runner-shaped
    // argv. A ready pipe is setup evidence; the test never relies on a sleep.
    let mut runner = Command::new("/bin/sh")
        .args([
            "-c",
            "printf 'ready\\n'; read line",
            "__bg-runner",
            &binding.session_id,
            "--",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    BufReader::new(runner.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready, "ready\n");
    let close = |request_id: &str| RunAction::CloseAgent {
        run_id: run.id.clone(),
        attempt_id: attempt.id.clone(),
        request_id: request_id.into(),
        expected_sequence: cancelled.sequence,
        successor_attempt_id: None,
    };
    // The runtime may still reconcile the cancelled launch under the shared
    // transaction lock. Follow the public retry contract with the same request;
    // contention is setup, not a background-receipt refusal or stop decision.
    let close_when_available = |action: RunAction| {
        let state = state.clone();
        let project = project.path().to_string_lossy().into_owned();
        async move {
            tokio::time::timeout(std::time::Duration::from_secs(120), async {
                loop {
                    let state = state.clone();
                    let project = project.clone();
                    let action = action.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        run_action_with_events(&state, &project, action)
                    })
                    .await
                    .unwrap();
                    if result.as_ref().is_err_and(|error| {
                        error == "managed launch/close is active; retry the same request"
                    }) {
                        tokio::task::yield_now().await;
                        continue;
                    }
                    return result;
                }
            })
            .await
            .expect("launch/close transaction setup never became available")
        }
    };
    let refused = close_when_available(close("live-receipt")).await;
    runner.kill().unwrap();
    runner.wait().unwrap();
    let RunReply::AgentClose(refused) = refused.unwrap() else {
        panic!("wrong refusal reply");
    };
    let still_live = state
        .acp
        .snapshot(acp.connection_id)
        .unwrap()
        .settlement
        .is_none();
    let stopped = close_when_available(close("receipt-ended")).await;
    // Always stop only this test's ACP process before assertions can panic.
    let _ = state.acp.disconnect(acp.connection_id).await;
    let RunReply::AgentClose(stopped) = stopped.unwrap() else {
        panic!("wrong close reply");
    };
    assert_eq!(refused["state"], "refused");
    assert!(
        refused["reason"]
            .as_str()
            .unwrap()
            .contains("background receipt"),
        "{refused}"
    );
    assert!(still_live, "refused close cannot disconnect the worker");
    assert_eq!(stopped["state"], "closed", "{stopped}");
    assert_eq!(stopped["stopReceipt"]["observedAbsent"], true);
    assert!(workspace.join(".git").is_file());
    assert_eq!(
        std::fs::read(workspace.join("scenario.jsonl")).unwrap(),
        std::fs::read(project.path().join("scenario.jsonl")).unwrap()
    );
    assert_eq!(store.snapshot(&run.id).unwrap(), cancelled);
    assert_eq!(store.replay(&run.id).unwrap(), cancelled);
}
