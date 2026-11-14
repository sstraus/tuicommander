//! Local approval uses the existing retained effect question and answer inbox.
use super::neutral_runtime::NeutralInvocation;
use crate::workflows::catalog::*;
use std::collections::BTreeMap;

pub(super) fn descriptor() -> ActivityDescriptor {
    ActivityDescriptor {
        id: DescriptorId {
            namespace: "inbox".into(),
            name: "approval".into(),
            version: 1,
        },
        purpose: "Request an exact local approval in the workflow inbox".into(),
        handler: HandlerRef {
            name: "host.approval".into(),
            version: 1,
        },
        target: None,
        config: ObjectSchema::default(),
        inputs: ObjectSchema {
            fields: BTreeMap::from([
                (
                    "text".into(),
                    FieldSchema {
                        value_type: ValueType::String,
                        required: true,
                        default: None,
                    },
                ),
                // The evidence behind the question, shown below it (for example
                // a verification summary).
                (
                    "details".into(),
                    FieldSchema {
                        value_type: ValueType::String,
                        required: false,
                        default: Some(serde_json::json!("")),
                    },
                ),
            ]),
        },
        subject: None,
        outputs: ObjectSchema {
            fields: BTreeMap::from([(
                "approved".into(),
                FieldSchema {
                    value_type: ValueType::Boolean,
                    required: true,
                    default: None,
                },
            )]),
        },
        outcomes: ["complete".into()].into(),
        required_capabilities: Default::default(),
        execution_policies: ["local".into()].into(),
    }
}

pub(super) fn validate(invocation: &NeutralInvocation) -> Result<(), String> {
    let text = invocation
        .inputs
        .get("text")
        .and_then(serde_json::Value::as_str)
        .ok_or("approval text missing")?;
    if text.trim().is_empty() || text.len() > 4096 {
        return Err("approval text must be nonempty and bounded".into());
    }
    if invocation
        .inputs
        .get("details")
        .is_some_and(|details| !details.is_string())
    {
        return Err("approval details must be text".into());
    }
    Ok(())
}

/// Model output can be long; the question shows at most this much of it.
const MAX_DETAILS_BYTES: usize = 8192;

/// The question text followed by its details, so the operator sees the
/// evidence that caused the question.
pub(super) fn title(invocation: &NeutralInvocation) -> Result<String, String> {
    validate(invocation)?;
    let text = invocation.inputs["text"].as_str().unwrap_or_default();
    let details = invocation
        .inputs
        .get("details")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim();
    if details.is_empty() {
        return Ok(text.into());
    }
    let mut end = details.len().min(MAX_DETAILS_BYTES);
    while !details.is_char_boundary(end) {
        end -= 1;
    }
    let cut = if end < details.len() { "…" } else { "" };
    Ok(format!("{text}\n\n{}{cut}", &details[..end]))
}
