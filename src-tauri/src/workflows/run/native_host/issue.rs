//! Installed issue-domain service composition uses the ordinary effect ledger.
use super::super::native_issue_service::{IssueChildRequest, IssueServices};
use super::super::neutral_effects::{EffectObservation, EffectOutcome};
use super::super::neutral_execution::NeutralResult;
use super::*;
use std::sync::Arc;

impl NativeHost {
    /// Backend owner entry. Absence is a visible prerequisite, never a launcher fallback.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "tests substitute a scripted owner for the installed production owner"
        )
    )]
    pub(in crate::workflows::run) fn install_issue_services(
        &self,
        services: Arc<dyn IssueServices>,
    ) {
        *self.issue_services.write() = Some(services);
    }
    #[cfg(test)]
    pub(in crate::workflows::run) fn issue_services(&self) -> Option<Arc<dyn IssueServices>> {
        self.issue_services.read().clone()
    }
    pub(super) fn issue_invoke(&self, effect: NeutralEffect) -> ObservationFuture {
        let binding = self.binding(&effect.invocation);
        let services = self.issue_services.read().clone();
        Box::pin(async move {
            let bound = binding?;
            let services =
                services.ok_or("scoped issue triage/child service owner is not installed")?;
            tokio::task::spawn_blocking(move || {
                let result = tokio::runtime::Handle::current().block_on(async {
                    match effect.invocation.descriptor.handler.name.as_str() {
                        "native.issue.triage" => {
                            let request =
                                serde_json::from_value(effect.invocation.inputs["request"].clone())
                                    .map_err(|e| e.to_string())?;
                            let receipt = super::super::native_issue_triage::triage(
                                &bound,
                                &effect.key,
                                &request,
                                services.as_ref(),
                            )
                            .await?;
                            serde_json::to_value(receipt).map_err(|e| {
                                super::super::native_git::NativeFailure::Uncertain(e.to_string())
                            })
                        }
                        "native.issue.handoff" => {
                            let request =
                                serde_json::from_value(effect.invocation.inputs["request"].clone())
                                    .map_err(|e| e.to_string())?;
                            let receipt = super::super::native_issue_handoff::handoff(
                                &bound,
                                &effect.key,
                                &request,
                                services.as_ref(),
                            )
                            .await?;
                            serde_json::to_value(receipt).map_err(|e| {
                                super::super::native_git::NativeFailure::Uncertain(e.to_string())
                            })
                        }
                        _ => Err(super::super::native_git::NativeFailure::Refused(
                            "unknown issue service operation".into(),
                        )),
                    }
                });
                Ok(observation(
                    result,
                    effect.invocation.descriptor.handler.name == "native.issue.handoff",
                ))
            })
            .await
            .map_err(|e| format!("issue worker ownership uncertain: {e}"))?
        })
    }
    pub(super) fn issue_observe(&self, effect: NeutralEffect, cancel: bool) -> ObservationFuture {
        let services = self.issue_services.read().clone();
        let binding = self.binding(&effect.invocation);
        Box::pin(async move {
            let bound = binding?;
            let run = bound.store.neutral_execution(&effect.key.fence.run)?;
            if run.effect(&effect.key)? != &effect || !effect.dispatched {
                return Err("original issue child effect ownership changed".into());
            }
            let services = services.ok_or("original issue child owner is unavailable")?;
            let request: IssueChildRequest =
                serde_json::from_value(effect.invocation.inputs["request"].clone())
                    .map_err(|e| e.to_string())?;
            let result = if cancel {
                services.cancel(&effect.key).await
            } else {
                services.observe(&effect.key).await
            };
            let result = result
                .and_then(|receipt| {
                    super::super::native_issue_handoff::require_child(&request, &receipt)?;
                    if effect
                        .handle
                        .as_deref()
                        .is_some_and(|child| child != receipt.child)
                    {
                        return Err("observed child differs from original retained handle".into());
                    }
                    serde_json::to_value(receipt).map_err(|e| e.to_string())
                })
                .map_err(super::super::native_git::NativeFailure::Uncertain);
            Ok(observation(result, true))
        })
    }
}
fn observation(
    result: Result<Value, super::super::native_git::NativeFailure>,
    child: bool,
) -> EffectObservation {
    use super::super::native_git::NativeFailure;
    let mut stopped = true;
    let outcome = match result {
        Ok(value) if child && value["status"] == "running" => {
            stopped = false;
            EffectOutcome::Pending {
                handle: value["child"].as_str().unwrap_or_default().into(),
            }
        }
        Ok(value) if child && value["status"] == "uncertain" => {
            stopped = false;
            EffectOutcome::Uncertain {
                diagnostic: "original issue child state uncertain".into(),
            }
        }
        Ok(value) => EffectOutcome::Completed {
            result: NeutralResult::Completed {
                outcome: "ok".into(),
                outputs: BTreeMap::from([("receipt".into(), value)]),
            },
        },
        Err(NativeFailure::Refused(diagnostic)) => EffectOutcome::Completed {
            result: NeutralResult::Failed { diagnostic },
        },
        Err(NativeFailure::Uncertain(diagnostic)) => {
            stopped = false;
            EffectOutcome::Uncertain { diagnostic }
        }
    };
    EffectObservation { outcome, stopped }
}
