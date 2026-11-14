use super::native_test_support::Fixture;
use super::neutral_effects::NeutralEffect;
use super::neutral_runtime::{NeutralHost, NeutralInvocation};
use crate::workflows::catalog::*;
use crate::workflows::connector::{ConnectorContract, RateLimit, Reconciliation};
use crate::workflows::neutral_admission::HostAdmission;
use crate::workflows::neutral_host::NeutralCatalogHost;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn http() -> ActivityDescriptor {
    let contract = ConnectorContract {
        id: "saved-http".into(),
        authentication: None,
        resources: ObjectSchema::default(),
        pagination: None,
        rate_limit: RateLimit::FailWithRetryAfter,
        callback: None,
        reconciliation: Reconciliation::Unavailable,
    };
    ActivityDescriptor {
        id: DescriptorId {
            namespace: "user".into(),
            name: "saved-http".into(),
            version: 1,
        },
        purpose: "Write through an explicitly saved connector".into(),
        handler: HandlerRef {
            name: "host.http".into(),
            version: 1,
        },
        target: Some(ReusableTarget::Http {
            connector: contract,
            plugin: "workflow-http-test".into(),
            base: "https://example.invalid/".into(),
        }),
        config: ObjectSchema {
            fields: BTreeMap::from([(
                "request".into(),
                FieldSchema {
                    value_type: ValueType::Object,
                    required: true,
                    default: Some(
                        json!({"method":"POST","endpoint":"https://example.invalid/items","path":[],"query":{},"headers":{},"body":null,"response":{},"max_response_bytes":4096}),
                    ),
                },
            )]),
        },
        inputs: ObjectSchema::default(),
        subject: None,
        outputs: ObjectSchema::default(),
        outcomes: BTreeSet::from(["complete".into()]),
        required_capabilities: BTreeSet::new(),
        execution_policies: BTreeSet::from(["local".into()]),
    }
}

fn invocation(f: &Fixture) -> NeutralInvocation {
    let run = f.start("workspace.pin", json!({}));
    let attempt = run.attempt.unwrap();
    let descriptor = http();
    NeutralInvocation {
        fence: attempt.fence,
        publication: run.publication,
        instance: ActivityInstance {
            descriptor: descriptor.id.clone(),
            config: BTreeMap::new(),
            execution_policy: "local".into(),
        },
        descriptor,
        inputs: BTreeMap::new(),
        subject: None,
    }
}

// Catches saved HTTP metadata being dropped/rebound on reopen, and unsafe bases passing save.
#[test]
fn native_saved_http_binding_survives_reopen_and_rejects_invalid_base() {
    let f = Fixture::new();
    let db = f.dir.path().join("http-operations.sqlite3");
    let project = f.context.workspace_path().to_string_lossy();
    let catalog = f.host.catalog(&project).unwrap();
    let registry = f.host.registry();
    let store = crate::workflows::WorkflowStore::open_at(&db).unwrap();
    let descriptor = http();
    store
        .save_reusable_operation(&project, 0, descriptor.clone(), &catalog, &registry)
        .unwrap();
    drop(store);
    let store = crate::workflows::WorkflowStore::open_at(&db).unwrap();
    assert_eq!(
        store
            .get_reusable_operation(&project, &descriptor.id, &catalog, &registry)
            .unwrap(),
        descriptor
    );
    let combined = store
        .build_project_catalog(&project, &catalog, &registry)
        .unwrap();
    assert_eq!(combined.get(&descriptor.id), Some(&descriptor));
    let mut invalid = http();
    invalid.id.name = "unsafe-base".into();
    if let Some(ReusableTarget::Http { base, .. }) = &mut invalid.target {
        *base = "file:///private/".into();
    }
    assert!(
        store
            .save_reusable_operation(&project, 0, invalid, &catalog, &registry)
            .is_err()
    );
}

// Catches a configured connector using an agent's ordinary transport admission as Operator.
#[test]
fn native_connector_agent_without_operator_is_refused() {
    let f = Fixture::new();
    let inv = invocation(&f);
    let error = f
        .host
        .dispatch(
            &f.context,
            &inv.publication,
            &inv.descriptor,
            &inv.instance,
            &inv.inputs,
            None,
        )
        .unwrap_err();
    assert!(error.contains("operator"), "{error}");
}

// Catches transport/caller authority substituting for exact write approval in the existing ledger.
#[tokio::test]
async fn native_connector_mutation_requires_exact_ledger_approval() {
    let mut f = Fixture::new();
    f.state.agent_capabilities.insert(
        "trusted".into(),
        crate::agent_capabilities::CapabilitySet::from_spawn_arg(&json!(["push", "operator"]))
            .unwrap(),
    );
    f.context = super::NativeContext::capture(
        &f.state,
        super::NativeCaller::Agent("trusted".into()),
        f.context.workspace_path(),
        vec![],
    )
    .unwrap();
    let inv = invocation(&f);
    let prepared = f.host.prepare(&inv).unwrap();
    assert!(prepared.mutation && prepared.approval.is_some());
    let mut effect = NeutralEffect::prepare(inv, prepared).unwrap();
    effect.dispatched = true;
    let error = f.host.invoke_effect(effect.clone()).await.unwrap_err();
    assert!(error.contains("approval/dispatch"), "{error}");
    effect.answer = Some(true);
    assert!(
        f.host.invoke_effect(effect).await.is_err(),
        "a fabricated answer is not an exact retained ledger effect"
    );
}

// Catches a synthetic preview fence requiring a persisted run/caller binding.
#[test]
fn native_preview_prepares_intent_without_binding_or_dispatch_authority() {
    let f = Fixture::new();
    let mut inv = invocation(&f);
    inv.fence.run = "preview".into();
    f.host.catalog(&inv.publication.project).unwrap();
    assert!(f.host.context(&inv).is_err());
    assert!(f.host.preview(&inv).unwrap().mutation);
    let native = f.start("workspace.pin", json!({}));
    let instance = native.publication.definition.instances[0].clone();
    let mut inv = NeutralInvocation {
        fence: native.attempt.unwrap().fence,
        publication: native.publication,
        descriptor: native.run.closure.descriptors[0].clone(),
        instance,
        inputs: BTreeMap::from([("request".into(), json!({}))]),
        subject: Some(BTreeMap::from([(
            "workspace".into(),
            json!({"reference":"workspace","id":f.context.workspace_path()}),
        )])),
    };
    inv.fence.run = "preview".into();
    assert!(f.host.context(&inv).is_err());
    assert!(!f.host.preview(&inv).unwrap().mutation);
}

// Catches two installed GitLab projects replacing each other's advertised descriptor pins.
#[test]
fn native_gitlab_factory_distinguishes_installed_projects() {
    use crate::workflows::gitlab::{GitLabInstall, Operation};
    let a = GitLabInstall::new(
        "https://example.invalid/api/v4/".into(),
        1,
        "account".into(),
    )
    .unwrap();
    let b = GitLabInstall::new(
        "https://example.invalid/api/v4/".into(),
        2,
        "account".into(),
    )
    .unwrap();
    let mut catalog = WorkflowCatalog::new();
    catalog.register(a.descriptor(Operation::Issue)).unwrap();
    catalog.register(b.descriptor(Operation::Issue)).unwrap();
    assert_ne!(
        a.descriptor(Operation::Issue).handler,
        b.descriptor(Operation::Issue).handler
    );
}

// Catches discovery inventing a model profile, or preparation substituting a missing model.
#[test]
fn native_model_catalog_requires_configured_profile_and_explicit_request() {
    let f = Fixture::new();
    let project = f.context.workspace_path().to_string_lossy();
    let id = DescriptorId {
        namespace: "model".into(),
        name: "request".into(),
        version: 1,
    };
    assert!(f.host.catalog(&project).unwrap().get(&id).is_none());
    {
        let mut config = f.state.config.write();
        config.ego_executable = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        config.ego_profile = "exact-fixture-profile".into();
    }
    let descriptor = f.host.catalog(&project).unwrap().get(&id).unwrap().clone();
    let mut inv = invocation(&f);
    inv.descriptor = descriptor.clone();
    inv.instance.descriptor = id;
    inv.instance.config.insert("request".into(), json!({
        "profile":"exact-fixture-profile", "model":"", "effort":"high", "instructions":"literal", "context":[],
        "output_schema":{"fields":{}}, "limits":{"timeout_ms":1000,"max_context_bytes":1024,"max_output_bytes":1024}, "unavailable_policy":"fail"
    }));
    assert!(
        f.host
            .preview(&inv)
            .unwrap_err()
            .contains("model and effort")
    );
    inv.instance.config.get_mut("request").unwrap()["model"] = json!("exact-fixture-model");
    assert!(f.host.preview(&inv).unwrap().model_tools.is_some());
    inv.instance.config.get_mut("request").unwrap()["profile"] = json!("another-profile");
    assert!(f.host.preview(&inv).is_err());
}
