//! A policy-bound role spends the same reviewer budget as the native review.
use super::{native_host::NativeBinding, neutral_effects::*, neutral_execution::NeutralResult};
use std::collections::BTreeMap;

pub(super) fn admit(
    bound: &NativeBinding,
    effect: &NeutralEffect,
) -> Result<Option<EffectObservation>, String> {
    if effect.invocation.descriptor.handler.name != super::native_agent::HANDLER
        || !effect.invocation.instance.config.contains_key("policy")
    {
        return Ok(None);
    }
    match super::native_review_policy::reserve(&bound.context, &bound.store, &effect.key) {
        Ok(_) => Ok(None),
        Err(reason) if reason == "independent reviewer admission budget exhausted" => {
            Ok(Some(EffectObservation {
                stopped: true,
                outcome: EffectOutcome::Completed {
                    result: NeutralResult::Completed {
                        outcome: "complete".into(),
                        outputs: BTreeMap::from([(
                            "report".into(),
                            serde_json::json!({
                                "effect":effect.key,"admission_exhausted":true
                            }),
                        )]),
                    },
                },
            }))
        }
        Err(reason) => Err(reason),
    }
}

/// Resolve the terminal budget marker only from its exact original role effect.
pub(super) fn exhausted(bound: &NativeBinding, effect: &NeutralEffect) -> Result<bool, String> {
    let value = &effect.invocation.inputs["report"];
    if value["admission_exhausted"] != true {
        return Ok(false);
    }
    let key: EffectKey =
        serde_json::from_value(value["effect"].clone()).map_err(|e| e.to_string())?;
    if key.fence.run != effect.key.fence.run {
        return Err("foreign exhausted role".into());
    }
    let run = bound.store.neutral_execution(&key.fence.run)?;
    let original = run.effect(&key)?;
    if original.invocation.descriptor.handler.name != super::native_agent::HANDLER
        || super::native_review_policy::policy(&original.invocation.instance)?
            != super::native_review_policy::policy(&effect.invocation.instance)?
        || !matches!(&original.observation,Some(EffectObservation{stopped:true,outcome:EffectOutcome::Completed{
            result:NeutralResult::Completed{outputs,..}
        }}) if outputs.get("report")==Some(value))
    {
        return Err("exhausted role marker differs from original observation".into());
    }
    let source = super::native_agent::source(&original.invocation)?;
    let state: crate::workflows::correction::CorrectionState =
        serde_json::from_value(effect.invocation.inputs["state"].clone())
            .map_err(|e| e.to_string())?;
    if effect.invocation.inputs["context"]["origin"].as_str() != Some(source.origin.as_str())
        || source.commit != state.artifact.head
        || source.tree != state.artifact.material_digest
        || source.generation != state.artifact.generation.to_string()
    {
        return Err("exhausted role source is stale".into());
    }
    Ok(true)
}
