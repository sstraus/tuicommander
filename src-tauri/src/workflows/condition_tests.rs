use super::catalog::{FieldSchema, ObjectSchema, ValueType};
use super::condition::*;
use super::data_binding::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn literal(value: Value) -> DataSource {
    DataSource::Constant { value }
}
fn compare(comparison: Comparison, left: Value, right: Value) -> Condition {
    Condition::Compare {
        comparison,
        left: literal(left),
        right: literal(right),
    }
}
fn run(condition: &Condition) -> Result<Decision, String> {
    condition.validate(&ObjectSchema::default(), &[], &BTreeMap::new())?;
    condition
        .evaluate(&BTreeMap::new(), &BTreeMap::new(), &BTreeMap::new())
        .map(|result| result.decision)
}

// Catches unknown/absent operands selecting false, and null or empty values mistaken for absence.
#[test]
fn conditions_keep_unknown_distinct_from_false_and_absent() {
    let source = DataSource::ActivityOutput {
        activity: 0,
        field: "value".into(),
    };
    let exists = Condition::Exists {
        value: source.clone(),
    };
    let equality = Condition::Compare {
        comparison: Comparison::Equal,
        left: source,
        right: literal(Value::Null),
    };
    let empty = BTreeMap::new();
    assert_eq!(
        exists
            .evaluate(&empty, &BTreeMap::new(), &BTreeMap::new())
            .unwrap()
            .decision,
        Decision::Unknown
    );
    assert_eq!(
        equality
            .evaluate(&empty, &BTreeMap::new(), &BTreeMap::new())
            .unwrap()
            .decision,
        Decision::Unknown
    );
    let absent = BTreeMap::from([(0, BTreeMap::new())]);
    assert_eq!(
        exists
            .evaluate(&empty, &absent, &BTreeMap::new())
            .unwrap()
            .decision,
        Decision::False
    );
    assert_eq!(
        equality
            .evaluate(&empty, &absent, &BTreeMap::new())
            .unwrap()
            .decision,
        Decision::Unknown
    );
    for value in [Value::Null, json!([]), json!(false), json!("")] {
        let outputs = BTreeMap::from([(
            0,
            BTreeMap::from([(
                "value".into(),
                Datum {
                    state: DataState::Present(value),
                    provenance: BTreeSet::new(),
                },
            )]),
        )]);
        assert_eq!(
            exists
                .evaluate(&empty, &outputs, &BTreeMap::new())
                .unwrap()
                .decision,
            Decision::True
        );
    }
    assert_eq!(
        run(&compare(Comparison::Equal, Value::Null, Value::Null)).unwrap(),
        Decision::True
    );
}

// Catches f64 rounding adjacent artifact-sized integer identifiers into the same number.
#[test]
fn numeric_comparison_preserves_large_signed_unsigned_and_float_boundaries() {
    let cases = [
        (
            Comparison::Equal,
            json!(9007199254740993u64),
            json!(9007199254740992u64),
            Decision::False,
        ),
        (
            Comparison::Greater,
            json!(u64::MAX),
            json!(i64::MAX),
            Decision::True,
        ),
        (Comparison::Less, json!(-1), json!(u64::MAX), Decision::True),
        (
            Comparison::Greater,
            json!(9007199254740993u64),
            json!(9007199254740992.0f64),
            Decision::True,
        ),
        (
            Comparison::Less,
            json!(-9007199254740993i64),
            json!(-9007199254740992.0f64),
            Decision::True,
        ),
        (
            Comparison::Greater,
            json!(18446744073709551616.0f64),
            json!(u64::MAX),
            Decision::True,
        ),
        (
            Comparison::LessOrEqual,
            json!(1),
            json!(1.5),
            Decision::True,
        ),
        (
            Comparison::GreaterOrEqual,
            json!(-1.0),
            json!(-1),
            Decision::True,
        ),
        (Comparison::Equal, json!(1), json!(1.0), Decision::True),
        (
            Comparison::Less,
            json!(-1e300),
            json!(i64::MIN),
            Decision::True,
        ),
        (Comparison::Less, json!(1.25), json!(1.5), Decision::True),
        (Comparison::Less, json!("a"), json!("b"), Decision::True),
    ];
    let mut failures = Vec::new();
    for (operator, a, b, expected) in cases {
        let condition = compare(operator, a, b);
        let result = run(&condition).unwrap();
        if result != expected {
            failures.push(format!("{condition:?}: {result:?}, expected {expected:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Catches arbitrary operators, unknown fields and invalid operand types being accepted at publication.
#[test]
fn conditions_validate_closed_operators_and_typed_sources() {
    for condition in [
        compare(Comparison::Equal, json!(1), json!("1")),
        compare(Comparison::Greater, json!(true), json!(false)),
        Condition::Contains {
            collection: literal(json!("abc")),
            value: literal(json!("a")),
        },
        Condition::Exists {
            value: DataSource::WorkflowInput {
                field: "missing".into(),
            },
        },
        Condition::Exists {
            value: DataSource::ActivityOutput {
                activity: 8,
                field: "missing".into(),
            },
        },
    ] {
        assert!(run(&condition).is_err(), "{condition:?}");
    }
    assert!(serde_json::from_value::<Condition>(json!({"operator":"eval","code":"true"})).is_err());
    assert!(
        serde_json::from_value::<Condition>(
            json!({"operator":"exists","value":{"source":"constant","value":null},"code":"true"})
        )
        .is_err()
    );
    let schema = ObjectSchema {
        fields: BTreeMap::from([(
            "nullable".into(),
            FieldSchema {
                value_type: ValueType::Null,
                required: true,
                default: None,
            },
        )]),
    };
    assert!(schema.resolve(&BTreeMap::new(), "input").is_err());
    assert_eq!(
        schema
            .resolve(&BTreeMap::from([("nullable".into(), Value::Null)]), "input")
            .unwrap()["nullable"],
        Value::Null
    );
}

// Catches membership coercing JSON kinds, accepting a string haystack, or dropping source provenance.
#[test]
fn membership_preserves_types_and_condition_provenance() {
    for (items, needle, expected) in [
        (json!([]), json!(1), Decision::False),
        (json!([1, "1", null]), Value::Null, Decision::True),
        (json!([1]), json!("1"), Decision::False),
        (json!([1]), json!(1.0), Decision::True),
    ] {
        assert_eq!(
            run(&Condition::Contains {
                collection: literal(items),
                value: literal(needle)
            })
            .unwrap(),
            expected
        );
    }
    let origin = DataOrigin::ActivityOutput {
        activity: 5,
        field: "remote".into(),
    };
    let workflow = BTreeMap::from([(
        "rows".into(),
        Datum {
            state: DataState::Present(json!(["yes"])),
            provenance: BTreeSet::from([origin.clone()]),
        },
    )]);
    let condition = Condition::Contains {
        collection: DataSource::WorkflowInput {
            field: "rows".into(),
        },
        value: literal(json!("yes")),
    };
    let result = condition
        .evaluate(&workflow, &BTreeMap::new(), &BTreeMap::new())
        .unwrap();
    assert_eq!(result.decision, Decision::True);
    assert_eq!(
        result.provenance,
        BTreeSet::from([
            origin,
            DataOrigin::WorkflowInput {
                field: "rows".into()
            },
            DataOrigin::Constant
        ])
    );
    let invalid = BTreeMap::from([(
        "rows".into(),
        Datum {
            state: DataState::Present(json!("yes")),
            provenance: BTreeSet::new(),
        },
    )]);
    assert!(
        condition
            .evaluate(&invalid, &BTreeMap::new(), &BTreeMap::new())
            .is_err()
    );
    let missing = Condition::Contains {
        collection: literal(json!([])),
        value: DataSource::ActivityOutput {
            activity: 0,
            field: "pending".into(),
        },
    };
    assert_eq!(
        missing
            .evaluate(&BTreeMap::new(), &BTreeMap::new(), &BTreeMap::new())
            .unwrap()
            .decision,
        Decision::Unknown
    );
}

// Catches: a condition on a nested receipt value comparing the wrong field, or
// a missing nested value deciding false instead of unknown.
#[test]
fn condition_branches_on_a_projected_receipt_value() {
    let outcome = |pointer: &str| DataSource::Projection {
        from: Box::new(DataSource::ActivityOutput {
            activity: 0,
            field: "receipt".into(),
        }),
        pointer: pointer.into(),
        value_type: ValueType::String,
    };
    let is = |pointer: &str, value: &str| Condition::Compare {
        comparison: Comparison::Equal,
        left: outcome(pointer),
        right: literal(json!(value)),
    };
    let receipt = ObjectSchema {
        fields: BTreeMap::from([(
            "receipt".into(),
            FieldSchema {
                value_type: ValueType::Object,
                required: true,
                default: None,
            },
        )]),
    };
    let outputs = BTreeMap::from([(
        0,
        BTreeMap::from([(
            "receipt".into(),
            Datum {
                state: DataState::Present(
                    json!({"assessment":{"decision":{"outcome":"accepted"},"report":"rejected"}}),
                ),
                provenance: BTreeSet::new(),
            },
        )]),
    )]);
    let decide = |condition: Condition| {
        condition
            .validate(&ObjectSchema::default(), &[&receipt], &BTreeMap::new())
            .unwrap();
        condition
            .evaluate(&BTreeMap::new(), &outputs, &BTreeMap::new())
            .unwrap()
    };
    let accepted = decide(is("/assessment/decision/outcome", "accepted"));
    assert_eq!(accepted.decision, Decision::True);
    assert!(accepted.provenance.contains(&DataOrigin::ActivityOutput {
        activity: 0,
        field: "receipt".into()
    }));
    assert_eq!(
        decide(is("/assessment/decision/outcome", "rejected")).decision,
        Decision::False
    );
    assert_eq!(
        decide(is("/assessment/decision/missing", "accepted")).decision,
        Decision::Unknown
    );
    let typed = Condition::Compare {
        comparison: Comparison::Equal,
        left: outcome("/assessment/decision/outcome"),
        right: literal(json!(1)),
    };
    assert!(
        typed
            .validate(&ObjectSchema::default(), &[&receipt], &BTreeMap::new())
            .is_err()
    );
}
