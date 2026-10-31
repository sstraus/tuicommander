use super::*;

fn report(store: &RunStore, run: &RunSnapshot, outcome: AttemptOutcome) {
    let attempt = run.attempts.last().unwrap();
    let reserved = store
        .command(
            &run.id,
            "critic-spawn",
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
                session_id: "critic-implementer".into(),
                task_id: None,
                effect_id: reserved.snapshot.effects.last().unwrap().id.clone(),
                prompt_contract_version: 1,
                prompt_sha256: "a".repeat(64),
                audit_preview: "Implement story".into(),
            },
        )
        .unwrap();
    let story = StoryStore::open()
        .unwrap()
        .get_story(&attempt.story_id)
        .unwrap();
    store
        .command(
            &run.id,
            "critic-report",
            RunCommand::ReportBoundAttempt {
                caller_acp: None,
                caller_session: "critic-implementer".into(),
                report: AttemptReport {
                    contract_version: 1,
                    run_id: run.id.clone(),
                    story_id: story.id,
                    story_revision: story.revision,
                    attempt_id: attempt.id.clone(),
                    generation: attempt.generation,
                    outcome,
                    summary: "Implementation has not completed".into(),
                    criterion_results: vec![],
                    evidence: vec![],
                    input_request: (outcome == AttemptOutcome::NeedsInput).then(|| InputRequest {
                        answer_schema: None,
                        deadline: None,
                        question: "Which implementation option should I use?".into(),
                        options: vec![],
                    }),
                    review: None,
                },
            },
        )
        .unwrap();
}

#[test]
fn failed_agent_report_does_not_release_its_successor() {
    // catches: an implementation failure being treated as successful graph work.
    let (config, project, _plan, story, _definition, _guard) = fixture();
    let project = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let published = definition(&project, false, true);
    let store = RunStore::open_at(&config.path().join("critic-runs.sqlite3")).unwrap();
    let run = store
        .start_graph_run(&request(&project, &story, &published, "failed-agent"))
        .unwrap();
    let run = drive_turn(&store, &run.id).unwrap();
    report(&store, &run, AttemptOutcome::Failed);
    let after = drive_turn(&store, &run.id).unwrap();
    assert!(
        !after.graph_executions[0]
            .activations
            .iter()
            .any(|a| a.node_id == "end"),
        "failed implementation released End instead of retaining unresolved work"
    );
    assert_eq!(after.status, RunStatus::Paused);
    // catches: resume reusing a cached pause receipt while durable state stays Running.
    for retry in 0..2 {
        let graph = &after.graph_executions[0];
        store
            .command(
                &run.id,
                &format!("critic-failed-resume:{retry}"),
                RunCommand::ResumeGraph {
                    execution_id: graph.id.clone(),
                    activation_id: graph.activations.last().unwrap().id.clone(),
                    resolution: "Retry without a completed report".into(),
                },
            )
            .unwrap();
        let current = drive_turn(&store, &run.id).unwrap();
        assert_eq!(current.status, RunStatus::Paused);
        assert_eq!(store.snapshot(&run.id).unwrap(), current);
        assert!(
            !current.graph_executions[0]
                .activations
                .iter()
                .any(|a| a.node_id == "end")
        );
    }
    assert_eq!(
        store
            .command(&run.id, "critic-cancel", RunCommand::Cancel)
            .unwrap()
            .snapshot
            .status,
        RunStatus::Cancelled
    );
}

#[test]
fn answered_input_does_not_complete_the_unfinished_agent() {
    // catches: answering a question advancing the graph without completed agent work.
    let (config, project, _plan, story, _definition, _guard) = fixture();
    let project = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let published = definition(&project, false, true);
    let store = RunStore::open_at(&config.path().join("critic-runs.sqlite3")).unwrap();
    let run = store
        .start_graph_run(&request(&project, &story, &published, "input-agent"))
        .unwrap();
    let run = drive_turn(&store, &run.id).unwrap();
    report(&store, &run, AttemptOutcome::NeedsInput);
    store
        .command(
            &run.id,
            "critic-answer",
            super::super::wf03_tests::current_answer(
                &store.snapshot(&run.id).unwrap(),
                &run.attempts.last().unwrap().id.clone(),
                "Use the first option",
            ),
        )
        .unwrap();
    let after = drive_turn(&store, &run.id).unwrap();
    assert!(
        !after.graph_executions[0]
            .activations
            .iter()
            .any(|a| a.node_id == "end"),
        "answered input released End without a completed implementation report"
    );
    assert_eq!(after.status, RunStatus::Running);
    let continued = after.attempts.last().unwrap();
    assert_eq!(continued.generation, 2);
    assert_eq!(
        continued.agent.as_ref().unwrap().session_id,
        "critic-implementer"
    );
    assert_eq!(
        continued.continuation.as_ref().unwrap().delivery,
        ContinuationDelivery::Pending
    );
    assert_eq!(store.snapshot(&run.id).unwrap(), after);
    assert_eq!(store.replay(&run.id).unwrap(), after);
}
