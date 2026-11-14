//! Descriptor and preparation for the configured mediated model runtime.
use super::neutral_effects::EffectPreparation;
use super::neutral_model_tools::ModelToolPolicy;
use super::neutral_runtime::NeutralInvocation;
use crate::acp::EgoAcpConfig;
use crate::workflows::catalog::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn descriptor(runtime: &EgoAcpConfig) -> Result<ActivityDescriptor, String> {
    if runtime.profile.trim().is_empty() || !runtime.executable.is_absolute() {
        return Err("model requires a configured absolute executable and exact profile".into());
    }
    let field = |value_type, default| FieldSchema {
        value_type,
        required: true,
        default,
    };
    Ok(ActivityDescriptor {
        id: DescriptorId {
            namespace: "model".into(),
            name: "request".into(),
            version: 1,
        },
        purpose: "Run an exact model request through the configured mediated profile".into(),
        handler: HandlerRef {
            name: "host.model".into(),
            version: 1,
        },
        target: None,
        config: ObjectSchema {
            fields: BTreeMap::from([
                (
                    "runtime".into(),
                    field(
                        ValueType::Object,
                        Some(json!({"executable":runtime.executable,"profile":runtime.profile})),
                    ),
                ),
                ("request".into(), field(ValueType::Object, None)),
                (
                    "tools".into(),
                    field(
                        ValueType::Object,
                        Some(json!(ModelToolPolicy {
                            tools: BTreeMap::new(),
                            max_calls: 1,
                            max_concurrency: 1
                        })),
                    ),
                ),
            ]),
        },
        inputs: ObjectSchema {
            fields: BTreeMap::from([(
                "text".into(),
                FieldSchema {
                    value_type: ValueType::String,
                    required: false,
                    default: Some(json!("")),
                },
            )]),
        },
        subject: None,
        outputs: ObjectSchema {
            fields: BTreeMap::from([("result".into(), field(ValueType::Object, None))]),
        },
        outcomes: BTreeSet::from(["complete".into()]),
        required_capabilities: BTreeSet::new(),
        execution_policies: BTreeSet::from(["local".into()]),
    })
}

pub(super) fn prepare(
    invocation: &NeutralInvocation,
    runtime: &EgoAcpConfig,
) -> Result<EffectPreparation, String> {
    prepare_inner(invocation, runtime, true)
}

pub(super) fn publication(
    invocation: &NeutralInvocation,
    runtime: &EgoAcpConfig,
) -> Result<EffectPreparation, String> {
    prepare_inner(invocation, runtime, false)
}

fn prepare_inner(
    invocation: &NeutralInvocation,
    runtime: &EgoAcpConfig,
    resolved_inputs: bool,
) -> Result<EffectPreparation, String> {
    let expected = if invocation.descriptor.handler.name == super::native_agent::HANDLER {
        super::native_agent::descriptor(runtime)?
    } else {
        descriptor(runtime)?
    };
    if expected != invocation.descriptor || invocation.instance.descriptor != expected.id {
        return Err("model descriptor differs from configured runtime".into());
    }
    let config = expected
        .config
        .resolve(&invocation.instance.config, "model config")?;
    if config.get("runtime") != expected.config.fields["runtime"].default.as_ref() {
        return Err("model instance cannot override configured runtime".into());
    }
    let mut resolved = invocation.clone();
    resolved.instance.config = config.clone();
    let request = super::native_agent::request(&resolved)?;
    request.validate(runtime)?;
    if resolved_inputs {
        expected
            .inputs
            .resolve(&invocation.inputs, "model inputs")?;
    }
    let tools: ModelToolPolicy =
        serde_json::from_value(config["tools"].clone()).map_err(|e| e.to_string())?;
    tools.validate()?;
    if invocation.descriptor.handler.name == super::native_agent::HANDLER {
        super::native_agent::validate_scope(invocation, &tools, resolved_inputs)?;
    }
    Ok(EffectPreparation {
        mutation: false,
        resources: BTreeSet::new(),
        approval: None,
        model_tools: Some(tools),
    })
}
