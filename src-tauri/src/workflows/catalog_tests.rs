use crate::workflows::catalog::{
    ActivityDescriptor, ActivityInstance, CatalogClosure, DescriptorId, FieldSchema, HandlerRef,
    ObjectSchema, SubjectSchema, ValueType, WorkflowCatalog,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|s| (*s).to_owned()).collect()
}

fn handlers() -> BTreeSet<HandlerRef> {
    BTreeSet::from([HandlerRef {
        name: "host.inspect".into(),
        version: 1,
    }])
}

fn descriptor() -> ActivityDescriptor {
    ActivityDescriptor {
        id: DescriptorId {
            namespace: "example".into(),
            name: "inspect".into(),
            version: 1,
        },
        purpose: "Inspect an item".into(),
        target: None,
        handler: HandlerRef {
            name: "host.inspect".into(),
            version: 1,
        },
        config: ObjectSchema {
            fields: BTreeMap::from([(
                "verbose".into(),
                FieldSchema {
                    value_type: ValueType::Boolean,
                    required: true,
                    default: Some(json!(false)),
                },
            )]),
        },
        inputs: ObjectSchema {
            fields: BTreeMap::from([(
                "item".into(),
                FieldSchema {
                    value_type: ValueType::String,
                    required: true,
                    default: None,
                },
            )]),
        },
        subject: None,
        outputs: ObjectSchema::default(),
        outcomes: set(&["done", "failed"]),
        required_capabilities: set(&["repository.read"]),
        execution_policies: set(&["local"]),
    }
}

fn instance() -> ActivityInstance {
    ActivityInstance {
        descriptor: descriptor().id,
        config: BTreeMap::new(),
        execution_policy: "local".into(),
    }
}

fn inputs() -> BTreeMap<String, Value> {
    BTreeMap::from([("item".into(), json!("example"))])
}

// Catches: a duplicate version replacing an existing contract and changing its handler.
#[test]
fn duplicate_version_cannot_overwrite_existing_descriptor() {
    let mut catalog = WorkflowCatalog::new();
    let original = descriptor();
    catalog.register(original.clone()).unwrap();
    let mut changed = original.clone();
    changed.handler.name = "other.handler".into();
    assert!(catalog.register(changed).unwrap_err().contains("duplicate"));
    assert_eq!(catalog.get(&original.id), Some(&original));
    let mut missing = instance();
    missing.descriptor.version = 2;
    assert!(
        catalog
            .validate_instance(&missing, &inputs(), &set(&[]), &handlers())
            .unwrap_err()
            .contains("missing descriptor")
    );
}

// Catches: required fields ignored, wrong types accepted, or valid defaults omitted.
#[test]
fn validation_resolves_defaults_and_rejects_missing_or_wrong_values() {
    let mut catalog = WorkflowCatalog::new();
    catalog.register(descriptor()).unwrap();
    let handlers = handlers();
    let valid = catalog
        .validate_instance(&instance(), &inputs(), &set(&["done"]), &handlers)
        .unwrap();
    assert_eq!(valid.config.get("verbose"), Some(&json!(false)));
    assert_eq!(valid.inputs.get("item"), Some(&json!("example")));
    assert!(
        catalog
            .validate_instance(&instance(), &BTreeMap::new(), &set(&[]), &handlers)
            .unwrap_err()
            .contains("required field missing")
    );
    assert!(
        catalog
            .validate_instance(
                &instance(),
                &BTreeMap::from([("item".into(), json!(7))]),
                &set(&[]),
                &handlers
            )
            .unwrap_err()
            .contains("wrong type")
    );
    let mut invalid = descriptor();
    invalid.id.version = 2;
    invalid.config.fields.get_mut("verbose").unwrap().default = Some(json!("yes"));
    assert!(
        catalog
            .register(invalid)
            .unwrap_err()
            .contains("default has wrong type")
    );
}

// Catches: stale handler references and invented outcomes/policies being silently accepted.
#[test]
fn validation_refuses_unknown_outcomes_policies_and_missing_handler() {
    let mut catalog = WorkflowCatalog::new();
    catalog.register(descriptor()).unwrap();
    let handlers = handlers();
    assert!(
        catalog
            .validate_instance(&instance(), &inputs(), &set(&["invented"]), &handlers)
            .unwrap_err()
            .contains("unknown outcome")
    );
    assert!(
        catalog
            .validate_instance(&instance(), &inputs(), &set(&[]), &BTreeSet::new())
            .unwrap_err()
            .contains("handler unavailable")
    );
    let mut bad = instance();
    bad.execution_policy = "remote".into();
    assert!(
        catalog
            .validate_instance(&bad, &inputs(), &set(&[]), &handlers)
            .unwrap_err()
            .contains("unsupported execution policy")
    );
}

// Catches: a client injecting a grant into the instance or losing declared requirements.
#[test]
fn client_labels_cannot_supply_authority() {
    let forged = json!({"descriptor":{"namespace":"example","name":"inspect","version":1},"config":{},"execution_policy":"local","grants":["repository.read"]});
    assert!(serde_json::from_value::<ActivityInstance>(forged).is_err());
    let mut catalog = WorkflowCatalog::new();
    catalog.register(descriptor()).unwrap();
    let validated = catalog
        .validate_instance(&instance(), &inputs(), &set(&[]), &handlers())
        .unwrap();
    assert_eq!(
        validated.descriptor.required_capabilities,
        set(&["repository.read"])
    );
}

// Catches: same identity resolving to changed semantic content after a restart.
#[test]
fn pinned_descriptor_rejects_changed_contract_fields() {
    let original = descriptor();
    let mut source = WorkflowCatalog::new();
    source.register(original.clone()).unwrap();
    let pin = source.pin(&original.id).unwrap();
    let mut changed = Vec::new();
    let mut d = original.clone();
    d.purpose.push('!');
    changed.push(("purpose", d));
    let mut d = original.clone();
    d.handler.version = 2;
    changed.push(("handler", d));
    let mut d = original.clone();
    d.config.fields.get_mut("verbose").unwrap().default = Some(json!(true));
    changed.push(("config", d));
    let mut d = original.clone();
    d.inputs.fields.get_mut("item").unwrap().required = false;
    changed.push(("inputs", d));
    let mut d = original.clone();
    d.subject = Some(SubjectSchema {
        required: true,
        fields: ObjectSchema::default(),
    });
    changed.push(("subject", d));
    let mut d = original.clone();
    d.outputs.fields.insert(
        "result".into(),
        FieldSchema {
            value_type: ValueType::String,
            required: true,
            default: None,
        },
    );
    changed.push(("outputs", d));
    let mut d = original.clone();
    d.outcomes.insert("other".into());
    changed.push(("outcomes", d));
    let mut d = original.clone();
    d.required_capabilities.insert("repo.write".into());
    changed.push(("capabilities", d));
    let mut d = original.clone();
    d.execution_policies.insert("remote".into());
    changed.push(("policies", d));
    let failures: Vec<_> = changed
        .into_iter()
        .filter_map(|(field, d)| {
            let mut other = WorkflowCatalog::new();
            other.register(d).unwrap();
            match other.resolve_pinned(&pin, &handlers()) {
                Err(e) if e.contains("content mismatch") => None,
                other => Some(format!("{field}: {other:?}")),
            }
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join(", "));
}

// Catches: substituted handler versions and changed, malformed, or missing pin claims.
#[test]
fn serialized_pin_is_verified_against_current_catalog_and_exact_handler() {
    let mut catalog = WorkflowCatalog::new();
    catalog.register(descriptor()).unwrap();
    let pin = catalog.pin(&descriptor().id).unwrap();
    let wire = serde_json::to_string(&pin).unwrap();
    let restored = serde_json::from_str(&wire).unwrap();
    assert_eq!(
        catalog.resolve_pinned(&restored, &handlers()).unwrap(),
        &descriptor()
    );
    let wrong_handler = BTreeSet::from([HandlerRef {
        name: "host.inspect".into(),
        version: 2,
    }]);
    assert!(
        catalog
            .resolve_pinned(&restored, &wrong_handler)
            .unwrap_err()
            .contains("handler unavailable")
    );
    let mut tampered = pin.clone();
    tampered.sha256 = "0".repeat(64);
    assert!(
        catalog
            .resolve_pinned(&tampered, &handlers())
            .unwrap_err()
            .contains("content mismatch")
    );
    tampered.sha256 = "NOT-A-DIGEST".into();
    assert!(
        catalog
            .resolve_pinned(&tampered, &handlers())
            .unwrap_err()
            .contains("malformed")
    );
    tampered = pin;
    tampered.id.version = 2;
    assert!(
        catalog
            .resolve_pinned(&tampered, &handlers())
            .unwrap_err()
            .contains("missing descriptor")
    );
}

// Catches: duplicate/order-dependent pins and omission from a serialized flat set.
#[test]
fn instance_pin_set_is_sorted_deduplicated_and_checks_coverage() {
    let mut catalog = WorkflowCatalog::new();
    let first = descriptor();
    let mut second = first.clone();
    second.id.name = "another".into();
    catalog.register(first).unwrap();
    catalog.register(second.clone()).unwrap();
    let a = instance();
    let mut b = instance();
    b.descriptor = second.id;
    let one =
        CatalogClosure::from_instances(&catalog, &[a.clone(), b.clone(), a.clone()], &handlers())
            .unwrap();
    let two =
        CatalogClosure::from_instances(&catalog, &[b.clone(), a.clone()], &handlers()).unwrap();
    assert_eq!(
        serde_json::to_string(&one).unwrap(),
        serde_json::to_string(&two).unwrap()
    );
    assert_eq!(one.pins().len(), 2);
    let wire = serde_json::to_string(&one).unwrap();
    let restored: CatalogClosure = serde_json::from_str(&wire).unwrap();
    restored
        .verify_instances(&catalog, &[a.clone(), b.clone()], &handlers())
        .unwrap();
    let mut tampered: Value = serde_json::from_str(&wire).unwrap();
    tampered["pins"][0]["sha256"] = json!("0".repeat(64));
    let tampered: CatalogClosure = serde_json::from_value(tampered).unwrap();
    assert!(
        tampered
            .resolve(&catalog, &handlers())
            .unwrap_err()
            .contains("content mismatch")
    );
    let incomplete =
        CatalogClosure::from_instances(&catalog, std::slice::from_ref(&a), &handlers()).unwrap();
    assert!(
        incomplete
            .verify_instances(&catalog, &[a, b], &handlers())
            .unwrap_err()
            .contains("does not match")
    );
}

// Catches: ambiguous separators, whitespace, empty names, and version zero.
#[test]
fn invalid_descriptor_and_handler_identifiers_are_refused() {
    let mut failures = Vec::new();
    for invalid in ["", "a:b", "a@b", " a", "a/b"] {
        for field in ["namespace", "name", "handler"] {
            let mut d = descriptor();
            match field {
                "namespace" => d.id.namespace = invalid.into(),
                "name" => d.id.name = invalid.into(),
                _ => d.handler.name = invalid.into(),
            }
            if WorkflowCatalog::new().register(d).is_ok() {
                failures.push(format!("{field}={invalid:?}"));
            }
        }
    }
    let mut d = descriptor();
    d.id.version = 0;
    assert!(WorkflowCatalog::new().register(d).is_err());
    let mut d = descriptor();
    d.handler.version = 0;
    assert!(WorkflowCatalog::new().register(d).is_err());
    assert!(failures.is_empty(), "accepted: {}", failures.join(", "));
}

// Catches: nested JSON object insertion order changing a pin under preserve_order.
#[test]
fn nested_default_object_order_does_not_change_digest() {
    let mut first = descriptor();
    let mut a = serde_json::Map::new();
    a.insert("z".into(), json!({"b": 2, "a": 1}));
    a.insert("a".into(), json!(true));
    first.config.fields.insert(
        "options".into(),
        FieldSchema {
            value_type: ValueType::Object,
            required: false,
            default: Some(Value::Object(a)),
        },
    );
    let mut second = first.clone();
    let mut b = serde_json::Map::new();
    b.insert("a".into(), json!(true));
    b.insert("z".into(), json!({"a": 1, "b": 2}));
    second.config.fields.get_mut("options").unwrap().default = Some(Value::Object(b));
    let mut c1 = WorkflowCatalog::new();
    let mut c2 = WorkflowCatalog::new();
    c1.register(first.clone()).unwrap();
    c2.register(second).unwrap();
    assert_eq!(c1.pin(&first.id).unwrap(), c2.pin(&first.id).unwrap());
}

// Catches a null default changing to no default on a persisted descriptor roundtrip.
#[test]
fn explicit_null_fields_do_not_allow_ambiguous_null_defaults() {
    let mut descriptor = descriptor();
    descriptor.inputs.fields.insert(
        "null_value".into(),
        FieldSchema {
            value_type: ValueType::Null,
            required: true,
            default: Some(Value::Null),
        },
    );
    let mut catalog = WorkflowCatalog::new();
    assert!(
        catalog
            .register(descriptor.clone())
            .unwrap_err()
            .contains("null defaults are unsupported")
    );
    descriptor
        .inputs
        .fields
        .get_mut("null_value")
        .unwrap()
        .default = None;
    catalog.register(descriptor).unwrap();
    let mut supplied = inputs();
    supplied.insert("null_value".into(), Value::Null);
    assert_eq!(
        catalog
            .validate_instance(&instance(), &supplied, &set(&[]), &handlers())
            .unwrap()
            .inputs["null_value"],
        Value::Null
    );
    assert!(
        catalog
            .validate_instance(&instance(), &inputs(), &set(&[]), &handlers())
            .unwrap_err()
            .contains("required field missing")
    );
}
