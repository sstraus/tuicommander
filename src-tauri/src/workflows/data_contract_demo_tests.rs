use super::WorkflowStore;
use super::catalog::{
    ActivityDescriptor, ActivityInstance, DescriptorId, FieldSchema, HandlerRef, ObjectSchema,
    ReusableTarget, ValueType, WorkflowCatalog, content_digest,
};
use super::collection::{
    AggregateState, CollectionIdentity, EmptyPolicy, FailurePolicy, ForEach, ItemResult, ItemState,
};
use super::condition::{Comparison, Condition, Decision};
use super::data_binding::{DataOrigin, DataSource, DataState, Datum};
use super::neutral_publication::{NeutralDefinition, PublicationPin, PublicationRef};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn schema(fields: &[(&str, ValueType)]) -> ObjectSchema {
    ObjectSchema {
        fields: fields
            .iter()
            .map(|(name, kind)| {
                (
                    name.to_string(),
                    FieldSchema {
                        value_type: kind.clone(),
                        required: true,
                        default: None,
                    },
                )
            })
            .collect(),
    }
}
fn definition(inputs: ObjectSchema, outputs: ObjectSchema) -> NeutralDefinition {
    NeutralDefinition {
        control: crate::workflows::neutral_control::ControlGraph {
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
        },
        inputs,
        outputs,
        outcomes: BTreeSet::from(["done".into()]),
        subject: None,
        instances: vec![],
        input_bindings: BTreeMap::new(),
        conditions: BTreeMap::new(),
        collections: BTreeMap::new(),
        loops: BTreeMap::new(),
        forks: BTreeMap::new(),
        output_bindings: BTreeMap::new(),
        subflows: vec![],
    }
}

// Catches: API-shaped data becoming agent-only fanout, subflow projection losing provenance,
// completion-order aggregation, or a pending aggregate selecting a final condition.
// This composes actual publication/data functions, not an HTTP adapter or run scheduler.
#[test]
fn documented_list_output_feeds_bounded_subflow_aggregate_and_condition() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/workflow_contracts/github-labels-documented-projection.json"
    ))
    .unwrap();
    let items = fixture["items"].clone();
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let mut catalog = WorkflowCatalog::new();
    let mut child = definition(
        schema(&[("item", ValueType::Object), ("index", ValueType::Integer)]),
        schema(&[("item", ValueType::Object)]),
    );
    child.outputs.fields.insert(
        "note".into(),
        FieldSchema {
            value_type: ValueType::String,
            required: false,
            default: None,
        },
    );
    child.output_bindings.insert(
        "item".into(),
        DataSource::WorkflowInput {
            field: "item".into(),
        },
    );
    let child = store
        .save_neutral_contract_fixture(
            "/project",
            "identity-item",
            0,
            child,
            &catalog,
            &BTreeSet::new(),
        )
        .unwrap();
    let api = ActivityDescriptor {
        id: DescriptorId {
            namespace: "contract".into(),
            name: "list-response".into(),
            version: 1,
        },
        purpose: "Declare a documented list-response shape; not a live adapter".into(),
        handler: HandlerRef {
            name: "contract.list".into(),
            version: 1,
        },
        target: None,
        config: ObjectSchema::default(),
        inputs: ObjectSchema::default(),
        outputs: schema(&[("items", ValueType::Array)]),
        subject: None,
        outcomes: BTreeSet::from(["done".into()]),
        required_capabilities: BTreeSet::new(),
        execution_policies: BTreeSet::from(["local".into()]),
    };
    let subflow = ActivityDescriptor {
        id: DescriptorId {
            namespace: "contract".into(),
            name: "identity-item".into(),
            version: 1,
        },
        purpose: "Return a structured item with provenance".into(),
        handler: HandlerRef {
            name: "contract.subflow".into(),
            version: 1,
        },
        target: Some(ReusableTarget::Subflow {
            publication: PublicationPin {
                reference: PublicationRef {
                    project: "/project".into(),
                    id: "identity-item".into(),
                    revision: 1,
                },
                sha256: content_digest(&child).unwrap(),
            },
        }),
        inputs: child.definition.inputs.clone(),
        outputs: child.definition.outputs.clone(),
        ..api.clone()
    };
    let handlers = BTreeSet::from([api.handler.clone(), subflow.handler.clone()]);
    catalog.register(api.clone()).unwrap();
    catalog.register(subflow.clone()).unwrap();
    let mut root = definition(
        ObjectSchema::default(),
        schema(&[("total", ValueType::Integer), ("items", ValueType::Array)]),
    );
    for field in ["total", "items"] {
        root.output_bindings.insert(
            field.into(),
            DataSource::ScopeOutput {
                scope: "labels".into(),
                field: field.into(),
            },
        );
    }
    root.conditions.insert(
        "all-processed".into(),
        Condition::Compare {
            comparison: Comparison::Equal,
            left: DataSource::ScopeOutput {
                scope: "labels".into(),
                field: "completed".into(),
            },
            right: DataSource::Constant { value: json!(2) },
        },
    );
    root.instances = [&api, &subflow]
        .into_iter()
        .map(|descriptor| ActivityInstance {
            descriptor: descriptor.id.clone(),
            config: BTreeMap::new(),
            execution_policy: "local".into(),
        })
        .collect();
    root.collections.insert(
        "labels".into(),
        ForEach {
            collection: DataSource::ActivityOutput {
                activity: 0,
                field: "items".into(),
            },
            item_type: ValueType::Object,
            body: 1,
            item_input: "item".into(),
            index_input: "index".into(),
            max_items: 2,
            max_concurrency: 1,
            empty: EmptyPolicy::Complete,
            failure: FailurePolicy::Blocked,
        },
    );
    let root = store
        .save_neutral_contract_fixture("/project", "list-data", 0, root, &catalog, &handlers)
        .unwrap();
    assert_eq!(root.subflow_pins.len(), 1);
    let collection = &root.definition.collections["labels"];
    let api_origin = DataOrigin::ActivityOutput {
        activity: 0,
        field: "items".into(),
    };
    let api_values = catalog
        .validate_output(
            &catalog.pin(&api.id).unwrap(),
            &handlers,
            "done",
            &BTreeMap::from([("items".into(), items)]),
        )
        .unwrap();
    let outputs = BTreeMap::from([(
        0,
        api_values
            .into_iter()
            .map(|(name, value)| {
                (
                    name,
                    Datum {
                        state: DataState::Present(value),
                        provenance: BTreeSet::from([api_origin.clone()]),
                    },
                )
            })
            .collect(),
    )]);
    let identity = CollectionIdentity {
        run: "contract-run".into(),
        scope: "labels".into(),
        generation: 1,
    };
    let expanded = collection
        .expand(
            identity.clone(),
            &BTreeMap::new(),
            &outputs,
            &BTreeMap::new(),
        )
        .unwrap();
    let mut results = Vec::new();
    for item in expanded.items().iter().rev() {
        let returned = child
            .definition
            .resolve_outputs(&item.inputs, &BTreeMap::new(), &BTreeMap::new())
            .unwrap();
        catalog
            .validate_output(
                &catalog.pin(&subflow.id).unwrap(),
                &handlers,
                "done",
                &returned.values,
            )
            .unwrap();
        let fields = returned
            .values
            .into_iter()
            .map(|(name, value)| {
                let provenance = returned.provenance[&name].clone();
                (
                    name,
                    Datum {
                        state: DataState::Present(value),
                        provenance,
                    },
                )
            })
            .collect();
        results.push(ItemResult {
            identity: item.identity.clone(),
            state: ItemState::Completed {
                outcome: "done".into(),
                outputs: fields,
            },
        });
    }
    let pending = collection
        .aggregate(
            &expanded,
            &results[..1],
            &subflow.outputs,
            &subflow.outcomes,
        )
        .unwrap();
    assert_eq!(pending.state, AggregateState::Pending);
    assert!(pending.summary().unwrap_err().contains("pending"));
    let aggregate = collection
        .aggregate(&expanded, &results, &subflow.outputs, &subflow.outcomes)
        .unwrap();
    for (index, expected) in ["bug", "enhancement"].into_iter().enumerate() {
        let ItemState::Completed { outputs, .. } = &aggregate.items[index].state else {
            panic!("expected a completed item")
        };
        let DataState::Present(value) = &outputs["item"].state else {
            panic!("expected item data")
        };
        assert_eq!(value["name"], json!(expected));
        assert!(outputs["item"].provenance.contains(&api_origin));
    }
    let summary = aggregate.summary().unwrap();
    assert_eq!(summary.values["total"], json!(2));
    assert_eq!(summary.values["failed"], json!(0));
    assert!(summary.provenance["completed"].contains(&DataOrigin::ScopeResult { identity }));
    let condition = &root.definition.conditions["all-processed"];
    assert!(
        super::scope_data::collection_snapshot(&pending)
            .unwrap_err()
            .contains("pending")
    );
    assert_eq!(
        condition
            .evaluate(&BTreeMap::new(), &BTreeMap::new(), &BTreeMap::new())
            .unwrap()
            .decision,
        Decision::Unknown
    );
    assert!(
        root.definition
            .resolve_outputs(&BTreeMap::new(), &BTreeMap::new(), &BTreeMap::new())
            .unwrap_err()
            .contains("unknown")
    );
    let scopes = BTreeMap::from([(
        "labels".into(),
        super::scope_data::collection_snapshot(&aggregate).unwrap(),
    )]);
    let evaluated = condition
        .evaluate(&BTreeMap::new(), &BTreeMap::new(), &scopes)
        .unwrap();
    assert_eq!(evaluated.decision, Decision::True);
    assert!(evaluated.provenance.contains(&api_origin));
    let returned = root
        .definition
        .resolve_outputs(&BTreeMap::new(), &BTreeMap::new(), &scopes)
        .unwrap();
    assert_eq!(returned.values["total"], json!(2));
    assert_eq!(
        returned.values["items"][0]["outputs"]["item"]["name"],
        json!("bug")
    );
    assert_eq!(
        returned.values["items"][1]["outputs"]["item"]["name"],
        json!("enhancement")
    );
    for item in returned.values["items"].as_array().unwrap() {
        assert_eq!(item["state"], json!("completed"));
        assert_eq!(item["outcome"], json!("done"));
        assert_eq!(item["absent"], json!(["note"]));
        assert!(item["outputs"].get("note").is_none());
    }
    assert!(returned.provenance["items"].contains(&api_origin));
    assert!(
        returned.provenance["total"].contains(&DataOrigin::ScopeOutput {
            scope: "labels".into(),
            field: "total".into()
        })
    );
}
