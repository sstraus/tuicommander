//! Child dispatch is a named pinned subflow, executed by the existing run actor.
use super::catalog::{
    ActivityDescriptor, DescriptorId, HandlerRef, ObjectSchema, ReusableTarget, content_digest,
};
use super::neutral_publication::{NeutralPublication, PublicationPin};
use std::collections::BTreeSet;

pub(super) fn descriptor(
    id: DescriptorId,
    pin: PublicationPin,
    child: &NeutralPublication,
    catalog: &super::catalog::WorkflowCatalog,
    cohort: Option<super::run::native_cohort::Cohort>,
) -> Result<ActivityDescriptor, String> {
    if pin.reference.project != child.project
        || pin.reference.id != child.id
        || pin.reference.revision != child.revision
        || pin.sha256 != content_digest(child)?
    {
        return Err("child dispatch requires the exact publication content pin".into());
    }
    if let Some(cohort) = &cohort {
        cohort.validate()?;
    }
    receipt_contract(child, catalog)?;
    Ok(ActivityDescriptor {
        id,
        purpose: "Dispatch and join an exact child with inherited run ownership".into(),
        handler: HandlerRef {
            name: super::run::native_cohort::HANDLER.into(),
            version: 1,
        },
        target: Some(ReusableTarget::Subflow { publication: pin }),
        // A pinned cohort is a config snapshot. Without one, the cohort is a run-time
        // input admitted only for stories this run created (decision a, 2026-11-12).
        config: match cohort {
            Some(cohort) => ObjectSchema {
                fields: std::collections::BTreeMap::from([(
                    "cohort".into(),
                    super::catalog::FieldSchema {
                        value_type: super::catalog::ValueType::Object,
                        required: true,
                        default: Some(serde_json::to_value(cohort).map_err(|e| e.to_string())?),
                    },
                )]),
            },
            None => ObjectSchema::default(),
        },
        inputs: child.definition.inputs.clone(),
        subject: child.definition.subject.clone(),
        outputs: child.definition.outputs.clone(),
        outcomes: child.definition.outcomes.clone(),
        required_capabilities: BTreeSet::new(),
        execution_policies: BTreeSet::from(["local".into()]),
    })
}

// The child returns native observations, never a model's story-completion claim.
pub(super) fn receipt_contract(
    child: &NeutralPublication,
    catalog: &super::catalog::WorkflowCatalog,
) -> Result<(), String> {
    use super::data_binding::DataSource;
    use super::neutral_control::ControlNode;
    let definition = &child.definition;
    let refused = || {
        "story child must return the native cohort observation with completed/blocked routes; combined-artifact batch obligations remain unattested".to_string()
    };
    if definition.subject.as_ref().is_none_or(|s| {
        !s.required
            || s.fields.fields.get("workspace").is_none_or(|f| {
                !f.required || f.value_type != super::catalog::ValueType::WorkspaceReference
            })
    }) {
        return Err(refused());
    }
    if definition
        .inputs
        .fields
        .get("cohort")
        .is_none_or(|f| f.value_type != super::catalog::ValueType::Object || !f.required)
        || definition
            .outputs
            .fields
            .get("receipt")
            .is_none_or(|f| f.value_type != super::catalog::ValueType::Object || !f.required)
        || definition.outcomes != BTreeSet::from(["completed".into(), "blocked".into()])
    {
        return Err(refused());
    }
    let Some(DataSource::ActivityOutput { activity, field }) =
        definition.output_bindings.get("receipt")
    else {
        return Err(refused());
    };
    if field != "receipt" {
        return Err(refused());
    }
    let instance = definition.instances.get(*activity).ok_or_else(refused)?;
    let descriptor = catalog.get(&instance.descriptor).ok_or_else(refused)?;
    if descriptor.handler.name != "native.story.cohort.observe"
        || definition
            .input_bindings
            .get(activity)
            .and_then(|b| b.get("request"))
            != Some(&DataSource::WorkflowInput {
                field: "cohort".into(),
            })
    {
        return Err(refused());
    }
    let node = definition
        .control
        .nodes
        .iter()
        .find_map(|(id, node)| match node {
            ControlNode::Activity { activity: n } if n == activity => Some(id),
            _ => None,
        })
        .ok_or_else(refused)?;
    for outcome in ["completed", "blocked"] {
        let target = definition
            .control
            .edges
            .get(node)
            .and_then(|edges| edges.get(outcome))
            .ok_or_else(refused)?;
        if definition.control.nodes.get(target)
            != Some(&ControlNode::End {
                outcome: outcome.into(),
            })
        {
            return Err(refused());
        }
    }
    Ok(())
}
