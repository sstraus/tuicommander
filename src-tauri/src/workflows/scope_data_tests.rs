use super::WorkflowStore;
use super::catalog::{
    ActivityDescriptor, ActivityInstance, DescriptorId, FieldSchema, HandlerRef, ObjectSchema,
    ValueType, WorkflowCatalog, content_digest,
};
use super::collection::{CollectionIdentity, EmptyPolicy, FailurePolicy, ForEach};
use super::condition::{Condition, Decision};
use super::data_binding::{DataOrigin, DataSource, DataState, resolve_bindings};
use super::fork_scope::{ArrivalState, BranchArrival, BranchAttempt, BranchContract, ForkContract};
use super::loop_data::LoopContract;
use super::neutral_control::ControlGraph;
use super::neutral_publication::NeutralDefinition;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn schema(fields: &[(&str, ValueType, bool)]) -> ObjectSchema {
    ObjectSchema {
        fields: fields
            .iter()
            .map(|(name, kind, required)| {
                (
                    name.to_string(),
                    FieldSchema {
                        value_type: kind.clone(),
                        required: *required,
                        default: None,
                    },
                )
            })
            .collect(),
    }
}
fn source(scope: &str, field: &str) -> DataSource {
    DataSource::ScopeOutput {
        scope: scope.into(),
        field: field.into(),
    }
}
fn bindings(source: DataSource) -> BTreeMap<String, DataSource> {
    BTreeMap::from([("value".into(), source)])
}
fn fixture() -> (WorkflowCatalog, BTreeSet<HandlerRef>, NeutralDefinition) {
    let descriptor = ActivityDescriptor {
        id: DescriptorId {
            namespace: "scope-test".into(),
            name: "transform".into(),
            version: 1,
        },
        purpose: "Declare typed scope data consumers".into(),
        handler: HandlerRef {
            name: "host.transform".into(),
            version: 1,
        },
        target: None,
        config: ObjectSchema::default(),
        subject: None,
        inputs: schema(&[
            ("item", ValueType::String, false),
            ("index", ValueType::Integer, false),
            ("value", ValueType::String, false),
        ]),
        outputs: schema(&[
            ("value", ValueType::String, true),
            ("items", ValueType::Array, true),
        ]),
        outcomes: BTreeSet::from(["done".into()]),
        required_capabilities: BTreeSet::new(),
        execution_policies: BTreeSet::from(["local".into()]),
    };
    let handlers = BTreeSet::from([descriptor.handler.clone()]);
    let instances = vec![
        ActivityInstance {
            descriptor: descriptor.id.clone(),
            config: BTreeMap::new(),
            execution_policy: "local".into()
        };
        4
    ];
    let mut catalog = WorkflowCatalog::new();
    catalog.register(descriptor).unwrap();
    let collections = [("labels", 0), ("other", 1)]
        .into_iter()
        .map(|(scope, body)| {
            (
                scope.into(),
                ForEach {
                    collection: DataSource::Constant {
                        value: json!(["a", "b"]),
                    },
                    item_type: ValueType::String,
                    body,
                    item_input: "item".into(),
                    index_input: "index".into(),
                    max_items: 2,
                    max_concurrency: 1,
                    empty: EmptyPolicy::Complete,
                    failure: FailurePolicy::Blocked,
                },
            )
        })
        .collect();
    (
        catalog,
        handlers,
        NeutralDefinition {
            control: ControlGraph {
                nodes: BTreeMap::new(),
                edges: BTreeMap::new(),
            },
            inputs: ObjectSchema::default(),
            outputs: ObjectSchema::default(),
            subject: None,
            outcomes: BTreeSet::from(["done".into()]),
            instances,
            input_bindings: BTreeMap::new(),
            conditions: BTreeMap::new(),
            collections,
            loops: BTreeMap::new(),
            forks: BTreeMap::new(),
            output_bindings: BTreeMap::new(),
            subflows: vec![],
        },
    )
}
fn loop_contract() -> LoopContract {
    LoopContract {
        body: 0,
        max_iterations: 2,
        carried: schema(&[("value", ValueType::String, true)]),
        initial: bindings(DataSource::Constant {
            value: json!("seed"),
        }),
        result: bindings(DataSource::ActivityOutput {
            activity: 0,
            field: "value".into(),
        }),
    }
}
fn fork_contract() -> ForkContract {
    ForkContract {
        branches: BTreeMap::from([
            (
                "left".into(),
                BranchContract {
                    activity: 1,
                    required: true,
                },
            ),
            (
                "right".into(),
                BranchContract {
                    activity: 2,
                    required: false,
                },
            ),
        ]),
        max_concurrency: 2,
    }
}

// Catches: scope references bypassing source/field/type checks at save or rehashed verified read.
#[test]
fn publication_validates_scope_fields_in_bindings_and_conditions() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("scope-fields.sqlite3");
    let store = WorkflowStore::open_at(&db).unwrap();
    let (catalog, handlers, mut definition) = fixture();
    definition
        .input_bindings
        .insert(2, bindings(source("labels", "state")));
    definition.outputs = schema(&[("value", ValueType::String, true)]);
    definition.output_bindings = bindings(source("labels", "state"));
    for (index, invalid) in [
        source("missing", "state"),
        source("labels", "missing"),
        source("labels", "total"),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("field-{index}");
        let mut saved = store
            .save_neutral_contract_fixture(
                "/project",
                &id,
                0,
                definition.clone(),
                &catalog,
                &handlers,
            )
            .unwrap();
        let mut broken = saved.definition.clone();
        broken.output_bindings = bindings(invalid.clone());
        assert!(
            store
                .save_neutral_publication(
                    "/project",
                    &format!("bad-{id}"),
                    0,
                    broken.clone(),
                    &catalog,
                    &handlers
                )
                .is_err(),
            "{id}"
        );
        // Condition operands use the same declared scope schemas, not an unchecked value path.
        broken = saved.definition.clone();
        broken.conditions.insert(
            "invalid".into(),
            Condition::Exists {
                value: source("labels", "missing"),
            },
        );
        assert!(
            store
                .save_neutral_publication(
                    "/project",
                    &format!("condition-{id}"),
                    0,
                    broken,
                    &catalog,
                    &handlers
                )
                .is_err()
        );
        saved.definition.output_bindings = bindings(invalid);
        rusqlite::Connection::open(&db)
            .unwrap()
            .execute(
                "UPDATE neutral_workflow_publications SET document=?1,sha256=?2 WHERE id=?3",
                rusqlite::params![
                    serde_json::to_string(&saved).unwrap(),
                    content_digest(&saved).unwrap(),
                    id
                ],
            )
            .unwrap();
        assert!(
            store
                .get_neutral_publication("/project", &id, 1, &catalog, &handlers)
                .is_err(),
            "{id}"
        );
    }
}

// Catches: aggregate references hiding own-scope/cross-scope dependencies, including nonfirst fork branches.
#[test]
fn publication_refuses_scope_body_cycles_but_keeps_explicit_loop_boundary() {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let db = dir.path().join("scope-cycles.sqlite3");
    let store = WorkflowStore::open_at(&db).unwrap();
    let (catalog, handlers, definition) = fixture();
    for scenario in 0..6 {
        let mut valid = definition.clone();
        if scenario == 3 {
            valid.collections.clear();
            valid.loops.insert("loop".into(), loop_contract());
        } else if scenario >= 4 {
            valid.collections.clear();
            valid.forks.insert("fork".into(), fork_contract());
        }
        let id = format!("cycle-{scenario}");
        let mut saved = store
            .save_neutral_contract_fixture("/project", &id, 0, valid, &catalog, &handlers)
            .unwrap();
        let mut broken = saved.definition.clone();
        match scenario {
            0 => {
                broken
                    .input_bindings
                    .insert(0, bindings(source("labels", "state")));
            }
            1 => {
                broken
                    .input_bindings
                    .insert(0, bindings(source("other", "state")));
                broken
                    .input_bindings
                    .insert(1, bindings(source("labels", "state")));
            }
            2 => {
                broken.collections.get_mut("labels").unwrap().collection =
                    source("labels", "items");
            }
            3 => {
                broken.loops.get_mut("loop").unwrap().initial = bindings(source("loop", "value"));
            }
            4 => {
                broken
                    .input_bindings
                    .insert(1, bindings(source("fork", "state")));
            }
            5 => {
                broken
                    .input_bindings
                    .insert(3, bindings(source("fork", "state")));
                broken.input_bindings.insert(
                    2,
                    bindings(DataSource::ActivityOutput {
                        activity: 3,
                        field: "value".into(),
                    }),
                );
            }
            _ => unreachable!(),
        }
        let error = store
            .save_neutral_publication(
                "/project",
                &format!("bad-{id}"),
                0,
                broken.clone(),
                &catalog,
                &handlers,
            )
            .unwrap_err();
        assert!(error.contains("cycle"), "{scenario}: {error}");
        saved.definition = broken;
        rusqlite::Connection::open(&db)
            .unwrap()
            .execute(
                "UPDATE neutral_workflow_publications SET document=?1,sha256=?2 WHERE id=?3",
                rusqlite::params![
                    serde_json::to_string(&saved).unwrap(),
                    content_digest(&saved).unwrap(),
                    id
                ],
            )
            .unwrap();
        let error = store
            .get_neutral_publication("/project", &id, 1, &catalog, &handlers)
            .unwrap_err();
        assert!(error.contains("cycle"), "{scenario}: {error}");
    }
}

// Catches: settled omitted carried fields becoming null, pending scopes choosing false, or lost identity provenance.
#[test]
fn loop_and_fork_snapshots_feed_ordinary_scope_sources_without_fabricated_outputs() {
    let identity = CollectionIdentity {
        run: "run".into(),
        scope: "loop".into(),
        generation: 2,
    };
    let mut contract = loop_contract();
    contract.carried.fields.insert(
        "note".into(),
        FieldSchema {
            value_type: ValueType::String,
            required: false,
            default: None,
        },
    );
    let result = contract
        .resolve(
            identity.clone(),
            0,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
    let mut scopes = BTreeMap::from([(
        "loop".into(),
        super::scope_data::loop_snapshot(&contract, &result),
    )]);
    let note = source("loop", "note").resolve(&BTreeMap::new(), &BTreeMap::new(), &scopes);
    assert_eq!(note.state, DataState::Absent);
    let value = source("loop", "value").resolve(&BTreeMap::new(), &BTreeMap::new(), &scopes);
    assert_eq!(value.state, DataState::Present(json!("seed")));
    assert!(value.provenance.contains(&DataOrigin::ScopeResult {
        identity: identity.clone()
    }));
    assert!(
        note.provenance
            .contains(&DataOrigin::ScopeResult { identity })
    );
    let condition = Condition::Exists {
        value: source("fork", "state"),
    };
    assert_eq!(
        condition
            .evaluate(&BTreeMap::new(), &BTreeMap::new(), &scopes)
            .unwrap()
            .decision,
        Decision::Unknown
    );
    let identity = CollectionIdentity {
        run: "run".into(),
        scope: "fork".into(),
        generation: 1,
    };
    let owners = BTreeMap::from([
        (
            "left".into(),
            BranchAttempt {
                activation: "left-1".into(),
                attempt: 1,
            },
        ),
        (
            "right".into(),
            BranchAttempt {
                activation: "right-1".into(),
                attempt: 1,
            },
        ),
    ]);
    let fork = fork_contract();
    let pending = fork.join(&identity, &owners, &[]).unwrap();
    assert!(
        super::scope_data::fork_snapshot(&pending)
            .unwrap_err()
            .contains("pending")
    );
    let arrivals = [
        ("right", ArrivalState::Failed),
        ("left", ArrivalState::Completed),
    ]
    .into_iter()
    .map(|(branch, state)| BranchArrival {
        identity: identity.clone(),
        branch: branch.into(),
        owner: owners[branch].clone(),
        state,
    })
    .collect::<Vec<_>>();
    let joined = fork.join(&identity, &owners, &arrivals).unwrap();
    scopes.insert(
        "fork".into(),
        super::scope_data::fork_snapshot(&joined).unwrap(),
    );
    let target = schema(&[
        ("state", ValueType::String, true),
        ("branches", ValueType::Object, true),
    ]);
    let bindings = BTreeMap::from([
        ("state".into(), source("fork", "state")),
        ("branches".into(), source("fork", "branches")),
    ]);
    let returned = resolve_bindings(
        &bindings,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &target,
        &scopes,
    )
    .unwrap();
    assert_eq!(returned.values["state"], json!("complete"));
    assert_eq!(
        returned.values["branches"],
        json!({"left":"completed", "right":"failed"})
    );
    assert!(returned.provenance["branches"].contains(&DataOrigin::ScopeResult { identity }));
    assert!(note.provenance.contains(&DataOrigin::ScopeOutput {
        scope: "loop".into(),
        field: "note".into()
    }));
}

// Catches: failed/skipped collection items being serialized as successful null outputs.
#[test]
fn scope_items_retain_failure_and_skip_without_success_fields() {
    use super::collection::{ItemResult, ItemState};
    let (catalog, _, definition) = fixture();
    let collection = &definition.collections["labels"];
    let identity = CollectionIdentity {
        run: "run".into(),
        scope: "labels".into(),
        generation: 1,
    };
    let expanded = collection
        .expand(
            identity,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
    let descriptor = catalog.get(&definition.instances[0].descriptor).unwrap();
    let aggregate = collection
        .aggregate(
            &expanded,
            &[
                ItemResult {
                    identity: expanded.items()[1].identity.clone(),
                    state: ItemState::Skipped {
                        reason: "not selected".into(),
                    },
                },
                ItemResult {
                    identity: expanded.items()[0].identity.clone(),
                    state: ItemState::Failed {
                        diagnostic: "unavailable".into(),
                    },
                },
            ],
            &descriptor.outputs,
            &descriptor.outcomes,
        )
        .unwrap();
    let fields = super::scope_data::collection_snapshot(&aggregate).unwrap();
    assert_eq!(fields["state"].state, DataState::Present(json!("blocked")));
    assert_eq!(fields["failed"].state, DataState::Present(json!(1)));
    assert_eq!(fields["skipped"].state, DataState::Present(json!(1)));
    assert_eq!(
        fields["items"].state,
        DataState::Present(json!([
            {"index":0,"state":"failed","diagnostic":"unavailable"},
            {"index":1,"state":"skipped","reason":"not selected"},
        ]))
    );
}
