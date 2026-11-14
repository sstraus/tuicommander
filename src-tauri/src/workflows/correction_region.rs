//! Lower a selected, pinned region to an ordinary neutral bounded subflow.
//! Setup belongs to its caller. No issue/workspace/PR creation is inserted here.
use super::catalog::{
    ActivityInstance, FieldSchema, ObjectSchema, ReusableTarget, ValueType, WorkflowCatalog,
};
use super::condition::{Comparison, Condition};
use super::data_binding::DataSource;
use super::loop_data::LoopContract;
use super::neutral_control::{ControlGraph, ControlNode};
use super::neutral_publication::NeutralDefinition;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorrectionRegion {
    /// Corrections after the initial attempt, never technical retries.
    pub max_corrections: u32,
    pub criteria: super::correction::CorrectionPolicy,
    pub selected: ActivityInstance,
}

pub(crate) fn correction_state_schema() -> ObjectSchema {
    ObjectSchema {
        fields: BTreeMap::from([
            ("state".into(), field(ValueType::Object, true)),
            // Absent is an explicit operator decision via the loop unknown port.
            ("repeat".into(), field(ValueType::Boolean, false)),
        ]),
    }
}

pub(crate) fn correction_input_schema() -> ObjectSchema {
    let mut schema = correction_state_schema();
    // Explicit immutable workspace/repository/PR/criteria references. Child
    // permissions and native authority still come from the installed host.
    schema
        .fields
        .insert("context".into(), field(ValueType::Object, true));
    schema
        .fields
        .insert("criteria".into(), field(ValueType::Object, true));
    schema
}

fn field(value_type: ValueType, required: bool) -> FieldSchema {
    FieldSchema {
        value_type,
        required,
        default: None,
    }
}

impl CorrectionRegion {
    pub(crate) fn definition(
        &self,
        catalog: &WorkflowCatalog,
    ) -> Result<NeutralDefinition, String> {
        if self.criteria.criteria.is_empty()
            || self
                .criteria
                .criteria
                .iter()
                .any(|name| name.trim().is_empty())
        {
            return Err("correction region requires named criteria".into());
        }
        let max_iterations = self
            .max_corrections
            .checked_add(1)
            .ok_or("correction limit overflows total traversal bound")?;
        let descriptor = catalog
            .get(&self.selected.descriptor)
            .ok_or("selected correction region descriptor unavailable")?;
        let Some(ReusableTarget::Subflow { publication }) = &descriptor.target else {
            return Err("selected correction region must be an inspectable pinned subflow".into());
        };
        if descriptor.inputs != correction_input_schema()
            || descriptor.outputs != correction_state_schema()
            || descriptor.outcomes != BTreeSet::from(["assessed".into()])
        {
            return Err(
                "selected region requires context/criteria/state/repeat and assessed outcome"
                    .into(),
            );
        }
        let output = |field: &str| DataSource::ActivityOutput {
            activity: 0,
            field: field.into(),
        };
        let scope_output = |field: &str| DataSource::ScopeOutput {
            scope: "correction".into(),
            field: field.into(),
        };
        let end = |outcome: &str| ControlNode::End {
            outcome: outcome.into(),
        };
        Ok(NeutralDefinition {
            control: ControlGraph {
                nodes: BTreeMap::from([
                    ("entry".into(), ControlNode::Start),
                    (
                        "correction".into(),
                        ControlNode::Loop {
                            scope: "correction".into(),
                            condition: "requires-correction".into(),
                        },
                    ),
                    ("success".into(), end("success")),
                    ("operator".into(), end("operator")),
                    ("technical-failure".into(), end("technical_failure")),
                ]),
                edges: BTreeMap::from([
                    (
                        "entry".into(),
                        BTreeMap::from([("next".into(), "correction".into())]),
                    ),
                    (
                        "correction".into(),
                        BTreeMap::from([
                            ("complete".into(), "success".into()),
                            ("exhausted".into(), "operator".into()),
                            ("unknown".into(), "operator".into()),
                            ("failed".into(), "technical-failure".into()),
                        ]),
                    ),
                ]),
            },
            inputs: {
                let mut inputs = correction_input_schema();
                inputs.fields.remove("criteria");
                inputs
            },
            subject: descriptor.subject.clone(),
            outputs: correction_state_schema(),
            outcomes: BTreeSet::from([
                "success".into(),
                "operator".into(),
                "technical_failure".into(),
            ]),
            instances: vec![self.selected.clone()],
            input_bindings: BTreeMap::from([(
                0,
                BTreeMap::from([
                    (
                        "criteria".into(),
                        DataSource::Constant {
                            value: json!(self.criteria),
                        },
                    ),
                    (
                        "context".into(),
                        DataSource::WorkflowInput {
                            field: "context".into(),
                        },
                    ),
                ]),
            )]),
            conditions: BTreeMap::from([(
                "requires-correction".into(),
                Condition::Compare {
                    comparison: Comparison::Equal,
                    left: scope_output("repeat"),
                    right: DataSource::Constant { value: json!(true) },
                },
            )]),
            loops: BTreeMap::from([(
                "correction".into(),
                LoopContract {
                    body: 0,
                    max_iterations,
                    carried: correction_state_schema(),
                    initial: BTreeMap::from([(
                        "state".into(),
                        DataSource::WorkflowInput {
                            field: "state".into(),
                        },
                    )]),
                    result: BTreeMap::from([
                        ("state".into(), output("state")),
                        ("repeat".into(), output("repeat")),
                    ]),
                },
            )]),
            collections: BTreeMap::new(),
            forks: BTreeMap::new(),
            output_bindings: BTreeMap::from([
                ("state".into(), scope_output("state")),
                ("repeat".into(), scope_output("repeat")),
            ]),
            subflows: vec![publication.reference.clone()],
        })
    }
}
