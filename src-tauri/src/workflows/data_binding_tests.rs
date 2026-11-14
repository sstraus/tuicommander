use super::catalog::{FieldSchema, ObjectSchema, ValueType};
use super::data_binding::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn schema(name: &str, kind: ValueType, required: bool, default: Option<Value>) -> ObjectSchema {
    ObjectSchema {
        fields: BTreeMap::from([(
            name.into(),
            FieldSchema {
                value_type: kind,
                required,
                default,
            },
        )]),
    }
}

fn datum(value: Value) -> Datum {
    Datum {
        state: DataState::Present(value),
        provenance: BTreeSet::new(),
    }
}

// Catches wrong-field, missing producer, self dependency, incompatible type and optional-to-required mappings.
#[test]
fn publication_mapping_checks_source_and_target_contracts() {
    let input = schema("query", ValueType::String, true, None);
    let output = schema("count", ValueType::Integer, true, None);
    let target = schema("total", ValueType::Number, true, None);
    let valid = BTreeMap::from([(
        "total".into(),
        DataSource::ActivityOutput {
            activity: 0,
            field: "count".into(),
        },
    )]);
    validate_bindings(&valid, &input, &[&output], &target, 1, &BTreeMap::new()).unwrap();
    let cases = [
        (
            DataSource::WorkflowInput {
                field: "missing".into(),
            },
            "unknown workflow",
        ),
        (
            DataSource::WorkflowInput {
                field: "query".into(),
            },
            "incompatible field",
        ),
        (
            DataSource::ActivityOutput {
                activity: 3,
                field: "count".into(),
            },
            "unknown activity",
        ),
        (
            DataSource::ActivityOutput {
                activity: 1,
                field: "count".into(),
            },
            "self dependency",
        ),
        (
            DataSource::Constant { value: json!("5") },
            "incompatible constant",
        ),
        (
            DataSource::Constant { value: Value::Null },
            "incompatible constant",
        ),
    ];
    for (source, expected) in cases {
        let error = validate_bindings(
            &BTreeMap::from([("total".into(), source.clone())]),
            &input,
            &[&output],
            &target,
            1,
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(error.contains(expected), "{source:?}: {error}");
    }
    let optional = schema("count", ValueType::Integer, false, None);
    assert!(
        validate_bindings(&valid, &input, &[&optional], &target, 1, &BTreeMap::new())
            .unwrap_err()
            .contains("not guaranteed")
    );
    assert!(
        validate_bindings(
            &BTreeMap::new(),
            &input,
            &[&output],
            &target,
            1,
            &BTreeMap::new()
        )
        .unwrap_err()
        .contains("unbound")
    );
}

// Catches pending outputs silently replaced by defaults, and null/empty/absent conflation.
#[test]
fn unknown_absent_null_and_empty_have_distinct_resolution() {
    let target = schema("items", ValueType::Array, false, Some(json!(["default"])));
    let bindings = BTreeMap::from([(
        "items".into(),
        DataSource::ActivityOutput {
            activity: 0,
            field: "rows".into(),
        },
    )]);
    assert!(
        resolve_bindings(
            &bindings,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &target,
            &BTreeMap::new()
        )
        .unwrap_err()
        .contains("unknown")
    );
    let absent = BTreeMap::from([(0, BTreeMap::new())]);
    let resolved = resolve_bindings(
        &bindings,
        &BTreeMap::new(),
        &absent,
        &target,
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(resolved.values["items"], json!(["default"]));
    assert_eq!(
        resolved.provenance["items"],
        BTreeSet::from([DataOrigin::Default])
    );
    for (value, expected) in [(json!([]), true), (Value::Null, false)] {
        let outputs =
            BTreeMap::from([(0, BTreeMap::from([("rows".into(), datum(value.clone()))]))]);
        let result = resolve_bindings(
            &bindings,
            &BTreeMap::new(),
            &outputs,
            &target,
            &BTreeMap::new(),
        );
        if expected {
            assert_eq!(result.unwrap().values["items"], json!([]));
        } else {
            assert!(result.unwrap_err().contains("wrong type"));
        }
    }
    let unknown = BTreeMap::from([(
        0,
        BTreeMap::from([(
            "rows".into(),
            Datum {
                state: DataState::Unknown,
                provenance: BTreeSet::new(),
            },
        )]),
    )]);
    assert!(
        resolve_bindings(
            &bindings,
            &BTreeMap::new(),
            &unknown,
            &target,
            &BTreeMap::new()
        )
        .is_err()
    );
}

// Catches projections dropping remote/model provenance or reading a completion-order slot instead of the pinned instance.
#[test]
fn projections_preserve_provenance_and_exact_producer_identity() {
    let original = DataOrigin::ActivityOutput {
        activity: 7,
        field: "remote".into(),
    };
    let source = Datum {
        state: DataState::Present(json!("payload")),
        provenance: BTreeSet::from([original.clone()]),
    };
    let outputs = BTreeMap::from([
        (2, BTreeMap::from([("text".into(), source)])),
        (
            0,
            BTreeMap::from([("text".into(), datum(json!("wrong producer")))]),
        ),
    ]);
    let bindings = BTreeMap::from([(
        "message".into(),
        DataSource::ActivityOutput {
            activity: 2,
            field: "text".into(),
        },
    )]);
    let result = resolve_bindings(
        &bindings,
        &BTreeMap::new(),
        &outputs,
        &schema("message", ValueType::String, true, None),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(result.values["message"], json!("payload"));
    assert_eq!(
        result.provenance["message"],
        BTreeSet::from([
            original,
            DataOrigin::ActivityOutput {
                activity: 2,
                field: "text".into()
            }
        ])
    );
    let workflow = BTreeMap::from([(
        "body".into(),
        Datum {
            state: DataState::Present(json!("payload")),
            provenance: result.provenance["message"].clone(),
        },
    )]);
    let bindings = BTreeMap::from([(
        "message".into(),
        DataSource::WorkflowInput {
            field: "body".into(),
        },
    )]);
    let child = resolve_bindings(
        &bindings,
        &workflow,
        &BTreeMap::new(),
        &schema("message", ValueType::String, true, None),
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(child.provenance["message"].is_superset(&result.provenance["message"]));
    assert!(
        child.provenance["message"].contains(&DataOrigin::WorkflowInput {
            field: "body".into()
        })
    );
}

fn receipt(activity: usize, outcome: &str) -> (usize, BTreeMap<String, Datum>) {
    (
        activity,
        BTreeMap::from([(
            "receipt".into(),
            datum(
                json!({"assessment":{"decision":{"outcome":outcome},"report":"other"},"source":{"number":activity}}),
            ),
        )]),
    )
}

fn project(activity: usize, pointer: &str, value_type: ValueType) -> DataSource {
    DataSource::Projection {
        from: Box::new(DataSource::ActivityOutput {
            activity,
            field: "receipt".into(),
        }),
        pointer: pointer.into(),
        value_type,
    }
}

// Catches: a projection reading another activity's field, a wrong-typed or
// malformed projection passing publication, a self dependency hidden inside a
// projection, and a missing path resolving to a value instead of absence.
#[test]
fn nested_projection_reads_its_own_producer_and_declared_type() {
    let receipt_schema = schema("receipt", ValueType::Object, true, None);
    let text = schema("outcome", ValueType::String, true, None);
    let outputs = [&receipt_schema, &receipt_schema];
    let bind = |source| BTreeMap::from([("outcome".to_string(), source)]);
    let ok = bind(project(
        1,
        "/assessment/decision/outcome",
        ValueType::String,
    ));
    validate_bindings(
        &ok,
        &ObjectSchema::default(),
        &outputs,
        &text,
        2,
        &BTreeMap::new(),
    )
    .unwrap();
    let refused = [
        (
            project(1, "/assessment/decision/outcome", ValueType::Integer),
            "incompatible",
        ),
        (project(1, "", ValueType::String), "JSON pointer"),
        (project(1, "assessment", ValueType::String), "JSON pointer"),
        (
            DataSource::Projection {
                from: Box::new(DataSource::Constant {
                    value: json!({"a": "b"}),
                }),
                pointer: "/a".into(),
                value_type: ValueType::String,
            },
            "input or output field",
        ),
    ];
    for (source, error) in refused {
        let err = validate_bindings(
            &bind(source),
            &ObjectSchema::default(),
            &outputs,
            &text,
            2,
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(err.contains(error), "{err}");
    }
    let scalar = schema("receipt", ValueType::String, true, None);
    let err = validate_bindings(
        &ok,
        &ObjectSchema::default(),
        &[&scalar, &scalar],
        &text,
        2,
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(err.contains("object or array"), "{err}");
    let err = validate_bindings(
        &ok,
        &ObjectSchema::default(),
        &outputs,
        &text,
        1,
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(err.contains("self dependency"), "{err}");

    let values = BTreeMap::from([receipt(0, "rejected"), receipt(1, "accepted")]);
    let result = resolve_bindings(&ok, &BTreeMap::new(), &values, &text, &BTreeMap::new()).unwrap();
    assert_eq!(result.values["outcome"], json!("accepted"));
    let missing = project(1, "/assessment/missing", ValueType::String);
    assert_eq!(
        missing
            .resolve(&BTreeMap::new(), &values, &BTreeMap::new())
            .state,
        DataState::Absent
    );
    let err = resolve_bindings(
        &bind(missing),
        &BTreeMap::new(),
        &values,
        &text,
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(err.contains("outcome"), "{err}");
    // Present with another type is unknown, never coerced.
    let wrong = project(1, "/source/number", ValueType::String);
    assert_eq!(
        wrong
            .resolve(&BTreeMap::new(), &values, &BTreeMap::new())
            .state,
        DataState::Unknown
    );
}

// Catches: a constructed object dropping a member origin, admitting an unknown
// member, or reading the wrong producer for one of its members.
#[test]
fn constructed_object_keeps_member_values_and_every_origin() {
    let request = DataSource::Object {
        fields: BTreeMap::from([
            ("source".into(), project(1, "/source", ValueType::Object)),
            (
                "profile".into(),
                DataSource::WorkflowInput {
                    field: "profile".into(),
                },
            ),
            (
                "policy".into(),
                DataSource::Constant {
                    value: json!({"label": "ok"}),
                },
            ),
        ]),
    };
    let target = schema("request", ValueType::Object, true, None);
    let receipt_schema = schema("receipt", ValueType::Object, true, None);
    let inputs = schema("profile", ValueType::String, true, None);
    let bindings = BTreeMap::from([("request".to_string(), request.clone())]);
    validate_bindings(
        &bindings,
        &inputs,
        &[&receipt_schema, &receipt_schema],
        &target,
        2,
        &BTreeMap::new(),
    )
    .unwrap();
    let workflow = BTreeMap::from([("profile".into(), datum(json!("triage")))]);
    let values = BTreeMap::from([receipt(0, "rejected"), receipt(1, "accepted")]);
    let result =
        resolve_bindings(&bindings, &workflow, &values, &target, &BTreeMap::new()).unwrap();
    assert_eq!(
        result.values["request"],
        json!({"source":{"number":1},"profile":"triage","policy":{"label":"ok"}})
    );
    assert_eq!(
        result.provenance["request"],
        BTreeSet::from([
            DataOrigin::ActivityOutput {
                activity: 1,
                field: "receipt".into()
            },
            DataOrigin::WorkflowInput {
                field: "profile".into()
            },
            DataOrigin::Constant,
        ])
    );
    let pending = BTreeMap::from([receipt(0, "rejected")]);
    let err =
        resolve_bindings(&bindings, &workflow, &pending, &target, &BTreeMap::new()).unwrap_err();
    assert!(err.contains("unknown"), "{err}");
}
