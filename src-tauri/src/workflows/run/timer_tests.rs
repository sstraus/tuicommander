use super::*;
use serde_json::{Value, json};

// Commands are decoded at the actual RunStore boundary. These regressions also
// compile against the pre-Timer model, where scheduling fails as an unknown action.
fn command(value: Value) -> RunCommand {
    serde_json::from_value(value).expect("durable timer command must be supported")
}

fn schedule(
    key: &str,
    scope: Value,
    purpose: Value,
    due: i64,
    expiry: i64,
    outcome: &str,
) -> RunCommand {
    command(
        json!({"action":"timer","command":{"action":"schedule","timer":{
            "key":key,"scope":scope,"purpose":purpose,"dueMs":due,"expiresMs":expiry,
            "maxAttempts":3,"baseBackoffMs":1000,"maxBackoffMs":5000,
            "expiryOutcome":outcome,"attempt":1,"state":"pending","outcome":null
        }}}),
    )
}

fn fire(key: &str, attempt: u16) -> RunCommand {
    command(json!({"action":"timer","command":{"action":"fire","key":key,"attempt":attempt}}))
}

fn retry(key: &str, attempt: u16, hint: u64) -> RunCommand {
    command(
        json!({"action":"timer","command":{"action":"retry","key":key,"attempt":attempt,"retry_after_ms":hint}}),
    )
}

fn timers(run: &RunSnapshot) -> Value {
    serde_json::to_value(run).unwrap()["timers"].clone()
}

#[test]
fn timer_reopen_fires_once_with_remaining_backoff() {
    // catches: lost deadlines, duplicate delivery, reset backoff and uncapped provider hints.
    let (config, project, plan, _, definition, _guard) = super::tests::fixture();
    assert!(
        config
            .path()
            .starts_with(crate::test_support::test_temp_root())
    );
    let db = config.path().join("timers.sqlite3");
    let store = RunStore::open_at(&db).unwrap();
    let run = store
        .start_plan(
            project.path().to_str().unwrap(),
            &plan,
            &definition,
            1,
            RunLimits::default(),
        )
        .unwrap();
    let at = run.started_ms;
    store
        .command_at(
            &run.id,
            "schedule",
            schedule(
                "provider",
                json!({"type":"service","owner":"review"}),
                json!({"type":"provider_retry","provider":"github"}),
                at + 1000,
                at + 30000,
                "incomplete",
            ),
            at,
        )
        .unwrap();
    assert!(
        store
            .command_at(&run.id, "early", fire("provider", 1), at + 999)
            .is_err()
    );
    let first = store
        .command_at(&run.id, "first", fire("provider", 1), at + 1000)
        .unwrap();
    store
        .command_at(&run.id, "retry", retry("provider", 1, 90000), at + 1000)
        .unwrap();
    drop(store);
    let store = RunStore::open_at(&db).unwrap();
    assert_eq!(
        timers(&store.snapshot(&run.id).unwrap())[0]["dueMs"],
        at + 6000
    );
    assert!(
        store
            .command_at(&run.id, "before-backoff", fire("provider", 2), at + 5999)
            .is_err()
    );
    assert_eq!(
        store
            .command_at(&run.id, "first", fire("provider", 1), at + 6000)
            .unwrap(),
        first
    );
    store
        .command_at(&run.id, "second", fire("provider", 2), at + 6000)
        .unwrap();
    assert!(
        store
            .command_at(&run.id, "duplicate", fire("provider", 2), at + 6000)
            .is_err()
    );
    store
        .command_at(&run.id, "retry-2", retry("provider", 2, 0), at + 6000)
        .unwrap();
    assert_eq!(
        timers(&store.snapshot(&run.id).unwrap())[0]["dueMs"],
        at + 8000
    );
    let exhausted = store
        .command_at(&run.id, "exhausted", fire("provider", 3), at + 8000)
        .unwrap();
    assert_eq!(timers(&exhausted.snapshot)[0]["outcome"], "incomplete");
    assert!(
        store
            .command_at(
                &run.id,
                "retry-exhausted",
                retry("provider", 3, 0),
                at + 8000
            )
            .is_err()
    );
    assert_eq!(
        store.replay(&run.id).unwrap(),
        store.snapshot(&run.id).unwrap()
    );
}

#[test]
fn timer_expiry_never_grants_approval() {
    // catches: question expiry silently providing consent, accepting a story or resuming work.
    let (config, project, plan, story, definition, _guard) = super::tests::fixture();
    let store = RunStore::open_at(&config.path().join("timers.sqlite3")).unwrap();
    let run = store
        .start_plan(
            project.path().to_str().unwrap(),
            &plan,
            &definition,
            1,
            RunLimits::default(),
        )
        .unwrap();
    let original = crate::stories::StoryStore::open()
        .unwrap()
        .get_story(&story)
        .unwrap();
    let at = run.started_ms;
    let purpose = json!({"type":"question_deadline","question_id":"approval-1"});
    assert!(
        store
            .command_at(
                &run.id,
                "auto-consent",
                schedule(
                    "bad",
                    json!({"type":"service","owner":"questions"}),
                    purpose.clone(),
                    at + 1000,
                    at + 1000,
                    "wake"
                ),
                at
            )
            .is_err()
    );
    store
        .command_at(
            &run.id,
            "question",
            schedule(
                "question",
                json!({"type":"service","owner":"questions"}),
                purpose,
                at + 1000,
                at + 1000,
                "timed_out",
            ),
            at,
        )
        .unwrap();
    store
        .command_at(&run.id, "pause", RunCommand::Pause, at)
        .unwrap();
    let result = store
        .command_at(&run.id, "expiry", fire("question", 1), at + 1000)
        .unwrap();
    assert_eq!(timers(&result.snapshot)[0]["outcome"], "timed_out");
    assert_eq!(result.snapshot.status, RunStatus::Paused);
    assert_eq!(result.snapshot.attempts, run.attempts);
    assert_eq!(result.snapshot.stories, run.stories);
    assert_eq!(
        crate::stories::StoryStore::open()
            .unwrap()
            .get_story(&story)
            .unwrap(),
        original
    );
    assert_eq!(store.replay(&run.id).unwrap(), result.snapshot);
}

#[test]
fn single_row_cohort_flush_survives_restart() {
    // catches: a single proof row waiting for a full cohort or an explicit run resume.
    let (config, project, plan, _, definition, _guard) = super::tests::fixture();
    let db = config.path().join("timers.sqlite3");
    let store = RunStore::open_at(&db).unwrap();
    let run = store
        .start_plan(
            project.path().to_str().unwrap(),
            &plan,
            &definition,
            1,
            RunLimits::default(),
        )
        .unwrap();
    let due = run.started_ms + 1000;
    store
        .command_at(
            &run.id,
            "cohort",
            schedule(
                "cohort-1",
                json!({"type":"service","owner":"proof-cohorts"}),
                json!({"type":"cohort_flush","cohort_id":"one-row","rows":1}),
                due,
                due + 10000,
                "incomplete",
            ),
            run.started_ms,
        )
        .unwrap();
    drop(store);
    let store = RunStore::open_at(&db).unwrap();
    store.reconcile_active_after_restart().unwrap();
    let paused = store.snapshot(&run.id).unwrap();
    assert_eq!(paused.status, RunStatus::Paused);
    let fired = store
        .command_at(&run.id, "flush", fire("cohort-1", 1), due)
        .unwrap();
    assert_eq!(timers(&fired.snapshot)[0]["outcome"], "wake");
    assert_eq!(timers(&fired.snapshot)[0]["purpose"]["rows"], 1);
    assert!(
        store
            .command_at(&run.id, "duplicate-flush", fire("cohort-1", 1), due)
            .is_err()
    );
    assert_eq!(store.replay(&run.id).unwrap(), fired.snapshot);
}

#[test]
fn timer_owner_loss_cancellation_and_replay_fence_delivery() {
    // catches: non-owner daemon delivery and cancelled work firing after reopen.
    use super::runtime::RuntimeOwner;
    let (config, project, plan, _, definition, _guard) = super::tests::fixture();
    let db = config.path().join("timers.sqlite3");
    let owner = RuntimeOwner::acquire(&db).unwrap();
    assert!(RuntimeOwner::acquire(&db).is_err());
    let run = owner
        .store
        .start_plan(
            project.path().to_str().unwrap(),
            &plan,
            &definition,
            1,
            RunLimits::default(),
        )
        .unwrap();
    owner
        .store
        .command_at(
            &run.id,
            "schedule",
            schedule(
                "cancelled",
                json!({"type":"service","owner":"retry"}),
                json!({"type":"provider_retry","provider":"github"}),
                run.started_ms,
                run.started_ms + 10000,
                "incomplete",
            ),
            run.started_ms,
        )
        .unwrap();
    drop(owner);
    let owner = RuntimeOwner::acquire(&db).unwrap();
    let latest = owner.store.snapshot(&run.id).unwrap();
    assert!(super::timer::fire_due(&owner.store, &latest, run.started_ms).unwrap());
    assert!(
        !super::timer::fire_due(
            &owner.store,
            &owner.store.snapshot(&run.id).unwrap(),
            run.started_ms
        )
        .unwrap()
    );
    owner
        .store
        .command_at(&run.id, "cancel", RunCommand::Cancel, run.started_ms)
        .unwrap();
    assert!(
        owner
            .store
            .command_at(
                &run.id,
                "late",
                fire("cancelled", 1),
                run.started_ms + 10000
            )
            .is_err()
    );
    assert_eq!(
        timers(&owner.store.snapshot(&run.id).unwrap())[0]["state"],
        "cancelled"
    );
    assert_eq!(
        owner.store.replay(&run.id).unwrap(),
        owner.store.snapshot(&run.id).unwrap()
    );
}

#[test]
fn activation_timer_pause_preserves_absolute_due_and_cancel_fences_wake() {
    // catches: activation deadlines firing while paused or replenishing on resume.
    let (_config, _project, store, run, story, _guard) = super::graph_tests::graph_fixture();
    let run = store
        .command(
            &run.id,
            "graph",
            RunCommand::Graph {
                transition: super::graph::GraphTransition::Start {
                    execution_id: "execution".into(),
                    target_id: story,
                },
            },
        )
        .unwrap()
        .snapshot;
    let graph = &run.graph_executions[0];
    let activation = graph.activations.last().unwrap();
    let at = run.started_ms;
    store
        .command_at(
            &run.id,
            "activation-timer",
            schedule(
                "branch",
                json!({"type":"activation","execution_id":graph.id,"activation_id":activation.id}),
                json!({"type":"provider_retry","provider":"github"}),
                at + 1000,
                at + 30000,
                "incomplete",
            ),
            at,
        )
        .unwrap();
    store
        .command_at(&run.id, "pause", RunCommand::Pause, at + 500)
        .unwrap();
    assert!(
        store
            .command_at(&run.id, "paused-fire", fire("branch", 1), at + 2000)
            .is_err()
    );
    let cancel = command(json!({"action":"timer","command":{"action":"cancel","key":"branch"}}));
    store
        .command_at(&run.id, "cancel-timer", cancel, at + 2000)
        .unwrap();
    assert!(
        store
            .command_at(&run.id, "late-fire", fire("branch", 1), at + 2000)
            .is_err()
    );
    assert_eq!(
        timers(&store.snapshot(&run.id).unwrap())[0]["dueMs"],
        at + 1000
    );
    assert_eq!(
        store.replay(&run.id).unwrap(),
        store.snapshot(&run.id).unwrap()
    );
}
