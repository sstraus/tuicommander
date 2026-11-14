use super::native_test_support::Fixture;
use super::neutral_execution::NeutralStatus;
use crate::agent_capabilities::{Capability, CapabilitySet};
use crate::workflows::neutral_admission::HostContext;
use serde_json::json;

// Catches native adapters substituting a stale artifact or arbitrary executable.
#[tokio::test]
async fn native_pins_and_named_verification_use_current_git_artifact() {
    let f = Fixture::new();
    let pin = f.context.pin_artifact(&["artifact.txt".into()]).unwrap();
    let run = f.start(
        "verification.run",
        json!({"check":"exact-tree","commit":pin.commit,"tree":pin.tree}),
    );
    let after = f.dispatch(&run).await;
    let effect = after.effects.values().next().unwrap();
    assert!(effect.settled(), "{effect:?}");
    if let Some(super::neutral_effects::EffectObservation {
        outcome:
            super::neutral_effects::EffectOutcome::Completed {
                result: super::neutral_execution::NeutralResult::Completed { outputs, .. },
            },
        ..
    }) = &effect.observation
    {
        assert_eq!(outputs["receipt"]["checkId"], "exact-tree");
    } else {
        panic!("native verification did not return a service receipt");
    }
    std::fs::write(f.context.workspace_path().join("artifact.txt"), "dirty").unwrap();
    assert!(
        f.context
            .verify(
                "exact-tree",
                &pin.commit,
                &pin.tree,
                f.owner.store.database_path(),
                &run.run.id
            )
            .is_err()
    );
    assert!(
        f.context
            .verify(
                "arbitrary-command",
                &pin.commit,
                &pin.tree,
                f.owner.store.database_path(),
                &run.run.id
            )
            .is_err()
    );
}

// Catches current backend revocation being replaced by the context's earlier snapshot.
#[test]
fn native_context_rechecks_revocation_and_does_not_amplify_inherited_rights() {
    let f = Fixture::new();
    f.context.current(Some(Capability::Push)).unwrap();
    let child = f.context.for_subflow(&Default::default()).unwrap();
    let child = (child.as_ref() as &dyn std::any::Any)
        .downcast_ref::<super::NativeContext>()
        .unwrap();
    f.state
        .agent_capabilities
        .insert("trusted".into(), CapabilitySet::default());
    assert!(child.current(Some(Capability::Push)).is_err());
    let narrowed = super::NativeContext::capture(
        &f.state,
        super::NativeCaller::Agent("trusted".into()),
        f.context.workspace_path(),
        vec![],
    )
    .unwrap();
    f.state.agent_capabilities.insert(
        "trusted".into(),
        CapabilitySet::from_spawn_arg(&json!(["push"])).unwrap(),
    );
    assert!(narrowed.current(Some(Capability::Push)).is_err());
    f.state.agent_capabilities.remove("trusted");
    assert!(f.context.pin_artifact(&[]).is_err());
}

// Catches references widening a child workspace/credential scope.
#[test]
fn native_subflow_refuses_foreign_workspace_artifact_and_credential() {
    use crate::workflows::data_reference::{DataReference, ReferenceInputs};
    let f = Fixture::new();
    for reference in [
        DataReference::Workspace {
            id: "/foreign".into(),
        },
        DataReference::Artifact {
            id: f.context.workspace_path().to_string_lossy().into_owned(),
            revision: "stale".into(),
        },
        DataReference::Credential {
            id: "graph-grant".into(),
        },
    ] {
        assert!(
            f.context
                .for_subflow(&ReferenceInputs {
                    fields: std::collections::BTreeMap::from([("value".into(), reference)])
                })
                .is_err()
        );
    }
}

// Catches graph JSON impersonating the caller or dispatching an uninstalled version.
#[tokio::test]
async fn native_unbound_run_and_missing_pinned_handler_refuse_without_success() {
    use super::neutral_runtime::NeutralHost;
    let f = Fixture::new();
    let run = f.start("artifact.pin", json!({"paths":["artifact.txt"]}));
    let (_, attempt) = run.pending_invocations().into_iter().next().unwrap();
    let mut invocation = super::neutral_runtime::NeutralInvocation {
        fence: attempt.fence.clone(),
        publication: run.publication.clone(),
        descriptor: run.descriptor(0).unwrap().clone(),
        instance: run.publication.definition.instances[0].clone(),
        inputs: attempt.inputs.clone(),
        subject: attempt.subject.clone(),
    };
    invocation.fence.run = "unbound".into();
    assert!(f.host.context(&invocation).is_err());
    let catalog = crate::workflows::native_operation_catalog().unwrap();
    let pin = catalog.pin(&invocation.descriptor.id).unwrap();
    let authoring = f.state.workflow_runtime.neutral_catalog_host().unwrap();
    assert_eq!(
        authoring
            .catalog(&run.publication.project)
            .unwrap()
            .pin(&invocation.descriptor.id)
            .unwrap(),
        pin
    );
    let context = authoring
        .publication_context(&run.publication.project)
        .unwrap();
    assert!(
        crate::workflows::neutral_admission::HostAdmission::dispatch(
            f.host.as_ref(),
            context.as_ref(),
            &run.publication,
            &invocation.descriptor,
            &invocation.instance,
            &invocation.inputs,
            invocation.subject.as_ref()
        )
        .is_err()
    );

    assert!(
        catalog
            .resolve_pinned(&pin, &Default::default())
            .unwrap_err()
            .contains("handler")
    );
    let forged = f.start(
        "artifact.pin",
        json!({"paths":[],"caller":"trusted","capabilities":["push"]}),
    );
    let refused = f.dispatch(&forged).await;
    assert!(matches!(refused.status, NeutralStatus::Failed { .. }));
    assert!(refused.effects.is_empty());
}

// Catches owner authentication being confused with an agent label, graph check
// names becoming commands, or a second provider installation for run binding.
#[tokio::test]
async fn native_typed_owner_and_host_checks_use_the_installed_provider() {
    use super::{NativeCaller, NativeContext};
    let mut f = Fixture::new();
    let owner = NativeContext::capture(
        &f.state,
        NativeCaller::Owner,
        f.context.workspace_path(),
        vec!["exact-tree".into()],
    )
    .unwrap();
    owner.current(Some(Capability::Push)).unwrap();
    assert!(
        NativeContext::capture(
            &f.state,
            NativeCaller::Agent("owner".into()),
            f.context.workspace_path(),
            vec![]
        )
        .is_err()
    );
    assert!(
        NativeContext::capture(
            &f.state,
            NativeCaller::Owner,
            f.context.workspace_path(),
            vec!["git push".into()]
        )
        .is_err()
    );
    let run = f.start("workspace.pin", json!({}));
    let provider = f.state.workflow_runtime.neutral_catalog_host().unwrap();
    provider
        .bind_run(&f.owner.store, &run.run.id, f.context.clone())
        .unwrap();
    assert!(
        provider
            .bind_run(&f.owner.store, &run.run.id, owner.clone())
            .is_err()
    );
    use super::neutral_effects::{EffectObservation, EffectOutcome};
    use super::neutral_execution::NeutralResult;
    let path = f.context.workspace_path();
    std::fs::write(path.join("artifact.txt"), "caller receipt change").unwrap();
    super::check::git_output(path, &["add", "."]).unwrap();
    for (context, expected) in [(f.context.clone(), "agent:trusted"), (owner, "owner")] {
        f.context = context;
        let key = super::native_review_tests::reviewed(
            &f,
            crate::workflows::review_verdict::CodeVerdict::Pass,
        )
        .await;
        let persisted = f.owner.store.neutral_execution(&key.fence.run).unwrap();
        let effect = persisted
            .effects
            .values()
            .find(|effect| effect.key == key)
            .unwrap();
        let Some(EffectObservation {
            outcome:
                EffectOutcome::Completed {
                    result: NeutralResult::Completed { outputs, .. },
                },
            ..
        }) = &effect.observation
        else {
            panic!("persisted review receipt absent");
        };
        assert_eq!(outputs["receipt"]["caller"], expected);
    }
}
