use super::catalog::{
    ActivityDescriptor, ActivityInstance, DescriptorId, FieldSchema, HandlerRef, ObjectSchema,
    ReusableTarget, ValueType, WorkflowCatalog,
};
use super::data_binding::{
    DataOrigin, DataSource, DataState, Datum, resolve_bindings, validate_bindings,
};
use super::data_reference::{DataReference, ReferenceInputs};
use super::neutral_admission::{HostAdmission, HostContext, HostRegistry, ReusableHandlerKind};
use super::neutral_publication::{NeutralPublication, PublicationPin, PublicationRef};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

fn schema(kind: ValueType) -> ObjectSchema {
    ObjectSchema {
        fields: BTreeMap::from([(
            "resource".into(),
            FieldSchema {
                value_type: kind,
                required: true,
                default: None,
            },
        )]),
    }
}

// Catches: wrong-kind, unversioned or secret-bearing objects passing typed reference validation.
#[test]
fn typed_references_refuse_malformed_or_wrong_kind_values() {
    for (kind, valid) in [
        (
            ValueType::WorkspaceReference,
            json!({"reference":"workspace","id":"workspace-1"}),
        ),
        (
            ValueType::ArtifactReference,
            json!({"reference":"artifact","id":"artifact-1","revision":"revision-2"}),
        ),
        (
            ValueType::CredentialReference,
            json!({"reference":"credential","id":"credential-1"}),
        ),
    ] {
        let schema = schema(kind.clone());
        schema
            .resolve(
                &BTreeMap::from([("resource".into(), valid.clone())]),
                "inputs",
            )
            .unwrap();
        let bindings = BTreeMap::from([(
            "resource".into(),
            DataSource::Constant {
                value: valid.clone(),
            },
        )]);
        validate_bindings(
            &bindings,
            &ObjectSchema::default(),
            &[],
            &schema,
            0,
            &BTreeMap::new(),
        )
        .unwrap();
        let mut extra = valid;
        extra["secret"] = json!("not-a-reference");
        assert!(
            schema
                .resolve(&BTreeMap::from([("resource".into(), extra)]), "inputs")
                .is_err()
        );
        for invalid in [
            json!({}),
            json!({"reference":"workspace","id":""}),
            json!({"reference":"artifact","id":"a"}),
            json!({"reference":"artifact","id":"a","revision":""}),
            json!("credential-value"),
        ] {
            assert!(
                schema
                    .resolve(&BTreeMap::from([("resource".into(), invalid)]), "inputs")
                    .is_err()
            );
        }
        let other_kind = if kind == ValueType::WorkspaceReference {
            ValueType::ArtifactReference
        } else {
            ValueType::WorkspaceReference
        };
        assert!(
            validate_bindings(
                &bindings,
                &ObjectSchema::default(),
                &[],
                &self::schema(other_kind),
                0,
                &BTreeMap::new()
            )
            .is_err()
        );
    }
}

// Catches: projection erasing a reference's exact identity/revision/provenance or granting meaning to an opaque object.
#[test]
fn reference_projections_keep_identity_and_provenance() {
    let typed = schema(ValueType::ArtifactReference);
    let value = json!({"reference":"artifact","id":"artifact-1","revision":"r2"});
    let origin = DataOrigin::ActivityOutput {
        activity: 1,
        field: "artifact".into(),
    };
    let workflow = BTreeMap::from([(
        "resource".into(),
        Datum {
            state: DataState::Present(value.clone()),
            provenance: BTreeSet::from([origin.clone()]),
        },
    )]);
    let bindings = BTreeMap::from([(
        "resource".into(),
        DataSource::WorkflowInput {
            field: "resource".into(),
        },
    )]);
    let projected = resolve_bindings(
        &bindings,
        &workflow,
        &BTreeMap::new(),
        &typed,
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(projected.values["resource"], value);
    assert!(projected.provenance["resource"].contains(&origin));
    let references = ReferenceInputs::resolve(&typed, &projected.values).unwrap();
    assert_eq!(
        references.fields["resource"],
        DataReference::Artifact {
            id: "artifact-1".into(),
            revision: "r2".into()
        }
    );
    assert!(
        ReferenceInputs::resolve(&schema(ValueType::Object), &projected.values)
            .unwrap()
            .fields
            .is_empty()
    );
}

struct Port;
impl HostAdmission for Port {
    fn publication(
        &self,
        _: &dyn HostContext,
        _: &NeutralPublication,
        _: &ActivityDescriptor,
        _: &ActivityInstance,
    ) -> Result<(), String> {
        Ok(())
    }
    fn dispatch(
        &self,
        _: &dyn HostContext,
        _: &NeutralPublication,
        _: &ActivityDescriptor,
        _: &ActivityInstance,
        _: &BTreeMap<String, Value>,
        _: Option<&BTreeMap<String, Value>>,
    ) -> Result<(), String> {
        Err("not an executable adapter".into())
    }
}
struct NoDerivation;
impl HostContext for NoDerivation {}
struct RecordingContext(Arc<Mutex<Option<ReferenceInputs>>>);
impl HostContext for RecordingContext {
    fn for_subflow(&self, references: &ReferenceInputs) -> Result<Box<dyn HostContext>, String> {
        *self.0.lock().unwrap() = Some(references.clone());
        // A contract fake only: no real native rights or freshness claim.
        Err("host refused inherited scope".into())
    }
}

// Catches: a missing child-context implementation silently inheriting unrestricted parent authority.
#[test]
fn subflow_context_derivation_is_host_only_typed_and_fail_closed() {
    let handler = HandlerRef {
        name: "host.subflow".into(),
        version: 1,
    };
    let mut registry = HostRegistry::default();
    registry
        .register_reusable_handler(handler.clone(), ReusableHandlerKind::Subflow, &Port)
        .unwrap();
    let descriptor = ActivityDescriptor {
        id: DescriptorId {
            namespace: "test".into(),
            name: "subflow".into(),
            version: 1,
        },
        purpose: "Call pinned subflow".into(),
        handler,
        target: Some(ReusableTarget::Subflow {
            publication: PublicationPin {
                reference: PublicationRef {
                    project: "/project".into(),
                    id: "child".into(),
                    revision: 1,
                },
                sha256: "a".repeat(64),
            },
        }),
        config: ObjectSchema::default(),
        inputs: schema(ValueType::WorkspaceReference),
        outputs: ObjectSchema::default(),
        subject: None,
        outcomes: BTreeSet::from(["done".into()]),
        required_capabilities: BTreeSet::new(),
        execution_policies: BTreeSet::from(["local".into()]),
    };
    let inputs = BTreeMap::from([(
        "resource".into(),
        json!({"reference":"workspace","id":"workspace-1"}),
    )]);
    let result = registry.subflow_context(&NoDerivation, &descriptor, &inputs);
    assert!(matches!(result, Err(error) if error.contains("unavailable")));
    let seen = Arc::new(Mutex::new(None));
    let parent = RecordingContext(seen.clone());
    assert!(
        matches!(registry.subflow_context(&parent, &descriptor, &inputs), Err(error) if error == "host refused inherited scope")
    );
    assert_eq!(
        seen.lock().unwrap().as_ref().unwrap().fields["resource"],
        DataReference::Workspace {
            id: "workspace-1".into()
        }
    );
    *seen.lock().unwrap() = None;
    let wrong = BTreeMap::from([(
        "resource".into(),
        json!({"reference":"credential","id":"credential-1"}),
    )]);
    assert!(
        registry
            .subflow_context(&parent, &descriptor, &wrong)
            .is_err()
    );
    assert!(seen.lock().unwrap().is_none());
    let mut native = descriptor;
    native.target = None;
    assert!(registry.subflow_context(&parent, &native, &inputs).is_err());
}

// Catches: descriptor defaults bypassing the same reference schema used for resolved inputs.
#[test]
fn catalog_rejects_reference_defaults_with_embedded_material() {
    let mut catalog = WorkflowCatalog::new();
    let mut descriptor = ActivityDescriptor {
        id: DescriptorId {
            namespace: "test".into(),
            name: "credential".into(),
            version: 1,
        },
        purpose: "Use credential reference".into(),
        handler: HandlerRef {
            name: "host.connector".into(),
            version: 1,
        },
        target: None,
        config: schema(ValueType::CredentialReference),
        inputs: ObjectSchema::default(),
        outputs: ObjectSchema::default(),
        subject: None,
        outcomes: BTreeSet::from(["done".into()]),
        required_capabilities: BTreeSet::new(),
        execution_policies: BTreeSet::from(["local".into()]),
    };
    descriptor
        .config
        .fields
        .get_mut("resource")
        .unwrap()
        .default =
        Some(json!({"reference":"credential","id":"credential-1","secret":"not-a-reference"}));
    assert!(
        catalog
            .register(descriptor.clone())
            .unwrap_err()
            .contains("wrong type")
    );
    descriptor
        .config
        .fields
        .get_mut("resource")
        .unwrap()
        .default = Some(json!({"reference":"credential","id":"credential-1"}));
    catalog.register(descriptor).unwrap();
}

// Catches: artifact equality dropping the pinned revision or confusing reference kinds.
#[test]
fn reference_conditions_compare_complete_identity_and_revision() {
    use super::condition::{Comparison, Condition, Decision};
    let typed = schema(ValueType::ArtifactReference);
    let first = json!({"reference":"artifact","id":"artifact-1","revision":"r1"});
    let second = json!({"reference":"artifact","id":"artifact-1","revision":"r2"});
    let condition = Condition::Compare {
        comparison: Comparison::Equal,
        left: DataSource::WorkflowInput {
            field: "resource".into(),
        },
        right: DataSource::Constant { value: second },
    };
    condition.validate(&typed, &[], &BTreeMap::new()).unwrap();
    let values = BTreeMap::from([(
        "resource".into(),
        Datum {
            state: DataState::Present(first),
            provenance: BTreeSet::new(),
        },
    )]);
    assert_eq!(
        condition
            .evaluate(&values, &BTreeMap::new(), &BTreeMap::new())
            .unwrap()
            .decision,
        Decision::False
    );
    assert!(
        condition
            .validate(
                &schema(ValueType::WorkspaceReference),
                &[],
                &BTreeMap::new()
            )
            .is_err()
    );
}

// Catches: typed reference schemas or return bindings being lost at the real publication boundary.
#[test]
fn typed_reference_publications_roundtrip_and_validate_defaults() {
    use super::WorkflowStore;
    use super::neutral_publication::NeutralDefinition;
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = WorkflowStore::open_at(&dir.path().join("workflows.sqlite3")).unwrap();
    let mut definition = NeutralDefinition {
        control: crate::workflows::neutral_control::ControlGraph {
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
        },
        inputs: schema(ValueType::ArtifactReference),
        outputs: schema(ValueType::ArtifactReference),
        outcomes: BTreeSet::from(["done".into()]),
        subject: None,
        instances: vec![],
        input_bindings: BTreeMap::new(),
        conditions: BTreeMap::new(),
        collections: BTreeMap::new(),
        loops: BTreeMap::new(),
        forks: BTreeMap::new(),
        output_bindings: BTreeMap::from([(
            "resource".into(),
            DataSource::WorkflowInput {
                field: "resource".into(),
            },
        )]),
        subflows: vec![],
    };
    let catalog = WorkflowCatalog::new();
    let handlers = BTreeSet::new();
    let published = store
        .save_neutral_contract_fixture(
            "/project",
            "references",
            0,
            definition.clone(),
            &catalog,
            &handlers,
        )
        .unwrap();
    assert_eq!(
        store
            .get_neutral_publication("/project", "references", 1, &catalog, &handlers)
            .unwrap(),
        published
    );
    definition
        .inputs
        .fields
        .get_mut("resource")
        .unwrap()
        .default = Some(json!({"reference":"artifact","id":"artifact-1"}));
    assert!(
        store
            .save_neutral_contract_fixture(
                "/project",
                "bad-reference",
                0,
                definition,
                &catalog,
                &handlers
            )
            .unwrap_err()
            .contains("wrong type")
    );
}
