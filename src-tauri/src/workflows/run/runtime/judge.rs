//! Routing consumes typed evidence; graph completion never fabricates approval.
use super::super::graph::{Activation, EdgeOutcome, GraphExecution};
use super::super::{AttemptOutcome, ReviewDecision};
use super::*;
use crate::workflows::AgentRole;

pub(in crate::workflows::run) fn judge(
    run: &RunSnapshot,
    graph: &GraphExecution,
    activation: &Activation,
) -> Result<(EdgeOutcome, DecisionEvidence), String> {
    let uncertain = |reason: &str| {
        (
            EdgeOutcome::Uncertain,
            DecisionEvidence {
                actor: "daemon".into(),
                reason: reason.into(),
                references: vec![format!("activation:{}:{}", graph.id, activation.id)],
            },
        )
    };
    let story = crate::stories::StoryStore::open()?.get_story(&graph.target_id)?;
    // A loop's previous review cannot decide a new implementation epoch.
    let epoch = graph
        .activations
        .iter()
        .rposition(|a| {
            graph.definition.graph.nodes.iter().any(|n| {
                n.id == a.node_id
                    && matches!(
                        n.kind,
                        NodeKind::Agent {
                            role: AgentRole::Implementer,
                            ..
                        }
                    )
            })
        })
        .unwrap_or(0);
    let mut current = graph.activations.iter().skip(epoch).filter_map(|a| {
        let node = graph
            .definition
            .graph
            .nodes
            .iter()
            .find(|n| n.id == a.node_id)?;
        let NodeKind::Agent { role, .. } = node.kind else {
            return None;
        };
        let attempt = super::effects::activation_attempt(run, graph, a)?;
        Some((role, attempt))
    });
    let implementer = current
        .find(|(role, _)| *role == AgentRole::Implementer)
        .map(|(_, a)| a);
    // FIX is submitted by this bound Reviewer, not by the last sibling in the graph.
    // A standalone Judge still resolves the current epoch's final review.
    let reviewer = if graph.definition.graph.nodes.iter().any(|n| {
        n.id == activation.node_id
            && matches!(
                n.kind,
                NodeKind::Agent {
                    role: AgentRole::Reviewer,
                    ..
                }
            )
    }) {
        super::effects::activation_attempt(run, graph, activation)
    } else {
        graph.activations.iter().skip(epoch).rev().find_map(|a| {
            let node = graph
                .definition
                .graph
                .nodes
                .iter()
                .find(|n| n.id == a.node_id)?;
            if !matches!(
                node.kind,
                NodeKind::Agent {
                    role: AgentRole::Reviewer,
                    ..
                }
            ) {
                return None;
            }
            super::effects::activation_attempt(run, graph, a)
        })
    };
    let Some(reviewer) = reviewer else {
        return Ok(uncertain("No current reviewer report"));
    };
    let Some(report) = reviewer.report.as_ref() else {
        return Ok(uncertain("Reviewer did not provide a typed report"));
    };
    if report.story_revision != story.revision
        && !(story.status == crate::stories::StoryStatus::Done
            && report.story_revision.checked_add(1) == Some(story.revision)
            && crate::stories::StoryStore::open()?
                .transition_history(&story.id)?
                .last()
                .is_some_and(|receipt| {
                    receipt.revision == story.revision
                        && receipt.command == crate::stories::StoryCommand::Approve
                        && receipt.actor
                            == crate::stories::StoryTransitionActor::ManagedSession {
                                session_id: binding_session(reviewer).unwrap_or_default().into(),
                            }
                }))
    {
        return Ok(uncertain(
            "Reviewer evidence targets an older story revision",
        ));
    }
    let Some(review) = report.review.as_ref() else {
        return Ok(uncertain("Reviewer assessment is missing"));
    };
    let Some(binding) = reviewer.agent.as_ref() else {
        return Ok(uncertain("Reviewer identity is missing"));
    };
    let Some(implementer) = implementer else {
        return Ok(uncertain("Current implementation attempt is missing"));
    };
    if implementer
        .agent
        .as_ref()
        .is_none_or(|a| a.session_id == binding.session_id)
    {
        return Ok(uncertain("Review must come from a separate bound agent"));
    }
    if report.outcome != AttemptOutcome::Completed {
        return Ok(uncertain("Reviewer report is not completed"));
    }
    let Some(execution) = run.stories.iter().find(|s| s.story_id == story.id) else {
        return Ok(uncertain("Story artifact is missing"));
    };
    let Some(path) = execution.worktree_path.as_deref() else {
        return Ok(uncertain("Isolated artifact is missing"));
    };
    let (commit, tree) = match super::super::check::clean_artifact(Path::new(path)) {
        Ok(artifact) => artifact,
        Err(_) => return Ok(uncertain("Review artifact is dirty or unavailable")),
    };
    if review.artifact_digest != artifact_digest(&commit, &tree) {
        return Ok(uncertain("Review artifact moved"));
    }
    let evidence = DecisionEvidence {
        actor: binding.session_id.clone(),
        reason: report.summary.clone(),
        references: vec![
            format!("attempt:{}:{}", reviewer.id, reviewer.generation),
            format!("artifact:{commit}:{tree}"),
        ],
    };
    if review.decision == ReviewDecision::ChangesRequested {
        return Ok((EdgeOutcome::No, evidence));
    }
    if implementer
        .report
        .as_ref()
        .is_none_or(|r| r.outcome != AttemptOutcome::Completed)
    {
        return Ok(uncertain("Current implementation did not complete"));
    }
    if !matches!(
        story.status,
        crate::stories::StoryStatus::Review | crate::stories::StoryStatus::Done
    ) {
        return Ok(uncertain("Story must be submitted for independent review"));
    }
    if !story.checked.iter().all(|checked| *checked)
        || (0..story.criteria.len()).any(|index| {
            !report
                .criterion_results
                .iter()
                .any(|r| r.index == index && r.satisfied)
        })
    {
        return Ok(uncertain(
            "Independent review lacks current criterion evidence",
        ));
    }
    Ok((EdgeOutcome::Yes, evidence))
}

fn binding_session(attempt: &super::super::NodeAttempt) -> Option<&str> {
    attempt.agent.as_ref().map(|a| a.session_id.as_str())
}

pub(in crate::workflows::run) use crate::workflows::prompt::review_artifact_digest as artifact_digest;
