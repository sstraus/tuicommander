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
    let reviewer = graph.activations.iter().skip(epoch).rev().find_map(|a| {
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
    });
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
    if graph.definition.graph.evidence_policy.is_some()
        && run.attempts.iter().any(|attempt| {
            attempt.story_id == story.id
                && binding_session(attempt) == Some(binding.session_id.as_str())
                && graph.definition.graph.nodes.iter().any(|node| {
                    node.id == attempt.node_id
                        && matches!(
                            node.kind,
                            NodeKind::Agent {
                                role: AgentRole::Implementer,
                                ..
                            }
                        )
                })
        })
    {
        return Ok(uncertain(
            "Review identity implemented this subject in an earlier generation",
        ));
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
    let typed_decision = if let Some(policy) = &graph.definition.graph.evidence_policy {
        let Some(typed) = &review.evidence else {
            return Ok(uncertain("Typed evidence required by pinned policy"));
        };
        let subject = super::super::evidence::ReviewSubject::Story {
            id: story.id.clone(),
            commit: commit.clone(),
            tree: tree.clone(),
            requirements: super::super::evidence::requirements_digest(&story),
        };
        if let Some(red) = &typed.proof.red {
            let pre_tree = super::super::check::git_output(
                Path::new(path),
                &["rev-parse", &format!("{}^{{tree}}", red.pre_fix_commit)],
            );
            let ancestor = super::super::check::git_output(
                Path::new(path),
                &["merge-base", &red.pre_fix_commit, &commit],
            );
            let valid = match (pre_tree, ancestor) {
                (Ok(pre_tree), Ok(ancestor)) => {
                    ancestor == red.pre_fix_commit
                        && red.check.subject
                            == super::super::evidence::ReviewSubject::Story {
                                id: story.id.clone(),
                                commit: red.pre_fix_commit.clone(),
                                tree: pre_tree,
                                requirements: super::super::evidence::requirements_digest(&story),
                            }
                }
                _ => false,
            };
            if !valid {
                return Ok(uncertain(
                    "RED is not bound to an actual pre-fix ancestor and current requirements",
                ));
            }
        }
        let decision = match typed.validate(&subject, policy) {
            Ok(decision) => decision,
            Err(error) => return Ok(uncertain(&error)),
        };
        // A re-review must account for each earlier FIX, including reviewer replacement.
        let unresolved = run
            .attempts
            .iter()
            .filter(|a| a.story_id == story.id && a.id != reviewer.id)
            .filter_map(|a| a.report.as_ref()?.review.as_ref()?.evidence.as_ref())
            .flat_map(|e| &e.findings)
            .filter(|f| f.decision() == super::super::evidence::TriageDecision::Fix)
            .any(|f| {
                !typed.resolved_fixes.contains(&f.id)
                    && !typed.findings.iter().any(|new| new.id == f.id)
            });
        if unresolved {
            return Ok(uncertain(
                "Re-review did not account for prior FIX findings",
            ));
        }
        Some(decision)
    } else {
        None
    };
    let evidence = DecisionEvidence {
        actor: binding.session_id.clone(),
        reason: report.summary.clone(),
        references: vec![
            format!("attempt:{}:{}", reviewer.id, reviewer.generation),
            format!("artifact:{commit}:{tree}"),
        ],
    };
    if typed_decision == Some(super::super::evidence::TriageDecision::Fix)
        || (typed_decision.is_none() && review.decision == ReviewDecision::ChangesRequested)
    {
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
    if typed_decision.is_some()
        && !super::policy::gate(run, graph)?.is_some_and(|d| d.0 == EdgeOutcome::Pass)
    {
        return Ok(uncertain(
            "Current required deterministic checks have not passed",
        ));
    }
    Ok((EdgeOutcome::Yes, evidence))
}

fn binding_session(attempt: &super::super::NodeAttempt) -> Option<&str> {
    attempt.agent.as_ref().map(|a| a.session_id.as_str())
}

pub(in crate::workflows::run) use crate::workflows::prompt::review_artifact_digest as artifact_digest;
