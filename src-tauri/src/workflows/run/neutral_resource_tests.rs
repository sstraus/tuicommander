use super::*;

impl crate::workflows::neutral_host::NeutralCatalogHost for FileHost {
    fn catalog(&self, _: &str) -> Result<crate::workflows::catalog::WorkflowCatalog, String> {
        Err("run-only host has no authoring catalog".into())
    }
    fn publication_context(&self, _: &str) -> Result<Box<dyn HostContext>, String> {
        Err("run-only host does not publish".into())
    }
}

fn another_run(
    dir: &std::path::Path,
    owner: &RuntimeOwner,
    run: &NeutralExecution,
) -> NeutralExecution {
    let workflows =
        crate::workflows::WorkflowStore::open_at(&dir.join("definitions.sqlite3")).unwrap();
    let mut catalog = crate::workflows::catalog::WorkflowCatalog::new();
    let mut handlers = BTreeSet::new();
    for descriptor in &run.run.closure.descriptors {
        handlers.insert(descriptor.handler.clone());
        catalog.register(descriptor.clone()).unwrap();
    }
    let mut request = run.run.request.clone();
    request.request_id = "resource-waiter".into();
    let other = owner
        .start_neutral(&workflows, &request, &catalog, &handlers)
        .unwrap();
    owner.store.neutral_execution(&other.id).unwrap()
}

fn resolution(key: &EffectKey, evidence: &str) -> Command {
    serde_json::from_value(json!({"type":"effect", "command":{
        "type":"resolve_mutation", "key":key, "evidence":evidence,
        "stop_proof":"recorded_observation"
    }}))
    .unwrap()
}

// Catches: silent resource conflicts, terminal runs with unreleasable uncertainty,
// and resource release notifying only its own actor instead of the waiting run.
#[tokio::test]
async fn cancelled_uncertain_mutation_records_wait_and_operator_release_wakes_dispatch() {
    let config = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(config.path().into());
    let (dir, owner, run, at) = fixture(30, false);
    let run = prepared(&owner, &run, at);
    let host = FileHost::new(&owner, dir.path().join("write"), Reply::Pending, false);
    owner.neutral_host(host.clone());
    let pending = dispatch(&owner, &run.run.id).await;
    let key = only_key(&pending);
    let uncertain = owner
        .neutral_effect_command(
            &run.run.id,
            EffectCommand::Observe {
                key: key.clone(),
                observation: EffectObservation {
                    stopped: true,
                    outcome: EffectOutcome::Uncertain {
                        diagnostic: "Inspect the original workspace before releasing ownership"
                            .into(),
                    },
                },
            },
        )
        .unwrap();
    let cancelled = owner
        .neutral_command(&run.run.id, "cancel", uncertain.sequence, Command::Cancel)
        .unwrap();
    assert_eq!(
        claims(&owner),
        1,
        "cancellation must retain uncertain ownership"
    );
    let other = another_run(dir.path(), &owner, &run);
    let other = prepared(&owner, &other, at);
    let notify = Arc::new(Notify::new());
    assert!(owner.dispatch_neutral(&other, &notify).unwrap());
    let (waiting, events) = owner
        .store
        .neutral_inspection(&other.run.id, other.sequence, 500)
        .unwrap();
    let wait = serde_json::to_value(&waiting).unwrap();
    let effect = wait["effects"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(effect["resource_wait"]["resource"], "owned-test-file");
    assert_eq!(effect["resource_wait"]["owner_run"], run.run.id);
    assert_eq!(effect["resource_wait"]["owner_effect"], key.digest);
    assert!(
        serde_json::to_string(&events)
            .unwrap()
            .contains("resource_wait")
    );
    assert!(
        !owner.dispatch_neutral(&waiting, &notify).unwrap(),
        "unchanged waits must not spin or duplicate events"
    );
    assert_eq!(host.calls.load(Ordering::Acquire), 1);
    // Catches: a durable wait failing to update the sequence-fenced summary/UI.
    let rows = owner
        .store
        .neutral_run_views(&run.run.request.publication.project)
        .unwrap();
    let row = rows.iter().find(|row| row["id"] == other.run.id).unwrap();
    assert_eq!(
        row["resource_waits"],
        json!([effect["resource_wait"].clone()])
    );
    let cached_sequence: i64 = owner
        .store
        .connect()
        .unwrap()
        .query_row(
            "SELECT sequence FROM neutral_run_summaries WHERE run_id=?1",
            [&other.run.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cached_sequence, waiting.sequence);
    let view = owner
        .store
        .neutral_inspection_view(&other.run.id, 0, 500)
        .unwrap();
    assert_eq!(
        view["progress"]["node_states"][&waiting.node],
        "resource_wait"
    );
    assert_eq!(view["progress"]["resource_waits"], row["resource_waits"]);

    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    state.workflow_runtime.attach_owner(owner.clone());
    state
        .workflow_runtime
        .install_neutral_catalog_host(host.clone());
    state.workflow_runtime.wake(&other.run.id);
    tokio::task::yield_now().await; // let the waiting actor subscribe before release
    let evidence = "Verified the original process stopped; inspected and reconciled the workspace";
    let action = serde_json::from_value(json!({"action":"resolve_mutation", "run_id":run.run.id,
        "command_id":"operator-resolution", "expected_sequence":cancelled.sequence,
        "key":key, "evidence":evidence}))
    .unwrap();
    let reply = super::super::neutral_api::run_action(
        &state,
        &run.run.request.publication.project,
        action,
        Some(crate::workflows::NativeCaller::Owner),
    )
    .unwrap();
    let crate::workflows::neutral_api::model::NeutralReply::Inspection(view) = reply else {
        panic!("resolution must return inspection");
    };
    assert!(view.is_object());
    let resolved = owner.store.neutral_execution(&run.run.id).unwrap();
    assert_eq!(
        resolved.status, cancelled.status,
        "resolution must never reopen the cancelled graph"
    );
    assert_eq!(
        resolved.effect(&key).unwrap().observation,
        uncertain.effect(&key).unwrap().observation,
        "retain original uncertainty and procedure"
    );
    assert!(serde_json::to_string(&resolved).unwrap().contains(evidence));
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let changed = owner.neutral_effect_updates.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if host.calls.load(Ordering::Acquire) == 2 {
                break;
            }
            changed.await;
        }
    })
    .await
    .expect("resource release must dispatch the waiting actor without a new start request");
    let dispatched = owner.store.neutral_execution(&other.run.id).unwrap();

    // A delayed pre-release projection must not resurrect the old wait.
    super::super::neutral_summaries::record(&owner.store.connect().unwrap(), &waiting, at).unwrap();
    let rows = owner
        .store
        .neutral_run_views(&run.run.request.publication.project)
        .unwrap();
    let row = rows.iter().find(|row| row["id"] == other.run.id).unwrap();
    assert_eq!(row["resource_waits"], json!([]));
    let effect = dispatched.effects.values().next().unwrap();
    assert!(effect.dispatched);
    assert!(
        serde_json::to_value(effect)
            .unwrap()
            .get("resource_wait")
            .is_none()
    );
}

#[path = "neutral_resource_resolution_tests.rs"]
mod resolution_tests;
