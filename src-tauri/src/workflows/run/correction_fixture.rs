//! Internal kernel fixture; no external provider responses are fabricated.
use super::neutral_execution::NeutralExecution;
use super::neutral_execution_tests::fixture_with;
use super::runtime::RuntimeOwner;
use crate::workflows::WorkflowStore;
use crate::workflows::catalog::{
    ActivityDescriptor, ActivityInstance, HandlerRef, ReusableTarget, WorkflowCatalog,
    content_digest,
};
use crate::workflows::correction::CorrectionState;
use crate::workflows::correction_region::{
    CorrectionRegion, correction_input_schema, correction_state_schema,
};
use crate::workflows::correction_tests::artifact;
use crate::workflows::data_binding::DataSource;
use crate::workflows::neutral_control::{ControlGraph, ControlNode};
use crate::workflows::neutral_publication::{NeutralDefinition, PublicationPin, PublicationRef};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn call(
    store: &WorkflowStore,
    catalog: &mut WorkflowCatalog,
    mut descriptor: ActivityDescriptor,
    name: &str,
    definition: NeutralDefinition,
) -> ActivityInstance {
    let handlers = BTreeSet::from([
        descriptor.handler.clone(),
        HandlerRef {
            name: "host.subflow".into(),
            version: 1,
        },
    ]);
    let publication = store
        .save_neutral_publication("/project", name, 0, definition, catalog, &handlers)
        .unwrap();
    descriptor.id.name = name.into();
    descriptor.handler = HandlerRef {
        name: "host.subflow".into(),
        version: 1,
    };
    descriptor.target = Some(ReusableTarget::Subflow {
        publication: PublicationPin {
            reference: PublicationRef {
                project: "/project".into(),
                id: name.into(),
                revision: 1,
            },
            sha256: content_digest(&publication).unwrap(),
        },
    });
    descriptor.inputs = publication.definition.inputs;
    descriptor.outputs = publication.definition.outputs;
    descriptor.outcomes = publication.definition.outcomes;
    let instance = ActivityInstance {
        descriptor: descriptor.id.clone(),
        config: BTreeMap::new(),
        execution_policy: "local".into(),
    };
    catalog.register(descriptor).unwrap();
    instance
}

pub(super) fn fixture(
    limit: u32,
) -> (
    tempfile::TempDir,
    std::sync::Arc<RuntimeOwner>,
    NeutralExecution,
    i64,
) {
    fixture_with(160, false, |root, catalog, store| {
        let setup = root.instances[0].clone();
        let original = catalog.get(&setup.descriptor).unwrap().clone();
        let mut leaf = original.clone();
        leaf.id.name = "assess".into();
        leaf.inputs = correction_input_schema();
        leaf.outputs = correction_state_schema();
        leaf.outcomes = BTreeSet::from(["assessed".into()]);
        catalog.register(leaf.clone()).unwrap();
        let mut body = root.clone();
        body.inputs = correction_input_schema();
        body.outputs = correction_state_schema();
        body.outcomes = BTreeSet::from(["assessed".into()]);
        body.conditions.clear();
        body.instances = vec![ActivityInstance {
            descriptor: leaf.id.clone(),
            ..setup.clone()
        }];
        body.input_bindings.clear();
        body.output_bindings = ["state", "repeat"]
            .into_iter()
            .map(|name| {
                (
                    name.into(),
                    DataSource::ActivityOutput {
                        activity: 0,
                        field: name.into(),
                    },
                )
            })
            .collect();
        body.control = ControlGraph {
            nodes: BTreeMap::from([
                ("start".into(), ControlNode::Start),
                ("assess".into(), ControlNode::Activity { activity: 0 }),
                (
                    "end".into(),
                    ControlNode::End {
                        outcome: "assessed".into(),
                    },
                ),
            ]),
            edges: BTreeMap::from([
                (
                    "start".into(),
                    BTreeMap::from([("next".into(), "assess".into())]),
                ),
                (
                    "assess".into(),
                    BTreeMap::from([("assessed".into(), "end".into())]),
                ),
            ]),
        };
        let selected = call(store, catalog, leaf, "selected-region", body);
        let region = CorrectionRegion {
            max_corrections: limit,
            criteria: crate::workflows::correction_tests::policy(),
            selected,
        };
        let wrapper = region.definition(catalog).unwrap();
        let correction = call(store, catalog, original, "bounded-correction", wrapper);
        root.instances = vec![setup.clone(), setup.clone(), setup, correction];
        root.conditions.clear();
        root.outputs = correction_state_schema();
        root.outcomes = BTreeSet::from([
            "success".into(),
            "operator".into(),
            "technical_failure".into(),
        ]);
        root.output_bindings = ["state", "repeat"]
            .into_iter()
            .map(|name| {
                (
                    name.into(),
                    DataSource::ActivityOutput {
                        activity: 3,
                        field: name.into(),
                    },
                )
            })
            .collect();
        root.input_bindings = (0..3)
            .map(|activity| {
                (
                    activity,
                    BTreeMap::from([(
                        "value".into(),
                        DataSource::WorkflowInput {
                            field: "initial".into(),
                        },
                    )]),
                )
            })
            .collect();
        root.input_bindings.insert(3, BTreeMap::from([
            ("context".into(), DataSource::Constant { value: json!({"issue":"issue-ref", "workspace":"workspace-ref", "pr":"pr-ref"}) }),
            ("state".into(), DataSource::Constant { value: json!(CorrectionState::initial(artifact(1))) }),
        ]));
        root.subflows = vec![PublicationRef {
            project: "/project".into(),
            id: "bounded-correction".into(),
            revision: 1,
        }];
        root.control = ControlGraph {
            nodes: BTreeMap::from([
                ("start".into(), ControlNode::Start),
                ("issue".into(), ControlNode::Activity { activity: 0 }),
                ("workspace".into(), ControlNode::Activity { activity: 1 }),
                ("pr".into(), ControlNode::Activity { activity: 2 }),
                ("correction".into(), ControlNode::Activity { activity: 3 }),
                (
                    "success".into(),
                    ControlNode::End {
                        outcome: "success".into(),
                    },
                ),
                (
                    "operator".into(),
                    ControlNode::End {
                        outcome: "operator".into(),
                    },
                ),
                (
                    "technical".into(),
                    ControlNode::End {
                        outcome: "technical_failure".into(),
                    },
                ),
            ]),
            edges: BTreeMap::from([
                (
                    "start".into(),
                    BTreeMap::from([("next".into(), "issue".into())]),
                ),
                (
                    "issue".into(),
                    BTreeMap::from([("ok".into(), "workspace".into())]),
                ),
                (
                    "workspace".into(),
                    BTreeMap::from([("ok".into(), "pr".into())]),
                ),
                (
                    "pr".into(),
                    BTreeMap::from([("ok".into(), "correction".into())]),
                ),
                (
                    "correction".into(),
                    BTreeMap::from([
                        ("success".into(), "success".into()),
                        ("operator".into(), "operator".into()),
                        ("technical_failure".into(), "technical".into()),
                    ]),
                ),
            ]),
        };
    })
}
