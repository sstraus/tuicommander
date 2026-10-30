//! Checks precede approval; native transition history is the independent receipt.
use super::super::graph::{Activation, EdgeOutcome, GraphExecution};
use super::*;
use crate::stories::{StoryStatus, StoryStore};

pub(in crate::workflows::run) fn approved(
    run: &RunSnapshot,
    graph: &GraphExecution,
) -> Result<bool, String> {
    let story = StoryStore::open()?.get_story(&graph.target_id)?;
    let Some(execution) = run.stories.iter().find(|s| s.story_id == story.id) else {
        return Ok(false);
    };
    Ok(story.status == StoryStatus::Done
        && execution.accepted_revision == Some(story.revision)
        && execution.accepted
        && gate(run, graph)?.is_some_and(|d| d.0 == EdgeOutcome::Pass))
}

pub(in crate::workflows::run) fn gate(
    run: &RunSnapshot,
    graph: &GraphExecution,
) -> Result<Option<(EdgeOutcome, DecisionEvidence)>, String> {
    let Some(execution) = run.stories.iter().find(|s| s.story_id == graph.target_id) else {
        return Ok(None);
    };
    let Some(path) = execution.worktree_path.as_deref() else {
        return Ok(None);
    };
    let (commit, tree) = super::super::check::clean_artifact(Path::new(path))?;
    let requirements = graph
        .definition
        .graph
        .evidence_policy
        .as_ref()
        .map(|_| {
            StoryStore::open()?
                .get_story(&graph.target_id)
                .map(|story| super::super::evidence::requirements_digest(&story))
        })
        .transpose()?;
    if graph.definition.required_checks.is_empty() {
        return Err("Gate requires deterministic checks".into());
    }
    let receipts: Option<Vec<_>> = graph
        .definition
        .required_checks
        .iter()
        .map(|check| {
            execution.check_receipts.iter().rev().find(|r| {
                r.check_id == check.id
                    && r.argv == check.argv
                    && r.commit == commit
                    && r.tree == tree
                    && (requirements.is_none() || r.requirements_digest == requirements)
            })
        })
        .collect();
    let Some(receipts) = receipts else {
        return Ok(None);
    };
    let passed = receipts.iter().all(|r| r.exit_code == 0);
    Ok(Some((
        if passed {
            EdgeOutcome::Pass
        } else {
            EdgeOutcome::Fail
        },
        DecisionEvidence {
            actor: "daemon".into(),
            reason: if passed {
                "Current deterministic checks passed"
            } else {
                "Current deterministic checks failed"
            }
            .into(),
            references: receipts
                .iter()
                .map(|r| {
                    format!(
                        "check:{}:{}:{}:{}",
                        r.check_id, r.commit, r.tree, r.exit_code
                    )
                })
                .collect(),
        },
    )))
}

pub(in crate::workflows::run) fn drive_policy(
    store: &RunStore,
    run: &RunSnapshot,
    graph: &GraphExecution,
    activation: &Activation,
) -> Result<bool, String> {
    let is_judge = graph
        .definition
        .graph
        .nodes
        .iter()
        .any(|n| n.id == activation.node_id && matches!(n.kind, NodeKind::Judge));
    if is_judge
        && graph.definition.graph.evidence_policy.is_none()
        && judge::judge(run, graph, activation)?.0 != EdgeOutcome::Yes
    {
        return Ok(false);
    }
    if gate(run, graph)?.is_none() {
        let execution = run
            .stories
            .iter()
            .find(|s| s.story_id == graph.target_id)
            .ok_or("Gate story execution missing")?;
        let path = execution
            .worktree_path
            .as_deref()
            .ok_or("Gate worktree missing")?;
        let (commit, tree) = super::super::check::clean_artifact(Path::new(path))?;
        let requirements = graph
            .definition
            .graph
            .evidence_policy
            .as_ref()
            .map(|_| {
                StoryStore::open()?
                    .get_story(&graph.target_id)
                    .map(|story| super::super::evidence::requirements_digest(&story))
            })
            .transpose()?;
        let check = graph
            .definition
            .required_checks
            .iter()
            .find(|check| {
                !execution.check_receipts.iter().any(|r| {
                    r.check_id == check.id
                        && r.argv == check.argv
                        && r.commit == commit
                        && r.tree == tree
                        && (requirements.is_none() || r.requirements_digest == requirements)
                })
            })
            .ok_or("Gate check policy missing")?;
        store.execute_check(
            &run.id,
            &graph.target_id,
            &check.id,
            &format!(
                "daemon:check:{}:{}:{}:{}",
                graph.id, activation.id, check.id, run.sequence
            ),
            run.sequence,
        )?;
        return Ok(true);
    }
    if !is_judge {
        return Ok(false);
    }
    if gate(run, graph)?.is_some_and(|d| d.0 == EdgeOutcome::Fail) {
        return Err("Pre-approval deterministic checks failed; approval refused".into());
    }
    if judge::judge(run, graph, activation)?.0 != EdgeOutcome::Yes {
        return Ok(false);
    }
    if approved(run, graph)? {
        return Ok(false);
    }
    let (_, evidence) = judge::judge(run, graph, activation)?;
    store.approve_graph_story(run, graph, &evidence.actor)?;
    Ok(true)
}
