use super::WorkflowStore;
use super::catalog::{
    ActivityDescriptor, ActivityInstance, DescriptorId, FieldSchema, HandlerRef, ObjectSchema,
    ValueType, WorkflowCatalog,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn fixture() -> (
    WorkflowCatalog,
    BTreeSet<HandlerRef>,
    ActivityInstance,
    ActivityDescriptor,
) {
    let handler = HandlerRef {
        name: "host.test".into(),
        version: 1,
    };
    let descriptor = ActivityDescriptor {
        id: DescriptorId {
            namespace: "test".into(),
            name: "activity".into(),
            version: 1,
        },
        purpose: "Test".into(),
        target: None,
        handler: handler.clone(),
        config: ObjectSchema {
            fields: BTreeMap::from([(
                "path".into(),
                FieldSchema {
                    value_type: ValueType::String,
                    required: true,
                    default: Some(json!("default")),
                },
            )]),
        },
        inputs: ObjectSchema::default(),
        subject: None,
        outputs: ObjectSchema::default(),
        outcomes: BTreeSet::from(["done".into()]),
        required_capabilities: BTreeSet::new(),
        execution_policies: BTreeSet::from(["local".into()]),
    };
    let instance = ActivityInstance {
        descriptor: descriptor.id.clone(),
        config: BTreeMap::from([("path".into(), json!("first"))]),
        execution_policy: "local".into(),
    };
    let mut catalog = WorkflowCatalog::new();
    catalog.register(descriptor.clone()).unwrap();
    (catalog, BTreeSet::from([handler]), instance, descriptor)
}

// Catches: non-durable presets, aliasing clones, or older revisions following latest.
#[test]
fn preset_reopens_and_clones_exact_revision() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let (catalog, handlers, instance, _) = fixture();
    let store = WorkflowStore::open_at(&db).unwrap();
    store
        .save_activity_preset("/project", "one", 0, instance.clone(), &catalog, &handlers)
        .unwrap();
    let mut next = instance.clone();
    next.config.insert("path".into(), json!("second"));
    store
        .save_activity_preset("/project", "one", 1, next, &catalog, &handlers)
        .unwrap();
    drop(store);
    let store = WorkflowStore::open_at(&db).unwrap();
    let mut first = store
        .apply_activity_preset("/project", "one", 1, &catalog, &handlers)
        .unwrap();
    let second = store
        .apply_activity_preset("/project", "one", 1, &catalog, &handlers)
        .unwrap();
    first.config.insert("path".into(), json!("edited"));
    assert_eq!(second.config.get("path"), Some(&json!("first")));
    assert_eq!(
        store
            .apply_activity_preset("/project", "one", 1, &catalog, &handlers)
            .unwrap()
            .config
            .get("path"),
        Some(&json!("first"))
    );
    assert_eq!(
        store
            .apply_activity_preset("/project", "one", 2, &catalog, &handlers)
            .unwrap()
            .config
            .get("path"),
        Some(&json!("second"))
    );
    assert_eq!(second.execution_policy, "local");
    assert_eq!(second.descriptor, instance.descriptor);
}

// Catches: concurrent writers both advancing the same expected revision.
#[test]
fn stale_preset_writers_cannot_overwrite() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let (catalog, handlers, instance, _) = fixture();
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let barrier = &barrier;
                let store = &store;
                let catalog = &catalog;
                let handlers = &handlers;
                let instance = &instance;
                scope.spawn(move || {
                    barrier.wait();
                    store.save_activity_preset(
                        "/project",
                        "one",
                        0,
                        instance.clone(),
                        catalog,
                        handlers,
                    )
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| r.as_ref().is_err_and(|e| e.contains("stale")))
            .count(),
        1
    );
}

// Catches: descriptor drift or unavailable handler silently applying a preset.
#[test]
fn preset_refuses_drift_and_missing_handler() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let (catalog, handlers, instance, mut descriptor) = fixture();
    assert!(
        store
            .save_activity_preset(
                "/project",
                "missing",
                0,
                instance.clone(),
                &catalog,
                &BTreeSet::new()
            )
            .unwrap_err()
            .contains("handler unavailable")
    );
    store
        .save_activity_preset("/project", "one", 0, instance, &catalog, &handlers)
        .unwrap();
    assert!(
        store
            .apply_activity_preset("/project", "one", 1, &catalog, &BTreeSet::new())
            .unwrap_err()
            .contains("handler unavailable")
    );
    descriptor.config.fields.get_mut("path").unwrap().default = Some(json!("changed"));
    let mut changed = WorkflowCatalog::new();
    changed.register(descriptor).unwrap();
    assert!(
        store
            .apply_activity_preset("/project", "one", 1, &changed, &handlers)
            .unwrap_err()
            .contains("content mismatch")
    );
}

// Catches: invalid config/policy or altered/foreign stored records reaching callers.
#[test]
fn preset_refuses_invalid_or_tampered_data() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("workflows.sqlite3");
    let store = WorkflowStore::open_at(&db).unwrap();
    let (mut catalog, handlers, instance, mut other) = fixture();
    other.id.name = "other".into();
    catalog.register(other.clone()).unwrap();
    let mut bad = instance.clone();
    bad.config.insert("path".into(), json!(5));
    assert!(
        store
            .save_activity_preset("/project", "bad", 0, bad, &catalog, &handlers)
            .is_err()
    );
    let mut bad = instance.clone();
    bad.execution_policy = "remote".into();
    assert!(
        store
            .save_activity_preset("/project", "bad", 0, bad, &catalog, &handlers)
            .is_err()
    );
    store
        .save_activity_preset("/project", "one", 0, instance, &catalog, &handlers)
        .unwrap();
    assert!(
        store
            .apply_activity_preset("/other", "one", 1, &catalog, &handlers)
            .is_err()
    );
    let conn = rusqlite::Connection::open(db).unwrap();
    let original: String = conn
        .query_row(
            "SELECT document FROM activity_presets WHERE id='one'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute(
        "UPDATE activity_presets SET document=replace(document,'first','forged') WHERE id='one'",
        [],
    )
    .unwrap();
    assert!(
        store
            .apply_activity_preset("/project", "one", 1, &catalog, &handlers)
            .unwrap_err()
            .contains("content mismatch")
    );
    // Catches: accepting a validly hashed document for a different project.
    let mut doc: serde_json::Value = serde_json::from_str(&original).unwrap();
    doc["project"] = json!("/other");
    let digest = super::catalog::content_digest(&doc).unwrap();
    conn.execute(
        "UPDATE activity_presets SET document=?1,sha256=?2 WHERE id='one'",
        rusqlite::params![doc.to_string(), digest],
    )
    .unwrap();
    assert!(
        store
            .apply_activity_preset("/project", "one", 1, &catalog, &handlers)
            .unwrap_err()
            .contains("identity or content mismatch")
    );

    // Catches: an available valid pin for B being accepted for instance A.
    doc = serde_json::from_str(&original).unwrap();
    doc["descriptor_pin"] = serde_json::to_value(catalog.pin(&other.id).unwrap()).unwrap();
    let digest = super::catalog::content_digest(&doc).unwrap();
    conn.execute(
        "UPDATE activity_presets SET document=?1,sha256=?2 WHERE id='one'",
        rusqlite::params![doc.to_string(), digest],
    )
    .unwrap();
    assert!(
        store
            .apply_activity_preset("/project", "one", 1, &catalog, &handlers)
            .unwrap_err()
            .contains("descriptor pin mismatch")
    );

    doc = serde_json::from_str(&original).unwrap();
    doc["format_version"] = json!(2);
    conn.execute(
        "UPDATE activity_presets SET document=?1 WHERE id='one'",
        [doc.to_string()],
    )
    .unwrap();
    assert!(
        store
            .apply_activity_preset("/project", "one", 1, &catalog, &handlers)
            .unwrap_err()
            .contains("unsupported")
    );
    doc["format_version"] = json!(1);
    doc["instance"]["config"]["path"] = json!(5);
    let digest = super::catalog::content_digest(&doc).unwrap();
    conn.execute(
        "UPDATE activity_presets SET document=?1,sha256=?2 WHERE id='one'",
        rusqlite::params![doc.to_string(), digest],
    )
    .unwrap();
    assert!(
        store
            .apply_activity_preset("/project", "one", 1, &catalog, &handlers)
            .unwrap_err()
            .contains("wrong type")
    );
}
