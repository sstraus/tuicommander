//! Project a service assessment into the ordinary selected-body contract.
use super::native_host::NativeBinding;
use super::neutral_effects::{EffectObservation, EffectOutcome, NeutralEffect};
use super::neutral_execution::NeutralResult;
use crate::workflows::correction::CorrectionEvaluation;
use serde_json::json;
use std::collections::BTreeMap;

pub(super) fn invoke(bound: &NativeBinding, effect: &NeutralEffect) -> EffectObservation {
    let outcome = match super::correction_native::assess(&bound.context, &bound.store, effect) {
        Ok(CorrectionEvaluation::Ready { state, repeat }) => {
            let mut outputs = BTreeMap::from([("state".into(), json!(state))]);
            if effect.invocation.descriptor.handler.name == super::correction_policy::HANDLER {
                let accounting = (|| {
                    let policy = super::native_review_policy::policy(&effect.invocation.instance)?;
                    let run = bound.store.neutral_execution(&effect.key.fence.run)?;
                    super::native_review_policy::accounting(&bound.store, &run, &policy)
                })();
                match accounting {
                    Ok(value) => {
                        outputs.insert("accounting".into(), value);
                    }
                    Err(diagnostic) => {
                        return EffectObservation {
                            stopped: true,
                            outcome: EffectOutcome::Completed {
                                result: NeutralResult::Failed { diagnostic },
                            },
                        };
                    }
                }
            }
            if let Some(repeat) = repeat {
                outputs.insert("repeat".into(), json!(repeat));
            }
            EffectOutcome::Completed {
                result: NeutralResult::Completed {
                    outcome: "assessed".into(),
                    outputs,
                },
            }
        }
        Ok(CorrectionEvaluation::Reconcile { effect, reason }) => EffectOutcome::Uncertain {
            diagnostic: format!("reconcile {effect}: {reason}"),
        },
        Ok(CorrectionEvaluation::Waiting { reason }) | Err(reason) => EffectOutcome::Completed {
            result: NeutralResult::Failed { diagnostic: reason },
        },
    };
    EffectObservation {
        outcome,
        stopped: true,
    }
}
