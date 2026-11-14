use super::catalog::{FieldSchema, ObjectSchema, ValueType};
use super::collection::CollectionIdentity;
use super::data_binding::{DataOrigin, DataSource, DataState, Datum};
use super::loop_data::LoopContract;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn schema(kind: ValueType) -> ObjectSchema {
    ObjectSchema {
        fields: BTreeMap::from([(
            "count".into(),
            FieldSchema {
                value_type: kind,
                required: true,
                default: None,
            },
        )]),
    }
}
fn contract() -> LoopContract {
    LoopContract {
        body: 0,
        max_iterations: 2,
        carried: schema(ValueType::Integer),
        initial: BTreeMap::from([("count".into(), DataSource::Constant { value: json!(0) })]),
        result: BTreeMap::from([(
            "count".into(),
            DataSource::ActivityOutput {
                activity: 0,
                field: "count".into(),
            },
        )]),
    }
}
fn identity() -> CollectionIdentity {
    CollectionIdentity {
        run: "run".into(),
        scope: "loop".into(),
        generation: 1,
    }
}

// Catches: an implicit output backedge, untyped carry or foreign-body result passing publication validation.
#[test]
fn loop_requires_explicit_typed_initial_and_body_results() {
    let valid = contract();
    let body = schema(ValueType::Integer);
    valid
        .validate(&ObjectSchema::default(), &[&body], &body, &BTreeMap::new())
        .unwrap();
    let mut invalid = valid.clone();
    invalid.max_iterations = 0;
    assert!(
        invalid
            .validate(&ObjectSchema::default(), &[&body], &body, &BTreeMap::new())
            .is_err()
    );
    invalid = valid.clone();
    invalid.initial = valid.result.clone();
    assert!(
        invalid
            .validate(&ObjectSchema::default(), &[&body], &body, &BTreeMap::new())
            .unwrap_err()
            .contains("self dependency")
    );
    invalid = valid.clone();
    invalid
        .result
        .insert("count".into(), DataSource::Constant { value: json!(1) });
    assert!(
        invalid
            .validate(&ObjectSchema::default(), &[&body], &body, &BTreeMap::new())
            .unwrap_err()
            .contains("current body")
    );
    assert!(
        valid
            .validate(
                &ObjectSchema::default(),
                &[&body],
                &schema(ValueType::String),
                &BTreeMap::new()
            )
            .is_err()
    );
    assert!(
        valid
            .validate(
                &ObjectSchema::default(),
                &[&schema(ValueType::String)],
                &body,
                &BTreeMap::new()
            )
            .is_err()
    );
}

// Catches: pending/missing/wrong state advancing a loop, lost provenance or exhausted traversal returning success.
#[test]
fn loop_results_are_immutable_typed_iteration_snapshots() {
    let contract = contract();
    let initial = contract
        .resolve(
            identity(),
            0,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
    assert_eq!(initial.fields.values["count"], json!(0));
    assert_eq!(initial.iteration, 0);
    assert!(
        contract
            .resolve(
                identity(),
                1,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &BTreeMap::new()
            )
            .unwrap_err()
            .contains("unknown")
    );
    for state in [DataState::Absent, DataState::Present(json!("wrong"))] {
        let outputs = BTreeMap::from([(
            0,
            BTreeMap::from([(
                "count".into(),
                Datum {
                    state,
                    provenance: BTreeSet::new(),
                },
            )]),
        )]);
        assert!(
            contract
                .resolve(identity(), 1, &BTreeMap::new(), &outputs, &BTreeMap::new())
                .is_err()
        );
    }
    let outputs = BTreeMap::from([(
        0,
        BTreeMap::from([(
            "count".into(),
            Datum {
                state: DataState::Present(json!(2)),
                provenance: BTreeSet::from([DataOrigin::WorkflowInput {
                    field: "seed".into(),
                }]),
            },
        )]),
    )]);
    let next = contract
        .resolve(identity(), 2, &BTreeMap::new(), &outputs, &BTreeMap::new())
        .unwrap();
    assert_eq!(next.fields.values["count"], json!(2));
    assert!(
        next.fields.provenance["count"].contains(&DataOrigin::WorkflowInput {
            field: "seed".into()
        })
    );
    assert!(
        next.fields.provenance["count"].contains(&DataOrigin::ActivityOutput {
            activity: 0,
            field: "count".into()
        })
    );
    assert_eq!(initial.fields.values["count"], json!(0));
    assert!(
        contract
            .resolve(identity(), 3, &BTreeMap::new(), &outputs, &BTreeMap::new())
            .unwrap_err()
            .contains("exhausted")
    );
}
