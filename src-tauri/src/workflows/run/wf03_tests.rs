//! WF03 wire tests also execute against the pre-feature contract for RED evidence.
use super::*;
use serde_json::json;

fn waiting() -> (
    RunStore,
    RunSnapshot,
    String,
    tempfile::TempDir,
    tempfile::TempDir,
    impl Drop,
) {
    waiting_with_artifact(false)
}

fn waiting_with_artifact(
    artifact: bool,
) -> (
    RunStore,
    RunSnapshot,
    String,
    tempfile::TempDir,
    tempfile::TempDir,
    impl Drop,
) {
    let (config, project, plan, story, definition, guard) = super::tests::fixture();
    let store = RunStore::open().unwrap();
    let stories = crate::stories::StoryStore::open().unwrap();
    let before = stories.get_story(&story).unwrap();
    stories
        .update_story(
            &story,
            before.revision,
            crate::stories::StoryPatch {
                file_scope: Some(vec!["first.txt".into()]),
                ..Default::default()
            },
        )
        .unwrap();
    let run = store
        .start_plan(
            project.path().to_str().unwrap(),
            &plan,
            &definition,
            1,
            RunLimits::default(),
        )
        .unwrap();
    let started = store
        .command(
            &run.id,
            "start",
            RunCommand::StartAttempt {
                story_id: story,
                node_id: "implement".into(),
            },
        )
        .unwrap();
    let attempt = started.snapshot.attempts[0].clone();
    if artifact {
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "WF03 Test"],
            vec!["config", "user.email", "wf03@example.invalid"],
            vec!["commit", "-q", "--allow-empty", "-m", "initial"],
        ] {
            crate::git_cli::git_cmd(project.path())
                .args(args)
                .run()
                .unwrap();
        }
        let workspace = config.path().join("question-workspace");
        crate::git_cli::git_cmd(project.path())
            .args([
                "worktree",
                "add",
                "-q",
                "-b",
                "question",
                workspace.to_str().unwrap(),
            ])
            .run()
            .unwrap();
        store
            .command(
                &run.id,
                "workspace",
                RunCommand::AssignWorktree {
                    story_id: attempt.story_id.clone(),
                    path: workspace
                        .canonicalize()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                },
            )
            .unwrap();
    }
    let run = report_agent(&store, &run, &attempt, AttemptOutcome::NeedsInput);
    (store, run, attempt.id, config, project, guard)
}

fn answer(run: &RunSnapshot, attempt: &str, digest: &str, value: &str) -> RunCommand {
    serde_json::from_value(json!({"action":"answer_question", "question_id":format!("input:{attempt}"), "expected_revision":1, "subject_digest":digest, "actor":{"kind":"human","id":"developer"}, "answer":value})).unwrap_or_else(|e| panic!("WF03 answer contract absent at run {}: {e}", run.id))
}

#[test]
fn unanswered_ask_does_not_stop_disjoint_story() {
    // Catches: NeedsInput pauses the whole run and prevents a disjoint story from starting.
    let (store, run, _, _config, _project, _guard) = waiting();
    let story = crate::stories::StoryStore::open()
        .unwrap()
        .create_story(crate::stories::NewStory {
            story_type: None,
            plan_id: run.plan_id.clone(),
            title: "Disjoint delivery".into(),
            criteria: vec!["Delivered".into()],
            priority: 1,
            origin: crate::stories::StoryOrigin::Native,
            file_scope: vec!["other.txt".into()],
        })
        .unwrap();
    let receipt = store
        .command(
            &run.id,
            "disjoint",
            RunCommand::StartAttempt {
                story_id: story.id.clone(),
                node_id: "implement".into(),
            },
        )
        .expect("unanswered question must not block disjoint work");
    assert!(
        receipt
            .snapshot
            .attempts
            .iter()
            .any(|a| a.story_id == story.id && a.state == AttemptState::Running)
    );
}

#[test]
fn old_subject_answer_cannot_resume_new_artifact() {
    // Catches: a stale subject digest is accepted and starts a continuation.
    let (store, run, attempt, _config, _project, _guard) = waiting_with_artifact(true);
    let workspace = run.stories[0].worktree_path.as_ref().unwrap();
    crate::git_cli::git_cmd(std::path::Path::new(workspace))
        .args(["commit", "-q", "--allow-empty", "-m", "new artifact"])
        .run()
        .unwrap();
    let command = current_answer(&run, &attempt, "yes");
    assert!(
        store
            .command_expected(&run.id, "stale", run.sequence, command)
            .is_err()
    );
    assert!(
        store.snapshot(&run.id).unwrap().attempts[0]
            .input_answer
            .is_none()
    );
}

#[test]
fn two_answers_commit_one_branch_transition() {
    // Catches: two answer clients both resume the same generation.
    let (store, run, attempt, _config, _project, _guard) = waiting();
    let wire = serde_json::to_value(&run).unwrap();
    let digest = wire["questions"][0]["subjectDigest"]
        .as_str()
        .unwrap_or("missing");
    let a = answer(&run, &attempt, digest, "yes");
    let b = answer(&run, &attempt, digest, "no");
    std::thread::scope(|scope| {
        let first = scope.spawn(|| store.command_expected(&run.id, "answer-a", run.sequence, a));
        let second = scope.spawn(|| store.command_expected(&run.id, "answer-b", run.sequence, b));
        assert_ne!(
            first.join().unwrap().is_ok(),
            second.join().unwrap().is_ok()
        );
    });
    let after = store.snapshot(&run.id).unwrap();
    assert_eq!(after.sequence, run.sequence + 1);
    assert_eq!(after.attempts[0].generation, 2);
    assert_eq!(
        after.attempts[0].continuation.as_ref().unwrap().delivery,
        ContinuationDelivery::Pending
    );
}

pub(super) fn current_answer(run: &RunSnapshot, attempt: &str, value: &str) -> RunCommand {
    let question = run
        .questions
        .iter()
        .rev()
        .find(|q| q.attempt_id == attempt)
        .unwrap();
    RunCommand::AnswerQuestion {
        question_id: question.id.clone(),
        expected_revision: question.revision,
        subject_digest: question.subject_digest.clone(),
        actor: super::questions::QuestionActor {
            kind: super::questions::QuestionActorKind::Human,
            id: "developer".into(),
        },
        answer: json!(value),
    }
}

fn report_agent(
    store: &RunStore,
    run: &RunSnapshot,
    attempt: &NodeAttempt,
    outcome: AttemptOutcome,
) -> RunSnapshot {
    let peer = format!("worker-{}", attempt.id);
    let reserved = store
        .command(
            &run.id,
            &format!("spawn:{}", attempt.id),
            RunCommand::ReserveEffect {
                key: format!("spawn:{}", attempt.id),
                kind: EffectKind::SpawnAgent,
            },
        )
        .unwrap();
    store
        .bind_agent(
            &run.id,
            &attempt.id,
            AgentBinding {
                acp: None,
                launch_id: None,
                session_id: peer.clone(),
                task_id: None,
                effect_id: reserved.snapshot.effects.last().unwrap().id.clone(),
                prompt_contract_version: 1,
                prompt_sha256: "a".repeat(64),
                audit_preview: "WF03 native report".into(),
            },
        )
        .unwrap();
    store
        .report_bound_agent(
            AttemptReport {
                contract_version: 1,
                run_id: run.id.clone(),
                story_id: attempt.story_id.clone(),
                story_revision: crate::stories::StoryStore::open()
                    .unwrap()
                    .get_story(&attempt.story_id)
                    .unwrap()
                    .revision,
                attempt_id: attempt.id.clone(),
                generation: attempt.generation,
                outcome,
                summary: "Current branch outcome".into(),
                criterion_results: vec![],
                evidence: vec![],
                input_request: (outcome == AttemptOutcome::NeedsInput).then(|| InputRequest {
                    question: "Proceed?".into(),
                    options: vec!["yes".into(), "no".into()],
                    answer_schema: None,
                    deadline: None,
                }),
                review: None,
            },
            &peer,
        )
        .unwrap()
        .snapshot
}

#[test]
fn actual_two_branch_engine_retains_ask_while_other_branch_arrives() {
    // Catches: a suspended critic halts the engine's independent checks activation.
    use super::graph::ActivationState;
    use crate::workflows::{WorkflowKind, WorkflowStore};
    let (config, project, _plan, story, _definition, _guard) = super::tests::fixture();
    let owner = crate::progress::resolve_owning_project(Some(project.path().to_str().unwrap()))
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let definitions = WorkflowStore::open().unwrap();
    let draft = definitions
        .create_draft(
            &owner,
            "Branch input",
            WorkflowKind::Story,
            serde_json::from_value(super::wf02_tests::graph_json()).unwrap(),
        )
        .unwrap();
    let draft = definitions
        .update_checks(
            &draft.id,
            draft.draft_revision,
            vec![crate::workflows::CheckDefinition {
                id: "integrity".into(),
                argv: vec!["git".into(), "fsck".into()],
                timeout_secs: 30,
            }],
        )
        .unwrap();
    let definition = definitions
        .publish(&draft.id, draft.draft_revision)
        .unwrap();
    let store = RunStore::open_at(&config.path().join("engine.sqlite3")).unwrap();
    let run = store
        .start_graph_run(&super::store::GraphStartRequest {
            project: owner,
            target: RunTarget::Story(story.clone()),
            expected_revision: Some(
                crate::stories::StoryStore::open()
                    .unwrap()
                    .get_story(&story)
                    .unwrap()
                    .revision,
            ),
            definition_id: definition.id,
            definition_revision: definition.revision,
            request_id: "two-branches".into(),
            limits: RunLimits::default(),
        })
        .unwrap();
    let run = super::runtime::drive_turn(&store, &run.id).unwrap();
    report_agent(&store, &run, &run.attempts[0], AttemptOutcome::Completed);
    let run = super::runtime::drive_turn(&store, &run.id).unwrap();
    let critic = run.attempts.iter().find(|a| a.node_id == "critic").unwrap();
    let waiting = report_agent(&store, &run, critic, AttemptOutcome::NeedsInput);
    assert_eq!(waiting.status, RunStatus::Running);
    assert!(
        waiting.graph_executions[0]
            .activations
            .iter()
            .any(|a| a.node_id == "critic" && a.state == ActivationState::Suspended)
    );
    let check = waiting.graph_executions[0]
        .activations
        .iter()
        .find(|a| a.node_id == "checks")
        .unwrap();
    let reserved = store
        .command(
            &run.id,
            "checks-notify",
            RunCommand::ReserveEffect {
                key: format!("notify:root:{}", check.id),
                kind: EffectKind::Notify,
            },
        )
        .unwrap();
    store
        .command(
            &run.id,
            "checks-notified",
            RunCommand::MarkEffect {
                effect_id: reserved.snapshot.effects.last().unwrap().id.clone(),
                succeeded: true,
            },
        )
        .unwrap();
    let advanced = super::runtime::drive_turn(&store, &run.id).unwrap();
    let fork = &advanced.graph_executions[0].forks[0];
    assert_eq!(
        fork.branches
            .iter()
            .find(|b| b.id == "checks")
            .unwrap()
            .arrival,
        Some(super::parallel::ArrivalOutcome::Completed)
    );
    assert_eq!(
        fork.branches
            .iter()
            .find(|b| b.id == "critic")
            .unwrap()
            .arrival,
        None
    );
    assert_eq!(
        store
            .question_inbox(&advanced.project)
            .unwrap()
            .unanswered_count,
        1
    );
    assert_eq!(store.replay(&run.id).unwrap(), advanced);
    // Catches: a superseded input attempt retaining NeedsInput and blocking final proof forever.
    let cancelled = store
        .command(
            &run.id,
            "supersede-question",
            RunCommand::Graph {
                transition: super::graph::GraphTransition::SupersedeFork {
                    execution_id: "root".into(),
                    fork_id: fork.id.clone(),
                    generation: fork.generation,
                },
            },
        )
        .unwrap()
        .snapshot;
    assert_eq!(
        store.question_inbox(&run.project).unwrap().unanswered_count,
        0
    );
    assert_eq!(
        cancelled
            .attempts
            .iter()
            .find(|a| a.id == critic.id)
            .unwrap()
            .outcome,
        Some(AttemptOutcome::Interrupted)
    );
    assert!(
        store
            .command(
                &run.id,
                "late-consent",
                current_answer(&waiting, &critic.id, "yes")
            )
            .is_err()
    );
    assert_eq!(store.replay(&run.id).unwrap(), cancelled);
}

#[test]
fn question_inbox_reconnect_deadline_and_explicit_pause_never_consent() {
    // Catches: reconnect losing questions, deadline granting consent, or an answer clearing manual pause.
    use super::questions::*;
    let (store, run, attempt, _config, _project, _guard) = waiting();
    let first = run.questions[0].clone();
    let deadline_ms = super::store::now_ms() + 60_000;
    let request = InputRequest {
        question: "Choose exact scope".into(),
        options: vec![],
        answer_schema: Some(AnswerSchema::Boolean),
        deadline: Some(QuestionDeadline {
            at_ms: deadline_ms,
            outcome: QuestionDeadlineOutcome::Incomplete,
        }),
    };
    let revised = store
        .command(
            &run.id,
            "revise",
            RunCommand::ReviseQuestion {
                question_id: first.id.clone(),
                expected_revision: first.revision,
                request,
                actor: first.requested_by.clone(),
            },
        )
        .unwrap()
        .snapshot;
    assert!(
        store
            .command(
                &run.id,
                "old-revision",
                current_answer(&run, &attempt, "yes")
            )
            .is_err()
    );
    assert!(
        store
            .command(
                &run.id,
                "wrong-schema",
                current_answer(&revised, &attempt, "yes")
            )
            .is_err()
    );
    store
        .command(&run.id, "manual-pause", RunCommand::Pause)
        .unwrap();
    let timer = revised.timers.last().unwrap();
    store
        .command_at(
            &run.id,
            "deadline",
            RunCommand::Timer {
                command: super::timer::TimerCommand::Fire {
                    key: timer.key.clone(),
                    attempt: 1,
                },
            },
            deadline_ms,
        )
        .unwrap();
    let reopened = RunStore::open().unwrap();
    let inbox = reopened.question_inbox(&run.project).unwrap();
    assert_eq!(inbox.unanswered_count, 1);
    assert_eq!(inbox.items[0].question.state, QuestionState::Incomplete);
    let after = reopened.snapshot(&run.id).unwrap();
    assert_eq!(after.status, RunStatus::Paused);
    assert!(after.attempts[0].input_answer.is_none());
    assert!(after.attempts[0].continuation.is_none());
    assert_eq!(reopened.replay(&run.id).unwrap(), after);
    reopened
        .command(&run.id, "cancel", RunCommand::Cancel)
        .unwrap();
    assert_eq!(
        reopened
            .question_inbox(&run.project)
            .unwrap()
            .unanswered_count,
        0
    );
}

#[test]
fn legacy_needs_input_event_retains_run_wide_pause() {
    // Catches: contract-v5 input behavior being applied to an old persisted report.
    let (_store, run, _attempt, _config, _project, _guard) = waiting();
    let mut legacy = run;
    legacy.event_contract_version = 4;
    legacy.questions.clear();
    legacy.attempts[0].state = AttemptState::Running;
    legacy.attempts[0].outcome = None;
    let event = RunEvent {
        sequence: legacy.sequence + 1,
        command_id: "legacy-report".into(),
        command_hash: None,
        at_ms: legacy.started_ms + 1,
        kind: RunEventKind::AttemptReported {
            attempt_id: legacy.attempts[0].id.clone(),
            generation: legacy.attempts[0].generation,
            outcome: AttemptOutcome::NeedsInput,
            report: None,
        },
    };
    let paused = super::reducer::apply_event(Some(legacy), &event).unwrap();
    assert_eq!(paused.status, RunStatus::Paused);
    assert_eq!(paused.questions, [] as [super::questions::Question; 0]);
    assert!(super::reducer::validate_resume(&paused).is_err());
}

#[tokio::test]
async fn question_inbox_http_matches_ipc_service_without_coordinator() {
    // Catches: an HTTP reconnect returning a different or coordinator-owned inbox projection.
    use tower::ServiceExt;
    let (store, run, _attempt, _config, _project, _guard) = waiting();
    let input = json!({"action":"question_inbox"});
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let ipc = run_action_with_events(
        &state,
        &run.project,
        serde_json::from_value(input.clone()).unwrap(),
    )
    .unwrap();
    let app = crate::mcp_http::build_router(state, false, true);
    let mut request = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/workflows/run/action?path={}", run.project))
        .header("host", "127.0.0.1:9876")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(input.to_string()))
        .unwrap();
    request
        .extensions_mut()
        .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            0,
        ))));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let http: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(http, serde_json::to_value(ipc).unwrap());
    assert_eq!(http["value"]["unansweredCount"], 1);
    assert_eq!(store.snapshot(&run.id).unwrap().sequence, run.sequence);
}
