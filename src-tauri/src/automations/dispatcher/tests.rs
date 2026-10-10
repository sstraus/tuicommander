use super::*;
use crate::automations::{model::Precheck, precheck::Termination, store::RunOwner};
use crate::test_support::{fail_with_stderr_script, sleep_script, test_temp_root};
use parking_lot::Mutex;
use tuic_test_support::{print_file_script, touch_script};

#[derive(Default)]
struct Effects {
    calls: Mutex<Vec<LaunchRequest>>,
    base: Mutex<Option<String>>,
    workspace: String,
    fail_workspace: bool,
    fail_launch: bool,
    pause_during_workspace: Option<std::path::PathBuf>,
    ledger: std::path::PathBuf,
    writer: Option<RunStore>,
    stop_during_launch: bool,
    stopped: Mutex<Vec<String>>,
}

impl DispatchEffects for Effects {
    async fn workspace(
        &self,
        definition: &AutomationDefinition,
        id: &str,
    ) -> Result<ResolvedWorkspace, String> {
        // A real workspace operation yields; let duplicate dispatch race the claim.
        tokio::task::yield_now().await;
        assert_eq!(
            RunStore::open_at(&self.ledger)?.get(id)?.status,
            RunStatus::Prechecking,
            "workspace effect ran before durable reservation/claim"
        );
        if let Workspace::NewPerRun { base_branch } = &definition.workspace {
            *self.base.lock() = Some(base_branch.clone());
        }
        if let Some(path) = &self.pause_during_workspace {
            DefinitionStore::at_path(path.clone()).set_enabled(&definition.id, false)?;
        }
        if self.fail_workspace {
            Err("workspace unavailable".into())
        } else {
            Ok(ResolvedWorkspace {
                path: self.workspace.clone(),
                id: Some("allocated-workspace".into()),
            })
        }
    }

    async fn launch(&self, request: LaunchRequest) -> Result<LaunchBinding, String> {
        tokio::task::yield_now().await;
        let saved = RunStore::open_at(&self.ledger)?.get(&request.run_id)?;
        assert_eq!(
            saved.status,
            RunStatus::Prechecking,
            "spawn ran before claim"
        );
        assert!(
            saved.precheck_outcome.is_some(),
            "spawn ran before persisting precheck decision"
        );
        if self.stop_during_launch {
            self.writer.as_ref().unwrap().transition(
                &request.run_id,
                RunStatus::Interrupted,
                RunDetails::default(),
                chrono::Utc::now().timestamp_millis(),
            )?;
        }
        self.calls.lock().push(request);
        if self.fail_launch {
            Err("agent spawn refused".into())
        } else {
            Ok(LaunchBinding {
                session_id: "owned-session".into(),
                task_id: "owned-task".into(),
            })
        }
    }

    fn stop(&self, binding: &LaunchBinding) -> Result<(), String> {
        self.stopped.lock().push(binding.session_id.clone());
        Ok(())
    }
}

struct Fixture {
    _root: tempfile::TempDir,
    _owner: RunOwner,
    dispatcher: Dispatcher<Effects>,
}

impl Fixture {
    fn new(precheck: Option<Precheck>, manual: bool) -> (Self, String) {
        let root = tempfile::tempdir_in(test_temp_root()).unwrap();
        let workspace = root.path().join("resolved-workspace");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::write(workspace.join("input.txt"), "resolved-output").unwrap();
        let repository = root.path().join("repository");
        std::fs::create_dir(&repository).unwrap();
        std::fs::write(repository.join("input.txt"), "wrong-repository").unwrap();
        let definition: AutomationDefinition = serde_json::from_value(serde_json::json!({
            "id":"once", "name":"Once run", "prompt":"Literal {{files}}\nKeep this prompt.",
            "run_config":"named-profile", "repository":repository,
            "workspace":{"mode":"new_per_run","base_branch":"configured-base"},
            "cron":"", "once_local":"2099-01-01T12:00:00", "timezone":"UTC",
            "enabled":true, "grace_secs":43200, "overlap":"skip", "max_duration_secs":3600,
            "precheck":precheck
        }))
        .unwrap();
        let definitions = DefinitionStore::at_path(root.path().join("automations.json"));
        definitions.create(definition.clone()).unwrap();
        let owner = RunOwner::acquire_at(&root.path().join("runs.sqlite3"), 0).unwrap();
        let id = if manual {
            String::new()
        } else {
            owner
                .store()
                .reserve(&definition, RunTrigger::Scheduled { occurrence_ms: 1 }, 1)
                .unwrap()
                .unwrap()
                .id
        };
        let dispatcher = Dispatcher {
            definitions,
            runs: owner.store().clone(),
            effects: Effects {
                workspace: workspace.to_string_lossy().into_owned(),
                ledger: root.path().join("runs.sqlite3"),
                writer: Some(owner.store().clone()),
                ..Default::default()
            },
        };
        (
            Self {
                _root: root,
                _owner: owner,
                dispatcher,
            },
            id,
        )
    }

    fn saved(&self, id: &str) -> AutomationRun {
        RunStore::open_at(&self._root.path().join("runs.sqlite3"))
            .unwrap()
            .get(id)
            .unwrap()
    }
}

// Catches: dispatch using the repo cwd, inheriting host environment, losing the
// precheck record, or inserting stdout/template expansion into the literal prompt.
#[tokio::test]
async fn dispatch_persists_clean_workspace_precheck_and_launches_exact_prompt() {
    assert!(std::env::var_os("TUIC_TEST_TMP_ROOT").is_some());
    let clean = if cfg!(windows) {
        "if defined TUIC_TEST_TMP_ROOT exit /b 9 & "
    } else {
        "[ -z \"${TUIC_TEST_TMP_ROOT+x}\" ] || exit 9; "
    };
    let (fixture, id) = Fixture::new(
        Some(Precheck {
            command: format!("{clean}{}", print_file_script("input.txt")),
            timeout_secs: 30,
        }),
        false,
    );
    fixture.dispatcher.dispatch(&id).await.unwrap();
    let saved = fixture.saved(&id);
    assert_eq!(saved.status, RunStatus::Running);
    assert_eq!(saved.session_id.as_deref(), Some("owned-session"));
    assert_eq!(saved.task_id.as_deref(), Some("owned-task"));
    assert_eq!(saved.workspace_id.as_deref(), Some("allocated-workspace"));
    let Some(PrecheckOutcome::Executed(result)) = saved.precheck_outcome else {
        panic!("missing executed outcome")
    };
    assert_eq!(result.termination, Termination::Exited(Some(0)));
    assert_eq!(result.stdout, "resolved-output");
    assert!(!result.stdout_truncated);
    let calls = fixture.dispatcher.effects.calls.lock();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].prompt, "Literal {{files}}\nKeep this prompt.");
    assert_eq!(calls[0].run_config, "named-profile");
    assert_eq!(
        saved.workspace.as_deref(),
        Some(calls[0].workspace.as_str())
    );
    assert_eq!(
        fixture.dispatcher.effects.base.lock().as_deref(),
        Some("configured-base")
    );
}

// Catches: failed or timed-out prechecks allowing spawn, or refusals not surviving a reader reopen.
#[tokio::test]
async fn dispatch_records_nonzero_and_timeout_as_skipped_precheck_without_launch() {
    for (command, timed_out) in [
        (fail_with_stderr_script("refused", 7), false),
        (sleep_script(), true),
    ] {
        let (fixture, id) = Fixture::new(
            Some(Precheck {
                command,
                timeout_secs: 1,
            }),
            false,
        );
        fixture.dispatcher.dispatch(&id).await.unwrap();
        let saved = fixture.saved(&id);
        assert_eq!(saved.status, RunStatus::SkippedPrecheck);
        assert!(saved.finished_ms.is_some());
        assert!(saved.session_id.is_none() && saved.task_id.is_none());
        assert!(fixture.dispatcher.effects.calls.lock().is_empty());
        let Some(PrecheckOutcome::Executed(result)) = saved.precheck_outcome else {
            panic!("missing outcome")
        };
        if timed_out {
            assert_eq!(result.termination, Termination::TimedOut);
            assert!(result.duration_ms >= 1000);
        } else {
            assert_eq!(result.termination, Termination::Exited(Some(7)));
            assert_eq!(result.stderr.trim(), "refused");
        }
    }
}

// Catches: paused Run Now running a destructive precheck or persisting bypass as no precheck.
#[tokio::test]
async fn run_now_dispatch_bypasses_precheck_durably_even_when_paused() {
    let (fixture, _) = Fixture::new(
        Some(Precheck {
            command: touch_script("must-not-exist"),
            timeout_secs: 1,
        }),
        true,
    );
    fixture
        .dispatcher
        .definitions
        .set_enabled("once", false)
        .unwrap();
    let run = fixture.dispatcher.run_now("once").await.unwrap();
    let saved = fixture.saved(&run.id);
    assert_eq!(saved.status, RunStatus::Running);
    assert_eq!(saved.precheck_outcome, Some(PrecheckOutcome::Bypassed));
    assert!(
        !std::path::Path::new(saved.workspace.as_ref().unwrap())
            .join("must-not-exist")
            .exists()
    );
    assert_eq!(
        fixture.dispatcher.effects.calls.lock()[0].prompt,
        "Literal {{files}}\nKeep this prompt."
    );
}

// Catches: an absent configured precheck being conflated with a manual bypass.
#[tokio::test]
async fn dispatch_records_absent_precheck_and_claims_each_reservation_once() {
    let (fixture, id) = Fixture::new(None, false);
    let (first, second) = tokio::join!(
        fixture.dispatcher.dispatch(&id),
        fixture.dispatcher.dispatch(&id)
    );
    first.unwrap();
    second.unwrap();
    let saved = fixture.saved(&id);
    assert_eq!(saved.status, RunStatus::Running);
    assert_eq!(saved.precheck_outcome, Some(PrecheckOutcome::NotConfigured));
    assert_eq!(fixture.dispatcher.effects.calls.lock().len(), 1);
}

// Catches: deleted, paused or edited definitions being launched from stale reservations.
#[tokio::test]
async fn dispatch_rechecks_definition_before_effects_and_again_before_spawn() {
    for change in ["delete", "pause", "edit", "pause_during_workspace"] {
        let (mut fixture, id) = Fixture::new(None, false);
        match change {
            "delete" => fixture.dispatcher.definitions.remove("once").unwrap(),
            "pause" => fixture
                .dispatcher
                .definitions
                .set_enabled("once", false)
                .unwrap(),
            "edit" => {
                let mut value = fixture.dispatcher.definitions.get("once").unwrap();
                value.prompt = "replacement prompt".into();
                fixture
                    .dispatcher
                    .definitions
                    .update("once", value)
                    .unwrap();
            }
            _ => {
                fixture.dispatcher.effects.pause_during_workspace =
                    Some(fixture._root.path().join("automations.json"))
            }
        }
        fixture.dispatcher.dispatch(&id).await.unwrap();
        let saved = fixture.saved(&id);
        assert_eq!(saved.status, RunStatus::Failed, "{change}");
        assert!(saved.reason.is_some());
        assert!(fixture.dispatcher.effects.calls.lock().is_empty());
    }
}

// Catches: workspace/spawn errors leaving live reservations or losing the allocated workspace.
#[tokio::test]
async fn dispatch_records_workspace_and_spawn_failure() {
    for workspace_error in [true, false] {
        let (mut fixture, id) = Fixture::new(None, false);
        fixture.dispatcher.effects.fail_workspace = workspace_error;
        fixture.dispatcher.effects.fail_launch = !workspace_error;
        fixture.dispatcher.dispatch(&id).await.unwrap();
        let saved = fixture.saved(&id);
        assert_eq!(saved.status, RunStatus::Failed);
        assert_eq!(
            saved.reason.as_deref(),
            Some(if workspace_error {
                "workspace unavailable"
            } else {
                "agent spawn refused"
            })
        );
        assert_eq!(saved.workspace.is_none(), workspace_error);
    }
}

// Catches: the boot admission path silently continuing to execute recurring cron schedules.
#[tokio::test]
async fn phase_one_admission_consumes_once_but_never_dispatches_cron() {
    let (fixture, _) = Fixture::new(None, true);
    let mut recurring = fixture.dispatcher.definitions.get("once").unwrap();
    recurring.id = "recurring".into();
    recurring.once_local = None;
    recurring.cron = "* * * * *".into();
    fixture.dispatcher.definitions.create(recurring).unwrap();
    let now = "2099-01-01T12:00:01Z".parse().unwrap();
    let due = fixture.dispatcher.reserve_due(now).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].definition.id, "once");
    assert!(fixture.dispatcher.reserve_due(now).unwrap().is_empty());
    assert!(
        fixture
            .dispatcher
            .runs
            .scheduled_cursor("recurring")
            .unwrap()
            .is_none()
    );
    assert!(
        fixture
            .dispatcher
            .run_now("recurring")
            .await
            .unwrap_err()
            .contains("Only Once")
    );
    assert!(fixture.dispatcher.effects.calls.lock().is_empty());
}

// Catches: an agent surviving unbound when a final transition wins during launch.
#[tokio::test]
async fn dispatch_stops_only_owned_child_when_binding_loses_terminal_transition() {
    let (mut fixture, id) = Fixture::new(None, false);
    fixture.dispatcher.effects.stop_during_launch = true;
    fixture.dispatcher.dispatch(&id).await.unwrap();
    let saved = fixture.saved(&id);
    assert_eq!(saved.status, RunStatus::Interrupted);
    assert!(saved.session_id.is_none());
    assert_eq!(
        *fixture.dispatcher.effects.stopped.lock(),
        vec!["owned-session"]
    );
}
