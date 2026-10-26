use super::*;
use crate::stories::{NewPlan, NewStory, StoryCommand, StoryOrigin};

#[test]
fn story_reservation_blocks_second_root() {
    // catches: concurrent plan/direct/manual ownership (28), including changed-payload retries.
    let (config, project, plan, story, template, _guard) = fixture();
    let store = RunStore::open_at(&config.path().join("workflow_runs.sqlite3")).unwrap();
    let published = definition(project.path().to_str().unwrap(), false, true);
    let start = request(
        project.path().to_str().unwrap(),
        &story,
        &published,
        "reservation",
    );
    let (first, retry) = std::thread::scope(|scope| {
        let a = scope.spawn(|| store.start_graph_run(&start));
        let b = scope.spawn(|| store.start_graph_run(&start));
        (a.join().unwrap().unwrap(), b.join().unwrap().unwrap())
    });
    assert_eq!(first.id, retry.id);
    let mut other = start;
    other.request_id = "second-root".into();
    assert!(store.start_graph_run(&other).is_err());
    assert!(
        store
            .start_plan(
                project.path().to_str().unwrap(),
                &plan,
                &template,
                1,
                RunLimits::default()
            )
            .is_err()
    );
    let stories = StoryStore::open().unwrap();
    let revision = stories.get_story(&story).unwrap().revision;
    assert!(
        stories
            .claim(&story, "manual", revision)
            .unwrap_err()
            .contains("reserved")
    );
    assert!(
        stories
            .transition(&story, revision, StoryCommand::StartManual)
            .unwrap_err()
            .contains("reserved")
    );
    store
        .command(&first.id, "release", RunCommand::Cancel)
        .unwrap();
    stories.claim(&story, "manual", revision).unwrap();
    other.expected_revision = Some(stories.get_story(&story).unwrap().revision);
    assert!(
        store
            .start_graph_run(&other)
            .unwrap_err()
            .to_string()
            .contains("manual claim")
    );
}

#[tokio::test]
async fn parallel_plan_runs_share_project_reservations() {
    // catches: one paused run starving another daemon actor (37, B foundation).
    // Dispatch waves and writable effects are added by C/E, not asserted here.
    let (config, project, _plan, story, _template, _guard) = fixture();
    let state = state(config.path());
    WorkflowRuntime::spawn(&state);
    owner_ready(&state).await;
    let project = project.path().to_str().unwrap();
    let stories = StoryStore::open().unwrap();
    let plan = stories
        .create_plan(NewPlan {
            // Match the API and the first plan's canonical project identity on Windows.
            project: canonical_owner(project),
            title: "Independent".into(),
            source: "second.md".into(),
        })
        .unwrap();
    let second = stories
        .create_story(NewStory {
            story_type: None,
            plan_id: plan.id,
            title: "Second".into(),
            criteria: vec!["Done".into()],
            priority: 1,
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    let published = definition(project, false, false);
    let first = state
        .workflow_runtime
        .start_graph(&state, &request(project, &story, &published, "one"))
        .unwrap();
    let mut second_request = request(project, &second.id, &published, "two");
    second_request.limits.max_duration_secs = 1;
    let mut hints = state.event_bus.subscribe();
    let second = state
        .workflow_runtime
        .start_graph(&state, &second_request)
        .unwrap();
    run_action_with_events(
        &state,
        project,
        RunAction::Command {
            run_id: first.id.clone(),
            command_id: "pause-one".into(),
            expected_sequence: first.sequence,
            command: Box::new(RunCommand::Pause),
        },
    )
    .unwrap();
    let store = RunStore::open().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            hints.recv().await.unwrap();
            if store.snapshot(&second.id).unwrap().status == RunStatus::Paused {
                break;
            }
        }
    })
    .await
    .expect("a paused run starved an independent run's timer");
    let first = store.snapshot(&first.id).unwrap();
    let second = store.snapshot(&second.id).unwrap();
    assert_eq!(first.status, RunStatus::Paused);
    assert!(
        store
            .events_after(&second.id, 0, 20)
            .unwrap()
            .iter()
            .any(|event| matches!(event.kind, RunEventKind::DeadlineExpired { .. }))
    );
    run_action_with_events(
        &state,
        project,
        RunAction::Command {
            run_id: first.id.clone(),
            command_id: "cancel-one".into(),
            expected_sequence: first.sequence,
            command: Box::new(RunCommand::Cancel),
        },
    )
    .unwrap();
    assert_eq!(store.snapshot(&second.id).unwrap(), second);
    assert_eq!(store.replay(&second.id).unwrap(), second);
}

#[tokio::test]
async fn unsupported_graph_start_and_non_owner_mutations_are_refused() {
    // catches: executing an unsupported story Coordinator, or a second process executing a graph.
    let (config, project, plan, story, template, _guard) = fixture();
    let state = state(config.path());
    let error = run_action_with_events(
        &state,
        project.path().to_str().unwrap(),
        RunAction::StartPlan {
            plan_id: plan,
            definition_id: template,
            definition_revision: 1,
            limits: RunLimits::default(),
        },
    )
    .unwrap_err();
    assert!(error.contains("executor unavailable"));
    WorkflowRuntime::spawn(&state);
    owner_ready(&state).await;
    let published = definition(project.path().to_str().unwrap(), false, true);
    let definitions = WorkflowStore::open().unwrap();
    let draft = definitions.get_draft(&published.id).unwrap();
    let mut graph = draft.graph;
    for node in &mut graph.nodes {
        if let NodeKind::Agent { role, .. } = &mut node.kind {
            *role = AgentRole::Coordinator;
        }
    }
    let draft = definitions
        .update_draft(&draft.id, draft.draft_revision, graph)
        .unwrap();
    let published = definitions
        .publish(&draft.id, draft.draft_revision)
        .unwrap();
    let mut start = request(
        project.path().to_str().unwrap(),
        &story,
        &published,
        "unsupported",
    );
    assert!(
        state
            .workflow_runtime
            .start_graph(&state, &start)
            .unwrap_err()
            .contains("not executable")
    );
    start.definition_revision += 1;
    assert!(
        state.workflow_runtime.start_graph(&state, &start).is_err(),
        "an unpublished revision cannot start"
    );
}

fn executable_plan(project: &str, template: &str) {
    let story = definition(project, false, true);
    let published = super::semantics::plan_dispatch::plan_definition(project, template, &story);
    assert_eq!(published.revision, 2);
}

fn plan_request(project: &str, plan: &str, template: &str, key: &str) -> GraphStartRequest {
    GraphStartRequest {
        project: project.into(),
        target: RunTarget::Plan(plan.into()),
        expected_revision: None,
        definition_id: template.into(),
        definition_revision: 2,
        request_id: key.into(),
        limits: RunLimits::default(),
    }
}

/// Catches: the SQLite UNIQUE message leaking instead of the active run identity,
/// either start path ignoring paused runs, or refusal modifying the existing ledger.
#[test]
fn duplicate_plan_start_reports_active_run_without_mutating_it() {
    let (config, project, plan, _story, template, _guard) = fixture();
    let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
    let project = project.path().to_str().unwrap();
    executable_plan(project, &template);
    for graph_first in [false, true] {
        let start = plan_request(
            project,
            &plan,
            &template,
            if graph_first { "graph" } else { "plan" },
        );
        let mut first = if graph_first {
            store.start_graph_run(&start).unwrap()
        } else {
            store
                .start_plan(project, &plan, &template, 2, RunLimits::default())
                .unwrap()
        };
        for paused in [false, true] {
            if paused {
                first = store
                    .command(&first.id, "pause", RunCommand::Pause)
                    .unwrap()
                    .snapshot;
            }
            let events = store.events_after(&first.id, 0, 100).unwrap();
            let expected = RunStartError::AlreadyActive {
                plan_id: plan.clone(),
                active_run_id: first.id.clone(),
            };
            assert_eq!(
                store
                    .start_plan(project, &plan, &template, 2, RunLimits::default())
                    .unwrap_err(),
                expected,
            );
            assert_eq!(
                store
                    .start_graph_run(&plan_request(project, &plan, &template, "duplicate"))
                    .unwrap_err(),
                expected,
            );
            assert_eq!(
                expected.to_string(),
                format!("workflow run already active for plan {plan}: {}", first.id)
            );
            if graph_first {
                assert_eq!(
                    store.start_graph_run(&start).unwrap(),
                    first,
                    "a same-request retry is not a second start"
                );
            }
            assert_eq!(store.snapshot(&first.id).unwrap(), first);
            assert_eq!(store.replay(&first.id).unwrap(), first);
            assert_eq!(store.events_after(&first.id, 0, 100).unwrap(), events);
            assert_eq!(
                store.list_plan_runs(&first.project, &plan, 100).unwrap()[0],
                first
            );
        }
        store
            .command(&first.id, "release", RunCommand::Cancel)
            .unwrap();
    }
    assert_eq!(
        store
            .list_plan_runs(&canonical_owner(project), &plan, 100)
            .unwrap()
            .len(),
        2
    );
}

/// Catches: distinct concurrent start requests bypassing atomic admission or
/// reporting a candidate id instead of the winning persisted run id.
#[test]
fn duplicate_plan_start_concurrent_requests_name_the_winner() {
    let (config, project, plan, _story, template, _guard) = fixture();
    let store = RunStore::open_at(&config.path().join("runs.sqlite3")).unwrap();
    let project = project.path().to_str().unwrap();
    executable_plan(project, &template);
    let a = plan_request(project, &plan, &template, "a");
    let b = plan_request(project, &plan, &template, "b");
    let (a, b) = std::thread::scope(|scope| {
        let a = scope.spawn(|| store.start_graph_run(&a));
        let b = scope.spawn(|| store.start_graph_run(&b));
        (a.join().unwrap(), b.join().unwrap())
    });
    let (winner, refusal) = match (a, b) {
        (Ok(run), Err(error)) | (Err(error), Ok(run)) => (run, error),
        other => panic!("expected one winner and one refusal: {other:?}"),
    };
    assert_eq!(
        refusal,
        RunStartError::AlreadyActive {
            plan_id: plan.clone(),
            active_run_id: winner.id.clone()
        }
    );
    assert_eq!(
        store.list_plan_runs(&winner.project, &plan, 100).unwrap(),
        vec![winner.clone()]
    );
    assert_eq!(store.replay(&winner.id).unwrap(), winner);
}

/// Catches: ready stories reserved by a running or paused graph accepting amendments.
#[test]
fn update_story_refuses_running_and_paused_workflow_reservations() {
    let (_config, project, _plan, story_id, _template, _guard) = fixture();
    let store = RunStore::open().unwrap();
    let published = definition(project.path().to_str().unwrap(), false, true);
    let run = store
        .start_graph_run(&request(
            project.path().to_str().unwrap(),
            &story_id,
            &published,
            "amendment-reservation",
        ))
        .unwrap();
    let stories = StoryStore::open().unwrap();
    let original = stories.get_story(&story_id).unwrap();
    for paused in [false, true] {
        if paused {
            store
                .command(&run.id, "pause-amendment", RunCommand::Pause)
                .unwrap();
        }
        let error = stories
            .update_story(
                &story_id,
                original.revision,
                crate::stories::StoryPatch {
                    priority: Some(3),
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert!(error.contains("reserved"), "{error}");
        assert_eq!(stories.get_story(&story_id).unwrap(), original);
    }
    store
        .command(&run.id, "cancel-amendment", RunCommand::Cancel)
        .unwrap();
    assert_eq!(
        stories
            .update_story(
                &story_id,
                original.revision,
                crate::stories::StoryPatch {
                    priority: Some(3),
                    ..Default::default()
                }
            )
            .unwrap()
            .priority,
        3
    );
}
