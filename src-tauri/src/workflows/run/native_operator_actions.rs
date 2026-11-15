//! Compose existing story authorities; readiness is never consent.
use super::native_git::NativeFailure;
use super::native_host::NativeBinding;
use super::native_operator::{AnswerReceipt, ClosureProof, OperatorKind, OperatorRequest};
use super::neutral_effects::{EffectOutcome, NeutralEffect};
use super::neutral_execution::NeutralResult;
use crate::stories::StoryCommand;
use serde_json::{Value, json};

fn closure_floor(
    bound: &NativeBinding,
    effect: &NeutralEffect,
    r: &OperatorRequest,
    p: &ClosureProof,
) -> Result<(), String> {
    let run = bound.store.neutral_execution(&effect.key.fence.run)?;
    let latest_review = run
        .effects
        .values()
        .filter(|e| {
            matches!(
                e.invocation.descriptor.handler.name.as_str(),
                "native.review.change" | "native.review.bounded"
            ) && e.invocation.inputs.get("request").is_some_and(|v| {
                v["base"] == p.base
                    && v["head"] == r.artifact.commit
                    && v["tree"] == r.artifact.tree
            })
        })
        .max_by_key(|e| (e.key.fence.activation, e.key.fence.attempt))
        .ok_or("automatic closure requires current stopped independent review")?;
    let review = super::observed_review(
        &bound.context,
        &bound.store,
        &latest_review.key,
        &p.base,
        &r.artifact.commit,
        &r.artifact.tree,
    )?;
    if review.verdict != crate::workflows::review_verdict::CodeVerdict::Pass {
        return Err("automatic closure requires current independent passing review".into());
    }
    let mut checks = std::collections::BTreeSet::new();
    let latest = run
        .effects
        .values()
        .filter(|e| {
            e.invocation.descriptor.handler.name == "native.verification.run"
                && e.invocation.inputs.get("request").is_some_and(|v| {
                    v["commit"] == r.artifact.commit && v["tree"] == r.artifact.tree
                })
        })
        .fold(std::collections::BTreeMap::new(), |mut map, e| {
            if let Some(name) = e.invocation.inputs["request"]["check"].as_str()
                && map.get(name).is_none_or(|prior: &&NeutralEffect| {
                    prior.key.fence.activation < e.key.fence.activation
                })
            {
                map.insert(name, e);
            }
            map
        });
    for e in latest.values() {
        if e.invocation.descriptor.handler.version != 1 || !e.settled() || e.cancel_started {
            return Err("closure check requires stopped current native verification".into());
        }
        let Some(super::neutral_effects::EffectObservation {
            outcome:
                EffectOutcome::Completed {
                    result: NeutralResult::Completed { outcome, outputs },
                },
            stopped: true,
        }) = &e.observation
        else {
            return Err("closure check has no completed receipt".into());
        };
        let receipt: super::check::CheckReceipt = serde_json::from_value(
            outputs
                .get("receipt")
                .ok_or("closure check receipt absent")?
                .clone(),
        )
        .map_err(|e| e.to_string())?;
        if outcome != "ok"
            || receipt.exit_code != 0
            || receipt.commit != r.artifact.commit
            || receipt.tree != r.artifact.tree
            || !bound.context.matches_check(&receipt)
            || !checks.insert(receipt.check_id.clone())
            || e.invocation.inputs.get("request")
                != Some(
                    &json!({"check":receipt.check_id,"commit":receipt.commit,"tree":receipt.tree}),
                )
        {
            return Err(
                "closure check is stale, duplicated or differs from host definition".into(),
            );
        }
    }
    if checks
        != bound
            .context
            .verification_check_names()
            .into_iter()
            .collect()
    {
        return Err("automatic closure requires every configured verification check".into());
    }
    Ok(())
}
pub(super) fn apply(
    bound: &NativeBinding,
    effect: &NeutralEffect,
    r: &OperatorRequest,
    answer: Option<&AnswerReceipt>,
) -> Result<Value, NativeFailure> {
    bound.context.current(if answer.is_none() {
        Some(crate::agent_capabilities::Capability::Operator)
    } else {
        None
    })?;
    match &r.decision {
        OperatorKind::Resume => Ok(json!({"continuation":r.resume_to})),
        OperatorKind::HumanClosure { story, revision }
        | OperatorKind::AutomaticClosure {
            story, revision, ..
        } => {
            let _ = (story, revision);
            let backend = crate::stories::story_backend_for(
                &bound.context.workspace_path().to_string_lossy(),
            )?;
            close_story(bound, effect, r, backend.as_ref())
        }
    }
}

/// The selected backend owns transitions, revision checks and self-review rules.
pub(super) fn close_story(
    bound: &NativeBinding,
    effect: &NeutralEffect,
    r: &OperatorRequest,
    backend: &dyn crate::stories::StoryBackend,
) -> Result<Value, NativeFailure> {
    super::native_operator::subject(bound, effect, r)?;
    if !effect.dispatched {
        return Err("closure was not dispatched".to_owned().into());
    }
    let (story, revision, actor) = match &r.decision {
        OperatorKind::AutomaticClosure {
            story,
            revision,
            proof,
        } => {
            bound
                .context
                .current(Some(crate::agent_capabilities::Capability::Operator))?;
            closure_floor(bound, effect, r, proof)?;
            (story, revision, bound.context.actor())
        }
        OperatorKind::HumanClosure { story, revision } => {
            if !super::native_operator::receipt(effect, r)?.answer.accept {
                return Err("closure consent declined".to_owned().into());
            }
            (story, revision, None)
        }
        _ => return Err("typed closure decision required".to_owned().into()),
    };
    let current = backend.require_unarchived_story(story)?;
    let plan = backend.require_unarchived_plan(&current.plan_id)?;
    if current.revision != *revision
        || plan.project != bound.context.workspace_path().to_string_lossy()
    {
        return Err("closure story subject/revision is stale".to_owned().into());
    }
    let closed = backend
        .transition_for_actor(story, *revision, StoryCommand::Approve, actor)
        .map_err(NativeFailure::Uncertain)?;
    serde_json::to_value(closed).map_err(|e| NativeFailure::Uncertain(e.to_string()))
}
