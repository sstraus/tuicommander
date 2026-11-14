//! Read-only issue receipt composition. Graph execution owns the child, not this adapter.
use super::native_host::NativeBinding;
use super::native_issue_service::{IssueChildReceipt, IssueChildStatus};
use super::native_issue_source::IssueSource;
use super::neutral_effects::{EffectKey, EffectOutcome};
use super::neutral_execution::{NeutralExecution, NeutralResult};
use crate::stories::StoryBackend;
use crate::workflows::neutral_publication::PublicationRef;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(super) const HANDLER: &str = "native.issue.graph.receipt";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub source: IssueSource,
    pub preparation: EffectKey,
    pub create: EffectKey,
    pub publication: PublicationRef,
    pub cohort: super::native_cohort::Cohort,
    pub observation: EffectKey,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub effect: EffectKey,
    pub child: IssueChildReceipt,
}

fn receipt(
    run: &NeutralExecution,
    key: &EffectKey,
    reference: &EffectKey,
    handler: &str,
) -> Result<Value, String> {
    if key.fence.run != reference.fence.run {
        return Err("issue graph evidence belongs to another run".into());
    }
    let effect = run.effect(reference)?;
    if effect.invocation.descriptor.handler.name != handler || !effect.settled() {
        return Err("issue graph needs exact stopped native evidence".into());
    }
    let Some(super::neutral_effects::EffectObservation {
        stopped: true,
        outcome:
            EffectOutcome::Completed {
                result: NeutralResult::Completed { outputs, .. },
            },
    }) = &effect.observation
    else {
        return Err("issue graph evidence did not complete".into());
    };
    outputs
        .get("receipt")
        .cloned()
        .ok_or("issue graph receipt absent".into())
}

pub(super) fn observe(
    bound: &NativeBinding,
    key: &EffectKey,
    request: &Request,
    backend: &dyn StoryBackend,
) -> Result<Receipt, String> {
    super::native_issue_triage::owned(bound, key, HANDLER, request)?;
    Ok(Receipt {
        effect: key.clone(),
        child: compose(bound, key, request, backend)?,
    })
}
/// Also used by the status/evidence policy: re-prove original graph evidence,
/// never trust the serialized IssueChildReceipt that the caller supplied.
pub(super) fn compose(
    bound: &NativeBinding,
    key: &EffectKey,
    request: &Request,
    backend: &dyn StoryBackend,
) -> Result<IssueChildReceipt, String> {
    request.source.require(bound, key)?;
    request.cohort.validate()?;
    let run = bound.store.neutral_execution(&key.fence.run)?;
    let prepared: super::native_issue_prepare::Receipt = serde_json::from_value(receipt(
        &run,
        key,
        &request.preparation,
        super::native_issue_prepare::HANDLER,
    )?)
    .map_err(|e| e.to_string())?;
    if prepared.effect != request.preparation || prepared.source != request.source {
        return Err("issue graph preparation differs from original source/effect".into());
    }
    let triage =
        super::native_issue_triage::require_triage(bound, key, &prepared.triage, &request.source)?;
    if triage.assessment.decision.outcome
        != crate::workflows::review_verdict::TriageOutcome::Accepted
    {
        return Err("issue graph triage was not accepted".into());
    }
    let created: super::native_story_create::CreateReceipt =
        serde_json::from_value(receipt(&run, key, &request.create, "native.story.create")?)
            .map_err(|e| e.to_string())?;
    if created.effect != request.create
        || serde_json::to_value(&created.source).map_err(|e| e.to_string())?
            != serde_json::to_value(&prepared.create).map_err(|e| e.to_string())?
        || run
            .effect(&request.create)?
            .invocation
            .inputs
            .get("request")
            != Some(&serde_json::to_value(&prepared.create).map_err(|e| e.to_string())?)
        || created.story.plan_id != prepared.plan.id
        || created.story.problem != prepared.create.problem
        || created.story.origin != prepared.create.story.origin
    {
        return Err("issue graph story differs from its exact prepared proposal".into());
    }
    let cohort = &request.cohort;
    if cohort.plan != prepared.plan.id
        || cohort.plan_revision != prepared.plan.revision
        || cohort.source != prepared.plan.source
        || cohort.stories
            != vec![super::native_cohort::StoryPin {
                id: created.story.id.clone(),
                revision: created.story.revision,
            }]
    {
        return Err("issue graph cohort differs from original created story".into());
    }
    let exact_cohort = serde_json::to_value(cohort).map_err(|e| e.to_string())?;
    if !run.effects.values().any(|e| {
        e.invocation.descriptor.handler.name == "native.story.cohort.created" && e.settled()
            && e.invocation.inputs.get("request") == Some(&serde_json::json!({"plan":prepared.plan.id,"review":prepared.create.review}))
            && matches!(&e.observation, Some(super::neutral_effects::EffectObservation { stopped: true, outcome: EffectOutcome::Completed { result: NeutralResult::Completed { outputs, .. } } }) if outputs.get("receipt").and_then(|r| r.get("cohort")) == Some(&exact_cohort))
    }) { return Err("issue graph lacks original created-cohort observation".into()); }
    let observed = receipt(
        &run,
        key,
        &request.observation,
        "native.story.cohort.observe",
    )?;
    let effect = run.effect(&request.observation)?;
    let child = run
        .run
        .closure
        .children
        .iter()
        .find(|p| {
            p.project == request.publication.project
                && p.id == request.publication.id
                && p.revision == request.publication.revision
        })
        .ok_or("issue child absent from retained publication closure")?;
    if effect.invocation.publication != *child
        || effect.invocation.inputs.get("request") != Some(&exact_cohort)
        || observed.get("cohort") != Some(&exact_cohort)
        || observed.get("owner")
            != Some(&serde_json::to_value(&request.observation).map_err(|e| e.to_string())?)
    {
        return Err("issue graph observation differs from the exact pinned child/cohort".into());
    }
    let (invocation, returned, outputs) = bound
        .store
        .neutral_returned_subflow(&key.fence.run, &effect.key.fence.execution)?;
    if invocation.fence.execution != key.fence.execution
        || invocation.descriptor.handler.name != super::native_cohort::HANDLER
        || invocation.inputs.get("cohort") != Some(&exact_cohort)
        || outputs.get("receipt") != Some(&observed)
    {
        return Err("issue graph lacks the exact returned story-cohort child".into());
    }
    let Some(crate::workflows::catalog::ReusableTarget::Subflow { publication }) =
        &invocation.descriptor.target
    else {
        return Err("issue graph child target is not pinned".into());
    };
    if publication.reference != request.publication
        || !matches!(returned.as_str(), "completed" | "blocked")
    {
        return Err("issue graph returned a different publication/outcome".into());
    }
    let stories = cohort.current(&bound.context, backend, false)?;
    if observed.get("stories") != Some(&serde_json::to_value(&stories).map_err(|e| e.to_string())?)
    {
        return Err("issue graph story state changed after child observation".into());
    }
    let completed = stories
        .iter()
        .all(|s| s.status == crate::stories::StoryStatus::Done);
    let outcome = if completed { "completed" } else { "blocked" };
    if returned != outcome || observed.get("outcome").and_then(Value::as_str) != Some(outcome) {
        return Err("issue graph completion differs from current native story state".into());
    }
    Ok(IssueChildReceipt {
        source: request.source.clone(),
        triage: prepared.triage,
        publication: request.publication.clone(),
        child: effect.key.fence.execution.clone(),
        status: if completed {
            IssueChildStatus::Completed
        } else {
            IssueChildStatus::Failed
        },
        evidence: if completed {
            Some(format!(
                "Story {} completed under pinned publication {} revision {}.",
                created.story.id, child.id, child.revision
            ))
        } else {
            None
        },
    })
}
