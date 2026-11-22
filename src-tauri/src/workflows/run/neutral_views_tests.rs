use super::neutral_effects::{EffectCommand, EffectPreparation, EffectQuestion, NeutralEffect};
use super::neutral_execution::NeutralExecutionCommand as Command;
use super::neutral_execution_tests::{fixture, step};
use super::neutral_runtime::NeutralInvocation;
use serde_json::json;

// Catches: a project sees another project's runs, or a one-event page loses visited nodes.
#[test]
fn neutral_run_views_keep_project_scope_and_replay_progress_beyond_the_event_page() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let (_dir, owner, run, at) = fixture(30, false);
    let run = step(&owner, &run, "route-start", at);
    let run = step(&owner, &run, "prepare", at + 1);
    let view = owner
        .store
        .neutral_inspection_view(&run.run.id, 0, 1)
        .unwrap();
    assert_eq!(view["events"].as_array().unwrap().len(), 1);
    assert_eq!(view["progress"]["node_states"]["start"], "completed");
    assert_eq!(view["progress"]["node_states"]["work"], "current");
    assert_eq!(view["progress"]["publication"]["id"], "execution");
    assert_eq!(
        owner.store.neutral_run_views("/other").unwrap(),
        Vec::<serde_json::Value>::new()
    );
    let runs = owner.store.neutral_run_views("/project").unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["id"], run.run.id);
    assert_eq!(runs[0]["publication"]["revision"], 1);
    assert_eq!(runs[0]["current_node"], "work");
    assert_eq!(runs[0]["waiting_on_approval"], false);
    assert_eq!(runs[0]["started_ms"], at);
    assert_eq!(runs[0]["updated_ms"], at + 1);
}

// Catches: unanswered approvals look running, survive an answer/cancel, or lose their exact revision.
#[test]
fn neutral_run_views_present_pending_approval_and_retract_it_after_an_answer() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let (_dir, owner, run, at) = fixture(30, false);
    let run = step(&owner, &run, "route-start", at);
    let run = step(&owner, &run, "prepare", at + 1);
    let attempt = run.attempt.as_ref().unwrap();
    let effect = NeutralEffect::prepare(
        NeutralInvocation {
            fence: attempt.fence.clone(),
            publication: run.publication.clone(),
            descriptor: run.descriptor(0).unwrap().clone(),
            instance: run.publication.definition.instances[0].clone(),
            inputs: attempt.inputs.clone(),
            subject: attempt.subject.clone(),
        },
        EffectPreparation {
            mutation: false,
            resources: Default::default(),
            model_tools: None,
            approval: Some(EffectQuestion {
                revision: 7,
                title: "Approve the exact work?".into(),
                deadline_ms: run.deadline_ms,
            }),
        },
    )
    .unwrap();
    let key = effect.key.clone();
    let run = owner
        .neutral_command_at(
            &run.run.id,
            "question",
            run.sequence,
            Command::Effect {
                command: EffectCommand::Prepare {
                    effect: Box::new(effect),
                },
            },
            at + 2,
        )
        .unwrap();
    let view = owner
        .store
        .neutral_inspection_view(&run.run.id, 0, 1)
        .unwrap();
    assert_eq!(view["progress"]["waiting_on_approval"], true);
    assert_eq!(view["progress"]["node_states"]["work"], "waiting");
    assert_eq!(view["progress"]["approvals"][0]["question"]["revision"], 7);
    let run = owner
        .neutral_command_at(
            &run.run.id,
            "answer",
            run.sequence,
            Command::Effect {
                command: EffectCommand::Answer {
                    key,
                    revision: 7,
                    approved: true,
                },
            },
            at + 3,
        )
        .unwrap();
    let view = owner
        .store
        .neutral_inspection_view(&run.run.id, 0, 1)
        .unwrap();
    assert_eq!(view["progress"]["waiting_on_approval"], false);
    assert_eq!(view["progress"]["approvals"], json!([]));
    assert_eq!(view["progress"]["node_states"]["work"], "current");
    let run = owner
        .neutral_command_at(&run.run.id, "cancel", run.sequence, Command::Cancel, at + 4)
        .unwrap();
    assert_eq!(
        owner
            .store
            .neutral_inspection_view(&run.run.id, 0, 1)
            .unwrap()["progress"]["node_states"]["work"],
        "stopped"
    );
}

// Catches: corrupt history blanks good runs, old terminal runs grow the list without bound,
// or the list starts replaying every cached ledger on each progress event.
#[test]
fn neutral_run_views_isolate_bad_history_bound_recent_runs_and_read_cached_summaries() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let (_dir, owner, run, at) = fixture(30, false);
    let run = step(&owner, &run, "route-start", at);
    let conn = owner.store.connect().unwrap();
    for index in 0..55 {
        let id = format!("broken-{index}");
        conn.execute(
            "INSERT INTO neutral_workflow_runs VALUES (?1,'/project',?1,'broken',1)",
            [&id],
        )
        .unwrap();
    }
    let rows = owner.store.neutral_run_views("/project").unwrap();
    assert_eq!(rows.len(), 51); // all active plus at most 50 recent terminal/unreadable rows
    assert!(
        rows.iter()
            .any(|row| row["id"] == run.run.id && row["current_node"] == "work")
    );
    assert!(
        rows.iter()
            .any(|row| row["status"]["type"] == "unavailable")
    );
    conn.execute(
        "DELETE FROM neutral_workflow_events WHERE run_id=?1",
        [&run.run.id],
    )
    .unwrap();
    let rows = owner.store.neutral_run_views("/project").unwrap();
    assert!(
        rows.iter()
            .any(|row| row["id"] == run.run.id && row["current_node"] == "work")
    );
    assert!(rows.iter().any(|row| row["id"] == "broken-54"));
    assert!(!rows.iter().any(|row| row["id"] == "broken-0"));
    assert!(
        owner
            .store
            .neutral_inspection_view(&run.run.id, 0, 1)
            .is_err()
    );
}

// Catches: a slow reader/recovery overwrites a newer committed terminal summary.
#[test]
fn neutral_run_views_ignore_a_stale_projection_after_a_terminal_commit() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let (_dir, owner, run, at) = fixture(30, false);
    let old = step(&owner, &run, "route-start", at);
    owner
        .neutral_command_at(&old.run.id, "cancel", old.sequence, Command::Cancel, at + 1)
        .unwrap();
    super::neutral_summaries::record(&owner.store.connect().unwrap(), &old, at).unwrap();
    let rows = owner.store.neutral_run_views("/project").unwrap();
    assert_ne!(rows[0]["status"]["type"], "running");
    assert_eq!(rows[0]["updated_ms"], at + 1);
}
