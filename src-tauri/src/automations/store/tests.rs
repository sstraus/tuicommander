use super::*;
use crate::automations::model::AutomationDefinition;
use crate::automations::run::RunTrigger;

fn definition() -> AutomationDefinition {
    serde_json::from_value(serde_json::json!({
        "id":"daily", "name":"Review", "prompt":"Review changes", "run_config":"codex",
        "repository":"/project", "workspace":{"mode":"existing"}, "cron":"0 9 * * *",
        "timezone":"Europe/Madrid", "enabled":true, "grace_secs":43200,
        "overlap":"skip", "max_duration_secs":3600, "precheck":null
    }))
    .unwrap()
}

// Catches: duplicate dispatch of a scheduled occurrence after a restart.
#[test]
fn durable_occurrence_reservation_survives_reopen_and_rejects_duplicate_dispatch() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let path = dir.path().join("automation_runs.sqlite3");
    let owner = RunOwner::acquire_at(&path, 100).unwrap();
    let first = owner
        .store()
        .reserve(
            &definition(),
            RunTrigger::Scheduled { occurrence_ms: 100 },
            100,
        )
        .unwrap()
        .unwrap();
    drop(owner);
    let owner = RunOwner::acquire_at(&path, 200).unwrap();
    assert_eq!(
        owner.store().get(&first.id).unwrap().status,
        RunStatus::Interrupted
    );
    assert!(
        owner
            .store()
            .reserve(
                &definition(),
                RunTrigger::Scheduled { occurrence_ms: 100 },
                200
            )
            .unwrap()
            .is_none()
    );
}

fn owner() -> (tempfile::TempDir, RunOwner) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let owner = RunOwner::acquire_at(&dir.path().join("automation_runs.sqlite3"), 0).unwrap();
    (dir, owner)
}
fn manual(store: &RunStore, at: i64) -> AutomationRun {
    store
        .reserve(&definition(), RunTrigger::Manual, at)
        .unwrap()
        .unwrap()
}

// Catches: desktop and remote both dispatching, or reader access interrupting live runs.
#[test]
fn owner_lock_blocks_competing_runtime_and_readers_cannot_mutate_live_runs() {
    let (dir, owner) = owner();
    let path = dir.path().join("automation_runs.sqlite3");
    let run = manual(owner.store(), 1);
    assert!(RunOwner::acquire_at(&path, 2).is_err());
    let reader = RunStore::open_at(&path).unwrap();
    assert_eq!(reader.get(&run.id).unwrap().status, RunStatus::Reserved);
    assert!(
        reader
            .reserve(&definition(), RunTrigger::Manual, 2)
            .is_err()
    );
    assert!(
        reader
            .transition(&run.id, RunStatus::Completed, RunDetails::default(), 2)
            .is_err()
    );
    assert!(reader.prune(100, None).is_err());
    assert!(
        reader
            .claim_notification(&run.id, "done", "native", 2)
            .is_err()
    );
    let writer = owner.store().clone();
    drop(owner);
    assert!(RunOwner::acquire_at(&path, 3).is_err());
    drop(writer);
    assert!(RunOwner::acquire_at(&path, 3).is_ok());
}

// Catches: check-then-insert races duplicating scheduled runs or deduplicating manual runs.
#[test]
fn concurrent_reservations_claim_one_occurrence_and_manual_runs_remain_distinct() {
    let (_dir, owner) = owner();
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let store = owner.store().clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store
                    .reserve(
                        &definition(),
                        RunTrigger::Scheduled { occurrence_ms: 10 },
                        10,
                    )
                    .unwrap()
            })
        })
        .collect();
    barrier.wait();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_some()).count(), 1);
    assert_ne!(manual(owner.store(), 20).id, manual(owner.store(), 20).id);
    let mut other = definition();
    other.id = "other".into();
    assert!(
        owner
            .store()
            .reserve(&other, RunTrigger::Scheduled { occurrence_ms: 10 }, 10)
            .unwrap()
            .is_some()
    );
}

// Catches: boot overlooking needs-you/precheck runs or overwriting confirmed final outcomes.
#[test]
fn restart_interrupts_every_open_state_and_keeps_all_final_states_immutable() {
    let (dir, owner) = owner();
    let mut open = vec![];
    for status in [
        RunStatus::Reserved,
        RunStatus::Prechecking,
        RunStatus::Running,
        RunStatus::NeedsYou,
    ] {
        let run = manual(owner.store(), 1);
        owner
            .store()
            .transition(&run.id, status, RunDetails::default(), 2)
            .unwrap();
        open.push(run.id);
    }
    let mut final_runs = vec![];
    for status in [
        RunStatus::Completed,
        RunStatus::Failed,
        RunStatus::Unknown,
        RunStatus::TimedOut,
        RunStatus::Interrupted,
        RunStatus::SkippedPrecheck,
        RunStatus::SkippedOverlap,
        RunStatus::SkippedConcurrency,
        RunStatus::SkippedExpired,
        RunStatus::SkippedMissed,
    ] {
        let run = manual(owner.store(), 1);
        let finished = owner
            .store()
            .transition(
                &run.id,
                status,
                RunDetails {
                    reason: Some("original".into()),
                    ..Default::default()
                },
                2,
            )
            .unwrap();
        assert_eq!(
            owner
                .store()
                .transition(
                    &run.id,
                    RunStatus::Completed,
                    RunDetails {
                        reason: Some("overwrite".into()),
                        ..Default::default()
                    },
                    3
                )
                .unwrap(),
            finished
        );
        final_runs.push(finished);
    }
    assert_eq!(owner.store().open_runs().unwrap().len(), 4);
    drop(owner);
    let owner = RunOwner::acquire_at(&dir.path().join("automation_runs.sqlite3"), 10).unwrap();
    for id in open {
        let run = owner.store().get(&id).unwrap();
        assert_eq!(run.status, RunStatus::Interrupted);
        assert_eq!(run.finished_ms, Some(10));
    }
    assert!(owner.store().open_runs().unwrap().is_empty());
    for run in final_runs {
        assert_eq!(owner.store().get(&run.id).unwrap(), run);
    }
}

// Catches: unbounded Unicode output, lost launch pointers or definition deletion losing history.
#[test]
fn saved_reports_bound_each_stream_and_keep_definition_snapshot_and_launch_pointers() {
    let (dir, owner) = owner();
    let definitions = crate::automations::definitions::DefinitionStore::at_path(
        dir.path().join("automations.json"),
    );
    definitions.create(definition()).unwrap();
    let run = manual(owner.store(), 1);
    let text = format!("{}日", "a".repeat(OUTPUT_LIMIT - 1));
    let precheck = SavedPrecheck {
        exit_code: Some(0),
        timed_out: false,
        stdout: SavedOutput {
            text: text.clone(),
            truncated: false,
        },
        stderr: SavedOutput {
            text: "ok".into(),
            truncated: true,
        },
    };
    let expected = owner
        .store()
        .transition(
            &run.id,
            RunStatus::Completed,
            RunDetails {
                task_id: Some("task-1".into()),
                session_id: Some("session-1".into()),
                workspace: Some("/worktree".into()),
                stdout: Some(text.clone()),
                stderr: Some(text),
                precheck: Some(precheck),
                precheck_outcome: Some(crate::automations::precheck::PrecheckOutcome::Executed(
                    crate::automations::precheck::PrecheckResult {
                        termination: crate::automations::precheck::Termination::Exited(Some(0)),
                        stdout: "z".repeat(OUTPUT_LIMIT + 1),
                        stderr: "日".repeat(OUTPUT_LIMIT),
                        stdout_truncated: false,
                        stderr_truncated: false,
                        duration_ms: 12,
                    },
                )),
                workspace_id: Some("allocated-workspace-id".into()),
                reason: Some("confirmed done".into()),
            },
            2,
        )
        .unwrap();
    definitions.remove("daily").unwrap();
    let actual = RunStore::open_at(&dir.path().join("automation_runs.sqlite3"))
        .unwrap()
        .get(&run.id)
        .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual.definition, definition());
    assert_eq!(actual.task_id.as_deref(), Some("task-1"));
    assert_eq!(actual.session_id.as_deref(), Some("session-1"));
    assert_eq!(actual.workspace.as_deref(), Some("/worktree"));
    assert_eq!(
        actual.workspace_id.as_deref(),
        Some("allocated-workspace-id")
    );
    let Some(crate::automations::precheck::PrecheckOutcome::Executed(result)) =
        actual.precheck_outcome
    else {
        panic!("missing full precheck outcome")
    };
    assert_eq!(result.stdout.len(), OUTPUT_LIMIT);
    assert_eq!(result.stderr.len(), OUTPUT_LIMIT - 1);
    assert!(result.stdout_truncated && result.stderr_truncated);
    assert_eq!(result.duration_ms, 12);
    assert_eq!(actual.stdout.text.len(), OUTPUT_LIMIT - 1);
    assert!(actual.stdout.truncated && actual.stderr.truncated);
    let precheck = actual.precheck.unwrap();
    assert_eq!(precheck.stdout.text.len(), OUTPUT_LIMIT - 1);
    assert!(precheck.stdout.truncated && precheck.stderr.truncated);
}

// Catches: local-calendar windows, future rows counted, unstable page ordering or pruning live work.
#[test]
fn history_pages_utc_windows_and_retention_preserve_open_runs() {
    let (_dir, owner) = owner();
    let store = owner.store();
    let now = 100 * SummaryWindow::Day.millis();
    let old_open = manual(store, 0);
    let old_final = manual(store, 0);
    store
        .transition(&old_final.id, RunStatus::Failed, RunDetails::default(), 1)
        .unwrap();
    let boundary = manual(store, now - SummaryWindow::Day.millis());
    store
        .transition(
            &boundary.id,
            RunStatus::Completed,
            RunDetails::default(),
            now,
        )
        .unwrap();
    manual(store, now - SummaryWindow::Week.millis());
    manual(store, now - SummaryWindow::Day.millis() - 1);
    let latest = manual(store, now);
    let future = manual(store, now + 1);
    let day = store.summary(SummaryWindow::Day, now).unwrap();
    assert_eq!(day.total, 2);
    assert_eq!(day.by_status[&RunStatus::Completed], 1);
    assert_eq!(store.summary(SummaryWindow::Week, now).unwrap().total, 4);
    assert_eq!(store.history(Some("daily"), 1, 0).unwrap()[0].id, future.id);
    assert_eq!(store.history(Some("daily"), 1, 1).unwrap()[0].id, latest.id);
    assert!(store.history(Some("deleted-id"), 10, 0).unwrap().is_empty());
    assert!(store.history(None, 0, 0).is_err());
    assert!(store.history(None, 101, 0).is_err());
    assert!(store.prune(now, Some(0)).is_err());
    assert_eq!(store.prune(now, None).unwrap(), 1);
    assert!(store.get(&old_final.id).is_err());
    assert!(store.get(&old_open.id).is_ok());
    assert!(store.get(&boundary.id).is_ok());
}

// Catches: schema downgrade or opportunistic replacement of an unknown populated database.
#[test]
fn unsupported_schemas_fail_closed_without_modifying_existing_database() {
    for version in [2, -1, 0] {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join("automation_runs.sqlite3");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE future_data(secret TEXT); INSERT INTO future_data VALUES('preserve')",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", version).unwrap();
        drop(conn);
        let before = std::fs::read(&path).unwrap();
        assert!(RunOwner::acquire_at(&path, 1).is_err());
        assert!(RunStore::open_at(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

// Catches: notification ambiguity replaying sends or changing successful execution status.
#[test]
fn notifications_deduplicate_independently_and_restart_marks_ambiguous_delivery_unknown() {
    let (dir, owner) = owner();
    let run = manual(owner.store(), 1);
    let completed = owner
        .store()
        .transition(
            &run.id,
            RunStatus::Completed,
            RunDetails {
                stdout: Some("canonical report".into()),
                ..Default::default()
            },
            2,
        )
        .unwrap();
    assert!(
        owner
            .store()
            .claim_notification(&run.id, "completed", "native", 3)
            .unwrap()
    );
    assert!(
        !owner
            .store()
            .claim_notification(&run.id, "completed", "native", 3)
            .unwrap()
    );
    assert!(
        owner
            .store()
            .claim_notification(&run.id, "completed", "push", 3)
            .unwrap()
    );
    assert!(
        owner
            .store()
            .claim_notification("missing", "completed", "native", 3)
            .is_err()
    );
    owner
        .store()
        .settle_notification(&run.id, "completed", "push", DeliveryStatus::Confirmed, 4)
        .unwrap();
    drop(owner);
    let owner = RunOwner::acquire_at(&dir.path().join("automation_runs.sqlite3"), 10).unwrap();
    assert!(
        !owner
            .store()
            .claim_notification(&run.id, "completed", "native", 11)
            .unwrap()
    );
    assert_eq!(
        owner
            .store()
            .notification(&run.id, "completed", "native")
            .unwrap()
            .status,
        DeliveryStatus::Unknown
    );
    assert_eq!(
        owner
            .store()
            .notification(&run.id, "completed", "push")
            .unwrap()
            .status,
        DeliveryStatus::Confirmed
    );
    assert_eq!(
        owner
            .store()
            .settle_notification(
                &run.id,
                "completed",
                "native",
                DeliveryStatus::Confirmed,
                11
            )
            .unwrap()
            .status,
        DeliveryStatus::Unknown
    );
    assert_eq!(owner.store().get(&run.id).unwrap(), completed);
}

// Catches: backwards live transitions discarding dispatch evidence or timestamps.
#[test]
fn live_transitions_reject_backwards_state_and_time_but_needs_you_stays_active() {
    let (_dir, owner) = owner();
    let store = owner.store();
    let run = manual(store, 10);
    store
        .transition(&run.id, RunStatus::Running, RunDetails::default(), 11)
        .unwrap();
    assert!(
        store
            .transition(&run.id, RunStatus::Reserved, RunDetails::default(), 12)
            .is_err()
    );
    assert!(
        store
            .transition(&run.id, RunStatus::Prechecking, RunDetails::default(), 12)
            .is_err()
    );
    assert!(
        store
            .transition(&run.id, RunStatus::Completed, RunDetails::default(), 9)
            .is_err()
    );
    let paused = store
        .transition(&run.id, RunStatus::NeedsYou, RunDetails::default(), 12)
        .unwrap();
    assert_eq!(paused.finished_ms, None);
    assert_eq!(store.open_runs().unwrap().len(), 1);
    assert_eq!(
        store
            .transition(&run.id, RunStatus::Running, RunDetails::default(), 13)
            .unwrap()
            .status,
        RunStatus::Running
    );
}
