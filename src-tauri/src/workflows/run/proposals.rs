//! Typed, activation-bound story input. Legacy serial runs keep their original author path.
use super::{RunSnapshot, RunStatus, StoryProposalBatch};
use crate::workflows::{NodeKind, WorkflowKind};
use sha2::{Digest, Sha256};

pub(super) fn proposal_key(batch: &StoryProposalBatch, index: usize) -> String {
    // Scope to a graph visit, not a mutable author name or checkout spelling.
    hex::encode(Sha256::digest(
        serde_json::to_vec(&(&batch.execution_id, &batch.activation_id, index))
            .expect("proposal identity contains only strings and an index"),
    ))
}

pub(super) fn validate_batch(run: &RunSnapshot, batch: &StoryProposalBatch) -> Result<(), String> {
    if run.event_contract_version < 3 || run.status != RunStatus::Running {
        return Err("typed story input requires a running contract-3 graph".into());
    }
    let graph = run
        .graph_executions
        .iter()
        .find(|g| g.id == batch.execution_id)
        .ok_or("story proposal graph missing")?;
    if graph.definition.kind != WorkflowKind::Plan || graph.target_id != run.plan_id {
        return Err("story proposal input requires a plan graph".into());
    }
    let activation = graph
        .activations
        .iter()
        .find(|a| a.id == batch.activation_id)
        .ok_or("story proposal activation missing")?;
    if activation.state != super::graph::ActivationState::Running
        || !graph
            .definition
            .graph
            .nodes
            .iter()
            .any(|n| n.id == activation.node_id && matches!(n.kind, NodeKind::CreateStories))
    {
        return Err("story proposal input requires a reached Create Stories activation".into());
    }
    if batch.plan_revision < 1
        || batch.proposals.len() > 100
        || batch.proposals.len() + usize::from(run.story_creations)
            > usize::from(run.limits.max_story_creations)
        || serde_json::to_vec(batch).map_err(|e| e.to_string())?.len() > 256_000
        || run
            .proposal_batches
            .iter()
            .any(|b| b.execution_id == batch.execution_id && b.activation_id == batch.activation_id)
    {
        return Err("story proposal input is duplicate or outside bounds".into());
    }
    for proposal in &batch.proposals {
        if proposal.plan_id != run.plan_id
            || !matches!(&proposal.origin, crate::stories::StoryOrigin::PlanStep { step } if !step.trim().is_empty())
        {
            return Err("story proposal requires this plan and a plan-step origin".into());
        }
        crate::stories::validate_workflow_proposal(proposal)?;
    }
    Ok(())
}

pub(super) fn require_creation_authority(
    run: &RunSnapshot,
    caller: Option<&str>,
    batch: Option<&StoryProposalBatch>,
) -> Result<(), String> {
    if let Some(batch) = batch {
        if run.event_contract_version < 3 || !run.proposal_batches.contains(batch) {
            return Err("story proposal input was not committed in this run".into());
        }
        let graph = run
            .graph_executions
            .iter()
            .find(|g| g.id == batch.execution_id)
            .ok_or("story proposal graph missing")?;
        if !graph.activations.iter().any(|a| {
            a.id == batch.activation_id && a.state == super::graph::ActivationState::Running
        }) {
            return Err("story proposal visit is no longer active".into());
        }
        let plan = crate::stories::StoryStore::open()?.get_plan(&run.plan_id)?;
        if plan.revision != batch.plan_revision {
            return Err("story proposal input has a stale plan revision".into());
        }
        return Ok(());
    }
    if run.attempts.iter().any(|attempt| {
        attempt.story_id == run.plan_id
            && attempt.state == super::AttemptState::Running
            && attempt
                .agent
                .as_ref()
                .is_some_and(|a| Some(a.session_id.as_str()) == caller)
    }) {
        return Ok(());
    }
    Err("only the active run author may create plan stories".into())
}

pub(super) fn batch_for<'a>(
    run: &'a RunSnapshot,
    graph: &super::graph::GraphExecution,
    activation: &super::graph::Activation,
) -> Option<&'a StoryProposalBatch> {
    run.proposal_batches
        .iter()
        .find(|b| b.execution_id == graph.id && b.activation_id == activation.id)
}

pub(super) fn complete(run: &RunSnapshot, batch: &StoryProposalBatch) -> Result<bool, String> {
    let stories = crate::stories::StoryStore::open()?;
    for (index, input) in batch.proposals.iter().enumerate() {
        let proposal_key = proposal_key(batch, index);
        let effect_key = format!("create-story:{proposal_key}");
        if !run.effects.iter().any(|e| {
            e.key == effect_key
                && e.kind == super::EffectKind::CreateStory
                && e.state == super::EffectState::Succeeded
        }) {
            return Ok(false);
        }
        if stories
            .existing_story_for_proposal(&run.id, &proposal_key, input)?
            .is_none()
        {
            return Err("completed story proposal has no native story receipt".into());
        }
    }
    Ok(true)
}

pub(super) fn drive(
    store: &super::RunStore,
    run: &RunSnapshot,
    batch: &StoryProposalBatch,
) -> Result<bool, String> {
    for (index, _) in batch.proposals.iter().enumerate() {
        let key = format!("create-story:{}", proposal_key(batch, index));
        if run
            .effects
            .iter()
            .any(|e| e.key == key && e.state == super::EffectState::Succeeded)
        {
            continue;
        }
        store.create_story_from_proposal(&run.id, batch, index)?;
        return Ok(true);
    }
    Ok(false)
}
