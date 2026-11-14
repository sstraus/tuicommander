//! One local external activity, using the existing owned-process boundary.
//! The enclosing installed host supplies current authority and workspace binding.
// DEFERRED (2026-11-11) — production host installation is owned by 008/015;
// this adapter does not install itself or grant script execution authority.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "007 adapter before host installation")
)]
mod contract;
mod descriptor;
pub(crate) use descriptor::script_descriptor;
#[cfg(test)]
mod descriptor_fixture_tests;
#[cfg(test)]
mod environment_tests;
#[cfg(test)]
mod fixtures;
mod process;
#[cfg(test)]
mod tests;

use super::neutral_effects::{EffectObservation, EffectOutcome, EffectPreparation, NeutralEffect};
use super::neutral_execution::NeutralResult;
use super::neutral_runtime::NeutralInvocation;
use crate::workflows::catalog::{HandlerRef, ValueType};
use crate::workflows::data_reference::DataReference;
use crate::workflows::neutral_admission::{HostAdmission, HostRegistry, ReusableHandlerKind};
pub(crate) use contract::{OutputMode, Reconciliation, ScriptPolicy, ScriptRequest, ScriptResult};
use std::path::PathBuf;

pub(crate) fn handler() -> HandlerRef {
    HandlerRef {
        name: "host.script".into(),
        version: 1,
    }
}

/// Registration is exact and host-owned. The supplied admission adapter must
/// check actual current execution authority; schema validation is not a grant.
pub(crate) fn register<'a>(
    registry: &mut HostRegistry<'a>,
    admission: &'a impl HostAdmission,
) -> Result<(), String> {
    registry.register_reusable_handler(handler(), ReusableHandlerKind::Script, admission)
}

#[derive(Clone)]
pub(crate) struct ScriptAdapter {
    pub runs_db: PathBuf,
    pub input_directory: PathBuf,
}

impl ScriptAdapter {
    pub fn prepare(invocation: &NeutralInvocation) -> Result<EffectPreparation, String> {
        let (policy, _) = ScriptPolicy::resolve(&invocation.descriptor, &invocation.instance)?;
        invocation
            .descriptor
            .inputs
            .resolve(&invocation.inputs, "script inputs")?;
        if let Some(schema) = &invocation.descriptor.subject {
            schema.resolve(invocation.subject.as_ref())?;
        } else if invocation.subject.is_some() {
            return Err("script subject is undeclared".into());
        }
        Ok(EffectPreparation {
            mutation: policy.mutation,
            resources: policy.resources,
            approval: None,
            model_tools: None,
        })
    }

    pub fn reconcile(&self, effect: &NeutralEffect) -> EffectObservation {
        // No call to invoke: missing output/handle is never permission to replay.
        let stopped = effect.observation.as_ref().is_some_and(|o| o.stopped);
        let policy =
            ScriptPolicy::resolve(&effect.invocation.descriptor, &effect.invocation.instance);
        let diagnostic = match policy {
            Ok((policy, _)) => {
                let procedure = match policy.reconciliation {
                    Reconciliation::Manual { procedure } => procedure,
                    Reconciliation::ReadOnly => "inspect the original read-only execution".into(),
                };
                format!(
                    "script output/effect unavailable; reconcile original {} using {procedure}; retry is never automatic",
                    effect.key.digest
                )
            }
            Err(reason) => reason,
        };
        EffectObservation {
            outcome: EffectOutcome::Uncertain { diagnostic },
            stopped,
        }
    }

    pub fn cancel(&self, effect: &NeutralEffect) -> EffectObservation {
        super::check::cancel_checks(&self.runs_db, &owner(effect));
        // Signalling is not stop proof or rollback. The original worker reports
        // its actual stop independently when execute_process returns.
        self.reconcile(effect)
    }
}

fn owner(effect: &NeutralEffect) -> String {
    format!("{}:script:{}", effect.key.fence.run, effect.key.digest)
}

pub(crate) struct ScriptObservation {
    pub effect: EffectObservation,
    pub result: Option<ScriptResult>,
    /// Redacted stderr is diagnostic-only and never parsed as result data.
    pub stderr: String,
}

fn failed(effect: &NeutralEffect, diagnostic: String, stopped: bool) -> ScriptObservation {
    ScriptObservation {
        effect: EffectObservation {
            outcome: if effect.preparation.mutation {
                EffectOutcome::Uncertain { diagnostic }
            } else {
                EffectOutcome::Completed {
                    result: NeutralResult::Failed { diagnostic },
                }
            },
            stopped,
        },
        result: None,
        stderr: String::new(),
    }
}

fn validate_result(
    effect: &NeutralEffect,
    policy: &ScriptPolicy,
    result: &ScriptResult,
) -> Result<NeutralResult, String> {
    if result.version != 1 || result.identity != effect.key {
        return Err(
            "script protocol: result version or activation/attempt identity mismatch".into(),
        );
    }
    let descriptor = &effect.invocation.descriptor;
    if !descriptor.outcomes.contains(&result.outcome)
        || result.artifacts.len() > policy.artifact_limit
    {
        return Err("script protocol: undeclared outcome or artifact limit exceeded".into());
    }
    let mut outputs = result.data.clone();
    for (name, reference) in &result.artifacts {
        if DataReference::parse(reference)?.value_type() != ValueType::ArtifactReference
            || outputs.insert(name.clone(), reference.clone()).is_some()
        {
            return Err("script protocol: invalid or duplicate artifact output".into());
        }
    }
    let outputs = descriptor.outputs.resolve(&outputs, "script outputs")?;
    Ok(NeutralResult::Completed {
        outcome: result.outcome.clone(),
        outputs,
    })
}
