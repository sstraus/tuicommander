//! Scoped roles translate to the same mediated model request and tool ledger.
use super::neutral_model_request::ModelRequest;
use super::neutral_runtime::NeutralInvocation;
use crate::acp::EgoAcpConfig;
use crate::workflows::AgentRole;
use crate::workflows::catalog::{ActivityDescriptor, FieldSchema, ValueType};

pub(super) const HANDLER: &str = "host.agent.role";

pub(super) fn descriptor(runtime: &EgoAcpConfig) -> Result<ActivityDescriptor, String> {
    let mut descriptor = super::native_model::descriptor(runtime)?;
    // Only host.model sends its literal `text` input; a role must refuse it, not drop it.
    descriptor.inputs.fields.remove("text");
    descriptor.id.namespace = "agent".into();
    descriptor.id.name = "role".into();
    descriptor.handler.name = HANDLER.into();
    descriptor.purpose =
        "Run a scoped role with an exact mediated profile, tools and typed report".into();
    descriptor.config.fields.insert(
        "role".into(),
        FieldSchema {
            value_type: ValueType::String,
            required: true,
            default: None,
        },
    );
    descriptor.config.fields.insert(
        "policy".into(),
        FieldSchema {
            value_type: ValueType::Object,
            required: false,
            default: None,
        },
    );
    descriptor.inputs.fields.insert(
        "source".into(),
        FieldSchema {
            value_type: ValueType::Object,
            required: true,
            default: None,
        },
    );
    descriptor.inputs.fields.insert(
        "data".into(),
        FieldSchema {
            value_type: ValueType::Object,
            required: false,
            default: None,
        },
    );
    descriptor.subject = Some(crate::workflows::catalog::SubjectSchema {
        required: true,
        fields: crate::workflows::catalog::ObjectSchema {
            fields: std::collections::BTreeMap::from([(
                "workspace".into(),
                FieldSchema {
                    value_type: ValueType::WorkspaceReference,
                    required: true,
                    default: None,
                },
            )]),
        },
    });
    descriptor.outputs.fields = std::collections::BTreeMap::from([(
        "report".into(),
        FieldSchema {
            value_type: ValueType::Object,
            required: true,
            default: None,
        },
    )]);
    Ok(descriptor)
}

pub(super) fn request(invocation: &NeutralInvocation) -> Result<ModelRequest, String> {
    let literal = if is_model(&invocation.descriptor.handler.name) {
        invocation
            .instance
            .config
            .get("request")
            .cloned()
            .ok_or("model request missing")?
    } else {
        serde_json::to_value(&invocation.instance.config).map_err(|e| e.to_string())?
    };
    let mut request: ModelRequest =
        serde_json::from_value(literal).map_err(|e| format!("model request: {e}"))?;
    if invocation.descriptor.handler.name == HANDLER {
        let role: AgentRole = serde_json::from_value(
            invocation
                .instance
                .config
                .get("role")
                .cloned()
                .ok_or("agent role missing")?,
        )
        .map_err(|e| format!("agent role: {e}"))?;
        // Role is literal task context. Tools and rights still come only from
        // retained tool bindings and current native admission on every call.
        request.instructions = serde_json::to_string(&serde_json::json!({
            "role": role, "instructions": request.instructions,
        }))
        .map_err(|e| e.to_string())?;
    }
    // A host.model text input (scheduled text runs) stays literal context, never a template.
    if invocation.descriptor.handler.name == "host.model"
        && let Some(text) = invocation
            .inputs
            .get("text")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
    {
        request.context.push(text.into());
    }
    if invocation.descriptor.handler.name == HANDLER && invocation.inputs.contains_key("source") {
        request
            .context
            .push(serde_json::to_string(&source(invocation)?).map_err(|e| e.to_string())?);
    }
    if invocation.descriptor.handler.name == HANDLER
        && let Some(data) = invocation.inputs.get("data")
    {
        let snapshot: super::native_role_context::Snapshot =
            serde_json::from_value(data.clone()).map_err(|e| format!("role context: {e}"))?;
        snapshot.validate(&source(invocation)?)?;
        request
            .context
            .push(serde_json::to_string(&snapshot).map_err(|e| e.to_string())?);
    }
    Ok(request)
}

pub(super) fn is_model(handler: &str) -> bool {
    matches!(handler, "host.model" | HANDLER)
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Source {
    pub origin: String,
    pub generation: String,
    pub commit: String,
    pub tree: String,
}

pub(super) fn source(invocation: &NeutralInvocation) -> Result<Source, String> {
    let source: Source = serde_json::from_value(
        invocation
            .inputs
            .get("source")
            .cloned()
            .ok_or("agent source missing")?,
    )
    .map_err(|e| format!("agent source: {e}"))?;
    if [
        &source.origin,
        &source.generation,
        &source.commit,
        &source.tree,
    ]
    .iter()
    .any(|s| s.trim().is_empty())
    {
        return Err("agent report requires an exact source generation and artifact".into());
    }
    Ok(source)
}

pub(super) fn validate_scope(
    invocation: &NeutralInvocation,
    tools: &super::neutral_model_tools::ModelToolPolicy,
    resolved_inputs: bool,
) -> Result<(), String> {
    if invocation.instance.config.contains_key("policy") {
        super::native_review_policy::policy(&invocation.instance)?;
        if !matches!(
            invocation
                .instance
                .config
                .get("role")
                .and_then(serde_json::Value::as_str),
            Some("reviewer" | "validator")
        ) {
            return Err(
                "review admission policy requires an independent reviewer/validator role".into(),
            );
        }
    }
    if resolved_inputs {
        source(invocation)?;
    }
    if matches!(
        invocation
            .instance
            .config
            .get("role")
            .and_then(serde_json::Value::as_str),
        Some("reviewer" | "validator")
    ) && !tools.tools.is_empty()
    {
        return Err("independent review/validation requires a fresh tool-less role".into());
    }
    Ok(())
}

pub(super) fn report(
    bound: &super::native_host::NativeBinding,
    effect: &super::neutral_effects::NeutralEffect,
    output: std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<serde_json::Value, String> {
    bound.context.current(None)?;
    let source = source(&effect.invocation)?;
    let snapshot = bound.store.neutral_execution(&effect.key.fence.run)?;
    let original = snapshot.effect(&effect.key)?;
    if original.invocation != effect.invocation
        || original.cancel_started
        || original.observation.is_some()
        || snapshot.status != super::neutral_execution::NeutralStatus::Running
    {
        return Err("agent report no longer owns its original active source".into());
    }
    let session = original
        .model_session
        .as_ref()
        .ok_or("agent report has no observed independent session identity")?;
    let request = request(&effect.invocation)?;
    request.output_schema.resolve(&output, "agent report")?;
    let role = &effect.invocation.instance.config["role"];
    if matches!(role.as_str(), Some("reviewer" | "validator")) {
        bound
            .context
            .require_artifact(&source.commit, &source.tree)?;
    }
    // Staged implementer work is the observed artifact (HEAD plus the index tree).
    let (commit, tree) = super::check::staged_artifact(bound.context.workspace_path())?;
    Ok(serde_json::json!({
        "effect": effect.key,
        "publication": crate::workflows::catalog::content_digest(&effect.invocation.publication)?,
        "role": role,
        "source": source,
        "observed_artifact": { "commit": commit, "tree": tree },
        "identity": session,
        "report": output,
    }))
}

/// Consumers resolve a retained role report by effect, never by caller JSON.
pub(super) fn read_report(
    bound: &super::native_host::NativeBinding,
    key: &super::neutral_effects::EffectKey,
    expected_source: &Source,
) -> Result<serde_json::Value, String> {
    bound.context.current(None)?;
    let snapshot = bound.store.neutral_execution(&key.fence.run)?;
    let effect = snapshot.effect(key)?;
    if effect.invocation.descriptor.handler.name != HANDLER
        || effect.invocation.publication.project != bound.context.workspace_path().to_string_lossy()
        || source(&effect.invocation)? != *expected_source
    {
        return Err("role report source/owner does not match the retained effect".into());
    }
    let Some(super::neutral_effects::EffectObservation {
        stopped: true,
        outcome:
            super::neutral_effects::EffectOutcome::Completed {
                result: super::neutral_execution::NeutralResult::Completed { outputs, .. },
            },
    }) = &effect.observation
    else {
        return Err("role report has no completed stopped observation".into());
    };
    let receipt = outputs.get("report").ok_or("role report receipt missing")?;
    if receipt["effect"] != serde_json::to_value(key).map_err(|e| e.to_string())?
        || receipt["source"] != serde_json::to_value(expected_source).map_err(|e| e.to_string())?
        || receipt["identity"]
            != serde_json::to_value(&effect.model_session).map_err(|e| e.to_string())?
        || effect.model_session.is_none()
    {
        return Err("role report receipt identity differs from the original effect".into());
    }
    let commit = receipt["observed_artifact"]["commit"]
        .as_str()
        .ok_or("report commit missing")?;
    let tree = receipt["observed_artifact"]["tree"]
        .as_str()
        .ok_or("report tree missing")?;
    bound.context.require_artifact(commit, tree)?;
    Ok(receipt.clone())
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReportRequest {
    pub report: super::neutral_effects::EffectKey,
    pub source: Source,
}
