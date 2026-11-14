//! Typed decisions bind a retained question to an immutable ordinary graph edge.
use super::{
    native_host::NativeBinding,
    neutral_effects::{EffectCommand, EffectKey, NeutralEffect},
};
use crate::workflows::catalog::content_digest;
use serde::{Deserialize, Serialize};

pub(crate) const HANDLER: &str = "native.operator.wait";
pub(crate) const AUTO_HANDLER: &str = "native.operator.automatic_close";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OperatorArtifact {
    pub commit: String,
    pub tree: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClosureProof {
    pub base: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum OperatorKind {
    Resume,
    HumanClosure {
        story: String,
        revision: i64,
    },
    AutomaticClosure {
        story: String,
        revision: i64,
        proof: ClosureProof,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OperatorRequest {
    pub source_node: String,
    pub resume_to: String,
    pub source_generation: Option<String>,
    pub artifact: OperatorArtifact,
    pub decision: OperatorKind,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OperatorAnswer {
    /// Echo the complete pinned subject. A bool cannot select another action.
    pub request: OperatorRequest,
    pub accept: bool,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AnswerReceipt {
    pub key: EffectKey,
    pub revision: u64,
    pub sequence: i64,
    pub publication_digest: String,
    pub answer: OperatorAnswer,
    pub actor: String,
    pub recorded_ms: i64,
}
pub(super) fn request(
    inputs: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<OperatorRequest, String> {
    if inputs.len() != 1 {
        return Err("operator activity requires one typed request".into());
    }
    let r: OperatorRequest = serde_json::from_value(
        inputs
            .get("request")
            .ok_or("operator request absent")?
            .clone(),
    )
    .map_err(|e| e.to_string())?;
    if [
        &r.source_node,
        &r.resume_to,
        &r.artifact.commit,
        &r.artifact.tree,
    ]
    .iter()
    .any(|s| s.trim().is_empty() || s.len() > 1024)
        || serde_json::to_vec(&r).map_err(|e| e.to_string())?.len() > 16384
    {
        return Err("operator request must be bounded and exact".into());
    }
    Ok(r)
}
pub(super) fn subject(
    bound: &NativeBinding,
    effect: &NeutralEffect,
    r: &OperatorRequest,
) -> Result<(), String> {
    bound
        .context
        .require_artifact(&r.artifact.commit, &r.artifact.tree)?;
    super::correction_generation::require_source(
        &bound.store,
        &effect.key.fence.run,
        r.source_generation.as_deref(),
    )?;
    let graph = &effect.invocation.publication.definition.control;
    let source = graph
        .nodes
        .get(&r.source_node)
        .ok_or("operator source node absent")?;
    if !matches!(source,crate::workflows::neutral_control::ControlNode::Activity{activity}
        if effect.invocation.publication.definition.instances.get(*activity)==Some(&effect.invocation.instance))
        || graph
            .edges
            .get(&r.source_node)
            .and_then(|e| e.get("resumed"))
            != Some(&r.resume_to)
    {
        return Err("operator resume target differs from pinned activity edge".into());
    }
    let run = bound.store.neutral_execution(&effect.key.fence.run)?;
    if run.node != r.source_node
        || run.effect(&effect.key)? != effect
        || run.status != super::neutral_execution::NeutralStatus::Running
        || run.deadline_ms <= super::store::now_ms()
        || effect.cancel_started
        || effect.observation.is_some()
    {
        return Err("operator wait is no longer the current owned subject".into());
    }
    Ok(())
}
pub(super) fn answer(
    bound: &NativeBinding,
    key: &EffectKey,
    revision: u64,
    sequence: i64,
    answer: &OperatorAnswer,
) -> Result<EffectCommand, String> {
    let run = bound.store.neutral_execution(&key.fence.run)?;
    let effect = run.effect(key)?;
    if effect.invocation.descriptor.handler.name != HANDLER
        || run.sequence != sequence
        || request(&effect.invocation.inputs)? != answer.request
        || answer.reason.trim().is_empty()
        || answer.reason.len() > 2000
    {
        return Err("operator answer differs from exact current wait/sequence/reason".into());
    }
    subject(bound, effect, &answer.request)?;
    let receipt = AnswerReceipt {
        key: key.clone(),
        revision,
        sequence,
        publication_digest: content_digest(&effect.invocation.publication)?,
        answer: answer.clone(),
        actor: "owner".into(),
        recorded_ms: super::store::now_ms(),
    };
    // Both choices execute the fixed activity; decline selects its declared edge.
    Ok(EffectCommand::AnswerRecorded {
        key: key.clone(),
        revision,
        approved: true,
        receipt: serde_json::to_value(receipt).map_err(|e| e.to_string())?,
    })
}
pub(super) fn receipt(
    effect: &NeutralEffect,
    r: &OperatorRequest,
) -> Result<AnswerReceipt, String> {
    let receipt: AnswerReceipt = serde_json::from_value(
        effect
            .answer_receipt
            .clone()
            .ok_or("operator wait requires a typed authenticated answer, not effect approval")?,
    )
    .map_err(|e| e.to_string())?;
    if receipt.key != effect.key
        || receipt.answer.request != *r
        || receipt.actor != "owner"
        || receipt.revision
            != effect
                .question
                .as_ref()
                .ok_or("operator question absent")?
                .revision
        || receipt.publication_digest != content_digest(&effect.invocation.publication)?
    {
        return Err("operator receipt differs from original wait".into());
    }
    Ok(receipt)
}
pub(super) fn execute(
    bound: &NativeBinding,
    effect: &NeutralEffect,
) -> Result<serde_json::Value, super::native_git::NativeFailure> {
    let r = request(&effect.invocation.inputs)?;
    subject(bound, effect, &r)?;
    if !effect.dispatched {
        return Err("operator effect was not dispatched".to_owned().into());
    }
    let automatic = effect.invocation.descriptor.handler.name == AUTO_HANDLER;
    let answer = if automatic {
        if !matches!(r.decision, OperatorKind::AutomaticClosure { .. }) {
            return Err("automatic handler requires automatic closure"
                .to_owned()
                .into());
        }
        None
    } else {
        if matches!(r.decision, OperatorKind::AutomaticClosure { .. }) {
            return Err("automatic closure uses its distinct handler"
                .to_owned()
                .into());
        }
        Some(receipt(effect, &r)?)
    };
    let accepted = answer.as_ref().is_none_or(|a| a.answer.accept);
    let result = if accepted {
        super::native_operator_actions::apply(bound, effect, &r, answer.as_ref())?
    } else {
        serde_json::Value::Null
    };
    Ok(
        serde_json::json!({"outcome":if accepted {"resumed"}else{"declined"},"request":r,"answer":answer,"result":result}),
    )
}
