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
        Some("Automation run configuration 'missing-automation-profile' not found")
    );
    assert!(state.session_maps.sessions.is_empty());
}

// Catches: common launch extraction removing the workflow actor's ownership fence.
#[test]
fn automation_launch_extraction_preserves_workflow_owner_check() {
    let root = tempfile::tempdir_in(test_temp_root()).unwrap();
    let state = Arc::new(crate::state::tests_support::make_test_app_state_in(
        root.path(),
    ));
    let error = crate::mcp_http::mcp_transport::launch_daemon_workflow_agent(
        &state,
        "missing-run",
        "missing-attempt",
        "unused",
        "unused",
        None,
    )
    .unwrap_err();
    assert!(error.contains("does not own the run database"));
}
