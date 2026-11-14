use super::WorkflowStore;
use super::catalog::{
    ActivityDescriptor, ActivityInstance, DescriptorId, FieldSchema, HandlerRef, ObjectSchema,
    ReusableTarget, ValueType, WorkflowCatalog, content_digest,
};
use super::command::Invocation;
use super::neutral_admission::{
    HostAdmission, HostContext, HostRegistry, NeutralAdmissionService, ReusableHandlerKind,
};
use super::neutral_publication::{
    NeutralDefinition, NeutralPublication, PublicationPin, PublicationRef,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn schema() -> ObjectSchema {
    ObjectSchema {
        fields: BTreeMap::from([(
            "text".into(),
            FieldSchema {
                value_type: ValueType::String,
                required: true,
                default: None,
            },
        )]),
    }
}

fn script() -> ActivityDescriptor {
    ActivityDescriptor {
        id: DescriptorId {
            namespace: "user".into(),
            name: "transform".into(),
            version: 1,
        },
        purpose: "Transform text".into(),
        handler: HandlerRef {
            name: "host.script".into(),
            version: 1,
        },
        target: Some(ReusableTarget::Script {
            invocation: Invocation::Shell {
                program: "sh".into(),
                script: "printf '%s' 'inline body'".into(),
                version: 1,
            },
        }),
        config: ObjectSchema::default(),
        inputs: schema(),
        subject: None,
        outputs: schema(),
        outcomes: BTreeSet::from(["done".into()]),
        required_capabilities: BTreeSet::new(),
        execution_policies: BTreeSet::from(["local".into()]),
    }
}

fn definition() -> NeutralDefinition {
    NeutralDefinition {
        control: crate::workflows::neutral_control::ControlGraph {
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
        },
        input_bindings: BTreeMap::new(),
        conditions: BTreeMap::new(),
        collections: BTreeMap::new(),
        loops: BTreeMap::new(),
        forks: BTreeMap::new(),
        output_bindings: BTreeMap::from([(
            "text".into(),
            super::data_binding::DataSource::WorkflowInput {
                field: "text".into(),
            },
        )]),
        inputs: schema(),
        outputs: schema(),
        outcomes: BTreeSet::from(["done".into()]),
        subject: None,
        instances: vec![],
        subflows: vec![],
    }
}

fn handlers() -> BTreeSet<HandlerRef> {
    BTreeSet::from([script().handler, subflow_handler(), native_handler()])
}
fn subflow_handler() -> HandlerRef {
    HandlerRef {
        name: "host.subflow".into(),
        version: 1,
    }
}
fn native_handler() -> HandlerRef {
    HandlerRef {
        name: "host.native".into(),
        version: 1,
    }
}
struct TestContext;
impl HostContext for TestContext {}
struct TestPort;
impl HostAdmission for TestPort {
    fn publication(
        &self,
        _: &dyn HostContext,
        _: &NeutralPublication,
        _: &ActivityDescriptor,
        _: &ActivityInstance,
    ) -> Result<(), String> {
        Ok(())
    }
    fn dispatch(
        &self,
        _: &dyn HostContext,
        _: &NeutralPublication,
        _: &ActivityDescriptor,
        _: &ActivityInstance,
        _: &BTreeMap<String, Value>,
        _: Option<&BTreeMap<String, Value>>,
    ) -> Result<(), String> {
        Ok(())
    }
}
struct NativeMustNotRun;
impl HostAdmission for NativeMustNotRun {
    fn publication(
        &self,
        _: &dyn HostContext,
        _: &NeutralPublication,
        _: &ActivityDescriptor,
        _: &ActivityInstance,
    ) -> Result<(), String> {
        panic!("user target reached native admission")
    }
    fn dispatch(
        &self,
        _: &dyn HostContext,
        _: &NeutralPublication,
        _: &ActivityDescriptor,
        _: &ActivityInstance,
        _: &BTreeMap<String, Value>,
        _: Option<&BTreeMap<String, Value>>,
    ) -> Result<(), String> {
        panic!("user target reached native dispatch")
    }
}
static PORT: TestPort = TestPort;
static NATIVE: NativeMustNotRun = NativeMustNotRun;
fn registry() -> HostRegistry<'static> {
    let mut registry = HostRegistry::default();
    registry
        .register_reusable_handler(script().handler, ReusableHandlerKind::Script, &PORT)
        .unwrap();
    registry
        .register_reusable_handler(subflow_handler(), ReusableHandlerKind::Subflow, &PORT)
        .unwrap();
    registry.handlers.insert(native_handler(), &NATIVE);
    registry
}
fn unavailable_registry() -> HostRegistry<'static> {
    let mut registry = registry();
    registry.handlers.clear();
    registry
}
fn instance(descriptor: &ActivityDescriptor) -> ActivityInstance {
    ActivityInstance {
        descriptor: descriptor.id.clone(),
        config: BTreeMap::new(),
        execution_policy: "local".into(),
    }
}

// Catches: editing a saved operation changing a published script, or restart
// losing the exact body and descriptor-level IO/outcome contract.
#[test]
fn script_versions_survive_reopen_without_changing_published_pins() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflow.sqlite3");
    let store = WorkflowStore::open_at(&db).unwrap();
    let mut catalog = WorkflowCatalog::new();
    let first = script();
    store
        .save_reusable_operation("/project", 0, first.clone(), &catalog, &registry())
        .unwrap();
    catalog
        .register(
            store
                .get_reusable_operation("/project", &first.id, &catalog, &registry())
                .unwrap(),
        )
        .unwrap();
    let mut graph = definition();
    graph.instances.push(instance(&first));
    let published = store
        .save_neutral_contract_fixture("/project", "root", 0, graph, &catalog, &handlers())
        .unwrap();
    let mut next = script();
    next.id.version = 2;
    if let Some(ReusableTarget::Script {
        invocation: Invocation::Shell { script, .. },
    }) = &mut next.target
    {
        *script = "printf '%s' 'replacement'".into();
    }
    store
        .save_reusable_operation("/project", 1, next.clone(), &catalog, &registry())
        .unwrap();
    drop(store);
    let store = WorkflowStore::open_at(&db).unwrap();
    assert_eq!(
        store
            .get_reusable_operation("/project", &first.id, &catalog, &registry())
            .unwrap(),
        first
    );
    catalog
        .register(
            store
                .get_reusable_operation("/project", &next.id, &catalog, &registry())
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        store
            .get_neutral_publication("/project", "root", 1, &catalog, &handlers())
            .unwrap(),
        published
    );
    assert_ne!(
        catalog.pin(&first.id).unwrap().sha256,
        catalog.pin(&next.id).unwrap().sha256
    );
    assert!(
        store
            .get_reusable_operation("/other", &first.id, &catalog, &registry())
            .unwrap_err()
            .contains("not found")
    );
    assert!(
        store
            .get_reusable_operation("/project", &first.id, &catalog, &unavailable_registry())
            .unwrap_err()
            .contains("handler unavailable")
    );
}

// Catches: a second writer overwriting the same version or a user operation
// masquerading as native privileged code rather than a reusable target.
#[test]
fn reusable_save_rejects_stale_writers_native_targets_and_invalid_scripts() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflow.sqlite3");
    let a = WorkflowStore::open_at(&db).unwrap();
    let b = WorkflowStore::open_at(&db).unwrap();
    let catalog = WorkflowCatalog::new();
    a.save_reusable_operation("/project", 0, script(), &catalog, &registry())
        .unwrap();
    assert!(
        b.save_reusable_operation("/project", 0, script(), &catalog, &registry())
            .unwrap_err()
            .contains("stale")
    );
    let mut native = script();
    native.target = None;
    assert!(
        b.save_reusable_operation("/project", 0, native, &catalog, &registry())
            .unwrap_err()
            .contains("host-owned")
    );
    for invocation in [
        Invocation::Argv {
            argv: vec!["mutable-script.sh".into()],
        },
        Invocation::Shell {
            program: "sh".into(),
            script: String::new(),
            version: 1,
        },
        Invocation::Shell {
            program: "sh".into(),
            script: "body".into(),
            version: 2,
        },
        Invocation::Shell {
            program: "sh".into(),
            script: "x".repeat(32_769),
            version: 1,
        },
    ] {
        let mut bad = script();
        bad.target = Some(ReusableTarget::Script { invocation });
        assert!(
            b.save_reusable_operation("/project", 0, bad, &catalog, &registry())
                .unwrap_err()
                .contains("inline shell")
        );
    }
}

// Catches: treating outcomes or IO schemas hidden in config as a valid result,
// or letting a script's exit success stand in for its required typed output.
#[test]
fn output_contract_rejects_unknown_outcomes_missing_and_wrong_typed_fields() {
    let mut catalog = WorkflowCatalog::new();
    let descriptor = script();
    catalog.register(descriptor.clone()).unwrap();
    let pin = catalog.pin(&descriptor.id).unwrap();
    for (outcome, values, message) in [
        ("unknown", BTreeMap::new(), "unknown outcome"),
        ("done", BTreeMap::new(), "required field missing"),
        (
            "done",
            BTreeMap::from([("text".into(), json!(42))]),
            "wrong type",
        ),
        (
            "done",
            BTreeMap::from([("schema".into(), json!({}))]),
            "unknown field",
        ),
    ] {
        assert!(
            catalog
                .validate_output(&pin, &handlers(), outcome, &values)
                .unwrap_err()
                .contains(message)
        );
    }
    assert_eq!(
        catalog
            .validate_output(
                &pin,
                &handlers(),
                "done",
                &BTreeMap::from([("text".into(), json!("result"))])
            )
            .unwrap()
            .get("text"),
        Some(&json!("result"))
    );
}

fn subflow(store: &WorkflowStore, catalog: &WorkflowCatalog) -> ActivityDescriptor {
    let child = store
        .save_neutral_contract_fixture("/project", "child", 0, definition(), catalog, &handlers())
        .unwrap();
    let mut descriptor = script();
    descriptor.id.name = "subflow".into();
    descriptor.handler = subflow_handler();
    descriptor.target = Some(ReusableTarget::Subflow {
        publication: PublicationPin {
            reference: PublicationRef {
                project: "/project".into(),
                id: "child".into(),
                revision: 1,
            },
            sha256: content_digest(&child).unwrap(),
        },
    });
    descriptor
}

// Catches: descriptor-backed child edges omitted from publication closure or
// a child revision edit changing the target chosen by an already-published root.
#[test]
fn subflow_operation_pins_target_and_validates_its_complete_interface() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflow.sqlite3")).unwrap();
    let mut catalog = WorkflowCatalog::new();
    let descriptor = subflow(&store, &catalog);
    store
        .save_reusable_operation("/project", 0, descriptor.clone(), &catalog, &registry())
        .unwrap();
    catalog
        .register(
            store
                .get_reusable_operation("/project", &descriptor.id, &catalog, &registry())
                .unwrap(),
        )
        .unwrap();
    let mut root = definition();
    root.instances.push(instance(&descriptor));
    let published = store
        .save_neutral_contract_fixture("/project", "root", 0, root, &catalog, &handlers())
        .unwrap();
    assert_eq!(published.subflow_pins.len(), 1);
    let target = match &descriptor.target {
        Some(ReusableTarget::Subflow { publication }) => publication,
        _ => unreachable!(),
    };
    assert!(published.subflow_pins.contains(target));
    let mut over_limit = definition();
    over_limit.instances = vec![instance(&descriptor); 33];
    assert!(
        store
            .save_neutral_contract_fixture(
                "/project",
                "too-many",
                0,
                over_limit,
                &catalog,
                &handlers()
            )
            .unwrap_err()
            .contains("expanded edges exceed 32")
    );
    let mut edited = definition();
    edited.outputs.fields.clear();
    edited.output_bindings.clear();
    store
        .save_neutral_contract_fixture("/project", "child", 1, edited, &catalog, &handlers())
        .unwrap();
    assert_eq!(
        store
            .get_neutral_publication("/project", "root", 1, &catalog, &handlers())
            .unwrap(),
        published
    );
    for field in ["inputs", "outputs", "outcomes", "subject", "pin", "project"] {
        let mut bad = descriptor.clone();
        bad.id.name = format!("bad-{field}");
        match field {
            "inputs" => bad.inputs.fields.clear(),
            "outputs" => bad.outputs.fields.clear(),
            "outcomes" => {
                bad.outcomes.insert("other".into());
            }
            "subject" => {
                bad.subject = Some(super::catalog::SubjectSchema {
                    required: false,
                    fields: ObjectSchema::default(),
                })
            }
            "pin" => {
                if let Some(ReusableTarget::Subflow { publication }) = &mut bad.target {
                    publication.sha256 = "0".repeat(64);
                }
            }
            "project" => {
                if let Some(ReusableTarget::Subflow { publication }) = &mut bad.target {
                    publication.reference.project = "/other".into();
                }
            }
            _ => unreachable!(),
        }
        assert!(
            store
                .save_reusable_operation("/project", 0, bad.clone(), &catalog, &registry())
                .is_err(),
            "{field}"
        );
        // Direct catalog registration must not bypass publication validation.
        let mut direct = catalog.clone();
        direct.register(bad.clone()).unwrap();
        let mut root = definition();
        root.instances.push(instance(&bad));
        assert!(
            store
                .save_neutral_contract_fixture(
                    "/project",
                    &bad.id.name,
                    0,
                    root,
                    &direct,
                    &handlers()
                )
                .is_err(),
            "direct {field}"
        );
    }
}

// Catches: a rehashed document bound to a different project or descriptor id
// being accepted as the row requested by the consumer.
#[test]
fn reusable_read_rejects_tampered_and_rehashed_identity_mismatch() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflow.sqlite3")).unwrap();
    let catalog = WorkflowCatalog::new();
    store
        .save_reusable_operation("/project", 0, script(), &catalog, &registry())
        .unwrap();
    let conn = store.connect().unwrap();
    let original: String = conn
        .query_row("SELECT document FROM reusable_operations", [], |r| r.get(0))
        .unwrap();
    for (field, rehash) in [("purpose", false), ("name", true), ("project", true)] {
        let mut value: Value = serde_json::from_str(&original).unwrap();
        match field {
            "project" => value["project"] = json!("/other"),
            "name" => value["descriptor"]["id"]["name"] = json!("other"),
            _ => value["descriptor"]["purpose"] = json!("changed"),
        }
        let digest = if rehash {
            content_digest(&value).unwrap()
        } else {
            "0".repeat(64)
        };
        conn.execute(
            "UPDATE reusable_operations SET document=?1,sha256=?2",
            rusqlite::params![value.to_string(), digest],
        )
        .unwrap();
        assert!(
            store
                .get_reusable_operation("/project", &script().id, &catalog, &registry())
                .unwrap_err()
                .contains("identity or content mismatch"),
            "{field}"
        );
    }
}

// Catches: a Script target selecting an available native/wrong-kind/version
// handler with no capabilities, through save, forged storage or direct admission.
#[test]
fn reusable_targets_cannot_impersonate_native_or_other_handler_kinds() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflow.sqlite3")).unwrap();
    let catalog = WorkflowCatalog::new();
    let mut registry = registry();
    let mut wrong_version = script().handler;
    wrong_version.version = 2;
    registry.handlers.insert(wrong_version.clone(), &NATIVE);
    assert!(
        registry
            .register_reusable_handler(native_handler(), ReusableHandlerKind::Script, &PORT)
            .is_err()
    );
    store
        .save_reusable_operation("/project", 0, script(), &catalog, &registry)
        .unwrap();
    let conn = store.connect().unwrap();
    let original: String = conn
        .query_row("SELECT document FROM reusable_operations", [], |r| r.get(0))
        .unwrap();
    for (index, handler) in [native_handler(), subflow_handler(), wrong_version]
        .into_iter()
        .enumerate()
    {
        let mut forged = script();
        forged.handler = handler;
        assert!(forged.required_capabilities.is_empty());
        assert!(
            store
                .save_reusable_operation("/project", 0, forged.clone(), &catalog, &registry)
                .unwrap_err()
                .contains("handler kind")
        );
        let mut record: Value = serde_json::from_str(&original).unwrap();
        record["descriptor"] = serde_json::to_value(&forged).unwrap();
        conn.execute(
            "UPDATE reusable_operations SET document=?1,sha256=?2",
            rusqlite::params![record.to_string(), content_digest(&record).unwrap()],
        )
        .unwrap();
        assert!(
            store
                .get_reusable_operation("/project", &forged.id, &catalog, &registry)
                .unwrap_err()
                .contains("handler kind")
        );
        assert!(
            store
                .build_project_catalog("/project", &catalog, &registry)
                .err()
                .unwrap()
                .contains("handler kind")
        );
        // A manually assembled catalog is not an alternate admission path.
        let mut direct = WorkflowCatalog::new();
        direct.register(forged.clone()).unwrap();
        let mut graph = definition();
        graph.instances.push(instance(&forged));
        graph.control = super::neutral_contract_fixture::authored_control(&mut graph, &direct);
        let id = format!("impersonation-{index}");
        let service = NeutralAdmissionService {
            store: &store,
            catalog: &direct,
            registry: &registry,
        };
        assert!(
            service
                .publish(Some(&TestContext), "/project", &id, 0, graph.clone())
                .unwrap_err()
                .contains("handler kind")
        );
        store
            .save_neutral_contract_fixture(
                "/project",
                &id,
                0,
                graph,
                &direct,
                &registry.available_handlers(),
            )
            .unwrap();
        assert!(
            service
                .check_dispatch(
                    Some(&TestContext),
                    "/project",
                    &id,
                    1,
                    &BTreeMap::from([("text".into(), json!("sample"))]),
                    None
                )
                .unwrap_err()
                .contains("handler kind")
        );
    }
    let mut child = subflow(&store, &catalog);
    child.handler = script().handler;
    assert!(
        store
            .save_reusable_operation("/project", 0, child, &catalog, &registry)
            .unwrap_err()
            .contains("handler kind")
    );
}

// Catches: saving a descriptor that collides with a host entry, or loading a
// conflicting body while preserving exact, verified existing-operation reads.
#[test]
fn save_and_project_builder_refuse_catalog_collisions() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflow.sqlite3")).unwrap();
    let registry = registry();
    let mut host = WorkflowCatalog::new();
    let mut native = script();
    native.target = None;
    native.handler = native_handler();
    host.register(native).unwrap();
    assert!(
        store
            .save_reusable_operation("/project", 0, script(), &host, &registry)
            .unwrap_err()
            .contains("collision")
    );
    assert!(
        store
            .get_reusable_operation("/project", &script().id, &WorkflowCatalog::new(), &registry)
            .unwrap_err()
            .contains("not found")
    );
    store
        .save_reusable_operation("/project", 0, script(), &WorkflowCatalog::new(), &registry)
        .unwrap();
    assert!(
        store
            .build_project_catalog("/project", &host, &registry)
            .err()
            .unwrap()
            .contains("duplicate descriptor")
    );
    let mut same = WorkflowCatalog::new();
    same.register(script()).unwrap();
    assert_eq!(
        store
            .get_reusable_operation("/project", &script().id, &same, &registry)
            .unwrap(),
        script()
    );
    assert!(
        store
            .save_reusable_operation("/project", 0, script(), &same, &registry)
            .unwrap_err()
            .contains("collision")
    );
    assert!(
        store
            .build_project_catalog("/project", &same, &registry)
            .err()
            .unwrap()
            .contains("duplicate descriptor")
    );
    let mut conflicting = script();
    conflicting.purpose = "Different".into();
    let mut changed = WorkflowCatalog::new();
    changed.register(conflicting).unwrap();
    assert!(
        store
            .get_reusable_operation("/project", &script().id, &changed, &registry)
            .unwrap_err()
            .contains("collision")
    );
}

// Catches: project catalog construction dropping old versions/host descriptors,
// leaking another project, or requiring subflow dependencies in row-sort order.
#[test]
fn project_catalog_loads_verified_versions_before_resolving_subflows() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflow.sqlite3");
    let store = WorkflowStore::open_at(&db).unwrap();
    let registry = registry();
    let mut host = WorkflowCatalog::new();
    let mut native = script();
    native.id.name = "native".into();
    native.target = None;
    native.handler = native_handler();
    host.register(native.clone()).unwrap();
    store
        .save_reusable_operation("/project", 0, script(), &host, &registry)
        .unwrap();
    let initial = store
        .build_project_catalog("/project", &host, &registry)
        .unwrap();
    let mut child_graph = definition();
    child_graph.instances.push(instance(&script()));
    let child = store
        .save_neutral_contract_fixture("/project", "child", 0, child_graph, &initial, &handlers())
        .unwrap();
    let mut composite = script();
    composite.id.name = "a-composite".into();
    composite.handler = subflow_handler();
    composite.target = Some(ReusableTarget::Subflow {
        publication: PublicationPin {
            reference: PublicationRef {
                project: "/project".into(),
                id: "child".into(),
                revision: 1,
            },
            sha256: content_digest(&child).unwrap(),
        },
    });
    store
        .save_reusable_operation("/project", 0, composite.clone(), &initial, &registry)
        .unwrap();
    let mut next = script();
    next.id.version = 2;
    next.purpose = "New version".into();
    store
        .save_reusable_operation("/project", 1, next.clone(), &initial, &registry)
        .unwrap();
    let mut other = script();
    other.id.name = "other-project".into();
    store
        .save_reusable_operation("/other", 0, other.clone(), &host, &registry)
        .unwrap();
    drop(store);
    let store = WorkflowStore::open_at(&db).unwrap();
    let catalog = store
        .build_project_catalog("/project", &host, &registry)
        .unwrap();
    for expected in [&native, &script(), &composite, &next] {
        assert_eq!(catalog.get(&expected.id), Some(expected));
    }
    assert_eq!(catalog.get(&other.id), None);
    assert!(
        store
            .build_project_catalog("/project", &host, &unavailable_registry())
            .err()
            .unwrap()
            .contains("handler unavailable")
    );
    let mut root = definition();
    root.instances.push(instance(&composite));
    let root = store
        .save_neutral_contract_fixture("/project", "root", 0, root, &catalog, &handlers())
        .unwrap();
    assert_eq!(root.subflow_pins.len(), 1);
}
