//! One production constructor for the saved script boundary and its fixtures.
use super::{ScriptPolicy, handler};
use crate::workflows::catalog::{
    ActivityDescriptor, FieldSchema, ReusableTarget, ValueType, WorkflowCatalog,
};
use crate::workflows::command::Invocation;

pub(crate) fn script_descriptor(
    mut contract: ActivityDescriptor,
    invocation: Invocation,
    policy: &ScriptPolicy,
) -> Result<ActivityDescriptor, String> {
    contract.handler = handler();
    contract.target = Some(ReusableTarget::Script { invocation });
    contract.config.fields.insert(
        "script".into(),
        FieldSchema {
            value_type: ValueType::Object,
            required: true,
            default: Some(serde_json::to_value(policy).map_err(|e| e.to_string())?),
        },
    );
    let mut catalog = WorkflowCatalog::new();
    catalog.register(contract.clone())?;
    Ok(contract)
}
