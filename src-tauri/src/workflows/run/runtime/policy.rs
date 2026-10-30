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
    gate_for_activation(run, graph, None)
}

pub(in crate::workflows::run) fn gate_for_activation(
    run: &RunSnapshot,
    graph: &GraphExecution,
    activation: Option<&Activation>,
) -> Result<Option<(EdgeOutcome, DecisionEvidence)>, String> {
    let Some(execution) = run.stories.iter().find(|s| s.story_id == graph.target_id) else {
        return Ok(None);
    };
    let Some(path) = execution.worktree_path.as_deref() else {
        return Ok(None);
    };
    let (commit, tree) = super::super::check::clean_artifact(Path::new(path))?;
    if graph.definition.required_checks.is_empty() {
        return Err("Gate requires deterministic checks".into());
    }
    let current_fork = graph.forks.iter().rev().find(|f| {
        f.parent.is_empty()
            && (f.state == super::super::parallel::ForkState::Open
                || f.state == super::super::parallel::ForkState::Joined)
    });
    let receipts: Option<Vec<_>> = graph
        .definition
        .required_checks
        .iter()
        .map(|check| {
            let receipts: Vec<_> = if let Some(fork) = current_fork {
                graph
                    .checks
                    .iter()
                    .filter(|c| {
                        activation.map_or_else(
                            || c.token.fork_id == fork.id && c.token.generation == fork.generation,
                            |a| c.activation_id == a.id,
                        )
                    })
                    .map(|c| &c.receipt)
                    .collect()
            } else {
                execution.check_receipts.iter().collect()
            };
            receipts.into_iter().rev().find(|r| {
                r.check_id == check.id
                    && r.argv == check.argv
                    && r.commit == commit
                    && r.tree == tree
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
    if is_judge && judge::judge(run, graph, activation)?.0 != EdgeOutcome::Yes {
        return Ok(false);
    }
    if gate_for_activation(run, graph, Some(activation))?.is_none() {
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
        let scoped: Vec<_> = graph
            .checks
            .iter()
            .filter(|c| c.activation_id == activation.id)
            .map(|c| &c.receipt)
            .collect();
        let receipts: Vec<_> = if activation.tokens.is_empty() {
            execution.check_receipts.iter().collect()
        } else {
            scoped
        };
        let check = graph
            .definition
            .required_checks
            .iter()
            .find(|check| {
                !receipts.iter().any(|r| {
                    r.check_id == check.id
                        && r.argv == check.argv
                        && r.commit == commit
                        && r.tree == tree
                })
            })
            .ok_or("Gate check policy missing")?;
        let owner = activation
            .tokens
            .last()
            .map(|token| super::super::parallel::check_owner(&run.id, &graph.id, token))
            .unwrap_or_else(|| run.id.clone());
        let result = store.execute_check_owned(
            &run.id,
            &graph.target_id,
            &check.id,
            &format!(
                "daemon:check:{}:{}:{}:{}",
                graph.id, activation.id, check.id, run.sequence
            ),
            run.sequence,
            &owner,
        )?;
        if !activation.tokens.is_empty() {
            let super::super::RunEventKind::CheckRecorded { receipt, .. } = result.event.kind
            else {
                return Err("check did not record a receipt".into());
            };
            let current = store.snapshot(&run.id)?;
            store.command_expected(
                &run.id,
                &format!(
                    "bind-check:{}:{}:{}:{}",
                    activation.id, check.id, receipt.commit, receipt.tree
                ),
                current.sequence,
                RunCommand::Graph {
                    transition: GraphTransition::CheckBound {
                        execution_id: graph.id.clone(),
                        activation_id: activation.id.clone(),
                        receipt,
                    },
                },
            )?;
        }
        return Ok(true);
    }
    if !is_judge {
        return Ok(false);
    }
    if gate(run, graph)?.is_some_and(|d| d.0 == EdgeOutcome::Fail) {
        return Err("Pre-approval deterministic checks failed; approval refused".into());
    }
    if approved(run, graph)? {
        return Ok(false);
    }
    let (_, evidence) = judge::judge(run, graph, activation)?;
    store.approve_graph_story(run, graph, &evidence.actor)?;
    Ok(true)
}
