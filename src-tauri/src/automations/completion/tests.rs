use super::*;
use crate::automations::{model::AutomationDefinition, run::RunTrigger, store::RunOwner};
use std::cell::{Cell, RefCell};

mod critic;

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
            reason: o.reason.clone(),
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

// Catches: raw escapes, cutting inside an escape before cleaning, or splitting UTF-8.
#[test]
fn saved_run_output_has_no_terminal_escapes() {
    let cases = [
        (
            "controls",
            "\x1b]0;run title\x07\x1b[6G\x1b[38;2;220;90;90mred\x1b[39m\x1b[49m\r\n界\n".to_owned(),
            "red\n界\n".to_owned(),
            false,
        ),
        (
            "raw size is not clean size",
            format!("{}done\n", "\x1b[31m\x1b[0m".repeat(30_000)),
            "done\n".to_owned(),
            false,
        ),
        (
            "escape spans old raw cutoff",
            format!("\x1b]0;{}\x07result", "x".repeat(270_000)),
            "result".to_owned(),
            false,
        ),
        (
            "clean UTF-8 boundary",
            format!("\x1b[32m{}界end\x1b[0m", "a".repeat(262_143)),
            format!("{}界end", "a".repeat(262_138)),
            true,
        ),
        (
            "tail starts inside UTF-8",
            format!("x界{}", "a".repeat(262_142)),
            "a".repeat(262_142),
            true,
        ),
        (
            "exact clean budget",
            format!("\x1b[32m{}\x1b[0m", "é".repeat(131_072)),
            "é".repeat(131_072),
            false,
        ),
    ];
    let mut failures = Vec::new();
    for (name, raw, expected, truncated) in cases {
        let (dir, owner, run) = setup();
        let _config = crate::config::set_config_dir_override(dir.path().to_owned());
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state_in(
            dir.path(),
        ));
        let mut buffer = crate::state::OutputRingBuffer::new(2 * 1024 * 1024);
        buffer.write(raw.as_bytes());
        state
            .session_maps
            .output_buffers
            .insert("owned".into(), parking_lot::Mutex::new(buffer));
        state.session_maps.exit_codes.insert("owned".into(), 0);
        reconcile(owner.store(), &NativeCompletion(&state), 300).unwrap();
        let saved = owner.store().get(&run.id).unwrap();
        assert_eq!(saved.status, RunStatus::Completed);
        if saved.stdout.text != expected || saved.stdout.truncated != truncated {
            failures.push(format!("{name}: got {} bytes, truncated={}, prefix={:?}; expected {} bytes, truncated={truncated}", saved.stdout.text.len(), saved.stdout.truncated, saved.stdout.text.chars().take(70).collect::<String>(), expected.len()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Catches: cleaned-output truncation discarding the final report instead of old logs.
#[test]
fn saved_run_output_retains_final_report_after_long_log() {
    let (dir, owner, run) = setup();
    let _config = crate::config::set_config_dir_override(dir.path().to_owned());
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state_in(
        dir.path(),
    ));
    let mut buffer = crate::state::OutputRingBuffer::new(2 * 1024 * 1024);
    buffer.write("build log\n".repeat(40_000).as_bytes());
    buffer.write(b"\x1b[32mFINAL REPORT: 3 files changed, all checks passed\x1b[0m\n");
    state
        .session_maps
        .output_buffers
        .insert("owned".into(), parking_lot::Mutex::new(buffer));
    state.session_maps.exit_codes.insert("owned".into(), 0);

    reconcile(owner.store(), &NativeCompletion(&state), 300).unwrap();

    let saved = owner.store().get(&run.id).unwrap();
    assert_eq!(saved.status, RunStatus::Completed);
    assert!(saved.stdout.truncated);
    assert!(saved.stdout.text.len() <= 256 * 1024);
    assert!(
        saved
            .stdout
            .text
            .ends_with("FINAL REPORT: 3 files changed, all checks passed\n"),
        "saved output lost the final report after truncating a long log"
    );
}

// Catches synchronous publication git/config work starving a one-worker runtime.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn event_publication_does_not_block_runtime_worker() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let workspace = dir.path().to_str().unwrap().to_owned();
    let git = |args: &[&str]| super::super::publish::run_git(dir.path(), args).unwrap();
    git(&["init", "--initial-branch=publication-test"]);
    git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--allow-empty",
        "-m",
        "test",
    ]);
    let fifo = dir.path().join("delayed-git-config");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    git(&["config", "include.path", fifo.to_str().unwrap()]);
    // Holding both ends lets git open the FIFO but withholds EOF until the
    // independent async task progresses (or the controlled hold expires).
    let held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&fifo)
        .unwrap();
    let (progress, observed) = std::sync::mpsc::channel();
    let release = std::thread::spawn(move || {
        let progressed = observed
            .recv_timeout(std::time::Duration::from_secs(5))
            .is_ok();
        std::fs::remove_file(&fifo).unwrap();
        std::fs::write(&fifo, "").unwrap();
        drop(held);
        progressed
    });
    let definitions = DefinitionStore::at_path(dir.path().join("automations.json"));
    let (result, ()) = tokio::join!(
        event_publication_prerequisites(move || definitions, workspace),
        async {
            tokio::task::yield_now().await;
            let _ = progress.send(());
        }
    );
    let progressed = release.join().unwrap();
    assert_eq!(result.unwrap().1, "publication-test");
    assert!(
        progressed,
        "publication prerequisites blocked the independent async task"
    );
}

// Catches dropping validated intake/head output or swallowing prerequisite errors.
#[tokio::test]
async fn event_publication_prerequisites_preserve_config_and_git_errors() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let path = dir.path().join("automations.json");
    let workspace = dir.path().to_str().unwrap().to_owned();
    std::fs::write(&path, "not JSON").unwrap();
    let parse_error =
        serde_json::from_str::<super::super::model::AutomationsConfig>("not JSON").unwrap_err();
    let expected = format!("Invalid {}: {parse_error}", path.display());
    let config_path = path.clone();
    let actual = event_publication_prerequisites(
        move || DefinitionStore::at_path(config_path),
        workspace.clone(),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(actual, expected);
    std::fs::write(&path, "{}").unwrap();
    std::fs::write(dir.path().join(".git"), "invalid git directory").unwrap();
    let error = event_publication_prerequisites(move || DefinitionStore::at_path(path), workspace)
        .await
        .err()
        .unwrap();
    assert_eq!(error, "git rev-parse --abbrev-ref HEAD failed");
}

// Catches: losing the actionable stale-evidence refusal after terminal execution.
#[test]
fn publication_refusal_is_durable_without_changing_completed_execution() {
    let (_dir, owner, run) = setup();
    // The real owner store supplies the event identity; completion is immutable.
    let mut definition = run.definition.clone();
    definition.id = "event".into();
    let event = owner
        .store()
        .admit_event(
            &definition,
            RunTrigger::Event {
                repository: "owner/repo".into(),
                issue_number: 1,
                label: "run".into(),
            },
            None,
            2,
            300,
        )
        .unwrap()
        .unwrap();
    owner
        .store()
        .transition(&event.id, RunStatus::Completed, Default::default(), 400)
        .unwrap();
    owner
        .store()
        .settle_outcome(
            &event.id,
            "publication_refused",
            None,
            Some("Review artifact moved".into()),
            500,
        )
        .unwrap();
    let saved = owner.store().get(&event.id).unwrap();
    assert_eq!(saved.status, RunStatus::Completed);
    assert_eq!(
        saved.outcome.as_ref().unwrap().failure_reason.as_deref(),
        Some("Review artifact moved")
    );
}
