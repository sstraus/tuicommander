use super::WorkflowStore;
use super::catalog::{
    ActivityDescriptor, ActivityInstance, DescriptorId, FieldSchema, HandlerRef, ObjectSchema,
    SubjectSchema, ValueType, WorkflowCatalog,
};
use super::neutral_publication::{NeutralDefinition, PublicationRef};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn field(value_type: ValueType, required: bool, default: Option<Value>) -> FieldSchema {
    FieldSchema {
        value_type,
        required,
        default,
    }
}

fn fixtures() -> (WorkflowCatalog, BTreeSet<HandlerRef>, NeutralDefinition) {
    let mut catalog = WorkflowCatalog::new();
    let mut handlers = BTreeSet::new();
    let mut instances = Vec::new();
    for name in ["model", "script", "tuic"] {
        let id = DescriptorId {
            namespace: "example".into(),
            name: name.into(),
            version: 1,
        };
        let handler = HandlerRef {
            name: format!("host.{name}"),
            version: 1,
        };
        handlers.insert(handler.clone());
        catalog
            .register(ActivityDescriptor {
                id: id.clone(),
                purpose: format!("Use {name}"),
                target: None,
                handler,
                config: ObjectSchema {
                    fields: BTreeMap::from([(
                        "enabled".into(),
                        field(ValueType::Boolean, false, Some(json!(true))),
                    )]),
                },
                inputs: ObjectSchema::default(),
                subject: None,
                outputs: ObjectSchema::default(),
                outcomes: BTreeSet::from(["done".into()]),
                required_capabilities: BTreeSet::new(),
                execution_policies: BTreeSet::from(["local".into()]),
            })
            .unwrap();
        instances.push(ActivityInstance {
            descriptor: id,
            config: BTreeMap::new(),
            execution_policy: "local".into(),
        });
    }
    let definition = NeutralDefinition {
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
            fields: BTreeMap::from([
                ("query".into(), field(ValueType::String, true, None)),
                (
                    "limit".into(),
                    field(ValueType::Integer, false, Some(json!(5))),
                ),
            ]),
        },
        subject: Some(SubjectSchema {
            required: true,
            fields: ObjectSchema {
                fields: BTreeMap::from([("key".into(), field(ValueType::String, true, None))]),
            },
        }),
        instances,
        subflows: vec![],
    };
    (catalog, handlers, definition)
}

// Catches: neutral revisions or flat pins disappearing after reopening, or cross-project reads.
#[test]
fn neutral_publication_roundtrips_and_preserves_prior_revisions() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let (catalog, handlers, definition) = fixtures();
    let store = WorkflowStore::open_at(&db).unwrap();
    let first = store
        .save_neutral_contract_fixture(
            "/project",
            "sample",
            0,
            definition.clone(),
            &catalog,
            &handlers,
        )
        .unwrap();
    assert_eq!(first.revision, 1);
    assert_eq!(first.pins.pins().len(), 3);
    let mut second_definition = definition.clone();
    second_definition.instances[0]
        .config
        .insert("enabled".into(), json!(false));
    let second = store
        .save_neutral_contract_fixture(
            "/project",
            "sample",
            1,
            second_definition,
            &catalog,
            &handlers,
        )
        .unwrap();
    assert_eq!(second.revision, 2);
    drop(store);
    let reopened = WorkflowStore::open_at(&db).unwrap();
    assert_eq!(
        reopened
            .get_neutral_publication("/project", "sample", 1, &catalog, &handlers)
            .unwrap(),
        first
    );
    assert_eq!(
        reopened
            .get_neutral_publication("/project", "sample", 2, &catalog, &handlers)
            .unwrap(),
        second
    );
    assert!(
        reopened
            .get_neutral_publication("/other", "sample", 1, &catalog, &handlers)
            .unwrap_err()
            .contains("not found")
    );
}

// Catches: a stale writer replacing a later revision or creating the same first revision.
#[test]
fn stale_writers_cannot_replace_a_revision() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let (catalog, handlers, definition) = fixtures();
    let a = WorkflowStore::open_at(&db).unwrap();
    let b = WorkflowStore::open_at(&db).unwrap();
    a.save_neutral_contract_fixture(
        "/project",
        "sample",
        0,
        definition.clone(),
        &catalog,
        &handlers,
    )
    .unwrap();
    assert!(
        b.save_neutral_contract_fixture(
            "/project",
            "sample",
            0,
            definition.clone(),
            &catalog,
            &handlers
        )
        .unwrap_err()
        .contains("stale")
    );
    b.save_neutral_contract_fixture(
        "/project",
        "sample",
        1,
        definition.clone(),
        &catalog,
        &handlers,
    )
    .unwrap();
    assert!(
        a.save_neutral_contract_fixture("/project", "sample", 1, definition, &catalog, &handlers)
            .unwrap_err()
            .contains("stale")
    );
}

// Catches: altered stored configuration or an unavailable pinned handler silently resolving.
#[test]
fn read_rejects_modified_content_and_missing_handler() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let (catalog, handlers, definition) = fixtures();
    let store = WorkflowStore::open_at(&db).unwrap();
    store
        .save_neutral_contract_fixture("/project", "sample", 0, definition, &catalog, &handlers)
        .unwrap();
    let mut missing = handlers.clone();
    missing.remove(&HandlerRef {
        name: "host.script".into(),
        version: 1,
    });
    assert!(
        store
            .get_neutral_publication("/project", "sample", 1, &catalog, &missing)
            .unwrap_err()
            .contains("handler unavailable")
    );
    let conn = rusqlite::Connection::open(&db).unwrap();
    let raw: String = conn.query_row("SELECT document FROM neutral_workflow_publications WHERE project='/project' AND id='sample' AND revision=1", [], |r| r.get(0)).unwrap();
    let mut doc: Value = serde_json::from_str(&raw).unwrap();
    doc["definition"]["instances"][0]["config"]["enabled"] = json!(false);
    conn.execute("UPDATE neutral_workflow_publications SET document=?1 WHERE project='/project' AND id='sample' AND revision=1", [doc.to_string()]).unwrap();
    assert!(
        store
            .get_neutral_publication("/project", "sample", 1, &catalog, &handlers)
            .unwrap_err()
            .contains("content mismatch")
    );
}

// Catches: typed inputs/subjects being treated as labels, or invalid defaults being persisted.
#[test]
fn workflow_input_and_subject_schemas_are_enforced() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let (catalog, handlers, definition) = fixtures();
    let inputs = BTreeMap::from([("query".into(), json!("hello"))]);
    let subject = BTreeMap::from([("key".into(), json!("abc"))]);
    let (resolved, _) = definition.validate_values(&inputs, Some(&subject)).unwrap();
    assert_eq!(resolved.get("limit"), Some(&json!(5)));
    assert!(
        definition
            .validate_values(&BTreeMap::new(), Some(&subject))
            .unwrap_err()
            .contains("required")
    );
    assert!(
        definition
            .validate_values(&inputs, None)
            .unwrap_err()
            .contains("subject is required")
    );
    assert!(
        definition
            .validate_values(&inputs, Some(&BTreeMap::from([("key".into(), json!(4))])))
            .unwrap_err()
            .contains("wrong type")
    );
    let mut optional = definition.clone();
    optional.subject.as_mut().unwrap().required = false;
    assert!(optional.validate_values(&inputs, None).is_ok());
    optional.subject = None;
    assert!(optional.validate_values(&inputs, None).is_ok());
    assert!(optional.validate_values(&inputs, Some(&subject)).is_err());
    optional.inputs.fields.get_mut("limit").unwrap().default = Some(json!("bad"));
    assert!(
        store
            .save_neutral_contract_fixture("/project", "sample", 0, optional, &catalog, &handlers)
            .unwrap_err()
            .contains("default has wrong type")
    );
    let mut forged = serde_json::to_value(&definition).unwrap();
    forged["pins"] = json!([]);
    assert!(serde_json::from_value::<NeutralDefinition>(forged).is_err());
}

// Catches: two concurrent writers both publishing the same expected revision.
#[test]
fn concurrent_writers_only_one_advances_the_revision() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let store = WorkflowStore::open_at(&db).unwrap();
    let (catalog, handlers, definition) = fixtures();
    store
        .save_neutral_contract_fixture(
            "/project",
            "sample",
            0,
            definition.clone(),
            &catalog,
            &handlers,
        )
        .unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let results = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let barrier = barrier.clone();
                let store = &store;
                let catalog = &catalog;
                let handlers = &handlers;
                let definition = &definition;
                scope.spawn(move || {
                    barrier.wait();
                    store.save_neutral_contract_fixture(
                        "/project",
                        "sample",
                        1,
                        definition.clone(),
                        catalog,
                        handlers,
                    )
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| result.as_ref().is_err_and(|e| e.contains("stale")))
            .count(),
        1
    );
}

fn reference(id: &str, revision: i64) -> PublicationRef {
    PublicationRef {
        project: "/project".into(),
        id: id.into(),
        revision,
    }
}

// Catches: parent following a child's latest revision and losing pinned inline content.
#[test]
fn child_revision_and_full_content_remain_pinned_across_reopen() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let (catalog, handlers, definition) = fixtures();
    let store = WorkflowStore::open_at(&db).unwrap();
    store
        .save_neutral_contract_fixture(
            "/project",
            "child",
            0,
            definition.clone(),
            &catalog,
            &handlers,
        )
        .unwrap();
    let mut parent = definition.clone();
    parent.subflows.push(reference("child", 1));
    let saved = store
        .save_neutral_contract_fixture("/project", "parent", 0, parent, &catalog, &handlers)
        .unwrap();
    assert_eq!(saved.closure_pins().unwrap().len(), 2);
    let mut edited = definition;
    edited.instances[0]
        .config
        .insert("enabled".into(), json!(false));
    store
        .save_neutral_contract_fixture("/project", "child", 1, edited, &catalog, &handlers)
        .unwrap();
    drop(store);
    let reopened = WorkflowStore::open_at(&db).unwrap();
    let read = reopened
        .get_neutral_publication("/project", "parent", 1, &catalog, &handlers)
        .unwrap();
    assert_eq!(read, saved);
    assert_eq!(
        read.subflow_pins.iter().next().unwrap().reference,
        reference("child", 1)
    );
}

// Catches: a descendant's changed content, identity, or handler availability escaping root verification.
#[test]
fn descendant_corruption_and_missing_handler_refuse_parent() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let (catalog, handlers, mut definition) = fixtures();
    let store = WorkflowStore::open_at(&db).unwrap();
    store
        .save_neutral_contract_fixture(
            "/project",
            "leaf",
            0,
            definition.clone(),
            &catalog,
            &handlers,
        )
        .unwrap();
    definition.subflows.push(reference("leaf", 1));
    definition.instances.clear();
    store
        .save_neutral_contract_fixture("/project", "parent", 0, definition, &catalog, &handlers)
        .unwrap();
    assert!(
        store
            .get_neutral_publication("/project", "parent", 1, &WorkflowCatalog::new(), &handlers)
            .unwrap_err()
            .contains("missing descriptor")
    );
    let mut missing = handlers.clone();
    missing.remove(&HandlerRef {
        name: "host.script".into(),
        version: 1,
    });
    assert!(
        store
            .get_neutral_publication("/project", "parent", 1, &catalog, &missing)
            .unwrap_err()
            .contains("handler unavailable")
    );
    let conn = rusqlite::Connection::open(&db).unwrap();
    let raw: String = conn
        .query_row(
            "SELECT document FROM neutral_workflow_publications WHERE id='leaf'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut doc: Value = serde_json::from_str(&raw).unwrap();
    doc["definition"]["instances"][0]["config"]["enabled"] = json!(false);
    // Recompute the leaf's own hash: the parent's independent child pin must still reject it.
    let digest = super::catalog::content_digest(&doc).unwrap();
    conn.execute(
        "UPDATE neutral_workflow_publications SET document=?1,sha256=?2 WHERE id='leaf'",
        rusqlite::params![doc.to_string(), digest],
    )
    .unwrap();
    assert!(
        store
            .get_neutral_publication("/project", "parent", 1, &catalog, &handlers)
            .unwrap_err()
            .contains("closure mismatch")
    );
    doc["id"] = json!("impostor");
    let digest = super::catalog::content_digest(&doc).unwrap();
    conn.execute(
        "UPDATE neutral_workflow_publications SET document=?1,sha256=?2 WHERE id='leaf'",
        rusqlite::params![doc.to_string(), digest],
    )
    .unwrap();
    assert!(
        store
            .get_neutral_publication("/project", "parent", 1, &catalog, &handlers)
            .unwrap_err()
            .contains("identity or content mismatch")
    );
}

// Catches: a legitimate shared child misidentified as a recursive cycle.
#[test]
fn shared_descendant_is_deduplicated_not_a_cycle() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let (catalog, handlers, mut definition) = fixtures();
    store
        .save_neutral_contract_fixture(
            "/project",
            "leaf",
            0,
            definition.clone(),
            &catalog,
            &handlers,
        )
        .unwrap();
    definition.subflows = vec![reference("leaf", 1)];
    for id in ["left", "right"] {
        store
            .save_neutral_contract_fixture(
                "/project",
                id,
                0,
                definition.clone(),
                &catalog,
                &handlers,
            )
            .unwrap();
    }
    definition.subflows = vec![reference("right", 1), reference("left", 1)];
    let root = store
        .save_neutral_contract_fixture("/project", "root", 0, definition, &catalog, &handlers)
        .unwrap();
    assert_eq!(root.subflow_pins.len(), 3);
    assert_eq!(
        store
            .get_neutral_publication("/project", "root", 1, &catalog, &handlers)
            .unwrap(),
        root
    );
}

// Catches: cyclic, cross-project, missing, or unbounded references being admitted.
#[test]
fn dependency_traversal_rejects_invalid_scope_cycles_and_limits() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let (catalog, handlers, mut definition) = fixtures();
    definition.subflows = vec![reference("root", 1)];
    assert!(
        store
            .save_neutral_contract_fixture(
                "/project",
                "root",
                0,
                definition.clone(),
                &catalog,
                &handlers
            )
            .unwrap_err()
            .contains("cycle")
    );
    definition.subflows[0].project = "/other".into();
    assert!(
        store
            .save_neutral_contract_fixture(
                "/project",
                "root",
                0,
                definition.clone(),
                &catalog,
                &handlers
            )
            .unwrap_err()
            .contains("cross-project")
    );
    definition.subflows = vec![reference("missing", 1)];
    assert!(
        store
            .save_neutral_contract_fixture(
                "/project",
                "root",
                0,
                definition.clone(),
                &catalog,
                &handlers
            )
            .unwrap_err()
            .contains("not found")
    );
    definition.subflows.clear();
    store
        .save_neutral_contract_fixture(
            "/project",
            "leaf",
            0,
            definition.clone(),
            &catalog,
            &handlers,
        )
        .unwrap();
    definition.subflows = vec![reference("leaf", 1); 33];
    assert!(
        store
            .save_neutral_contract_fixture(
                "/project",
                "root",
                0,
                definition.clone(),
                &catalog,
                &handlers
            )
            .unwrap_err()
            .contains("exceed 32")
    );
    definition.subflows = vec![reference("leaf", 1)];
    for id in [
        "level1", "level2", "level3", "level4", "level5", "level6", "level7", "level8",
    ] {
        store
            .save_neutral_contract_fixture(
                "/project",
                id,
                0,
                definition.clone(),
                &catalog,
                &handlers,
            )
            .unwrap();
        if id == "level8" {
            let accepted = store
                .get_neutral_publication("/project", id, 1, &catalog, &handlers)
                .unwrap();
            assert_eq!(accepted.subflow_pins.len(), 8);
        }
        definition.subflows = vec![reference(id, 1)];
    }
    assert!(
        store
            .save_neutral_contract_fixture("/project", "root", 0, definition, &catalog, &handlers)
            .unwrap_err()
            .contains("depth exceeds 8")
    );
}

// Catches: a same-name earlier revision misclassified as a recursion cycle.
#[test]
fn earlier_revision_of_same_workflow_is_a_distinct_reference() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let (catalog, handlers, mut definition) = fixtures();
    store
        .save_neutral_contract_fixture(
            "/project",
            "same",
            0,
            definition.clone(),
            &catalog,
            &handlers,
        )
        .unwrap();
    definition.subflows.push(reference("same", 1));
    let next = store
        .save_neutral_contract_fixture("/project", "same", 1, definition, &catalog, &handlers)
        .unwrap();
    assert_eq!(next.subflow_pins.len(), 1);
    assert_eq!(
        store
            .get_neutral_publication("/project", "same", 2, &catalog, &handlers)
            .unwrap(),
        next
    );
}

// Catches: silently interpreting an older internal format as the current contract.
#[test]
fn old_internal_format_is_explicitly_refused() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let store = WorkflowStore::open_at(&db).unwrap();
    let (catalog, handlers, definition) = fixtures();
    store
        .save_neutral_contract_fixture("/project", "old", 0, definition, &catalog, &handlers)
        .unwrap();
    let conn = rusqlite::Connection::open(&db).unwrap();
    let raw: String = conn
        .query_row(
            "SELECT document FROM neutral_workflow_publications WHERE id='old'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut value: Value = serde_json::from_str(&raw).unwrap();
    for version in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] {
        value["format_version"] = json!(version);
        conn.execute(
            "UPDATE neutral_workflow_publications SET document=?1 WHERE id='old'",
            [value.to_string()],
        )
        .unwrap();
        assert!(
            store
                .get_neutral_publication("/project", "old", 1, &catalog, &handlers)
                .unwrap_err()
                .contains("unsupported neutral publication format")
        );
    }
}

// Catches: publication accepting a subject contract that permits invalid operation values.
#[test]
fn publication_checks_every_possible_subject_binding() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let (base, handlers, definition) = fixtures();
    let mut descriptor = base
        .get(&definition.instances[0].descriptor)
        .unwrap()
        .clone();
    descriptor.subject = Some(SubjectSchema {
        required: true,
        fields: ObjectSchema {
            fields: BTreeMap::from([("key".into(), field(ValueType::Number, true, None))]),
        },
    });
    let mut catalog = WorkflowCatalog::new();
    catalog.register(descriptor.clone()).unwrap();
    let mut def = definition.clone();
    def.instances.truncate(1);
    def.subject
        .as_mut()
        .unwrap()
        .fields
        .fields
        .get_mut("key")
        .unwrap()
        .value_type = ValueType::Integer;
    let save = |id: &str, d: NeutralDefinition| {
        store.save_neutral_contract_fixture("/project", id, 0, d, &catalog, &handlers)
    };
    save("valid", def.clone()).unwrap();
    let mut optional = def.clone();
    optional.subject.as_mut().unwrap().required = false;
    assert!(save("optional", optional).is_err());
    let mut absent = def.clone();
    absent.subject = None;
    assert!(save("absent", absent).is_err());
    let mut missing = def.clone();
    missing.subject.as_mut().unwrap().fields.fields.clear();
    assert!(save("missing", missing).is_err());
    let mut optional_field = def.clone();
    optional_field
        .subject
        .as_mut()
        .unwrap()
        .fields
        .fields
        .get_mut("key")
        .unwrap()
        .required = false;
    assert!(save("optional-field", optional_field.clone()).is_err());
    optional_field
        .subject
        .as_mut()
        .unwrap()
        .fields
        .fields
        .get_mut("key")
        .unwrap()
        .default = Some(json!(7));
    save("default", optional_field).unwrap();
    let mut wrong = def.clone();
    wrong
        .subject
        .as_mut()
        .unwrap()
        .fields
        .fields
        .get_mut("key")
        .unwrap()
        .value_type = ValueType::Number; // Number -> Number is valid.
    save("number", wrong.clone()).unwrap();
    wrong
        .subject
        .as_mut()
        .unwrap()
        .fields
        .fields
        .get_mut("key")
        .unwrap()
        .value_type = ValueType::String;
    assert!(save("wrong", wrong).is_err());
    let mut extra = def;
    extra
        .subject
        .as_mut()
        .unwrap()
        .fields
        .fields
        .insert("other".into(), field(ValueType::String, false, None));
    assert!(save("extra", extra).is_err());
}

// Catches: a rehashed publication bypassing subject compatibility on read.
#[test]
fn read_rechecks_subject_binding_even_if_content_hash_matches() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let store = WorkflowStore::open_at(&db).unwrap();
    let (base, handlers, mut definition) = fixtures();
    definition.instances.truncate(1);
    let mut descriptor = base
        .get(&definition.instances[0].descriptor)
        .unwrap()
        .clone();
    descriptor.subject = definition.subject.clone();
    let mut catalog = WorkflowCatalog::new();
    catalog.register(descriptor).unwrap();
    store
        .save_neutral_contract_fixture("/project", "root", 0, definition, &catalog, &handlers)
        .unwrap();
    let conn = rusqlite::Connection::open(db).unwrap();
    let raw: String = conn
        .query_row(
            "SELECT document FROM neutral_workflow_publications WHERE id='root'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut value: Value = serde_json::from_str(&raw).unwrap();
    value["definition"]["subject"]["required"] = json!(false);
    let digest = super::catalog::content_digest(&value).unwrap();
    conn.execute(
        "UPDATE neutral_workflow_publications SET document=?1,sha256=?2 WHERE id='root'",
        rusqlite::params![value.to_string(), digest],
    )
    .unwrap();
    assert!(
        store
            .get_neutral_publication("/project", "root", 1, &catalog, &handlers)
            .unwrap_err()
            .contains("required workflow subject")
    );
}

// Catches invalid projections accepted by save or a rehashed persisted row, and lost mappings after reopen.
#[test]
fn explicit_input_mappings_are_persisted_and_revalidated() {
    use super::data_binding::DataSource;
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let store = WorkflowStore::open_at(&db).unwrap();
    let (old, handlers, mut definition) = fixtures();
    let mut descriptor = old
        .get(&definition.instances[0].descriptor)
        .unwrap()
        .clone();
    descriptor.inputs = ObjectSchema {
        fields: BTreeMap::from([("text".into(), field(ValueType::String, true, None))]),
    };
    definition.instances.truncate(1);
    definition.input_bindings.insert(
        0,
        BTreeMap::from([(
            "text".into(),
            DataSource::WorkflowInput {
                field: "query".into(),
            },
        )]),
    );
    let mut catalog = WorkflowCatalog::new();
    catalog.register(descriptor).unwrap();
    let saved = store
        .save_neutral_contract_fixture(
            "/project",
            "mapped",
            0,
            definition.clone(),
            &catalog,
            &handlers,
        )
        .unwrap();
    drop(store);
    let reopened = WorkflowStore::open_at(&db).unwrap();
    assert_eq!(
        reopened
            .get_neutral_publication("/project", "mapped", 1, &catalog, &handlers)
            .unwrap(),
        saved
    );
    definition.input_bindings.get_mut(&0).unwrap().insert(
        "text".into(),
        DataSource::WorkflowInput {
            field: "limit".into(),
        },
    );
    assert!(
        reopened
            .save_neutral_contract_fixture(
                "/project",
                "invalid",
                0,
                definition.clone(),
                &catalog,
                &handlers
            )
            .unwrap_err()
            .contains("incompatible field")
    );
    definition.input_bindings.insert(9, BTreeMap::new());
    assert!(
        reopened
            .save_neutral_contract_fixture(
                "/project",
                "missing",
                0,
                definition.clone(),
                &catalog,
                &handlers
            )
            .is_err()
    );
    let mut tampered = saved;
    tampered.definition.input_bindings = definition.input_bindings;
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE neutral_workflow_publications SET document=?1,sha256=?2 WHERE id='mapped'",
        rusqlite::params![
            serde_json::to_string(&tampered).unwrap(),
            super::catalog::content_digest(&tampered).unwrap()
        ],
    )
    .unwrap();
    assert!(
        reopened
            .get_neutral_publication("/project", "mapped", 1, &catalog, &handlers)
            .is_err()
    );
}

// Catches conditions omitted from immutable publications or invalid predicates accepted on save/read.
#[test]
fn condition_contracts_survive_reopen_and_reject_invalid_persisted_sources() {
    use super::condition::{Comparison, Condition};
    use super::data_binding::DataSource;
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let store = WorkflowStore::open_at(&db).unwrap();
    let (catalog, handlers, mut definition) = fixtures();
    definition.conditions.insert(
        "selected".into(),
        Condition::Compare {
            comparison: Comparison::Equal,
            left: DataSource::WorkflowInput {
                field: "query".into(),
            },
            right: DataSource::Constant {
                value: json!("selected"),
            },
        },
    );
    let published = store
        .save_neutral_contract_fixture(
            "/project",
            "conditional",
            0,
            definition.clone(),
            &catalog,
            &handlers,
        )
        .unwrap();
    drop(store);
    let store = WorkflowStore::open_at(&db).unwrap();
    assert_eq!(
        store
            .get_neutral_publication("/project", "conditional", 1, &catalog, &handlers)
            .unwrap(),
        published
    );
    definition.conditions.insert(
        "invalid".into(),
        Condition::Exists {
            value: DataSource::WorkflowInput {
                field: "missing".into(),
            },
        },
    );
    assert!(
        store
            .save_neutral_contract_fixture(
                "/project",
                "invalid-condition",
                0,
                definition.clone(),
                &catalog,
                &handlers
            )
            .unwrap_err()
            .contains("unknown workflow input")
    );
    let mut changed = published;
    changed.definition = definition;
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE neutral_workflow_publications SET document=?1,sha256=?2 WHERE id='conditional'",
        rusqlite::params![
            serde_json::to_string(&changed).unwrap(),
            super::catalog::content_digest(&changed).unwrap()
        ],
    )
    .unwrap();
    assert!(
        store
            .get_neutral_publication("/project", "conditional", 1, &catalog, &handlers)
            .unwrap_err()
            .contains("unknown workflow input")
    );
}

// Catches: collections losing scoped bindings or bypassing source/bound/cycle validation at save or verified read.
#[test]
fn publication_persists_and_revalidates_collection_scopes() {
    use super::collection::{EmptyPolicy, FailurePolicy, ForEach};
    use super::data_binding::DataSource;
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let store = WorkflowStore::open_at(&db).unwrap();
    let (mut catalog, mut handlers, mut definition) = fixtures();
    let descriptor = ActivityDescriptor {
        id: DescriptorId {
            namespace: "example".into(),
            name: "item".into(),
            version: 1,
        },
        purpose: "Transform one item".into(),
        target: None,
        handler: HandlerRef {
            name: "host.item".into(),
            version: 1,
        },
        config: ObjectSchema::default(),
        inputs: ObjectSchema {
            fields: BTreeMap::from([
                ("item".into(), field(ValueType::String, true, None)),
                ("index".into(), field(ValueType::Integer, true, None)),
            ]),
        },
        outputs: ObjectSchema::default(),
        subject: None,
        outcomes: BTreeSet::from(["done".into()]),
        required_capabilities: BTreeSet::new(),
        execution_policies: BTreeSet::from(["local".into()]),
    };
    handlers.insert(descriptor.handler.clone());
    definition.instances.push(ActivityInstance {
        descriptor: descriptor.id.clone(),
        config: BTreeMap::new(),
        execution_policy: "local".into(),
    });
    catalog.register(descriptor).unwrap();
    let collection = ForEach {
        collection: DataSource::Constant {
            value: json!(["a", "b"]),
        },
        item_type: ValueType::String,
        body: 3,
        item_input: "item".into(),
        index_input: "index".into(),
        max_items: 2,
        max_concurrency: 1,
        empty: EmptyPolicy::Complete,
        failure: FailurePolicy::Collect,
    };
    definition.collections.insert("items".into(), collection);
    let published = store
        .save_neutral_contract_fixture(
            "/project",
            "collection",
            0,
            definition.clone(),
            &catalog,
            &handlers,
        )
        .unwrap();
    assert_eq!(
        store
            .get_neutral_publication("/project", "collection", 1, &catalog, &handlers)
            .unwrap(),
        published
    );
    let mut overwritten = definition.clone();
    overwritten.input_bindings.insert(
        3,
        BTreeMap::from([(
            "item".into(),
            DataSource::Constant {
                value: json!("override"),
            },
        )]),
    );
    assert!(
        store
            .save_neutral_contract_fixture(
                "/project",
                "overwrite",
                0,
                overwritten,
                &catalog,
                &handlers
            )
            .unwrap_err()
            .contains("overwrites")
    );
    let mut duplicate = definition.clone();
    duplicate
        .collections
        .insert("other".into(), definition.collections["items"].clone());
    assert!(
        store
            .save_neutral_contract_fixture(
                "/project",
                "duplicate",
                0,
                duplicate,
                &catalog,
                &handlers
            )
            .unwrap_err()
            .contains("more than one")
    );
    definition.collections.get_mut("items").unwrap().max_items = 1;
    assert!(
        store
            .save_neutral_contract_fixture(
                "/project",
                "invalid-collection",
                0,
                definition.clone(),
                &catalog,
                &handlers
            )
            .unwrap_err()
            .contains("max-items")
    );
    let mut changed = published;
    changed.definition = definition;
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE neutral_workflow_publications SET document=?1,sha256=?2 WHERE id='collection'",
        rusqlite::params![
            serde_json::to_string(&changed).unwrap(),
            super::catalog::content_digest(&changed).unwrap()
        ],
    )
    .unwrap();
    assert!(
        store
            .get_neutral_publication("/project", "collection", 1, &catalog, &handlers)
            .unwrap_err()
            .contains("max-items")
    );
}

// Catches: untyped loop carry or unbound subflow returns bypassing publication, and provenance disappearing at return.
#[test]
fn publication_validates_loop_state_and_structured_return_projections() {
    use super::data_binding::{DataOrigin, DataSource, DataState, Datum};
    use super::loop_data::LoopContract;
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let store = WorkflowStore::open_at(&db).unwrap();
    let (mut catalog, handlers, mut definition) = fixtures();
    let mut descriptor = catalog
        .get(&definition.instances[0].descriptor)
        .unwrap()
        .clone();
    descriptor.id.name = "loop-body".into();
    descriptor.inputs = ObjectSchema {
        fields: BTreeMap::from([("count".into(), field(ValueType::Integer, true, None))]),
    };
    descriptor.outputs = descriptor.inputs.clone();
    definition.instances.push(ActivityInstance {
        descriptor: descriptor.id.clone(),
        config: BTreeMap::new(),
        execution_policy: "local".into(),
    });
    definition.outputs = descriptor.outputs.clone();
    let returned = BTreeMap::from([(
        "count".into(),
        DataSource::ActivityOutput {
            activity: 3,
            field: "count".into(),
        },
    )]);
    definition.output_bindings = returned.clone();
    definition.loops.insert(
        "counter".into(),
        LoopContract {
            body: 3,
            max_iterations: 2,
            carried: descriptor.inputs.clone(),
            initial: BTreeMap::from([("count".into(), DataSource::Constant { value: json!(0) })]),
            result: returned,
        },
    );
    catalog.register(descriptor).unwrap();
    let published = store
        .save_neutral_contract_fixture(
            "/project",
            "loop",
            0,
            definition.clone(),
            &catalog,
            &handlers,
        )
        .unwrap();
    assert_eq!(
        store
            .get_neutral_publication("/project", "loop", 1, &catalog, &handlers)
            .unwrap(),
        published
    );
    assert!(
        definition
            .resolve_outputs(&BTreeMap::new(), &BTreeMap::new(), &BTreeMap::new())
            .unwrap_err()
            .contains("unknown")
    );
    let origin = DataOrigin::WorkflowInput {
        field: "remote".into(),
    };
    let outputs = BTreeMap::from([(
        3,
        BTreeMap::from([(
            "count".into(),
            Datum {
                state: DataState::Present(json!(2)),
                provenance: BTreeSet::from([origin.clone()]),
            },
        )]),
    )]);
    let returned = definition
        .resolve_outputs(&BTreeMap::new(), &outputs, &BTreeMap::new())
        .unwrap();
    assert_eq!(returned.values["count"], json!(2));
    assert!(returned.provenance["count"].contains(&origin));
    assert!(
        returned.provenance["count"].contains(&DataOrigin::ActivityOutput {
            activity: 3,
            field: "count".into()
        })
    );
    let mut unbound = definition.clone();
    unbound.output_bindings.clear();
    assert!(
        store
            .save_neutral_contract_fixture(
                "/project",
                "unbound-return",
                0,
                unbound,
                &catalog,
                &handlers
            )
            .unwrap_err()
            .contains("unbound")
    );
    definition.loops.get_mut("counter").unwrap().max_iterations = 0;
    assert!(
        store
            .save_neutral_contract_fixture(
                "/project",
                "invalid-loop",
                0,
                definition.clone(),
                &catalog,
                &handlers
            )
            .unwrap_err()
            .contains("iteration bound")
    );
    let mut changed = published;
    changed.definition = definition;
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE neutral_workflow_publications SET document=?1,sha256=?2 WHERE id='loop'",
        rusqlite::params![
            serde_json::to_string(&changed).unwrap(),
            super::catalog::content_digest(&changed).unwrap()
        ],
    )
    .unwrap();
    assert!(
        store
            .get_neutral_publication("/project", "loop", 1, &catalog, &handlers)
            .unwrap_err()
            .contains("iteration bound")
    );
}

// Catches: fork declarations escaping content pins or bypassing branch/bound validation on verified read.
#[test]
fn publication_persists_and_revalidates_fork_scopes() {
    use super::fork_scope::{BranchContract, ForkContract};
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let store = WorkflowStore::open_at(&db).unwrap();
    let (catalog, handlers, mut definition) = fixtures();
    definition.forks.insert(
        "parallel".into(),
        ForkContract {
            branches: BTreeMap::from([
                (
                    "left".into(),
                    BranchContract {
                        activity: 0,
                        required: true,
                    },
                ),
                (
                    "right".into(),
                    BranchContract {
                        activity: 1,
                        required: false,
                    },
                ),
            ]),
            max_concurrency: 2,
        },
    );
    let published = store
        .save_neutral_contract_fixture(
            "/project",
            "fork",
            0,
            definition.clone(),
            &catalog,
            &handlers,
        )
        .unwrap();
    assert_eq!(
        store
            .get_neutral_publication("/project", "fork", 1, &catalog, &handlers)
            .unwrap(),
        published
    );
    let mut conflicting = definition.clone();
    conflicting
        .forks
        .insert("other".into(), definition.forks["parallel"].clone());
    assert!(
        store
            .save_neutral_contract_fixture(
                "/project",
                "conflicting-scope",
                0,
                conflicting,
                &catalog,
                &handlers
            )
            .unwrap_err()
            .contains("more than one")
    );
    definition
        .forks
        .get_mut("parallel")
        .unwrap()
        .max_concurrency = 0;
    assert!(
        store
            .save_neutral_contract_fixture(
                "/project",
                "invalid-fork",
                0,
                definition.clone(),
                &catalog,
                &handlers
            )
            .unwrap_err()
            .contains("concurrency")
    );
    let mut changed = published;
    changed.definition = definition;
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE neutral_workflow_publications SET document=?1,sha256=?2 WHERE id='fork'",
        rusqlite::params![
            serde_json::to_string(&changed).unwrap(),
            super::catalog::content_digest(&changed).unwrap()
        ],
    )
    .unwrap();
    assert!(
        store
            .get_neutral_publication("/project", "fork", 1, &catalog, &handlers)
            .unwrap_err()
            .contains("concurrency")
    );
}
