//! Reuse the native review/check authority at the event publication boundary.
use super::{RunSnapshot, RunStatus, graph::EdgeOutcome};
use crate::workflows::NodeKind;

pub(crate) struct PublicationReview {
    pub workspace: String,
    pub commit: String,
    pub findings: String,
}

/// Completion alone is insufficient: reload current native story and artifact evidence.
pub(crate) fn publication_review(
    run: &RunSnapshot,
    story_id: &str,
) -> Result<PublicationReview, String> {
    if !matches!(run.status, RunStatus::Running | RunStatus::Completed) {
        return Err("Event workflow has not completed".into());
    }
    let graph = run
        .graph_executions
        .iter()
        .find(|g| g.target_id == story_id)
        .ok_or("Event story graph is missing")?;
    if !graph.completed {
        return Err("Event story graph has not completed".into());
    }
    let judge = graph
        .activations
        .iter()
        .rev()
        .find(|a| {
            graph
                .definition
                .graph
                .nodes
                .iter()
                .any(|n| n.id == a.node_id && matches!(n.kind, NodeKind::Judge))
        })
        .ok_or("Event workflow has no independent review decision")?;
    let (outcome, evidence) = super::runtime::judge::judge(run, graph, judge)?;
    if outcome != EdgeOutcome::Yes {
        return Err(evidence.reason);
    }
    if !super::runtime::policy::approved(run, graph)? {
        return Err("Event story lacks current approval or passing configured checks".into());
    }
    let execution = run
        .stories
        .iter()
        .find(|s| s.story_id == story_id)
        .ok_or("Event story artifact missing")?;
    let workspace = execution
        .worktree_path
        .clone()
        .ok_or("Event story worktree missing")?;
    let (commit, _) = super::check::clean_artifact(std::path::Path::new(&workspace))?;
    let report = run
        .attempts
        .iter()
        .rev()
        .find(|a| {
            a.story_id == story_id
                && a.agent
                    .as_ref()
                    .is_some_and(|b| b.session_id == evidence.actor)
        })
        .and_then(|a| a.report.as_ref())
        .ok_or("Independent review report missing")?;
    let review = report
        .review
        .as_ref()
        .ok_or("Independent review findings missing")?;
    let mut findings = format!(
        "Independent review by `{}` on `{commit}`\n\n{}",
        evidence.actor, report.summary
    );
    if let Some(typed) = &review.evidence {
        for finding in &typed.findings {
            findings.push_str(&format!(
                "\n\n- {:?} {}: risk={:?}; proposal={:?}; scope={:?}",
                finding.decision(),
                finding.id,
                finding.accepted_risk,
                finding.proposal,
                finding.scope
            ));
        }
        for omission in &typed.proof.omissions {
            findings.push_str(&format!("\n\nOmission: {omission}"));
        }
    }
    if review.findings.is_empty()
        && review
            .evidence
            .as_ref()
            .is_none_or(|e| e.findings.is_empty())
    {
        findings.push_str("\n\nNo review findings.");
    } else {
        for finding in &review.findings {
            findings.push_str(&format!(
                "\n\n- {:?}: {}\n  {}",
                finding.severity, finding.summary, finding.evidence
            ));
        }
    }
    Ok(PublicationReview {
        workspace,
        commit,
        findings,
    })
}
