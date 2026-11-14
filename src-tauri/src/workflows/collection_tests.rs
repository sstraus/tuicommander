use super::catalog::{FieldSchema, ObjectSchema, ValueType};
use super::collection::*;
use super::data_binding::{DataOrigin, DataSource, DataState, Datum};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn spec() -> ForEach {
    ForEach {
        collection: DataSource::Constant {
            value: json!(["first", "second", "third"]),
        },
        item_type: ValueType::String,
        body: 0,
        item_input: "item".into(),
        index_input: "index".into(),
        max_items: 3,
        max_concurrency: 2,
        empty: EmptyPolicy::Complete,
        failure: FailurePolicy::Collect,
    }
}

fn identity(generation: u64) -> CollectionIdentity {
    CollectionIdentity {
        run: "run-1".into(),
        scope: "items".into(),
        generation,
    }
}

fn field(kind: ValueType, required: bool) -> FieldSchema {
    FieldSchema {
        value_type: kind,
        required,
        default: None,
    }
}

fn body() -> ObjectSchema {
    ObjectSchema {
        fields: BTreeMap::from([
            ("item".into(), field(ValueType::String, true)),
            ("index".into(), field(ValueType::Integer, true)),
        ]),
    }
}

// Catches: completion order replacing source identity, or failed/skipped/absent results becoming null successes.
#[test]
fn aggregation_preserves_source_order_and_non_success_states() {
    let spec = spec();
    let expanded = spec
        .expand(
            identity(1),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
    assert_eq!(
        expanded.items()[1].inputs["item"].state,
        DataState::Present(json!("second"))
    );
    assert_eq!(
        expanded.items()[1].inputs["index"].state,
        DataState::Present(json!(1))
    );
    let results = vec![
        ItemResult {
            identity: expanded.items()[2].identity.clone(),
            state: ItemState::Skipped {
                reason: "filtered".into(),
            },
        },
        ItemResult {
            identity: expanded.items()[0].identity.clone(),
            state: ItemState::Completed {
                outcome: "done".into(),
                outputs: BTreeMap::new(),
            },
        },
        ItemResult {
            identity: expanded.items()[1].identity.clone(),
            state: ItemState::Failed {
                diagnostic: "refused".into(),
            },
        },
    ];
    let schema = ObjectSchema {
        fields: BTreeMap::from([("optional".into(), field(ValueType::String, false))]),
    };
    let outcomes = BTreeSet::from(["done".into()]);
    let aggregate = spec
        .aggregate(&expanded, &results, &schema, &outcomes)
        .unwrap();
    assert_eq!(aggregate.state, AggregateState::Complete);
    assert_eq!(
        aggregate
            .items
            .iter()
            .map(|item| item.identity.index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    let ItemState::Completed { outputs, .. } = &aggregate.items[0].state else {
        panic!("expected first item output")
    };
    assert_eq!(outputs["optional"].state, DataState::Absent);
    assert!(
        matches!(&aggregate.items[1].state, ItemState::Failed { diagnostic } if diagnostic == "refused")
    );
    assert!(
        matches!(&aggregate.items[2].state, ItemState::Skipped { reason } if reason == "filtered")
    );
    let mut blocked = spec.clone();
    blocked.failure = FailurePolicy::Blocked;
    assert_eq!(
        blocked
            .aggregate(&expanded, &results, &schema, &outcomes)
            .unwrap()
            .state,
        AggregateState::Blocked
    );
    assert_eq!(
        spec.aggregate(&expanded, &results[..2], &schema, &outcomes)
            .unwrap()
            .state,
        AggregateState::Pending
    );
}

// Catches: a prior run/scope/generation, duplicate index or invalid result contaminating a new collection.
#[test]
fn aggregate_refuses_stale_duplicate_and_invalid_results() {
    let spec = spec();
    let expanded = spec
        .expand(
            identity(2),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
    let outcomes = BTreeSet::from(["done".into()]);
    let schema = ObjectSchema {
        fields: BTreeMap::from([("answer".into(), field(ValueType::Integer, true))]),
    };
    let mut result = ItemResult {
        identity: expanded.items()[0].identity.clone(),
        state: ItemState::Completed {
            outcome: "done".into(),
            outputs: BTreeMap::from([(
                "answer".into(),
                Datum {
                    state: DataState::Present(json!(42)),
                    provenance: BTreeSet::from([DataOrigin::Constant]),
                },
            )]),
        },
    };
    let valid = result.clone();
    for stale in [
        CollectionIdentity {
            run: "other".into(),
            ..identity(2)
        },
        CollectionIdentity {
            scope: "other".into(),
            ..identity(2)
        },
        identity(1),
    ] {
        result.identity.collection = stale;
        assert!(
            spec.aggregate(&expanded, &[result.clone()], &schema, &outcomes)
                .unwrap_err()
                .contains("generation")
        );
    }
    assert!(
        spec.aggregate(
            &expanded,
            &[valid.clone(), valid.clone()],
            &schema,
            &outcomes
        )
        .unwrap_err()
        .contains("duplicate")
    );
    result = valid.clone();
    result.identity.index = 3;
    assert!(
        spec.aggregate(&expanded, &[result], &schema, &outcomes)
            .unwrap_err()
            .contains("range")
    );
    for state in [
        DataState::Unknown,
        DataState::Absent,
        DataState::Present(json!("wrong")),
    ] {
        result = valid.clone();
        if let ItemState::Completed { outputs, .. } = &mut result.state {
            outputs.get_mut("answer").unwrap().state = state;
        }
        assert!(
            spec.aggregate(&expanded, &[result], &schema, &outcomes)
                .is_err()
        );
    }
    let mut defaulted = schema.clone();
    defaulted.fields.get_mut("answer").unwrap().default = Some(json!(7));
    let omitted = ItemResult {
        identity: valid.identity.clone(),
        state: ItemState::Completed {
            outcome: "done".into(),
            outputs: BTreeMap::new(),
        },
    };
    assert!(
        spec.aggregate(&expanded, &[omitted], &defaulted, &outcomes)
            .unwrap_err()
            .contains("required output absent")
    );
    result = valid;
    if let ItemState::Completed { outcome, .. } = &mut result.state {
        *outcome = "invented".into();
    }
    assert!(
        spec.aggregate(&expanded, &[result], &schema, &outcomes)
            .is_err()
    );
}

// Catches: unknown/absent/null becoming empty success, unbounded fanout or partially expanding invalid arrays.
#[test]
fn expansion_validates_all_items_and_preserves_source_provenance() {
    let mut spec = spec();
    for value in [json!(["a", "b", "c", "d"]), json!(["a", 2]), json!(null)] {
        spec.collection = DataSource::Constant { value };
        assert!(
            spec.expand(
                identity(1),
                &BTreeMap::new(),
                &BTreeMap::new(),
                &BTreeMap::new()
            )
            .is_err()
        );
    }
    spec.collection = DataSource::ActivityOutput {
        activity: 1,
        field: "page".into(),
    };
    assert!(
        spec.expand(
            identity(1),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new()
        )
        .unwrap_err()
        .contains("unknown")
    );
    let mut outputs = BTreeMap::from([(1, BTreeMap::new())]);
    assert!(
        spec.expand(identity(1), &BTreeMap::new(), &outputs, &BTreeMap::new())
            .unwrap_err()
            .contains("absent")
    );
    outputs.get_mut(&1).unwrap().insert(
        "page".into(),
        Datum {
            state: DataState::Present(json!(["x", "x"])),
            provenance: BTreeSet::from([DataOrigin::WorkflowInput {
                field: "remote".into(),
            }]),
        },
    );
    let expanded = spec
        .expand(identity(1), &BTreeMap::new(), &outputs, &BTreeMap::new())
        .unwrap();
    assert_ne!(expanded.items()[0].identity, expanded.items()[1].identity);
    assert!(
        expanded.items()[1].inputs["item"]
            .provenance
            .contains(&DataOrigin::WorkflowInput {
                field: "remote".into()
            })
    );
    assert!(
        expanded.items()[1].inputs["item"]
            .provenance
            .contains(&DataOrigin::ActivityOutput {
                activity: 1,
                field: "page".into()
            })
    );
    spec.collection = DataSource::WorkflowInput {
        field: "items".into(),
    };
    let workflow = BTreeMap::from([(
        "items".into(),
        Datum {
            state: DataState::Present(json!([])),
            provenance: BTreeSet::new(),
        },
    )]);
    let empty = spec
        .expand(identity(1), &workflow, &BTreeMap::new(), &BTreeMap::new())
        .unwrap();
    assert_eq!(empty.items(), []);
    assert_eq!(
        spec.aggregate(&empty, &[], &ObjectSchema::default(), &BTreeSet::new())
            .unwrap()
            .state,
        AggregateState::Complete
    );
    spec.empty = EmptyPolicy::Blocked;
    assert_eq!(
        spec.aggregate(&empty, &[], &ObjectSchema::default(), &BTreeSet::new())
            .unwrap()
            .state,
        AggregateState::Blocked
    );
}

// Catches: invalid collection bounds, scoped ports or source types being admitted by publication validation.
#[test]
fn collection_declarations_require_typed_scopes_and_finite_limits() {
    let valid = spec();
    valid
        .validate(&ObjectSchema::default(), &[], &body(), &BTreeMap::new())
        .unwrap();
    for (items, concurrency) in [(0, 1), (3, 0), (3, 4)] {
        let mut invalid = valid.clone();
        invalid.max_items = items;
        invalid.max_concurrency = concurrency;
        assert!(
            invalid
                .validate(&ObjectSchema::default(), &[], &body(), &BTreeMap::new())
                .is_err()
        );
    }
    let mut invalid = valid.clone();
    invalid.index_input = "item".into();
    assert!(
        invalid
            .validate(&ObjectSchema::default(), &[], &body(), &BTreeMap::new())
            .is_err()
    );
    assert!(
        valid
            .validate(
                &ObjectSchema::default(),
                &[],
                &ObjectSchema::default(),
                &BTreeMap::new()
            )
            .is_err()
    );
    invalid = valid.clone();
    invalid.collection = DataSource::Constant {
        value: json!("not-array"),
    };
    assert!(
        invalid
            .validate(&ObjectSchema::default(), &[], &body(), &BTreeMap::new())
            .is_err()
    );
    invalid.collection = DataSource::ActivityOutput {
        activity: 0,
        field: "items".into(),
    };
    let outputs = ObjectSchema {
        fields: BTreeMap::from([("items".into(), field(ValueType::Array, true))]),
    };
    assert!(
        invalid
            .validate(
                &ObjectSchema::default(),
                &[&outputs],
                &body(),
                &BTreeMap::new()
            )
            .is_err()
    );
}

// Catches: failed/skipped items disappearing from typed aggregate counts or an empty aggregate losing scope provenance.
#[test]
fn aggregate_summaries_preserve_non_success_counts_and_empty_scope_identity() {
    let mut spec = spec();
    let expanded = spec
        .expand(
            identity(1),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
    let results = vec![
        ItemResult {
            identity: expanded.items()[0].identity.clone(),
            state: ItemState::Completed {
                outcome: "done".into(),
                outputs: BTreeMap::new(),
            },
        },
        ItemResult {
            identity: expanded.items()[1].identity.clone(),
            state: ItemState::Failed {
                diagnostic: "refused".into(),
            },
        },
        ItemResult {
            identity: expanded.items()[2].identity.clone(),
            state: ItemState::Skipped {
                reason: "filtered".into(),
            },
        },
    ];
    let outcomes = BTreeSet::from(["done".into()]);
    let schema = ObjectSchema::default();
    let summary = spec
        .aggregate(&expanded, &results, &schema, &outcomes)
        .unwrap()
        .summary()
        .unwrap();
    assert_eq!(summary.values["total"], json!(3));
    assert_eq!(summary.values["completed"], json!(1));
    assert_eq!(summary.values["failed"], json!(1));
    assert_eq!(summary.values["skipped"], json!(1));
    spec.failure = FailurePolicy::Blocked;
    assert_eq!(
        spec.aggregate(&expanded, &results, &schema, &outcomes)
            .unwrap()
            .summary()
            .unwrap()
            .values["state"],
        json!("blocked")
    );
    spec.collection = DataSource::Constant { value: json!([]) };
    let empty = spec
        .expand(
            identity(2),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
    let summary = spec
        .aggregate(&empty, &[], &schema, &outcomes)
        .unwrap()
        .summary()
        .unwrap();
    assert_eq!(summary.values["total"], json!(0));
    assert_eq!(
        summary.provenance["total"],
        BTreeSet::from([DataOrigin::ScopeResult {
            identity: identity(2)
        }])
    );
}
