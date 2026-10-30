//! Real daemon/managed-launch seam regressions. No external model is contacted.
use super::*;

// Catches: closing a reported child deleting unattributed work, moving baseline
// ownership on rename, or losing its refusal/decision when a daemon restarts.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_preserves_unattributed_dirty_work() {
    let (config, project, _plan, story_id, _template, _guard) = fixture();
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.name", "WF04"],
        vec!["config", "user.email", "wf04@example.test"],
        vec!["commit", "--allow-empty", "-qm", "baseline"],
    ] {
        crate::git_cli::git_cmd(project.path())
            .args(args)
            .run()
            .unwrap();
    }
    let worktree = config.path().join("isolated");
    crate::git_cli::git_cmd(project.path())
        .args([
            "worktree",
            "add",
            "-qb",
            "worker",
            worktree.to_str().unwrap(),
        ])
        .run()
        .unwrap();
    let worktree = worktree.canonicalize().unwrap();
    let base = crate::config::load_agents_config();
    let desired = serde_json::from_value(serde_json::json!({
        "launch_instructions":"", "agents":{"gemini":{
            "run_configs":[{"name":"sol","command":"/bin/sh","model":"owned-shell-fixture","args":["-c","exec cat","{prompt}","--effort","fixture"]}]
        }}
    }))
    .unwrap();
    crate::config::save_agents_config(base, desired).unwrap();
    WorkflowStore::open()
        .unwrap()
        .import_agent_policy(
            0,
            crate::workflows::AgentPolicyInput {
                source: "owned shell fixture policy".into(),
                source_revision: "1".into(),
                routes: vec![crate::workflows::RoleRoute {
                    role: AgentRole::Implementer,
                    execution: crate::workflows::AgentExecution::Pty {
                        profile: "sol".into(),
                    },
                    acp_runtime: None,
                    model: "owned-shell-fixture".into(),
                    effort: "fixture".into(),
                    capabilities: vec!["story_read".into(), "story_report".into()],
                    scope: crate::workflows::AgentScope::IsolatedWorkspace,
                    output_contract_version: crate::workflows::PROMPT_CONTRACT_VERSION,
                    default_for_role: true,
                }],
            },
        )
        .unwrap();
    let state = state(config.path());
    WorkflowRuntime::spawn(&state);
    owner_ready(&state).await;
    let published = definition(project.path().to_str().unwrap(), false, true);
    let mut events = state.event_bus.subscribe();
    let run = state
        .workflow_runtime
        .start_graph_in_worktree(
            &state,
            &request(
                project.path().to_str().unwrap(),
                &story_id,
                &published,
                "owned-close",
            ),
            Some((&story_id, worktree.to_str().unwrap())),
        )
        .unwrap();
    let store = RunStore::open().unwrap();
    // This bound waits only for setup, not a cleanup timing assertion.
    let bound = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        loop {
            let current = store.snapshot(&run.id).unwrap();
            if current.attempts.first().is_some_and(|a| a.agent.is_some()) {
                break current;
            }
            assert_eq!(
                current.status,
                RunStatus::Running,
                "PTY launch setup stopped: {:?}",
                current.pause_cause
            );
            events.recv().await.unwrap();
        }
    })
    .await
    .expect("managed launch setup did not produce a binding");
    let attempt = &bound.attempts[0];
    let binding = attempt.agent.as_ref().unwrap();
    let peer = binding.session_id.clone();
    let child_exists = state.session_maps.sessions.contains_key(&peer);
    let report = AttemptReport {
        contract_version: crate::workflows::PROMPT_CONTRACT_VERSION,
        run_id: run.id.clone(),
        story_id: story_id.clone(),
        story_revision: StoryStore::open()
            .unwrap()
            .get_story(&story_id)
            .unwrap()
            .revision,
        attempt_id: attempt.id.clone(),
        generation: attempt.generation,
        outcome: AttemptOutcome::Completed,
        summary: "Bound worker report is evidence, not artifact approval".into(),
        criterion_results: vec![],
        evidence: vec![],
        input_request: None,
        review: None,
    };
    // Catches: an answer being dropped, replayed to a replacement, or accepted
    // as completion; delayed pre-answer reports cannot finish generation two.
    let mut question = report.clone();
    question.outcome = AttemptOutcome::NeedsInput;
    question.input_request = Some(InputRequest {
        question: "Which bytes must remain?".into(),
        options: vec![],
    });
    store.report_bound_agent(question, &peer).unwrap();
    let answered = store
        .command(
            &run.id,
            "answer-owned-worker",
            RunCommand::AnswerInput {
                attempt_id: attempt.id.clone(),
                answer: "Keep unsaved ownership must remain".into(),
            },
        )
        .unwrap()
        .snapshot;
    let continued = answered
        .attempts
        .iter()
        .find(|a| a.id == attempt.id)
        .unwrap();
    assert_eq!(continued.generation, 2);
    assert_eq!(continued.agent, attempt.agent);
    assert_eq!(
        continued.continuation.as_ref().unwrap().delivery,
        ContinuationDelivery::Pending
    );
    assert!(
        store
            .report_bound_agent(report.clone(), &peer)
            .unwrap_err()
            .contains("reused with a different payload")
    );
    let late = store.snapshot(&run.id).unwrap();
    assert_eq!(late.attempts[0].generation, 2);
    assert_eq!(late.attempts[0].state, AttemptState::Running);
    let graph = &late.graph_executions[0];
    let resumed = store
        .command(
            &run.id,
            "resume-owned-worker",
            RunCommand::ResumeGraph {
                execution_id: graph.id.clone(),
                activation_id: graph.activations.last().unwrap().id.clone(),
                resolution: "Deliver recorded answer".into(),
            },
        )
        .unwrap()
        .snapshot;
    let mut unknown = crate::state::tests_support::make_test_app_state();
    unknown.data_dir = state.data_dir.clone();
    let unknown = Arc::new(unknown);
    assert!(
        super::super::runtime::effects::drive_effect(&unknown, &store, &resumed)
            .await
            .unwrap_err()
            .contains("inventory unknown")
    );
    assert_eq!(
        store.snapshot(&run.id).unwrap(),
        resumed,
        "missing inventory must leave the continuation pending"
    );
    assert!(
        super::super::runtime::effects::drive_effect(&state, &store, &resumed)
            .await
            .unwrap()
    );
    let delivered = store.snapshot(&run.id).unwrap();
    assert_eq!(
        delivered.attempts[0]
            .continuation
            .as_ref()
            .unwrap()
            .delivery,
        ContinuationDelivery::Delivered
    );
    let mail = state.agent_inbox.get(&peer).unwrap();
    let input_mail = mail
        .iter()
        .filter(|m| m.id.starts_with("workflow-input:"))
        .collect::<Vec<_>>();
    assert_eq!(input_mail.len(), 1);
    let payload: serde_json::Value = serde_json::from_str(&input_mail[0].content).unwrap();
    assert_eq!(payload["generation"], 2);
    assert_eq!(payload["answer"], "Keep unsaved ownership must remain");
    drop(mail);
    assert!(
        !super::super::runtime::effects::drive_effect(&state, &store, &delivered)
            .await
            .unwrap(),
        "confirmed continuation is never delivered twice"
    );
    assert_eq!(store.replay(&run.id).unwrap(), delivered);
    let mut report = report;
    report.generation = 2;
    let completed = store.report_bound_agent(report.clone(), &peer).unwrap();
    assert_eq!(
        store.report_bound_agent(report.clone(), &peer).unwrap(),
        completed,
        "same-generation report retry must retain the original receipt"
    );
    report.summary = "Conflicting generation-two report".into();
    assert!(
        store
            .report_bound_agent(report, &peer)
            .unwrap_err()
            .contains("reused with a different payload")
    );
    let cancelled = store
        .command(&run.id, "cancel-before-owned-close", RunCommand::Cancel)
        .unwrap()
        .snapshot;
    std::fs::write(
        worktree.join("unattributed.txt"),
        b"unsaved ownership must remain\n",
    )
    .unwrap();
    crate::git_cli::git_cmd(&worktree)
        .args(["branch", "-m", "renamed-worker"])
        .run()
        .unwrap();
    let state_bg = state.clone();
    let project_path = project.path().to_string_lossy().into_owned();
    let close_action = RunAction::CloseAgent {
        run_id: run.id.clone(),
        attempt_id: attempt.id.clone(),
        request_id: "close-with-dirt".into(),
        expected_sequence: cancelled.sequence,
        successor_attempt_id: None,
    };
    let action_bg = close_action.clone();
    let project_bg = project_path.clone();
    let closed = tokio::task::spawn_blocking(move || {
        run_action_with_events(&state_bg, &project_bg, action_bg)
    })
    .await
    .unwrap()
    .unwrap();
    let RunReply::AgentClose(decision) = closed else {
        panic!("close returned wrong reply");
    };
    let bytes = std::fs::read(worktree.join("unattributed.txt")).unwrap();
    let live_after = state.session_maps.sessions.contains_key(&peer);
    let retry = run_action_with_events(&state, &project_path, close_action).unwrap();
    let RunReply::AgentClose(retry) = retry else {
        panic!("close retry returned wrong reply");
    };
    let mut restarted = crate::state::tests_support::make_test_app_state();
    restarted.data_dir = state.data_dir.clone();
    let journal = run_action_with_events(
        &restarted,
        &project_path,
        RunAction::AgentCloses {
            run_id: run.id.clone(),
        },
    )
    .unwrap();
    // Only the test cleans up its own PTY; production close must have refused it.
    crate::pty::close_pty_core_with_reason(&state, &peer, false, "test_cleanup");
    assert!(
        child_exists && live_after,
        "refused close must preserve the launched process"
    );
    assert_eq!(decision["state"], "refused");
    assert!(
        decision["reason"].as_str().unwrap().contains("dirty work"),
        "{decision}"
    );
    assert_eq!(bytes, b"unsaved ownership must remain\n");
    assert!(
        worktree.join(".git").is_file(),
        "close never removes a checkout"
    );
    assert_eq!(
        decision, retry,
        "identical close returns the original durable decision"
    );
    let RunReply::AgentClose(journal) = journal else {
        panic!("journal returned wrong reply");
    };
    assert_eq!(journal, serde_json::json!([decision]));
    assert_eq!(
        store.snapshot(&run.id).unwrap(),
        cancelled,
        "close must not certify or advance work"
    );
    assert_eq!(store.replay(&run.id).unwrap(), cancelled);
}
