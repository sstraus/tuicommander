use super::WorkflowStore;
use super::catalog::{
    ActivityDescriptor, ActivityInstance, DescriptorId, FieldSchema, HandlerRef, ObjectSchema,
    SubjectSchema, ValueType, WorkflowCatalog,
};
use super::neutral_admission::{HostAdmission, HostContext, HostRegistry, NeutralAdmissionService};
use super::neutral_publication::{NeutralDefinition, NeutralPublication, PublicationRef};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};

struct Context;
impl HostContext for Context {}

// Port contract test only: this is not a real host permission/freshness source.
struct Port {
    available: AtomicBool,
}
impl HostAdmission for Port {
    fn publication(
        &self,
        _: &dyn HostContext,
        _: &NeutralPublication,
        _: &ActivityDescriptor,
        _: &ActivityInstance,
    ) -> Result<(), String> {
        if self.available.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err("unknown host authority".into())
        }
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
        if self.available.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err("host permission revoked or subject stale".into())
        }
    }
}

fn fixture() -> (WorkflowCatalog, HandlerRef, NeutralDefinition) {
    let id = DescriptorId {
        namespace: "sample".into(),
        name: "read".into(),
        version: 1,
    };
    let handler = HandlerRef {
        name: "host.read".into(),
        version: 1,
    };
    let mut catalog = WorkflowCatalog::new();
    catalog
        .register(ActivityDescriptor {
            id: id.clone(),
            purpose: "Read".into(),
            target: None,
            handler: handler.clone(),
            config: ObjectSchema::default(),
            inputs: ObjectSchema {
                fields: BTreeMap::from([(
                    "query".into(),
                    FieldSchema {
                        value_type: ValueType::String,
                        required: true,
                        default: None,
                    },
                )]),
            },
            subject: None,
            outputs: ObjectSchema::default(),
            outcomes: BTreeSet::from(["done".into()]),
            required_capabilities: BTreeSet::from(["repo.read".into()]),
            execution_policies: BTreeSet::from(["local".into()]),
        })
        .unwrap();
    let field = FieldSchema {
        value_type: ValueType::String,
        required: true,
        default: None,
    };
    let mut definition = NeutralDefinition {
        control: crate::workflows::neutral_control::ControlGraph {
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
        },
        input_bindings: BTreeMap::new(),
        conditions: BTreeMap::new(),
        collections: BTreeMap::new(),
        loops: BTreeMap::new(),
        forks: BTreeMap::new(),
        output_bindings: BTreeMap::new(),
        outputs: ObjectSchema::default(),
        outcomes: BTreeSet::from(["done".into()]),
        inputs: ObjectSchema {
            fields: BTreeMap::from([("query".into(), field.clone())]),
        },
        subject: Some(SubjectSchema {
            required: true,
            fields: ObjectSchema {
                fields: BTreeMap::from([("id".into(), field)]),
            },
        }),
        instances: vec![ActivityInstance {
            descriptor: id,
            config: BTreeMap::new(),
            execution_policy: "local".into(),
        }],
        subflows: vec![],
    };
    definition.control =
        super::neutral_contract_fixture::authored_control(&mut definition, &catalog);
    (catalog, handler, definition)
}

// Catches: labels, absent host context, or missing/unknown adapters granting publication.
#[test]
fn publication_refuses_missing_context_handler_and_capability() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let (catalog, handler, definition) = fixture();
    let port = Port {
        available: AtomicBool::new(true),
    };
    let mut registry = HostRegistry::default();
    let service = NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    };
    assert!(
        service
            .publish(None, "/project", "one", 0, definition.clone())
            .unwrap_err()
            .contains("host context unavailable")
    );
    assert!(
        service
            .publish(Some(&Context), "/project", "one", 0, definition.clone())
            .unwrap_err()
            .contains("handler unavailable")
    );
    registry.handlers.insert(handler, &port);
    let service = NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    };
    assert!(
        service
            .publish(Some(&Context), "/project", "one", 0, definition.clone())
            .unwrap_err()
            .contains("capability adapter unavailable")
    );
    assert!(
        store
            .get_neutral_publication(
                "/project",
                "one",
                1,
                &catalog,
                &registry.handlers.keys().cloned().collect()
            )
            .is_err()
    );
    registry.capabilities.insert("repo.read".into(), &port);
    port.available.store(false, Ordering::SeqCst);
    let service = NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    };
    assert!(
        service
            .publish(Some(&Context), "/project", "one", 0, definition.clone())
            .unwrap_err()
            .contains("unknown host authority")
    );
    let mut forged = serde_json::to_value(&definition).unwrap();
    forged["approved"] = json!(true);
    forged["grants"] = json!(["repo.read"]);
    assert!(serde_json::from_value::<NeutralDefinition>(forged).is_err());
}

// Catches: an earlier publication decision granting dispatch after port state changes.
#[test]
fn dispatch_checks_typed_values_and_rechecks_the_host_port() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let (catalog, handler, definition) = fixture();
    let port = Port {
        available: AtomicBool::new(true),
    };
    let mut registry = HostRegistry::default();
    registry.handlers.insert(handler, &port);
    registry.capabilities.insert("repo.read".into(), &port);
    let service = NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    };
    service
        .publish(Some(&Context), "/project", "one", 0, definition)
        .unwrap();
    let inputs = BTreeMap::from([("query".into(), json!("hello"))]);
    let subject = BTreeMap::from([("id".into(), json!("s1"))]);
    assert!(
        service
            .check_dispatch(Some(&Context), "/project", "one", 1, &inputs, None)
            .unwrap_err()
            .contains("subject is required")
    );
    assert!(
        service
            .check_dispatch(
                Some(&Context),
                "/project",
                "one",
                1,
                &BTreeMap::from([("query".into(), json!(4))]),
                Some(&subject)
            )
            .unwrap_err()
            .contains("wrong type")
    );
    service
        .check_dispatch(
            Some(&Context),
            "/project",
            "one",
            1,
            &inputs,
            Some(&subject),
        )
        .unwrap();
    port.available.store(false, Ordering::SeqCst);
    assert!(
        service
            .check_dispatch(
                Some(&Context),
                "/project",
                "one",
                1,
                &inputs,
                Some(&subject)
            )
            .unwrap_err()
            .contains("revoked or subject stale")
    );
}

// Catches: a transitive handler or altered child being ignored by the service.
#[test]
fn service_refuses_unavailable_or_tampered_transitive_children() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let store = WorkflowStore::open_at(&db).unwrap();
    let (catalog, handler, mut definition) = fixture();
    let port = Port {
        available: AtomicBool::new(true),
    };
    let mut registry = HostRegistry::default();
    registry.handlers.insert(handler, &port);
    registry.capabilities.insert("repo.read".into(), &port);
    {
        let service = NeutralAdmissionService {
            store: &store,
            catalog: &catalog,
            registry: &registry,
        };
        service
            .publish(Some(&Context), "/project", "child", 0, definition.clone())
            .unwrap();
        definition.instances.clear();
        definition.control =
            super::neutral_contract_fixture::authored_control(&mut definition, &catalog);
        definition.subflows.push(PublicationRef {
            project: "/project".into(),
            id: "child".into(),
            revision: 1,
        });
        service
            .publish(Some(&Context), "/project", "root", 0, definition.clone())
            .unwrap();
    }
    registry.handlers.clear();
    let service = NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    };
    let inputs = BTreeMap::from([("query".into(), json!("hello"))]);
    let subject = BTreeMap::from([("id".into(), json!("s1"))]);
    assert!(
        service
            .check_dispatch(
                Some(&Context),
                "/project",
                "root",
                1,
                &inputs,
                Some(&subject)
            )
            .unwrap_err()
            .contains("handler unavailable")
    );
    registry.handlers.insert(
        HandlerRef {
            name: "host.read".into(),
            version: 1,
        },
        &port,
    );
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE neutral_workflow_publications SET sha256='forged' WHERE id='child'",
        [],
    )
    .unwrap();
    let service = NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    };
    assert!(
        service
            .check_dispatch(
                Some(&Context),
                "/project",
                "root",
                1,
                &inputs,
                Some(&subject)
            )
            .unwrap_err()
            .contains("content mismatch")
    );
}

// Catches: unresolved child arguments being admitted as the root's arguments.
#[test]
fn dispatch_refuses_unmapped_child_arguments() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let (mut catalog, handler, definition) = fixture();
    let port = Port {
        available: AtomicBool::new(true),
    };
    let mut registry = HostRegistry::default();
    registry.handlers.insert(handler, &port);
    registry.capabilities.insert("repo.read".into(), &port);
    let child = NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    }
    .publish(Some(&Context), "/project", "child", 0, definition.clone())
    .unwrap();
    let reference = PublicationRef {
        project: "/project".into(),
        id: "child".into(),
        revision: 1,
    };
    let mut composite = catalog
        .get(&definition.instances[0].descriptor)
        .unwrap()
        .clone();
    composite.id.name = "composite".into();
    composite.handler = HandlerRef {
        name: "host.subflow".into(),
        version: 1,
    };
    registry
        .register_reusable_handler(
            composite.handler.clone(),
            super::neutral_admission::ReusableHandlerKind::Subflow,
            &port,
        )
        .unwrap();
    composite.subject = definition.subject.clone();
    composite.target = Some(super::catalog::ReusableTarget::Subflow {
        publication: super::neutral_publication::PublicationPin {
            reference: reference.clone(),
            sha256: super::catalog::content_digest(&child).unwrap(),
        },
    });
    catalog.register(composite.clone()).unwrap();
    let service = NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    };
    // Both direct and descriptor-backed edges must refuse unresolved arguments.
    for use_operation in [false, true] {
        let mut root = definition.clone();
        if use_operation {
            root.instances[0].descriptor = composite.id.clone();
        } else {
            root.subflows.push(reference.clone());
        }
        let id = if use_operation {
            "operation-root"
        } else {
            "root"
        };
        service
            .publish(Some(&Context), "/project", id, 0, root)
            .unwrap();
        assert!(
            service
                .check_dispatch(
                    Some(&Context),
                    "/project",
                    id,
                    1,
                    &BTreeMap::from([("query".into(), json!("hello"))]),
                    Some(&BTreeMap::from([("id".into(), json!("s1"))])),
                )
                .unwrap_err()
                .contains("child arguments unresolved")
        );
    }
}

// Catches: valid workflow values being accepted as incompatible activity inputs.
#[test]
fn dispatch_refuses_incompatible_activity_inputs() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let (catalog, handler, mut definition) = fixture();
    definition.inputs.fields.insert(
        "extra".into(),
        FieldSchema {
            value_type: ValueType::String,
            required: false,
            default: None,
        },
    );
    let port = Port {
        available: AtomicBool::new(true),
    };
    let mut registry = HostRegistry::default();
    registry.handlers.insert(handler, &port);
    registry.capabilities.insert("repo.read".into(), &port);
    let service = NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    };
    service
        .publish(Some(&Context), "/project", "root", 0, definition)
        .unwrap();
    let subject = BTreeMap::from([("id".into(), json!("s1"))]);
    let inputs = BTreeMap::from([
        ("query".into(), json!("hello")),
        ("extra".into(), json!("no mapping")),
    ]);
    assert!(
        service
            .check_dispatch(
                Some(&Context),
                "/project",
                "root",
                1,
                &inputs,
                Some(&subject)
            )
            .unwrap_err()
            .contains("unknown field")
    );
}

// Catches: same-descriptor instances becoming indistinguishable or losing defaults/policy.
#[test]
fn host_checks_resolved_instance_in_both_modes() {
    struct ConfigPort(AtomicBool);
    impl HostAdmission for ConfigPort {
        fn publication(
            &self,
            _: &dyn HostContext,
            _: &NeutralPublication,
            _: &ActivityDescriptor,
            instance: &ActivityInstance,
        ) -> Result<(), String> {
            if instance.execution_policy != "local"
                || instance.config.get("mode") != Some(&json!("default"))
            {
                return Err("lost policy or default".into());
            }
            if self.0.load(Ordering::SeqCst)
                && instance.config.get("target") == Some(&json!("blocked"))
            {
                return Err("blocked target".into());
            }
            Ok(())
        }
        fn dispatch(
            &self,
            c: &dyn HostContext,
            p: &NeutralPublication,
            d: &ActivityDescriptor,
            i: &ActivityInstance,
            _: &BTreeMap<String, Value>,
            _: Option<&BTreeMap<String, Value>>,
        ) -> Result<(), String> {
            self.publication(c, p, d, i)
        }
    }
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let (old, handler, mut definition) = fixture();
    let mut descriptor = old
        .get(&definition.instances[0].descriptor)
        .unwrap()
        .clone();
    descriptor.config.fields.insert(
        "target".into(),
        FieldSchema {
            value_type: ValueType::String,
            required: true,
            default: None,
        },
    );
    descriptor.config.fields.insert(
        "mode".into(),
        FieldSchema {
            value_type: ValueType::String,
            required: false,
            default: Some(json!("default")),
        },
    );
    let mut catalog = WorkflowCatalog::new();
    catalog.register(descriptor).unwrap();
    definition.instances[0]
        .config
        .insert("target".into(), json!("allowed"));
    let mut other = definition.instances[0].clone();
    other.config.insert("target".into(), json!("blocked"));
    let port = ConfigPort(AtomicBool::new(false));
    let mut registry = HostRegistry::default();
    registry.handlers.insert(handler, &port);
    registry.capabilities.insert("repo.read".into(), &port);
    let service = NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    };
    service
        .publish(Some(&Context), "/project", "allowed", 0, definition.clone())
        .unwrap();
    let inputs = BTreeMap::from([("query".into(), json!("hello"))]);
    let subject = BTreeMap::from([("id".into(), json!("s1"))]);
    service
        .check_dispatch(
            Some(&Context),
            "/project",
            "allowed",
            1,
            &inputs,
            Some(&subject),
        )
        .unwrap();
    definition.instances.push(other);
    definition.control =
        super::neutral_contract_fixture::authored_control(&mut definition, &catalog);
    service
        .publish(Some(&Context), "/project", "both", 0, definition.clone())
        .unwrap();
    port.0.store(true, Ordering::SeqCst);
    assert!(
        service
            .check_dispatch(
                Some(&Context),
                "/project",
                "both",
                1,
                &inputs,
                Some(&subject)
            )
            .unwrap_err()
            .contains("blocked target")
    );
    assert!(
        service
            .publish(Some(&Context), "/project", "blocked", 0, definition)
            .unwrap_err()
            .contains("blocked target")
    );
}

// Catches: host receiving unresolved activity input or subject defaults, or explicit null accepted.
#[test]
fn dispatch_delivers_operation_resolved_values() {
    struct Check(std::sync::Mutex<Vec<Option<BTreeMap<String, Value>>>>);
    impl HostAdmission for Check {
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
            inputs: &BTreeMap<String, Value>,
            subject: Option<&BTreeMap<String, Value>>,
        ) -> Result<(), String> {
            if inputs.get("added") != Some(&json!("input-default")) {
                return Err("missing input default".into());
            }
            self.0.lock().unwrap().push(subject.cloned());
            Ok(())
        }
    }
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let (old, handler, mut definition) = fixture();
    let mut descriptor = old
        .get(&definition.instances[0].descriptor)
        .unwrap()
        .clone();
    let default = |v: &str| FieldSchema {
        value_type: ValueType::String,
        required: true,
        default: Some(json!(v)),
    };
    descriptor
        .inputs
        .fields
        .insert("added".into(), default("input-default"));
    descriptor.subject = Some(SubjectSchema {
        required: false,
        fields: ObjectSchema {
            fields: BTreeMap::from([
                (
                    "id".into(),
                    FieldSchema {
                        value_type: ValueType::String,
                        required: true,
                        default: None,
                    },
                ),
                ("added".into(), default("subject-default")),
            ]),
        },
    });
    definition.subject.as_mut().unwrap().required = false;
    let mut catalog = WorkflowCatalog::new();
    catalog.register(descriptor).unwrap();
    let port = Check(std::sync::Mutex::new(Vec::new()));
    let mut registry = HostRegistry::default();
    registry.handlers.insert(handler, &port);
    registry.capabilities.insert("repo.read".into(), &port);
    let service = NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    };
    service
        .publish(Some(&Context), "/project", "root", 0, definition)
        .unwrap();
    let inputs = BTreeMap::from([("query".into(), json!("q"))]);
    let subject = BTreeMap::from([("id".into(), json!("s"))]);
    service
        .check_dispatch(
            Some(&Context),
            "/project",
            "root",
            1,
            &inputs,
            Some(&subject),
        )
        .unwrap();
    service
        .check_dispatch(Some(&Context), "/project", "root", 1, &inputs, None)
        .unwrap();
    let resolved = BTreeMap::from([
        ("id".into(), json!("s")),
        ("added".into(), json!("subject-default")),
    ]);
    assert_eq!(
        *port.0.lock().unwrap(),
        vec![Some(resolved.clone()), Some(resolved), None, None]
    );
    let null_subject = BTreeMap::from([("id".into(), Value::Null)]);
    assert!(
        service
            .check_dispatch(
                Some(&Context),
                "/project",
                "root",
                1,
                &inputs,
                Some(&null_subject)
            )
            .is_err()
    );
}

// Catches preflight ignoring explicit bindings and admitting the wrong resolved arguments.
// This verifies host-port input delivery, not real permission or effect dispatch.
#[test]
fn preflight_delivers_mapped_inputs_not_root_field_names() {
    use super::data_binding::DataSource;
    struct Capture(std::sync::Mutex<Vec<BTreeMap<String, Value>>>);
    impl HostAdmission for Capture {
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
            inputs: &BTreeMap<String, Value>,
            _: Option<&BTreeMap<String, Value>>,
        ) -> Result<(), String> {
            self.0.lock().unwrap().push(inputs.clone());
            Ok(())
        }
    }
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let (catalog, handler, mut definition) = fixture();
    let schema = definition.inputs.fields.remove("query").unwrap();
    definition.inputs.fields.insert("body".into(), schema);
    definition.input_bindings.insert(
        0,
        BTreeMap::from([(
            "query".into(),
            DataSource::WorkflowInput {
                field: "body".into(),
            },
        )]),
    );
    let capture = Capture(std::sync::Mutex::new(Vec::new()));
    let mut registry = HostRegistry::default();
    registry.handlers.insert(handler, &capture);
    registry.capabilities.insert("repo.read".into(), &capture);
    let service = NeutralAdmissionService {
        store: &store,
        catalog: &catalog,
        registry: &registry,
    };
    service
        .publish(Some(&Context), "/project", "mapped", 0, definition.clone())
        .unwrap();
    let subject = BTreeMap::from([("id".into(), json!("s1"))]);
    service
        .check_dispatch(
            Some(&Context),
            "/project",
            "mapped",
            1,
            &BTreeMap::from([("body".into(), json!("real input"))]),
            Some(&subject),
        )
        .unwrap();
    assert_eq!(
        *capture.0.lock().unwrap(),
        vec![BTreeMap::from([("query".into(), json!("real input"))]); 2]
    );
    capture.0.lock().unwrap().clear();
    definition.input_bindings.get_mut(&0).unwrap().insert(
        "query".into(),
        DataSource::Constant {
            value: json!("literal"),
        },
    );
    service
        .publish(Some(&Context), "/project", "literal", 0, definition)
        .unwrap();
    service
        .check_dispatch(
            Some(&Context),
            "/project",
            "literal",
            1,
            &BTreeMap::from([("body".into(), json!("not the argument"))]),
            Some(&subject),
        )
        .unwrap();
    assert_eq!(
        *capture.0.lock().unwrap(),
        vec![BTreeMap::from([("query".into(), json!("literal"))]); 2]
    );
}
