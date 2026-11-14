use super::{native::NativeEffects, *};
use crate::automations::{precheck::PrecheckOutcome, runtime::AutomationRuntime};
use crate::test_support::test_temp_root;
use std::{path::Path, sync::Arc};

fn git(repo: &Path, args: &[&str]) {
    crate::git_cli::git_cmd(repo).args(args).run().unwrap();
}

fn repo(root: &Path) -> std::path::PathBuf {
    let repo = root.join("repository");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-b", "main"]);
    git(&repo, &["config", "user.email", "test@example.invalid"]);
    git(&repo, &["config", "user.name", "Automation Test"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("input.txt"), "configured base").unwrap();
    git(&repo, &["add", "input.txt"]);
    git(&repo, &["commit", "-m", "base"]);
    git(&repo, &["branch", "configured-base"]);
    std::fs::write(repo.join("input.txt"), "current HEAD").unwrap();
    git(&repo, &["commit", "-am", "current"]);
    repo
}

fn definition(repo: &Path) -> AutomationDefinition {
    serde_json::from_value(serde_json::json!({
        "id":"native-once", "name":"Native Once", "prompt":"Exact literal prompt",
        "run_config":"missing-automation-profile", "repository":repo, "workspace":{"mode":"existing"},
        "cron":"", "once_local":"2099-01-01T12:00:00", "timezone":"UTC", "enabled":true,
        "grace_secs":43200, "overlap":"skip", "max_duration_secs":3600, "precheck":null
    })).unwrap()
}

// Catches: automation worktrees being created from current HEAD instead of the configured base.
#[tokio::test]
async fn native_worktree_uses_configured_base_and_existing_workspace_uses_repository() {
    let root = tempfile::tempdir_in(test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(root.path().to_owned());
    let repo = repo(root.path());
    let state = Arc::new(crate::state::tests_support::make_test_app_state_in(
        root.path(),
    ));
    let effects = NativeEffects(state);
    let mut definition = definition(&repo);
    let existing = effects.workspace(&definition, "existing").await.unwrap();
    assert_eq!(Path::new(&existing.path), repo.canonicalize().unwrap());
    assert!(existing.id.is_none());
    definition.workspace = Workspace::NewPerRun {
        base_branch: "configured-base".into(),
    };
    let created = effects.workspace(&definition, "base-test").await.unwrap();
    assert_ne!(created.path, existing.path);
    assert!(created.id.is_some());
    assert_eq!(
        std::fs::read_to_string(Path::new(&created.path).join("input.txt")).unwrap(),
        "configured base"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("input.txt")).unwrap(),
        "current HEAD"
    );
}

// Catches: boot retrying open runs, double-start recovery, or missing profiles falling back to another CLI.
#[tokio::test]
async fn shared_runtime_recovers_once_and_manual_dispatch_records_native_spawn_failure() {
    let root = tempfile::tempdir_in(test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(root.path().to_owned());
    let repo = repo(root.path());
    let definitions = DefinitionStore::new();
    let definition = definition(&repo);
    definitions.create(definition.clone()).unwrap();
    let owner = crate::automations::store::RunOwner::acquire(0).unwrap();
    let old = owner
        .store()
        .reserve(&definition, RunTrigger::Manual, 1)
        .unwrap()
        .unwrap();
    drop(owner);
    let state = Arc::new(crate::state::tests_support::make_test_app_state_in(
        root.path(),
    ));
    AutomationRuntime::spawn(&state);
    // Setup readiness is not a behavior deadline: nextest owns the outer hang bound.
    loop {
        match AutomationRuntime::run_now(&state, "missing").await {
            Err(error) if error.contains("this process is not the owner") => {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await
            }
            Err(error) => {
                assert_eq!(error, "Automation not found");
                break;
            }
            Ok(_) => panic!("missing automation must not run"),
        }
    }
    AutomationRuntime::spawn(&state);
    let run = AutomationRuntime::run_now(&state, "native-once")
        .await
        .unwrap();
    let reader = RunStore::open().unwrap();
    assert_eq!(reader.get(&old.id).unwrap().status, RunStatus::Interrupted);
    let saved = reader.get(&run.id).unwrap();
    assert_eq!(saved.status, RunStatus::Failed);
    assert_eq!(saved.precheck_outcome, Some(PrecheckOutcome::Bypassed));
    assert_eq!(
        saved.reason.as_deref(),
        Some(
            "Automation run configuration 'missing-automation-profile' not found. Choose a saved run configuration: Codex Default"
        )
    );
    assert!(state.session_maps.sessions.is_empty());
}

fn due_every_minute(repo: &Path) -> AutomationDefinition {
    serde_json::from_value(serde_json::json!({
        "id":"due-now", "name":"Due Now", "prompt":"Exact literal prompt",
        "run_config":"missing-automation-profile", "repository":repo, "workspace":{"mode":"existing"},
        "cron":"* * * * *", "timezone":"UTC", "enabled":true,
        "grace_secs":3600, "overlap":"skip", "max_duration_secs":3600, "precheck":null
    }))
    .unwrap()
}

// Control for the disabled case: the same due schedule does start a run when automations are on.
// Catches: a RED that passes because the fixture never makes the schedule due.
#[tokio::test]
async fn a_due_schedule_starts_a_run_when_automations_are_enabled() {
    let root = tempfile::tempdir_in(test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(root.path().to_owned());
    let repo = repo(root.path());
    DefinitionStore::new()
        .create(due_every_minute(&repo))
        .unwrap();
    let state = Arc::new(crate::state::tests_support::make_test_app_state_in(
        root.path(),
    ));
    AutomationRuntime::spawn(&state);
    // Owner acquisition and the first tick are setup; nextest owns the outer hang bound.
    loop {
        if let Some(store) = state.automation_runtime.notification_store()
            && !store.history(None, 1, 0).unwrap().is_empty()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

// Catches: AutomationRuntime ignoring the daemon option, so a due schedule still starts a run.
#[tokio::test]
async fn a_disabled_runtime_starts_no_run_for_a_due_schedule() {
    let root = tempfile::tempdir_in(test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(root.path().to_owned());
    let repo = repo(root.path());
    DefinitionStore::new()
        .create(due_every_minute(&repo))
        .unwrap();
    let state = Arc::new(crate::state::tests_support::make_test_app_state_in(
        root.path(),
    ));
    state.automation_runtime.disable();
    AutomationRuntime::spawn(&state);
    // The enabled control starts its run within one scheduler wake; give this one far more.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert!(state.automation_runtime.notification_store().is_none());
    assert!(
        !root.path().join("automation_runs.sqlite3").exists(),
        "a disabled runtime must not even open the run ledger"
    );
    assert!(
        AutomationRuntime::run_now(&state, "due-now").await.is_err(),
        "a disabled runtime must refuse a manual start"
    );
    assert!(state.session_maps.sessions.is_empty());
}

// Catches: runs stuck Running forever after a restart with the flag, and a refusal that
// blames ownership.
#[tokio::test]
async fn no_automations_restart_reconciles_open_runs() {
    let root = tempfile::tempdir_in(test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(root.path().to_owned());
    let repo = repo(root.path());
    let owner = crate::automations::store::RunOwner::acquire(0).unwrap();
    let open = owner
        .store()
        .reserve(&definition(&repo), RunTrigger::Manual, 1)
        .unwrap()
        .unwrap();
    drop(owner);
    let state = Arc::new(crate::state::tests_support::make_test_app_state_in(
        root.path(),
    ));
    state.automation_runtime.disable();
    AutomationRuntime::spawn(&state);
    let reader = RunStore::open().unwrap();
    // Setup readiness is not a behavior deadline: nextest owns the outer hang bound.
    while reader.get(&open.id).unwrap().status.is_open() {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(reader.get(&open.id).unwrap().status, RunStatus::Interrupted);
    let error = AutomationRuntime::run_now(&state, "native-once")
        .await
        .unwrap_err();
    assert!(error.contains("--no-automations"), "{error}");
}

// Catches: a --no-automations worker keeping the run-ledger owner lock after it settled the
// runs a previous process left open, so an enabled instance that shares its config dir and
// starts later never gets an automation executor while the worker lives.
#[tokio::test]
async fn a_disabled_runtime_releases_the_ledger_owner_after_settling_left_over_runs() {
    let root = tempfile::tempdir_in(test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(root.path().to_owned());
    let repo = repo(root.path());
    let definition = definition(&repo);
    let owner = crate::automations::store::RunOwner::acquire(0).unwrap();
    let left_over = owner
        .store()
        .reserve(&definition, RunTrigger::Manual, 1)
        .unwrap()
        .unwrap();
    drop(owner);
    let state = Arc::new(crate::state::tests_support::make_test_app_state_in(
        root.path(),
    ));
    state.automation_runtime.disable();
    AutomationRuntime::spawn(&state);
    // Settlement is setup; nextest owns the outer hang bound.
    let reader = crate::automations::store::RunStore::open().unwrap();
    loop {
        let runs = reader.history(None, 100, 0).unwrap();
        let run = runs.iter().find(|run| run.id == left_over.id).unwrap();
        if run.status != RunStatus::Reserved {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    // Behaviour under test: once the runs are settled, the lock is free promptly.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match crate::automations::store::RunOwner::acquire(2) {
            Ok(_) => break,
            Err(error) if std::time::Instant::now() >= deadline => panic!(
                "the enabled instance must be able to own the ledger after the disabled \
                 worker settled it: {error}"
            ),
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
        }
    }
}
