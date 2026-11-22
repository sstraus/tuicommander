use super::super::neutral_execution::NeutralStatus;
use super::fixtures::*;
use serde_json::json;
use std::sync::Arc;

// Catches: an expired in-flight mutation remaining locked forever when its
// original process exited but the stopped callback was lost before persistence.
#[cfg(unix)]
#[test]
fn expired_mutation_with_lost_stop_callback_resolves_from_original_process_proof() {
    let script = "from pathlib import Path; Path('mutation-proof').write_text('once')";
    let (dir, owner, effect, adapter) = prepared(script, &policy(true));
    let observation = adapter.invoke(&effect, dir.path());
    assert!(observation.effect.stopped);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("mutation-proof")).unwrap(),
        "once"
    );
    // Lose the completion before persistence: the durable effect is still in flight.
    let run = owner
        .store
        .neutral_execution(&effect.key.fence.run)
        .unwrap();
    assert!(run.effect(&effect.key).unwrap().observation.is_none());
    let expired = owner
        .neutral_command_at(
            &run.run.id,
            "expire",
            run.sequence,
            super::super::neutral_execution::NeutralExecutionCommand::Step,
            run.deadline_ms,
        )
        .unwrap();
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    state.workflow_runtime.attach_owner(owner.clone());
    state
        .workflow_runtime
        .install_neutral_catalog_host(Arc::new(Host {
            adapter,
            workspace: dir.path().into(),
        }));
    let action = serde_json::from_value(json!({"action":"resolve_mutation", "run_id":run.run.id,
        "command_id":"reconcile", "expected_sequence":expired.sequence, "key":effect.key,
        "evidence":"Inspected original workspace and retained procedure; mutation reconciled"}))
    .unwrap();
    // Enter the same runtime service used by HTTP, IPC and MCP.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    super::super::neutral_api::run_action(
        &state,
        &run.run.request.publication.project,
        action,
        Some(crate::workflows::NativeCaller::Owner),
    )
    .unwrap();
    let resolved = owner.store.neutral_execution(&run.run.id).unwrap();
    assert_eq!(resolved.status, NeutralStatus::Expired);
    assert!(resolved.effect(&effect.key).unwrap().observation.is_none());
    let wire = serde_json::to_value(resolved.effect(&effect.key).unwrap()).unwrap();
    assert_eq!(
        wire["mutation_resolution"]["stop_proof"],
        "original_process_gone"
    );
    let claims: i64 = owner
        .store
        .connect()
        .unwrap()
        .query_row("SELECT count(*) FROM neutral_effect_resources", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(claims, 0);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("mutation-proof")).unwrap(),
        "once"
    );
}

// Catches: operator reconciliation releasing a claim while a recorded original
// process group still exists after the application's process registry was lost.
#[cfg(unix)]
#[test]
fn mutation_resolution_api_refuses_live_original_process_and_keeps_claim() {
    use std::os::unix::process::CommandExt;
    let (dir, owner, effect, adapter) = prepared(ECHO, &policy(true));
    let run = owner
        .store
        .neutral_execution(&effect.key.fence.run)
        .unwrap();
    let expired = owner
        .neutral_command_at(
            &run.run.id,
            "expire",
            run.sequence,
            super::super::neutral_execution::NeutralExecutionCommand::Step,
            run.deadline_ms,
        )
        .unwrap();
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .unwrap();
    let name: String = super::owner(&effect)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let record = adapter.runs_db.with_extension("processes").join(name);
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    std::fs::write(&record, child.id().to_string()).unwrap();
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    state.workflow_runtime.attach_owner(owner.clone());
    state
        .workflow_runtime
        .install_neutral_catalog_host(Arc::new(Host {
            adapter,
            workspace: dir.path().into(),
        }));
    let action = serde_json::from_value(json!({"action":"resolve_mutation", "run_id":run.run.id,
        "command_id":"unsafe-resolution", "expected_sequence":expired.sequence, "key":effect.key,
        "evidence":"operator assertion cannot replace process proof"}))
    .unwrap();
    let result = super::super::neutral_api::run_action(
        &state,
        &run.run.request.publication.project,
        action,
        Some(crate::workflows::NativeCaller::Owner),
    );
    child.kill().unwrap();
    child.wait().unwrap();
    let refusal = result.unwrap_err();
    assert!(refusal.contains("may still be running"), "{refusal}");
    assert!(refusal.contains(&effect.key.digest));
    let claims: i64 = owner
        .store
        .connect()
        .unwrap()
        .query_row("SELECT count(*) FROM neutral_effect_resources", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(claims, 1);
    assert!(
        owner
            .store
            .neutral_execution(&run.run.id)
            .unwrap()
            .effect(&effect.key)
            .unwrap()
            .mutation_resolution
            .is_none()
    );
}

// Catches: after a restart the production host refusing reconciliation with a
// bare "binding unavailable" that never names the rebind recovery, or the hint
// changing the refusal class away from not-found.
#[cfg(unix)]
#[test]
fn mutation_resolution_on_unbound_native_host_names_rebind_recovery() {
    let (_dir, owner, effect, _adapter) = prepared(ECHO, &policy(true));
    let run = owner
        .store
        .neutral_execution(&effect.key.fence.run)
        .unwrap();
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    state.workflow_runtime.attach_owner(owner.clone());
    super::super::native_host::NativeHost::install(&state).unwrap();
    let action = serde_json::from_value(json!({"action":"resolve_mutation", "run_id":run.run.id,
        "command_id":"after-restart", "expected_sequence":run.sequence, "key":effect.key,
        "evidence":"Original process stopped; workspace reconciled"}))
    .unwrap();
    let refusal = super::super::neutral_api::run_action(
        &state,
        &run.run.request.publication.project,
        action,
        Some(crate::workflows::NativeCaller::Owner),
    )
    .unwrap_err();
    assert!(
        refusal.contains("original start request again"),
        "{refusal}"
    );
    assert_eq!(
        crate::workflows::neutral_api::refusal::classify(&refusal),
        Some(crate::workflows::neutral_api::refusal::Refusal::NotFound)
    );
}
