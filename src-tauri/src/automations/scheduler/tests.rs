use super::*;
use crate::automations::{
    model::AutomationDefinition,
    run::{RunDetails, RunStatus},
    store::RunOwner,
};

fn definition(id: &str) -> AutomationDefinition {
    serde_json::from_value(serde_json::json!({
        "id":id,"name":"Review","prompt":"Review changes","run_config":"codex",
        "repository":"/project","workspace":{"mode":"existing"},"cron":"0 * * * *",
        "timezone":"UTC","enabled":true,"grace_secs":1800,"overlap":"skip",
        "max_duration_secs":3600,"precheck":null
    }))
    .unwrap()
}
fn config() -> AutomationsConfig {
    AutomationsConfig {
        definitions: vec![definition("a")],
        ..Default::default()
    }
}
fn owner() -> (tempfile::TempDir, RunOwner) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let owner = RunOwner::acquire_at(&dir.path().join("runs.sqlite3"), 0).unwrap();
    (dir, owner)
}
fn now(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}

// Catches replay backlog, exclusive grace, rollback replay and loss of cursor after pruning/restart.
#[test]
fn latest_only_inclusive_grace_and_durable_cursor_prevent_replay() {
    let (dir, owner) = owner();
    let cfg = config();
    let at = now("2026-10-09T12:30:00Z");
    let runs = tick(owner.store(), &cfg, at).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, RunStatus::Reserved);
    assert_eq!(
        runs[0].trigger,
        RunTrigger::Scheduled {
            occurrence_ms: now("2026-10-09T12:00:00Z").timestamp_millis()
        }
    );
    assert!(tick(owner.store(), &cfg, at).unwrap().is_empty());
    assert!(
        tick(owner.store(), &cfg, now("2026-10-09T11:00:00Z"))
            .unwrap()
            .is_empty()
    );
    owner
        .store()
        .transition(
            &runs[0].id,
            RunStatus::Completed,
            RunDetails::default(),
            at.timestamp_millis(),
        )
        .unwrap();
    owner
        .store()
        .prune(now("2027-10-09T12:00:00Z").timestamp_millis(), Some(1))
        .unwrap();
    drop(owner);
    let owner =
        RunOwner::acquire_at(&dir.path().join("runs.sqlite3"), at.timestamp_millis()).unwrap();
    assert!(tick(owner.store(), &cfg, at).unwrap().is_empty());
    let late = tick(owner.store(), &cfg, now("2026-10-12T12:30:00.001Z")).unwrap();
    assert_eq!(late.len(), 1);
    assert_eq!(late[0].status, RunStatus::SkippedMissed);
    assert!(owner.store().open_runs().unwrap().is_empty());
}

// Catches paused manual runs being refused, overlap ignoring needs-you, cap ignoring pending, or manual advancing cursor.
#[test]
fn manual_and_scheduled_share_capacity_without_sharing_cursor() {
    let (_dir, owner) = owner();
    let store = owner.store();
    let mut cfg = config();
    cfg.definitions[0].enabled = false;
    let at = now("2026-10-09T12:00:00Z");
    assert!(tick(store, &cfg, at).unwrap().is_empty());
    let first = run_now(store, &cfg, "a", at).unwrap();
    assert_eq!(first.status, RunStatus::Reserved);
    store
        .transition(
            &first.id,
            RunStatus::NeedsYou,
            RunDetails::default(),
            at.timestamp_millis(),
        )
        .unwrap();
    assert_eq!(
        run_now(store, &cfg, "a", at).unwrap().status,
        RunStatus::SkippedOverlap
    );
    cfg.definitions.push(definition("b"));
    assert_eq!(
        run_now(store, &cfg, "b", at).unwrap().status,
        RunStatus::Reserved
    );
    cfg.definitions.push(definition("c"));
    assert_eq!(
        run_now(store, &cfg, "c", at).unwrap().status,
        RunStatus::SkippedConcurrency
    );
    cfg.max_concurrent_runs = 1;
    assert_eq!(store.open_runs().unwrap().len(), 2);
    assert_eq!(
        run_now(store, &cfg, "c", at).unwrap().status,
        RunStatus::SkippedConcurrency
    );
    let open = store.open_runs().unwrap();
    assert_eq!(
        open.len(),
        2,
        "lowering capacity must not cancel existing runs"
    );
    assert!(
        open.iter()
            .any(|run| run.id == first.id && run.status == RunStatus::NeedsYou)
    );
    for run in open {
        store
            .transition(
                &run.id,
                RunStatus::Completed,
                RunDetails::default(),
                at.timestamp_millis(),
            )
            .unwrap();
    }
    cfg.definitions[0].enabled = true;
    cfg.definitions.truncate(1);
    assert_eq!(
        tick(store, &cfg, at).unwrap()[0].status,
        RunStatus::Reserved
    );
}

// Catches race oversubscription between Run Now and scheduled admission, and replay after restart.
#[test]
fn concurrent_manual_and_tick_reserve_atomically_and_restart_never_retries() {
    let (dir, owner) = owner();
    let mut cfg = config();
    cfg.max_concurrent_runs = 1;
    cfg.definitions.push(definition("b"));
    let at = now("2026-10-09T12:00:00Z");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let handles: Vec<_> = (0..2)
        .map(|i| {
            let store = owner.store().clone();
            let cfg = cfg.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                if i == 0 {
                    tick(&store, &cfg, at).unwrap()
                } else {
                    vec![run_now(&store, &cfg, "b", at).unwrap()]
                }
            })
        })
        .collect();
    barrier.wait();
    let runs: Vec<_> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    assert_eq!(
        runs.iter()
            .filter(|r| r.status == RunStatus::Reserved)
            .count(),
        1
    );
    assert_eq!(owner.store().open_runs().unwrap().len(), 1);
    drop(owner);
    let owner =
        RunOwner::acquire_at(&dir.path().join("runs.sqlite3"), at.timestamp_millis() + 1).unwrap();
    assert!(owner.store().open_runs().unwrap().is_empty());
    assert!(tick(owner.store(), &cfg, at).unwrap().is_empty());
    assert_eq!(
        run_now(owner.store(), &cfg, "a", at).unwrap().status,
        RunStatus::Reserved
    );
}

// Catches full capacity secretly queueing a scheduled occurrence for a later tick.
#[test]
fn capacity_refusal_is_final_for_that_occurrence() {
    let (_dir, owner) = owner();
    let mut cfg = config();
    cfg.max_concurrent_runs = 1;
    cfg.definitions.push(definition("b"));
    let at = now("2026-10-09T12:00:00Z");
    let runs = tick(owner.store(), &cfg, at).unwrap();
    assert_eq!(
        runs.iter().map(|r| r.status).collect::<Vec<_>>(),
        vec![RunStatus::Reserved, RunStatus::SkippedConcurrency]
    );
    owner
        .store()
        .transition(
            &runs[0].id,
            RunStatus::Completed,
            RunDetails::default(),
            at.timestamp_millis(),
        )
        .unwrap();
    assert!(tick(owner.store(), &cfg, at).unwrap().is_empty());
    assert!(run_now(owner.store(), &cfg, "missing", at).is_err());
    cfg.max_concurrent_runs = 0;
    assert!(tick(owner.store(), &cfg, at).is_err());
}

// Catches cap accounting dropping prechecking/running states or overlap checks using only running.
#[test]
fn every_open_state_blocks_overlap_and_consumes_capacity() {
    for status in [
        RunStatus::Reserved,
        RunStatus::Prechecking,
        RunStatus::Running,
        RunStatus::NeedsYou,
    ] {
        let (_dir, owner) = owner();
        let mut cfg = config();
        cfg.max_concurrent_runs = 1;
        cfg.definitions.push(definition("b"));
        let at = now("2026-10-09T12:00:00Z");
        let run = run_now(owner.store(), &cfg, "a", at).unwrap();
        owner
            .store()
            .transition(
                &run.id,
                status,
                RunDetails::default(),
                at.timestamp_millis(),
            )
            .unwrap();
        assert_eq!(
            run_now(owner.store(), &cfg, "a", at).unwrap().status,
            RunStatus::SkippedOverlap,
            "{status:?}"
        );
        assert_eq!(
            run_now(owner.store(), &cfg, "b", at).unwrap().status,
            RunStatus::SkippedConcurrency,
            "{status:?}"
        );
    }
}

// Catches two simultaneous scheduler ticks returning the same dispatchable occurrence.
#[test]
fn concurrent_ticks_dispatch_an_occurrence_only_once() {
    let (_dir, owner) = owner();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let store = owner.store().clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                tick(&store, &config(), now("2026-10-09T12:00:00Z")).unwrap()
            })
        })
        .collect();
    barrier.wait();
    let runs: Vec<_> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, RunStatus::Reserved);
}

// Catches Once dispatching early, manual runs consuming the Once occurrence, or restart/retention replaying it.
#[test]
fn once_waits_for_due_time_and_remains_consumed_after_restart_and_retention() {
    let (dir, owner) = owner();
    let mut value = serde_json::to_value(definition("once")).unwrap();
    value["cron"] = serde_json::json!("");
    value["once_local"] = serde_json::json!("2026-10-09T12:00:00");
    let cfg = AutomationsConfig {
        definitions: vec![serde_json::from_value(value).unwrap()],
        ..Default::default()
    };
    let before = now("2026-10-09T11:59:59Z");
    assert!(tick(owner.store(), &cfg, before).unwrap().is_empty());
    let manual = run_now(owner.store(), &cfg, "once", before).unwrap();
    assert_eq!(owner.store().scheduled_cursor("once").unwrap(), None);
    assert_eq!(manual.status, RunStatus::Reserved);
    owner
        .store()
        .transition(
            &manual.id,
            RunStatus::Completed,
            RunDetails::default(),
            before.timestamp_millis(),
        )
        .unwrap();
    let at = now("2026-10-09T12:00:00Z");
    let runs = tick(owner.store(), &cfg, at).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, RunStatus::Reserved);
    drop(owner);
    let owner =
        RunOwner::acquire_at(&dir.path().join("runs.sqlite3"), at.timestamp_millis() + 1).unwrap();
    assert_eq!(
        owner.store().get(&runs[0].id).unwrap().status,
        RunStatus::Interrupted
    );
    let later = now("2027-10-09T12:00:00Z");
    owner
        .store()
        .prune(later.timestamp_millis(), Some(1))
        .unwrap();
    assert!(tick(owner.store(), &cfg, later).unwrap().is_empty());
    let cursor = owner.store().scheduled_cursor("once").unwrap();
    assert_eq!(cursor, Some(at.timestamp_millis()));
    let preview = crate::automations::schedule::once::preview_definition(
        &cfg.definitions[0],
        later,
        5,
        cursor.and_then(DateTime::from_timestamp_millis),
    )
    .unwrap();
    assert!(preview.completed);
    assert!(preview.occurrences.is_empty());
}

mod critic_failure {
    use crate::automations::run::RunStatus;
    use crate::automations::{
        model::{AutomationDefinition, AutomationsConfig},
        scheduler::tick,
        store::RunOwner,
    };
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    // Catches: a later SQLite write failure discarding already committed dispatch decisions.
    #[test]
    fn later_ledger_failure_must_not_strand_an_earlier_reserved_run() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let path = dir.path().join("runs.sqlite3");
        let now = Utc.with_ymd_and_hms(2026, 10, 9, 12, 0, 0).unwrap();
        let owner = RunOwner::acquire_at(&path, now.timestamp_millis()).unwrap();
        let definition = |id| -> AutomationDefinition {
            serde_json::from_value(json!({
                "id": id, "name": "Review", "prompt": "Review repository",
                "run_config": "codex", "repository": "/project",
                "workspace": {"mode": "existing"},
                "cron": "* * * * *", "timezone": "UTC", "enabled": true,
                "grace_secs": 60, "overlap": "skip", "max_duration_secs": 3600,
                "precheck": null
            }))
            .unwrap()
        };
        let config = AutomationsConfig {
            definitions: vec![definition("first"), definition("second")],
            ..AutomationsConfig::default()
        };
        // Deterministically inject the later write error, as disk/IO failures can
        // occur between independent per-definition transactions in one tick.
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER fail_second BEFORE INSERT ON automation_runs
             WHEN NEW.automation_id = 'second'
             BEGIN SELECT RAISE(ABORT, 'injected later ledger write failure'); END;",
        )
        .unwrap();
        let result = tick(owner.store(), &config, now);
        let dispatchable = owner.store().open_runs().unwrap();
        match result {
            Ok(decisions) => {
                for run in dispatchable {
                    assert!(
                        decisions.iter().any(|decision| decision.id == run.id),
                        "committed reservation must reach the caller for dispatch"
                    );
                }
            }
            Err(error) => {
                assert!(
                    error.contains("injected later ledger write failure"),
                    "{error}"
                );
                assert!(
                    dispatchable.is_empty(),
                    "tick returned only Err but left {} committed reservation(s) with no dispatch decision",
                    dispatchable.len()
                );
                assert_eq!(
                    owner.store().scheduled_cursor("first").unwrap(),
                    None,
                    "an undispatched occurrence must remain eligible for the next tick"
                );
                assert_eq!(owner.store().scheduled_cursor("second").unwrap(), None);
                conn.execute_batch("DROP TRIGGER fail_second").unwrap();
                let retried = tick(owner.store(), &config, now).unwrap();
                assert_eq!(retried.len(), 2);
                assert!(retried.iter().all(|run| run.status == RunStatus::Reserved));
                assert!(tick(owner.store(), &config, now).unwrap().is_empty());
            }
        }
    }
}
