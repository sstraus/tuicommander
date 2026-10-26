use super::*;
use crate::workflows::{AgentBinding, AttemptReport, InputRequest, RunLimits};

fn run() -> RunSnapshot {
    RunSnapshot {
        event_contract_version: 1,
        id: "run-1".into(),
        project: "/repo".into(),
        canonical_ref: None,
        plan_id: "plan-1".into(),
        root_target: None,
        definition_id: "plan-flow".into(),
        definition_revision: 1,
        story_definition_id: "story-flow".into(),
        story_definition_revision: 1,
        status: RunStatus::Running,
        sequence: 1,
        started_ms: 0,
        paused_since_ms: None,
        paused_duration_ms: 0,
        pause_cause: None,
        limits: RunLimits::default(),
        loops: 0,
        story_creations: 0,
        spawns: 0,
        planning_fingerprint: None,
        verification_fingerprint: None,
        stories: vec![],
        canonical_recertification: None,
        attempts: vec![NodeAttempt {
            id: "attempt-1".into(),
            story_id: "story-1".into(),
            node_id: "implement".into(),
            generation: 1,
            state: AttemptState::Running,
            outcome: None,
            input_answer: None,
            report: None,
            agent: Some(AgentBinding {
                session_id: "session-1".into(),
                task_id: None,
                effect_id: "effect-1".into(),
                prompt_contract_version: 1,
                prompt_sha256: "digest".into(),
                audit_preview: "preview".into(),
            }),
        }],
        effects: vec![],
        graph_executions: vec![],
    }
}

#[test]
fn incident_reports_keep_cause_and_identity_instead_of_only_transition_labels() {
    let mut run = run();
    run.attempts[0].state = AttemptState::Reported;
    run.attempts[0].outcome = Some(AttemptOutcome::Failed);
    run.attempts[0].report = Some(AttemptReport {
        contract_version: 1,
        run_id: run.id.clone(),
        story_id: "story-1".into(),
        story_revision: 1,
        attempt_id: "attempt-1".into(),
        generation: 1,
        outcome: AttemptOutcome::Failed,
        summary: "Compiler rejected the generated API".into(),
        criterion_results: vec![],
        evidence: vec![],
        input_request: None,
        review: None,
    });
    let entries = project_incidents(&run, None);
    assert_eq!(entries[0].cause, "Compiler rejected the generated API");
    assert_eq!(entries[0].run_id, "run-1");
    assert_eq!(entries[0].story_id.as_deref(), Some("story-1"));
    assert_eq!(entries[0].attempt_id.as_deref(), Some("attempt-1"));
    assert_eq!(entries[0].session_id.as_deref(), Some("session-1"));
    assert!(entries[0].next_action.contains("manually"));
    run.attempts[0].outcome = Some(AttemptOutcome::NeedsInput);
    run.attempts[0].report.as_mut().unwrap().input_request = Some(InputRequest {
        question: "Which API version?".into(),
        options: vec![],
    });
    assert_eq!(project_incidents(&run, None)[0].cause, "Which API version?");
    run.attempts[0].input_answer = Some("v2".into());
    assert_eq!(
        project_incidents(&run, None),
        [] as [crate::workflows::run::incidents::RunIncident; 0]
    );
    run.attempts[0].outcome = Some(AttemptOutcome::Interrupted);
    assert_eq!(
        project_incidents(&run, None)[0].source,
        "attempt_interrupted"
    );
}

#[test]
fn incident_live_evidence_does_not_leak_other_tasks_or_retry_pending_prompts() {
    use crate::tasks::{TaskKind, TaskUpdate};
    let state = crate::state::tests_support::make_test_app_state();
    let mut run = run();
    let task_id = state
        .tasks
        .create(TaskKind::AgentSpawn, "parent", Some("session-1"));
    run.attempts[0].agent.as_mut().unwrap().task_id = Some(task_id.clone());
    state
        .tasks
        .set_status(
            &task_id,
            TaskStatus::Failed,
            TaskUpdate {
                error: Some("agent exited 7".into()),
                ..TaskUpdate::default()
            },
        )
        .unwrap();
    state.session_maps.exit_codes.insert("session-1".into(), 7);
    state.pending_initial_prompts.insert(
        "session-1".into(),
        crate::state::PendingInitialPrompt {
            prompt: "private assignment".into(),
            notified: true,
        },
    );
    let entries = project_incidents(&run, Some(&state));
    assert_eq!(
        entries
            .iter()
            .map(|item| item.source.as_str())
            .collect::<Vec<_>>(),
        vec!["prompt_delivery_failed", "session_exit", "task_record"]
    );
    assert!(
        entries
            .iter()
            .all(|item| !item.cause.contains("private assignment"))
    );
    assert_eq!(
        state
            .pending_initial_prompts
            .get("session-1")
            .unwrap()
            .prompt,
        "private assignment"
    );
    assert_eq!(
        state.tasks.get(&task_id).unwrap().status,
        TaskStatus::Failed
    );
    let input_task = state
        .tasks
        .create(TaskKind::AgentSpawn, "parent", Some("session-1"));
    state
        .tasks
        .set_status(
            &input_task,
            TaskStatus::InputRequired,
            TaskUpdate {
                status_message: Some("Approval needed".into()),
                ..TaskUpdate::default()
            },
        )
        .unwrap();
    run.attempts[0].agent.as_mut().unwrap().task_id = Some(input_task);
    assert!(
        project_incidents(&run, Some(&state))
            .iter()
            .any(|item| item.cause == "Approval needed")
    );
    state
        .pending_initial_prompts
        .get_mut("session-1")
        .unwrap()
        .notified = false;
    assert!(
        !project_incidents(&run, Some(&state))
            .iter()
            .any(|item| item.source == "prompt_delivery_failed")
    );
    run.attempts[0].state = AttemptState::Interrupted;
    run.attempts[0].outcome = Some(AttemptOutcome::Interrupted);
    assert!(
        project_incidents(&run, Some(&state))
            .iter()
            .any(|item| item.source == "session_exit")
    );
    run.attempts[0].state = AttemptState::Running;
    run.attempts[0].outcome = None;
    run.attempts[0].agent.as_mut().unwrap().session_id = "other-session".into();
    assert_eq!(
        project_incidents(&run, Some(&state)),
        [] as [crate::workflows::run::incidents::RunIncident; 0]
    );
    run.attempts[0].agent.as_mut().unwrap().session_id = "session-1".into();
    run.attempts[0].state = AttemptState::Reported;
    run.attempts[0].outcome = Some(AttemptOutcome::Completed);
    assert_eq!(
        project_incidents(&run, Some(&state)),
        [] as [crate::workflows::run::incidents::RunIncident; 0]
    );
}

#[test]
fn incident_working_sessions_are_not_inferred_stuck_and_paused_cause_is_honest() {
    let state = crate::state::tests_support::make_test_app_state();
    let mut run = run();
    let mut session = crate::state::SessionState {
        agent_state: Some("working".into()),
        ..Default::default()
    };
    state
        .session_maps
        .session_states
        .insert("session-1".into(), session.clone());
    assert_eq!(
        project_incidents(&run, Some(&state)),
        [] as [crate::workflows::run::incidents::RunIncident; 0]
    );
    session.agent_state = Some("awaiting_input".into());
    state
        .session_maps
        .session_states
        .insert("session-1".into(), session);
    assert_eq!(
        project_incidents(&run, Some(&state))[0].source,
        "state_change"
    );
    state.session_maps.session_states.clear();
    run.status = RunStatus::Paused;
    assert!(
        project_incidents(&run, Some(&state))[0]
            .cause
            .contains("no more specific cause")
    );
}

/// Catches: completed agent attempts hiding a story that still waits for human QA.
#[test]
fn qa_story_incident_survives_completed_attempt_and_clears_after_approval() {
    use crate::stories::{NewPlan, NewStory, StoryCommand, StoryOrigin, StoryStore};
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).unwrap();
    let plan = store
        .create_plan(NewPlan {
            project: "/repo".into(),
            title: "QA".into(),
            source: "plan.md".into(),
        })
        .unwrap();
    let story = store
        .create_story(NewStory {
            story_type: None,
            plan_id: plan.id.clone(),
            title: "Device".into(),
            criteria: vec!["[MANUAL] Device check".into()],
            priority: 1,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    let qa = store
        .transition(&story.id, story.revision, StoryCommand::StartManual)
        .unwrap();
    let mut run = run();
    run.plan_id = plan.id.clone();
    run.attempts[0].story_id = story.id.clone();
    run.attempts[0].outcome = Some(AttemptOutcome::Completed);
    run.attempts[0].state = AttemptState::Reported;
    let entries = project_story_incidents(&run, None, &store.list_stories(&plan.id).unwrap());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source, "story_qa");
    assert_eq!(entries[0].story_id.as_deref(), Some(story.id.as_str()));
    assert_eq!(entries[0].attempt_id.as_deref(), Some("attempt-1"));
    assert!(entries[0].next_action.contains("manual checks"));
    let passed = store
        .transition(&qa.id, qa.revision, StoryCommand::CheckCriterion(0))
        .unwrap();
    store
        .transition(&passed.id, passed.revision, StoryCommand::Approve)
        .unwrap();
    assert_eq!(
        project_story_incidents(&run, None, &store.list_stories(&plan.id).unwrap()),
        Vec::<RunIncident>::new()
    );
}
