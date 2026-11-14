//! Story transitions stay in the selected native backend and its revision rules.
use super::RunStore;
use super::native_git::NativeFailure;
use super::native_operations::NativeContext;
use super::neutral_effects::EffectKey;
use crate::stories::{Story, StoryBackend, StoryCommand};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct StoryRequest {
    pub story: String,
    pub revision: i64,
    pub command: StoryCommand,
}

/// Run-covered transitions (decisions A(a) and 2(i), 2026-11-12, recorded for Boss):
/// StartManual of a story this run created and admitted to its batch slot, and any
/// later step only after a passing verdict. Everything else asks.
pub(super) fn transition_covered(
    context: &NativeContext,
    ledger: &RunStore,
    run: &str,
    request: &StoryRequest,
) -> bool {
    // Decision (a), 2026-11-12: Reopen only the story this run armed for FIX re-entry.
    if request.command == crate::stories::StoryCommand::Reopen {
        return super::native_batch::reentry_open(ledger, run, &request.story);
    }
    if request.command == crate::stories::StoryCommand::StartManual {
        return ledger.neutral_execution(run).is_ok_and(|snapshot| {
            super::native_cohort::created_stories(&snapshot)
                .iter()
                .any(|c| c.id == request.story)
        }) && super::native_batch::admitted(ledger, run, &request.story);
    }
    verdict_covers(context, ledger, run, &request.story)
}

/// Coordinator decision 2(i), 2026-11-12, recorded for Boss: a story transition asks
/// no question only when this run holds a settled policy assessment for exactly that
/// story (context origin) that passed (explicit `repeat: false`, the region's success
/// route) on the committed artifact now at the clean HEAD. Any other transition keeps its own exact approval.
pub(super) fn verdict_covers(
    context: &NativeContext,
    ledger: &RunStore,
    run: &str,
    story: &str,
) -> bool {
    let Ok((head, tree)) = super::check::clean_artifact(context.workspace_path()) else {
        return false;
    };
    let Ok(snapshot) = ledger.neutral_execution(run) else {
        return false;
    };
    snapshot.effects.values().any(|e| {
        let Some(super::neutral_effects::EffectObservation {
            stopped: true,
            outcome:
                super::neutral_effects::EffectOutcome::Completed {
                    result: super::neutral_execution::NeutralResult::Completed { outputs, .. },
                },
        }) = &e.observation
        else {
            return false;
        };
        let state = &outputs.get("state").cloned().unwrap_or_default();
        e.invocation.descriptor.handler.name == super::correction_policy::HANDLER
            && e.invocation.inputs["context"]["origin"].as_str() == Some(story)
            && outputs.get("repeat") == Some(&serde_json::Value::Bool(false))
            && state["artifact"]["head"].as_str() == Some(head.as_str())
            && state["artifact"]["material_digest"].as_str() == Some(tree.as_str())
    })
}

pub(super) fn transition(
    context: &NativeContext,
    ledger: &RunStore,
    key: &EffectKey,
    backend: &dyn StoryBackend,
    request: &StoryRequest,
) -> Result<Story, NativeFailure> {
    let prepared = ledger
        .neutral_execution(&key.fence.run)?
        .effect(key)?
        .preparation
        .clone();
    if prepared.approval.is_none() {
        // Run verdict coverage is re-proved at execution, never trusted from preparation.
        if !transition_covered(context, ledger, &key.fence.run, request) {
            return Err(NativeFailure::Refused(
                "native story transition is not covered by this run; it needs its own approval"
                    .into(),
            ));
        }
        context.authorize_covered(ledger, key, "native.story.transition", request)?;
    } else {
        context.authorize(ledger, key, "native.story.transition", request, None)?;
    }
    let story = backend.require_unarchived_story(&request.story)?;
    let plan = backend.require_unarchived_plan(&story.plan_id)?;
    if story.revision != request.revision
        || plan.project != context.workspace_path().to_string_lossy()
    {
        return Err(NativeFailure::Refused(
            "native story subject/revision is stale".into(),
        ));
    }
    if request.command == crate::stories::StoryCommand::Reopen {
        // Only this run's armed FIX re-entry, recorded as the owning workflow actor.
        if !super::native_batch::reentry_open(ledger, &key.fence.run, &request.story) {
            return Err(NativeFailure::Refused(
                "native Reopen needs a FIX re-entry armed by this run for that story".into(),
            ));
        }
        return backend
            .reopen_for_workflow(&request.story, request.revision, &key.fence.run)
            .map_err(NativeFailure::Uncertain);
    }
    // Actor is retained backend provenance. Never use None (human authority) for
    // a managed caller; the native backend enforces claims and self-review rules.
    backend
        .transition_for_actor(
            &request.story,
            request.revision,
            request.command.clone(),
            context.actor(),
        )
        .map_err(NativeFailure::Uncertain)
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadRequest {
    pub story: String,
}

/// native.story.read (decision G2, 2026-11-12): a read of one story of this run's
/// project, so a graph can check each criterion. No write, no claim.
pub(super) fn read(
    context: &NativeContext,
    backend: &dyn StoryBackend,
    request: &ReadRequest,
) -> Result<serde_json::Value, String> {
    context.current(None)?;
    let story = backend.require_unarchived_story(&request.story)?;
    let plan = backend.require_unarchived_plan(&story.plan_id)?;
    if plan.project != context.workspace_path().to_string_lossy() {
        return Err("native story read is outside this run's project".into());
    }
    // One item per criterion, so a ForEach body can check it by index.
    let items: Vec<_> = (0..story.criteria.len())
        .map(|criterion| serde_json::json!({"story": story.id, "criterion": criterion}))
        .collect();
    Ok(
        serde_json::json!({"story": story.id, "revision": story.revision,
        "criteria": story.criteria.len(), "status": story.status, "items": items}),
    )
}
