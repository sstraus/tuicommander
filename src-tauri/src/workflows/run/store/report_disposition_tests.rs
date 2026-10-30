use super::*;
use crate::workflows::run::tests::fixture;
use crate::workflows::{RunAction, RunReply};

#[test]
fn report_decisions_survive_retry_conflict_and_restart_without_accepting_work() {
    let (config, project, plan_id, story_id, definition_id, _guard) = fixture();
    let path = config.path().join("workflow_runs.sqlite3");
    let store = RunStore::open_at(&path).unwrap();
    let project = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let run = store
        .start_plan(&project, &plan_id, &definition_id, 1, RunLimits::default())
        .unwrap();
    let started = store
        .command(
            &run.id,
            "attempt",
            RunCommand::StartAttempt {
                story_id: story_id.clone(),
                node_id: "implement".into(),
            },
        )
        .unwrap();
    let attempt = &started.snapshot.attempts[0];
    let reserved = store
        .command(
            &run.id,
            "reserve",
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
                launch_id: Some("durable-launch".into()),
                session_id: "worker".into(),
                task_id: None,
                effect_id: reserved.snapshot.effects[0].id.clone(),
                prompt_contract_version: 1,
                prompt_sha256: "a".repeat(64),
                audit_preview: "test".into(),
            },
        )
        .unwrap();
    let story = StoryStore::open().unwrap().get_story(&story_id).unwrap();
    let report = AttemptReport {
        contract_version: 1,
        run_id: run.id.clone(),
        story_id: story_id.clone(),
        story_revision: story.revision,
        attempt_id: attempt.id.clone(),
        generation: attempt.generation,
        outcome: AttemptOutcome::Completed,
        summary: "Implementation result, not approval".into(),
        criterion_results: vec![CriterionResult {
            index: 0,
            satisfied: true,
            evidence: "targeted test".into(),
        }],
        evidence: vec!["test receipt".into()],
        input_request: None,
        review: None,
    };
    let before = store.snapshot(&run.id).unwrap().sequence;
    // Catches a report event committed despite a failed disposition write.
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TRIGGER refuse_decision BEFORE INSERT ON workflow_report_dispositions BEGIN SELECT RAISE(ABORT, 'decision unavailable'); END;").unwrap();
    assert!(
        store
            .report_bound_agent(report.clone(), "worker")
            .unwrap_err()
            .contains("decision unavailable")
    );
    assert_eq!(store.snapshot(&run.id).unwrap().sequence, before);
    assert!(
        store.snapshot(&run.id).unwrap().attempts[0]
            .report
            .is_none()
    );
    conn.execute_batch("DROP TRIGGER refuse_decision").unwrap();

    // Catches lost refusals and accidental acceptance of a different caller.
    let refusal = store
        .report_bound_agent(report.clone(), "other-worker")
        .unwrap_err();
    assert_eq!(store.snapshot(&run.id).unwrap().sequence, before);
    let accepted = store.report_bound_agent(report.clone(), "worker").unwrap();
    let decisions = store.report_dispositions(&run.id).unwrap();
    assert_eq!(decisions.len(), 2);
    assert_eq!(decisions[0].decision, ReportDecision::Refused);
    assert_eq!(decisions[0].reason.as_deref(), Some(refusal.as_str()));
    assert_eq!(decisions[0].caller_session, "other-worker");
    assert_eq!(decisions[1].decision, ReportDecision::Accepted);
    assert_eq!(decisions[1].sequence, Some(accepted.sequence));
    assert_eq!(decisions[1].attempt_id, attempt.id);
    assert_eq!(decisions[1].generation, attempt.generation);
    assert_eq!(decisions[1].caller_session, "worker");
    // Catches exposing a journal helper without wiring the shared run adapter.
    let reply = crate::workflows::run_action(
        &project,
        RunAction::ReportDispositions {
            run_id: run.id.clone(),
        },
    )
    .unwrap();
    let RunReply::ReportDispositions(exposed) = reply else {
        panic!("report decision reply");
    };
    assert_eq!(exposed.len(), 2);
    assert_eq!(exposed[1].sequence, Some(accepted.sequence));
    assert_ne!(
        decisions[0].submission_sha256,
        decisions[1].submission_sha256
    );
    assert_ne!(
        StoryStore::open()
            .unwrap()
            .get_story(&story_id)
            .unwrap()
            .status,
        StoryStatus::Done
    );

    // Catches a conflicting payload overwriting the accepted decision and a
    // response-lost retry creating another event or decision after restart.
    let mut conflicting = report.clone();
    conflicting.summary = "Different result".into();
    assert!(store.report_bound_agent(conflicting, "worker").is_err());
    let reopened = RunStore::open_at(&path).unwrap();
    assert_eq!(
        reopened
            .report_bound_agent(report, "worker")
            .unwrap()
            .sequence,
        accepted.sequence
    );
    let retained = reopened.report_dispositions(&run.id).unwrap();
    assert_eq!(retained.len(), 3);
    assert_eq!(&retained[..2], decisions.as_slice());
    assert_eq!(retained[2].decision, ReportDecision::Refused);
    assert_eq!(reopened.replay(&run.id).unwrap(), accepted.snapshot);
}
