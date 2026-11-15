//! Issue-tail graph disposition is separate from the original publication result.
//! An unknown effect is never completed, retried or stripped of its resources.
use super::native_git::NativeFailure;
use super::native_host::NativeBinding;
use super::native_issue_source::IssueSource;
use super::neutral_effects::{EffectKey, EffectObservation, EffectOutcome, NeutralEffect};
use super::neutral_execution::{NeutralExecution, NeutralResult};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) const HANDLER: &str = "native.issue.publication";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub source: IssueSource,
    pub publication: super::native_github::GitHubRequest,
}
impl Request {
    pub(super) fn validate(&self) -> Result<(), String> {
        self.source.validate()?;
        super::native_github::validate(&self.publication)?;
        if self.publication.account != self.source.account
            || self.publication.repository != self.source.repository
            || !matches!(
                self.publication.action,
                super::native_github::GitHubAction::PushBranch { .. }
                    | super::native_github::GitHubAction::OpenPr { .. }
                    | super::native_github::GitHubAction::MarkReady { .. }
            )
        {
            return Err(
                "issue publication must be an exact source-bound push, draft PR or mark-ready"
                    .into(),
            );
        }
        Ok(())
    }
}
pub(super) async fn publish(
    bound: &NativeBinding,
    key: &EffectKey,
    request: &Request,
) -> Result<Value, NativeFailure> {
    request.validate()?;
    request.source.require(bound, key)?;
    let receipt =
        super::native_github::publish_owned(bound, key, &request.publication, HANDLER, request)
            .await?;
    request
        .source
        .require(bound, key)
        .map_err(NativeFailure::Uncertain)?;
    Ok(json!({"source":request.source,"effect":key,"publication":receipt}))
}

fn request(effect: &NeutralEffect) -> Result<Request, String> {
    if effect.invocation.descriptor.handler.name != HANDLER
        || effect.invocation.descriptor.handler.version != 1
    {
        return Err("original effect is not an issue publication".into());
    }
    let request: Request = serde_json::from_value(
        effect
            .invocation
            .inputs
            .get("request")
            .ok_or("issue publication request absent")?
            .clone(),
    )
    .map_err(|e| e.to_string())?;
    request.validate()?;
    Ok(request)
}
fn disposition(
    effect: &NeutralEffect,
    outcome: &str,
    diagnostic: &str,
) -> Result<NeutralResult, String> {
    let request = request(effect)?;
    Ok(NeutralResult::Completed {
        outcome: outcome.into(),
        outputs: BTreeMap::from([(
            "receipt".into(),
            json!({"source":request.source,"effect":effect.key,"disposition":outcome,"diagnostic":diagnostic}),
        )]),
    })
}

/// Only the graph callback changes shape; the retained Failed result is untouched.
pub(super) fn refusal(
    effect: &NeutralEffect,
    result: &NeutralResult,
) -> Result<Option<NeutralResult>, String> {
    if effect.invocation.descriptor.handler.name != HANDLER {
        return Ok(None);
    }
    match result {
        NeutralResult::Failed { diagnostic } => {
            disposition(effect, "refused", diagnostic).map(Some)
        }
        _ => Ok(None),
    }
}
/// Stop is required: a potentially live writer cannot run beside its status.
pub(super) fn uncertainty(
    effect: &NeutralEffect,
    observation: &EffectObservation,
) -> Result<Option<NeutralResult>, String> {
    if effect.invocation.descriptor.handler.name != HANDLER {
        return Ok(None);
    }
    match observation {
        EffectObservation {
            stopped: true,
            outcome: EffectOutcome::Uncertain { diagnostic },
        } => disposition(effect, "uncertain", diagnostic).map(Some),
        _ => Ok(None),
    }
}
fn failed(effect: &NeutralEffect) -> bool {
    effect.answer == Some(false)
        || matches!(
            &effect.observation,
            Some(EffectObservation {
                stopped: true,
                outcome: EffectOutcome::Uncertain { .. }
                    | EffectOutcome::Completed {
                        result: NeutralResult::Failed { .. }
                    }
            })
        )
}

/// An exact original effect, not a caller-supplied disposition receipt.
pub(super) fn status(
    snapshot: &NeutralExecution,
    key: &EffectKey,
    reference: &EffectKey,
    source: &IssueSource,
) -> Result<String, String> {
    if key.fence.run != reference.fence.run {
        return Err("issue publication belongs to another run".into());
    }
    let original = snapshot.effect(reference)?;
    let request = request(original)?;
    if &request.source != source || !failed(original) {
        return Err(
            "issue status needs this source's original stopped refusal or uncertainty".into(),
        );
    }
    let state = if original.answer == Some(false) {
        "refused"
    } else {
        match &original.observation {
            Some(EffectObservation {
                outcome: EffectOutcome::Uncertain { .. },
                ..
            }) => "uncertain",
            _ => "refused",
        }
    };
    Ok(format!(
        "TUICommander issue publication {} is {state}. Check its retained evidence before acting.",
        reference.digest
    ))
}

fn status_reference<'a>(effect: &'a NeutralEffect, kind: &str) -> Option<(&'a Value, Value)> {
    if effect.invocation.descriptor.handler.name != "native.issue.effect" {
        return None;
    }
    let request = effect.invocation.inputs.get("request")?;
    let action = request.get("action")?;
    if action.get("type")?.as_str()? != kind {
        return None;
    }
    let field = if kind == "publication_status" {
        "publication"
    } else {
        "child"
    };
    Some((action.get(field)?, request.get("source")?.clone()))
}

/// All mutation dispatches pass this gate, including nested model tools. No
/// persistent flag is needed: the original effect ledger is the only state.
pub(super) fn admit_mutation(
    snapshot: &NeutralExecution,
    effect: &NeutralEffect,
) -> Result<(), String> {
    if !effect.preparation.mutation {
        return Ok(());
    }
    for original in snapshot.effects.values().filter(|e| {
        e.invocation.descriptor.handler.name == HANDLER
            && (failed(e)
                || matches!(
                    e.observation.as_ref().map(|o| &o.outcome),
                    Some(EffectOutcome::Uncertain { .. })
                ))
    }) {
        if !failed(original) {
            return Err(
                "issue publication may still be running; no follow-on write admitted".into(),
            );
        }
        let Some((reference, source)) = status_reference(effect, "publication_status") else {
            return Err(
                "issue publication stopped; only its exact source-bound status may write".into(),
            );
        };
        if reference != &serde_json::to_value(&original.key).map_err(|e| e.to_string())?
            || source
                != serde_json::to_value(request(original)?.source).map_err(|e| e.to_string())?
        {
            return Err("issue status differs from the original publication/source".into());
        }
        if snapshot.effects.values().any(|previous| {
            previous.key != effect.key
                && previous.dispatched
                && status_reference(previous, "publication_status")
                    .is_some_and(|(r, _)| r == reference)
        }) {
            return Err(
                "original issue publication status was already attempted; no write repeated".into(),
            );
        }
    }
    for original in snapshot.effects.values().filter(|e| {
        e.invocation.descriptor.handler.name == super::native_issue_graph::HANDLER && e.settled()
    }) {
        let Some(EffectObservation {
            stopped: true,
            outcome:
                EffectOutcome::Completed {
                    result: NeutralResult::Completed { outputs, .. },
                },
        }) = &original.observation
        else {
            continue;
        };
        let receipt: super::native_issue_graph::Receipt = serde_json::from_value(
            outputs
                .get("receipt")
                .ok_or("issue graph receipt absent")?
                .clone(),
        )
        .map_err(|e| e.to_string())?;
        if receipt.child.status == super::native_issue_service::IssueChildStatus::Completed {
            continue;
        }
        let Some((reference, source)) = status_reference(effect, "status_comment") else {
            return Err("issue child stopped; only its exact source-bound status may write".into());
        };
        if reference != &json!(original.key)
            || source != json!(receipt.child.source)
            || receipt.effect != original.key
        {
            return Err("issue status differs from the original child/source".into());
        }
        if snapshot.effects.values().any(|previous| {
            previous.key != effect.key
                && previous.dispatched
                && status_reference(previous, "status_comment").is_some_and(|(r, _)| r == reference)
        }) {
            return Err(
                "original issue child status was already attempted; no write repeated".into(),
            );
        }
    }
    Ok(())
}

/// Share only this run's exact stopped-unknown publication workspace claim.
/// The unknown owner is retained; other runs and all other writes still conflict.
pub(super) fn shares_unknown(
    snapshot: &NeutralExecution,
    effect: &NeutralEffect,
    owner_run: &str,
    owner_effect: &str,
) -> bool {
    if owner_run != effect.key.fence.run || admit_mutation(snapshot, effect).is_err() {
        return false;
    }
    let Some((reference, source)) = status_reference(effect, "publication_status") else {
        return false;
    };
    let Some(original) = snapshot.effects.get(owner_effect) else {
        return false;
    };
    original.key.fence.run == owner_run
        && reference == &json!(original.key)
        && matches!(
            &original.observation,
            Some(EffectObservation {
                stopped: true,
                outcome: EffectOutcome::Uncertain { .. }
            })
        )
        && request(original).is_ok_and(|request| json!(request.source) == source)
}
