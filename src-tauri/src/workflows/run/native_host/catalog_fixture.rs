//! Direct production-service wire capture: no run binding, dispatch or network.
mod correction;
mod setup;
use super::NativeHost;
use crate::workflows::WorkflowStore;
use crate::workflows::catalog::*;
use crate::workflows::data_binding::DataSource;
use crate::workflows::neutral_admission::NeutralAdmissionService;
use crate::workflows::neutral_api::{catalog_reply, model::NeutralReply, validation};
use crate::workflows::neutral_host::NeutralCatalogHost;
use crate::workflows::neutral_publication::{NeutralDefinition, PublicationPin, PublicationRef};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const PROJECT: &str = "/workflow-fixture/native";

fn definition(descriptor: &ActivityDescriptor) -> NeutralDefinition {
    let port = descriptor.outcomes.first().unwrap();
    let policy = descriptor.execution_policies.first().unwrap();
    let bindings: BTreeMap<_, _> = descriptor
        .inputs
        .fields
        .keys()
        .map(|field| {
            (
                field.clone(),
                DataSource::WorkflowInput {
                    field: field.clone(),
                },
            )
        })
        .collect();
    let outputs: BTreeMap<_, _> = descriptor
        .outputs
        .fields
        .keys()
        .map(|field| {
            (
                field.clone(),
                DataSource::ActivityOutput {
                    activity: 0,
                    field: field.clone(),
                },
            )
        })
        .collect();
    serde_json::from_value(json!({
        "control":{"nodes":{"start":{"type":"start"},"work":{"type":"activity","activity":0},
            "end":{"type":"end","outcome":"done"}},"edges":{"start":{"next":"work"},"work":{(port):"end"}}},
        "inputs":descriptor.inputs,"subject":descriptor.subject,"outputs":descriptor.outputs,"outcomes":["done"],
        "instances":[{"descriptor":descriptor.id,"config":{},"execution_policy":policy}],
        "input_bindings":{"0":bindings},"output_bindings":outputs,
        "conditions":{},"collections":{},"loops":{},"forks":{},"subflows":[]
    })).unwrap()
}

fn capture() -> Value {
    let root = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let _guard = crate::config::set_config_dir_override(root.path().into());
    let state = Arc::new(crate::state::tests_support::make_test_app_state());
    setup::configure(root.path(), &state);
    let host = NativeHost::install(&state).unwrap();
    let store = WorkflowStore::open_at(&root.path().join("workflows.sqlite3")).unwrap();
    let installed = host.catalog(PROJECT).unwrap();
    let registry = crate::workflows::run::neutral_runtime::NeutralHost::registry(host.as_ref());
    let script = setup::script();
    let http = crate::workflows::run::native_family_tests::http();
    for descriptor in [&script, &http] {
        store
            .save_reusable_operation(PROJECT, 0, descriptor.clone(), &installed, &registry)
            .unwrap();
    }
    let mut catalog = store
        .build_project_catalog(PROJECT, &installed, &registry)
        .unwrap();
    let context = host.publication_context(PROJECT).unwrap();
    let child = NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    }
    .publish(
        Some(context.as_ref()),
        PROJECT,
        "calculate-child",
        0,
        definition(&script),
    )
    .unwrap();
    let mut subflow = script.clone();
    subflow.id.name = "calculate-subflow".into();
    subflow.purpose = "Call an exact pinned calculation publication".into();
    subflow.handler = HandlerRef {
        name: "host.subflow".into(),
        version: 1,
    };
    subflow.config = ObjectSchema::default();
    subflow.outcomes = BTreeSet::from(["done".into()]);
    subflow.target = Some(ReusableTarget::Subflow {
        publication: PublicationPin {
            reference: PublicationRef {
                project: PROJECT.into(),
                id: child.id.clone(),
                revision: child.revision,
            },
            sha256: content_digest(&child).unwrap(),
        },
    });
    store
        .save_reusable_operation(PROJECT, 0, subflow.clone(), &catalog, &registry)
        .unwrap();
    catalog = store
        .build_project_catalog(PROJECT, &installed, &registry)
        .unwrap();
    let valid = definition(&subflow);
    let valid_reply = validation::validate(&valid, &catalog, &registry.available_handlers());
    assert!(valid_reply.valid);
    // Prove actual backend publication admission traverses the pinned child.
    NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    }
    .publish(
        Some(context.as_ref()),
        PROJECT,
        "calculate-parent",
        0,
        valid.clone(),
    )
    .unwrap();
    let mut invalid = valid.clone();
    invalid.input_bindings.get_mut(&0).unwrap().insert(
        "value".into(),
        DataSource::Constant {
            value: json!("incompatible integer input"),
        },
    );
    let invalid_reply = validation::validate(&invalid, &catalog, &registry.available_handlers());
    assert!(!invalid_reply.valid && !invalid_reply.diagnostics.is_empty());
    let native = catalog
        .get(&DescriptorId {
            namespace: "native".into(),
            name: "commit.create".into(),
            version: 1,
        })
        .unwrap();
    let preview = definition(native);
    let inputs = BTreeMap::from([(
        "request".into(),
        json!({"parent":"0000000000000000000000000000000000000000",
        "tree":"1111111111111111111111111111111111111111","message":"Sample approved candidate",
        "review":{"fence":{"run":"sample-review","execution":"sample-review","activation":1,"attempt":1,"scope":null},
            "digest":"2222222222222222222222222222222222222222222222222222222222222222"}}),
    )]);
    let subject = Some(BTreeMap::from([(
        "workspace".into(),
        json!({"reference":"workspace","id":PROJECT}),
    )]));
    let reply = validation::preview(
        host.as_ref(),
        PROJECT,
        preview.clone(),
        inputs.clone(),
        subject.clone(),
        &catalog,
        &registry.available_handlers(),
    )
    .unwrap();
    assert_eq!(reply["activities"][0]["effect_intent"]["mutation"], true);
    assert!(reply["activities"][0]["effect_intent"]["approval"].is_object());
    let correction = correction::capture(root.path(), &store, host.as_ref(), &mut catalog);
    let catalog_reply = catalog_reply(&store, PROJECT, &catalog, host.as_ref()).unwrap();
    let namespaces: BTreeSet<_> = catalog
        .descriptors()
        .map(|d| d.id.namespace.as_str())
        .collect();
    for family in [
        "native", "custom", "user", "model", "jira", "gitlab", "telegram", "github",
    ] {
        assert!(
            namespaces.contains(family),
            "configured family absent: {family}"
        );
    }
    json!({"project":PROJECT,"catalog":catalog_reply,
        "preparation":{"definition":preview,"inputs":inputs,"subject":subject,"reply":NeutralReply::Preview(reply)},
        "subflow":{"valid":{"definition":valid,"reply":NeutralReply::Validation(valid_reply)},
            "invalid":{"definition":invalid,"reply":NeutralReply::Validation(invalid_reply)}},
        "nested_publication":child,"correction_region":correction})
}

// Catches production descriptor, preparation or subflow diagnostics drifting
// from the Designer's captured contract, including a configured family omission.
#[test]
fn native_catalog_wire_matches_shared_designer_fixture() {
    let actual = capture();
    if std::env::var_os("TUIC_CAPTURE_NATIVE_CATALOG").is_some() {
        println!(
            "TUIC_NATIVE_CATALOG={}",
            serde_json::to_string(&actual).unwrap()
        );
        return; // Generator mode is not drift-validation evidence.
    }
    let fixture: Value = serde_json::from_slice(
        &std::fs::read("src/workflows/fixtures/neutral-native-catalog.json")
            .expect("capture the native catalog first"),
    )
    .unwrap();
    assert_eq!(
        fixture, actual,
        "update the wire capture and Designer contract together"
    );
}
