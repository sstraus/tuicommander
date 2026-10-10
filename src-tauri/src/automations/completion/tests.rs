use super::*;
use crate::automations::{model::AutomationDefinition, run::RunTrigger, store::RunOwner};
use std::cell::{Cell, RefCell};

struct Effects {
    observation: RefCell<Observation>,
    stopped: RefCell<Vec<String>>,
    published: RefCell<Vec<RunStatus>>,
    stop_fails: Cell<bool>,
}
impl Default for Effects {
    fn default() -> Self {
        Self {
            observation: RefCell::new(Observation {
                live: true,
                ..Default::default()
            }),
            stopped: Default::default(),
            published: Default::default(),
            stop_fails: Cell::new(false),
        }
    }
}
impl CompletionEffects for Effects {
    fn observe(&self, _: &AutomationRun) -> Observation {
        let o = self.observation.borrow();
        Observation {
            task: o.task,
            task_unknown_exit: o.task_unknown_exit,
            live: o.live,
            awaiting: o.awaiting,
            exit: o.exit,
            progress: o.progress,
            output: o.output.clone(),
        }
    }
    fn stop(&self, run: &AutomationRun) -> Result<(), String> {
        if self.stop_fails.get() {
            return Err("stop failed".into());
        }
        if let Some(session) = &run.session_id {
            self.stopped.borrow_mut().push(session.clone());
        }
        Ok(())
    }
    fn publish(&self, run: &AutomationRun) {
        self.published.borrow_mut().push(run.status);
    }
}
fn setup() -> (tempfile::TempDir, RunOwner, AutomationRun) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let owner = RunOwner::acquire_at(&dir.path().join("runs.sqlite3"), 0).unwrap();
    let definition: AutomationDefinition = serde_json::from_value(serde_json::json!({
        "id":"test", "name":"Test", "prompt":"test", "run_config":"codex",
        "repository":dir.path(), "workspace":{"mode":"existing"}, "cron":"0 9 * * *",
        "timezone":"UTC", "enabled":true, "grace_secs":43200,
        "overlap":"skip", "max_duration_secs":1, "precheck":null
    }))
    .unwrap();
    let run = owner
        .store()
        .reserve(&definition, RunTrigger::Manual, 100)
        .unwrap()
        .unwrap();
    owner.store().claim_dispatch(&run.id, 150).unwrap().unwrap();
    let run = owner
        .store()
        .transition(
            &run.id,
            RunStatus::Running,
            RunDetails {
                session_id: Some("owned".into()),
                task_id: Some("task".into()),
                ..Default::default()
            },
            200,
        )
        .unwrap();
    (dir, owner, run)
}

// Catches: treating idle/no evidence as success, or finalizing blocked runs early.
#[test]
fn idle_remains_active_and_needs_you_expires_at_persisted_deadline() {
    let (_dir, owner, run) = setup();
    let effects = Effects::default();
    reconcile(owner.store(), &effects, 300).unwrap();
    assert_eq!(
        owner.store().get(&run.id).unwrap().status,
        RunStatus::Running
    );
    effects.observation.borrow_mut().awaiting = true;
    reconcile(owner.store(), &effects, 400).unwrap();
    assert_eq!(
        owner.store().get(&run.id).unwrap().status,
        RunStatus::NeedsYou
    );
    reconcile(owner.store(), &effects, 1099).unwrap();
    assert!(effects.stopped.borrow().is_empty());
    reconcile(owner.store(), &effects, 1100).unwrap();
    let final_run = owner.store().get(&run.id).unwrap();
    assert_eq!(final_run.status, RunStatus::TimedOut);
    assert_eq!(final_run.started_ms, Some(150));
    assert_eq!(final_run.deadline_ms, Some(1100));
    assert_eq!(*effects.stopped.borrow(), ["owned"]);
    assert_eq!(
        *effects.published.borrow(),
        [RunStatus::NeedsYou, RunStatus::TimedOut]
    );
}

// Catches: absent exit being promoted to success, or a failure hidden by stale done.
#[test]
fn evidence_matrix_preserves_failure_and_unknown_provenance() {
    for (o, expected) in [
        (
            Observation {
                exit: Some(0),
                ..Default::default()
            },
            RunStatus::Completed,
        ),
        (
            Observation {
                exit: Some(2),
                progress: Some(crate::progress::ProgressKind::Done),
                ..Default::default()
            },
            RunStatus::Failed,
        ),
        (
            Observation {
                task: Some(TaskStatus::Failed),
                live: true,
                ..Default::default()
            },
            RunStatus::Failed,
        ),
        (
            Observation {
                task: Some(TaskStatus::Completed),
                task_unknown_exit: true,
                ..Default::default()
            },
            RunStatus::Unknown,
        ),
        (Observation::default(), RunStatus::Unknown),
        (
            Observation {
                task: Some(TaskStatus::Cancelled),
                live: true,
                ..Default::default()
            },
            RunStatus::Unknown,
        ),
        (
            Observation {
                progress: Some(crate::progress::ProgressKind::Done),
                live: true,
                ..Default::default()
            },
            RunStatus::Completed,
        ),
        (
            Observation {
                task: Some(TaskStatus::InputRequired),
                live: true,
                ..Default::default()
            },
            RunStatus::NeedsYou,
        ),
    ] {
        let (_dir, owner, run) = setup();
        let effects = Effects::default();
        *effects.observation.borrow_mut() = o;
        reconcile(owner.store(), &effects, 300).unwrap();
        assert_eq!(owner.store().get(&run.id).unwrap().status, expected);
    }
}

// Catches: duplicate signals mutating history, unbounded snapshots, boot retry.
#[test]
fn final_output_is_bounded_and_late_signals_and_boot_cannot_rewrite_it() {
    let (dir, owner, run) = setup();
    let effects = Effects::default();
    effects.observation.borrow_mut().exit = Some(0);
    effects.observation.borrow_mut().output = Some("é".repeat(200_000));
    reconcile(owner.store(), &effects, 300).unwrap();
    let saved = owner.store().get(&run.id).unwrap();
    assert_eq!(saved.stdout.text.len(), 262144);
    assert!(saved.stdout.truncated);
    effects.observation.borrow_mut().exit = Some(1);
    reconcile(owner.store(), &effects, 1200).unwrap();
    assert_eq!(owner.store().get(&run.id).unwrap(), saved);
    assert_eq!(effects.published.borrow().len(), 1);
    drop(owner);
    let owner = RunOwner::acquire_at(&dir.path().join("runs.sqlite3"), 2000).unwrap();
    assert_eq!(owner.store().get(&run.id).unwrap(), saved);
}

// Catches: recording timeout as if cancellation succeeded, or retrying interrupted runs.
#[test]
fn stop_failure_stays_open_for_reconciliation_and_boot_interrupts_without_retry() {
    let (dir, owner, run) = setup();
    let effects = Effects::default();
    effects.stop_fails.set(true);
    assert!(reconcile(owner.store(), &effects, 1100).is_err());
    assert_eq!(
        owner.store().get(&run.id).unwrap().status,
        RunStatus::Running
    );
    drop(owner);
    let owner = RunOwner::acquire_at(&dir.path().join("runs.sqlite3"), 1200).unwrap();
    reconcile(owner.store(), &effects, 1300).unwrap();
    assert_eq!(
        owner.store().get(&run.id).unwrap().status,
        RunStatus::Interrupted
    );
    assert!(effects.published.borrow().is_empty());
}

// Catches: task/session mismatch, optimistic unknown exit, or lost SSE publication.
#[test]
fn native_reconciles_task_snapshot_without_events_and_publishes_identical_bus_payload() {
    let (dir, owner, run) = setup();
    let _config = crate::config::set_config_dir_override(dir.path().to_owned());
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state_in(
        dir.path(),
    ));
    let task = state
        .tasks
        .create(crate::tasks::TaskKind::AgentSpawn, "owner", Some("owned"));
    owner
        .store()
        .transition(
            &run.id,
            RunStatus::Running,
            RunDetails {
                task_id: Some(task.clone()),
                ..Default::default()
            },
            201,
        )
        .unwrap();
    state
        .tasks
        .set_status(
            &task,
            TaskStatus::Completed,
            crate::tasks::TaskUpdate {
                result: Some(serde_json::json!({"exit_code":null})),
                ..Default::default()
            },
        )
        .unwrap();
    let mut receiver = state.event_bus.subscribe();
    reconcile(owner.store(), &NativeCompletion(&state), 300).unwrap();
    let saved = owner.store().get(&run.id).unwrap();
    assert_eq!(saved.status, RunStatus::Unknown);
    let AppEvent::AutomationRunChanged { payload } = receiver.try_recv().unwrap() else {
        panic!("wrong event")
    };
    assert_eq!(payload["run"]["status"], "unknown");
}

// Catches: progress lost during broadcast lag never reaching durable history.
#[test]
fn native_recovers_durable_progress_and_snapshots_without_receiving_its_event() {
    let (dir, owner, run) = setup();
    let _config = crate::config::set_config_dir_override(dir.path().to_owned());
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state_in(
        dir.path(),
    ));
    let project = dir
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    owner
        .store()
        .transition(
            &run.id,
            RunStatus::Running,
            RunDetails {
                workspace: Some(project.clone()),
                ..Default::default()
            },
            201,
        )
        .unwrap();
    let store = crate::progress::ProgressStore::open().unwrap();
    store
        .record_for_pty(
            &project,
            &crate::progress::NewProgressEntry {
                kind: crate::progress::ProgressKind::Done,
                text: "result".into(),
                step: None,
                agent_name: None,
            },
            Some("unrelated"),
        )
        .unwrap();
    let saved = owner.store().get(&run.id).unwrap();
    assert_ne!(
        NativeCompletion(&state).observe(&saved).progress,
        Some(crate::progress::ProgressKind::Done)
    );
    store
        .record_for_pty(
            &project,
            &crate::progress::NewProgressEntry {
                kind: crate::progress::ProgressKind::Done,
                text: "result".into(),
                step: None,
                agent_name: None,
            },
            Some("owned"),
        )
        .unwrap();
    let mut buffer = crate::state::OutputRingBuffer::new(1024);
    buffer.write(b"final output");
    state
        .session_maps
        .output_buffers
        .insert("owned".into(), parking_lot::Mutex::new(buffer));
    reconcile(owner.store(), &NativeCompletion(&state), 300).unwrap();
    let saved = owner.store().get(&run.id).unwrap();
    assert_eq!(saved.status, RunStatus::Completed);
    assert_eq!(saved.stdout.text, "final output");
}
