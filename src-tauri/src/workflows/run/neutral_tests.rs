use super::RunStore;
use super::neutral::{NeutralLimits, NeutralStartRequest};
use super::runtime::RuntimeOwner;
use crate::workflows::WorkflowStore;
use crate::workflows::catalog::{
    ActivityDescriptor, ActivityInstance, DescriptorId, FieldSchema, HandlerRef, ObjectSchema,
    SubjectSchema, ValueType, WorkflowCatalog,
};
use crate::workflows::neutral_publication::{NeutralDefinition, PublicationRef};
use rusqlite::params;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn definition(instances: Vec<ActivityInstance>) -> NeutralDefinition {
    NeutralDefinition {
        control: crate::workflows::neutral_control::ControlGraph {
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
        },
        inputs: ObjectSchema {
            fields: BTreeMap::from([(
                "text".into(),
                FieldSchema {
                    value_type: ValueType::String,
                    required: false,
                    default: Some(json!("retained input")),
                },
            )]),
        },
        subject: Some(SubjectSchema {
            required: false,
            fields: ObjectSchema {
                fields: BTreeMap::from([(
                    "revision".into(),
                    FieldSchema {
                        value_type: ValueType::Integer,
                        required: false,
                        default: Some(json!(7)),
                    },
                )]),
            },
        }),
        outputs: ObjectSchema::default(),
        outcomes: BTreeSet::from(["done".into()]),
        instances,
        input_bindings: BTreeMap::new(),
        conditions: BTreeMap::new(),
        collections: BTreeMap::new(),
        loops: BTreeMap::new(),
        forks: BTreeMap::new(),
        output_bindings: BTreeMap::new(),
        subflows: vec![],
    }
}

fn fixture() -> (
    tempfile::TempDir,
    WorkflowStore,
    WorkflowCatalog,
    BTreeSet<HandlerRef>,
    NeutralStartRequest,
) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let workflows = WorkflowStore::open_at(&dir.path().join("definitions.sqlite3")).unwrap();
    let id = DescriptorId {
        namespace: "test".into(),
        name: "transform".into(),
        version: 1,
    };
    let handler = HandlerRef {
        name: "host.transform".into(),
        version: 1,
    };
    let handlers = BTreeSet::from([handler.clone()]);
    let mut catalog = WorkflowCatalog::new();
    catalog
        .register(ActivityDescriptor {
            id: id.clone(),
            purpose: "Transform explicit text".into(),
            handler,
            target: None,
            config: ObjectSchema {
                fields: BTreeMap::from([(
                    "instructions".into(),
                    FieldSchema {
                        value_type: ValueType::String,
                        required: true,
                        default: None,
                    },
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
    let instance = ActivityInstance {
        descriptor: id,
        config: BTreeMap::from([(
            "instructions".into(),
            json!("Keep this exact inline prompt"),
        )]),
        execution_policy: "local".into(),
    };
    let child = workflows
        .save_neutral_contract_fixture(
            "/project",
            "child",
            0,
            definition(vec![instance.clone()]),
            &catalog,
            &handlers,
        )
        .unwrap();
    let mut root = definition(vec![instance]);
    root.subflows.push(PublicationRef {
        project: "/project".into(),
        id: child.id,
        revision: child.revision,
    });
    let root = workflows
        .save_neutral_contract_fixture("/project", "root", 0, root, &catalog, &handlers)
        .unwrap();
    let request = NeutralStartRequest {
        publication: PublicationRef {
            project: "/project".into(),
            id: root.id,
            revision: root.revision,
        },
        request_id: "start-1".into(),
        inputs: BTreeMap::new(),
        subject: Some(BTreeMap::new()),
        limits: NeutralLimits {
            max_activations: 20,
            max_concurrency: 2,
            max_duration_ms: 60_000,
        },
    };
    (dir, workflows, catalog, handlers, request)
}

// Catches: replay reading a changed draft/catalog, lost normalized values or inline
// prompt bodies, and accidentally requiring a story backend for neutral runs.
#[test]
fn neutral_start_replays_retained_closure_and_normalized_values_after_owner_restart() {
    let (dir, workflows, catalog, handlers, request) = fixture();
    let path = dir.path().join("runs.sqlite3");
    let owner = RuntimeOwner::acquire(&path).unwrap();
    let run = owner
        .start_neutral(&workflows, &request, &catalog, &handlers)
        .unwrap();
    assert_eq!(
        run.inputs,
        BTreeMap::from([("text".into(), json!("retained input"))])
    );
    assert_eq!(
        run.subject,
        Some(BTreeMap::from([("revision".into(), json!(7))]))
    );
    assert_eq!(run.closure.descriptors.len(), 1);
    assert_eq!(run.closure.children.len(), 1);
    assert_eq!(
        run.closure.root.definition.instances[0].config["instructions"],
        json!("Keep this exact inline prompt")
    );
    let mut edited = run.closure.root.definition.clone();
    edited.instances[0]
        .config
        .insert("instructions".into(), json!("new prompt"));
    workflows
        .save_neutral_contract_fixture("/project", "root", 1, edited, &catalog, &handlers)
        .unwrap();
    drop(owner);
    drop(catalog);
    drop(handlers);
    let owner = RuntimeOwner::acquire(&path).unwrap();
    assert_eq!(owner.store.neutral_run(&run.id).unwrap(), run);
    // Exact retry is acknowledged even after the originally pinned handler disappears.
    let retry = owner
        .start_neutral(
            &workflows,
            &request,
            &WorkflowCatalog::new(),
            &BTreeSet::new(),
        )
        .unwrap();
    assert_eq!(retry, run);
    drop(owner);
    assert_eq!(
        RunStore::open_at(&path)
            .unwrap()
            .neutral_run(&run.id)
            .unwrap(),
        run
    );
}

// Catches: duplicate concurrent starts making separate executions, or changed
// arguments being silently accepted under an existing idempotency identity.
#[test]
fn neutral_start_is_atomic_and_refuses_changed_request_identity() {
    let (dir, workflows, catalog, handlers, request) = fixture();
    let owner = RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
    let barrier = std::sync::Barrier::new(4);
    let runs = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    owner
                        .start_neutral(&workflows, &request, &catalog, &handlers)
                        .unwrap()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(runs.iter().all(|run| run == &runs[0]));
    let conn = owner.store.connect().unwrap();
    let counts: (i64, i64) = conn.query_row("SELECT (SELECT count(*) FROM neutral_workflow_runs), (SELECT count(*) FROM neutral_workflow_events)", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
    assert_eq!(counts, (1, 1));
    let mut cases = Vec::new();
    let mut changed = request.clone();
    changed.inputs.insert("text".into(), json!("changed"));
    cases.push(changed);
    let mut changed = request.clone();
    changed.subject = None;
    cases.push(changed);
    let mut changed = request.clone();
    changed.limits.max_activations += 1;
    cases.push(changed);
    let mut changed = request.clone();
    changed.publication.revision = 2;
    cases.push(changed);
    for changed in cases {
        assert!(
            owner
                .start_neutral(
                    &workflows,
                    &changed,
                    &WorkflowCatalog::new(),
                    &BTreeSet::new()
                )
                .unwrap_err()
                .contains("reused"),
            "{changed:?}"
        );
    }
}

// Catches: invalid typed starts leaving a durable run, missing handlers being
// interpreted as permission, and generic zero/overflow limits reaching the ledger.
#[test]
fn invalid_neutral_starts_leave_no_run_or_event() {
    let (dir, workflows, catalog, handlers, request) = fixture();
    let owner = RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
    let mut cases = Vec::new();
    let mut bad = request.clone();
    bad.inputs.insert("text".into(), json!(3));
    cases.push(bad);
    let mut bad = request.clone();
    bad.subject = Some(BTreeMap::from([("revision".into(), json!("wrong"))]));
    cases.push(bad);
    let mut bad = request.clone();
    bad.limits.max_concurrency = 0;
    cases.push(bad);
    let mut bad = request.clone();
    bad.limits.max_concurrency = 21;
    cases.push(bad);
    let mut bad = request.clone();
    bad.limits.max_duration_ms = u64::MAX;
    cases.push(bad);
    let mut bad = request.clone();
    bad.request_id = " ".into();
    cases.push(bad);
    for bad in cases {
        assert!(
            owner
                .start_neutral(&workflows, &bad, &catalog, &handlers)
                .is_err(),
            "{bad:?}"
        );
    }
    assert!(
        owner
            .start_neutral(&workflows, &request, &catalog, &BTreeSet::new())
            .unwrap_err()
            .contains("handler unavailable")
    );
    let conn = owner.store.connect().unwrap();
    let count: i64 = conn.query_row("SELECT (SELECT count(*) FROM neutral_workflow_runs) + (SELECT count(*) FROM neutral_workflow_events)", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 0);
}

// Catches: a corrupted immutable event or newer run contract being decoded as
// this build's contract. Inspection is retained, not repaired from current drafts.
#[test]
fn neutral_replay_refuses_corrupt_bytes_and_unsupported_contracts() {
    let (dir, workflows, catalog, handlers, request) = fixture();
    let owner = RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
    let run = owner
        .start_neutral(&workflows, &request, &catalog, &handlers)
        .unwrap();
    let conn = owner.store.connect().unwrap();
    conn.execute(
        "UPDATE neutral_workflow_runs SET contract_version=99 WHERE id=?1",
        [&run.id],
    )
    .unwrap();
    assert!(
        owner
            .store
            .neutral_run(&run.id)
            .unwrap_err()
            .contains("unsupported")
    );
    conn.execute(
        "UPDATE neutral_workflow_runs SET contract_version=1 WHERE id=?1",
        [&run.id],
    )
    .unwrap();
    let original: String = conn
        .query_row(
            "SELECT event_json FROM neutral_workflow_events WHERE run_id=?1",
            [&run.id],
            |row| row.get(0),
        )
        .unwrap();
    let mut corrupted: serde_json::Value = serde_json::from_str(&original).unwrap();
    corrupted["kind"]["run"]["inputs"]["text"] = json!("corrupted");
    conn.execute(
        "UPDATE neutral_workflow_events SET event_json=?2 WHERE run_id=?1",
        params![run.id, corrupted.to_string()],
    )
    .unwrap();
    assert!(
        owner
            .store
            .neutral_run(&run.id)
            .unwrap_err()
            .contains("content mismatch")
    );
    conn.execute(
        "UPDATE neutral_workflow_events SET event_json=?2 WHERE run_id=?1",
        params![run.id, original],
    )
    .unwrap();
    assert_eq!(owner.store.neutral_run(&run.id).unwrap(), run);
}

// Catches: neutral duration accepting timestamp-sized values that overflow a
// later deadline, or extraction accidentally changing the existing run ceiling.
#[test]
fn neutral_duration_reuses_existing_workflow_ceiling_at_real_start_boundary() {
    assert_eq!(super::super::duration::MAX_DURATION_MS, 604_800_000);
    let (dir, workflows, catalog, handlers, mut request) = fixture();
    let owner = RuntimeOwner::acquire(&dir.path().join("runs.sqlite3")).unwrap();
    request.limits.max_duration_ms = 604_800_000;
    let run = owner
        .start_neutral(&workflows, &request, &catalog, &handlers)
        .unwrap();
    assert_eq!(run.request.limits.max_duration_ms, 604_800_000);
    for duration_ms in [604_800_001, i64::MAX as u64] {
        request.request_id = format!("invalid-duration-{duration_ms}");
        request.limits.max_duration_ms = duration_ms;
        assert!(
            owner
                .start_neutral(&workflows, &request, &catalog, &handlers)
                .unwrap_err()
                .contains("neutral limits")
        );
    }
    let conn = owner.store.connect().unwrap();
    let count: i64 = conn
        .query_row("SELECT count(*) FROM neutral_workflow_runs", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 1, "invalid durations must not create a run");
}
