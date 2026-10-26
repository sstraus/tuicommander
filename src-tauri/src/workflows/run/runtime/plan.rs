//! Plan waves reuse graph children, native dependencies and operator Git receipts.
use super::super::graph::{Activation, EdgeOutcome, GraphExecution};
use super::super::store::{fingerprint, plan_stories, ready_to_verify, receipt_current};
use super::*;
use crate::stories::{StoryStatus, StoryStore};

pub(in crate::workflows::run) enum Dispatch {
    Advanced,
    Waiting,
    Decided((EdgeOutcome, DecisionEvidence)),
}

pub(in crate::workflows::run) fn dispatch(
    store: &RunStore,
    run: &RunSnapshot,
    graph: &GraphExecution,
    activation: &Activation,
) -> Result<Dispatch, String> {
    let all_stories = StoryStore::open()?.list_stories(&run.plan_id)?;
    let stories: Vec<_> = all_stories
        .iter()
        .filter(|story| !story.archived)
        .cloned()
        .collect();
    if run.planning_fingerprint.is_none() {
        store.command_expected(
            &run.id,
            &format!("daemon:planning:{}", run.sequence),
            run.sequence,
            RunCommand::ClosePlanning,
        )?;
        return Ok(Dispatch::Advanced);
    }
    let mut candidates: Vec<_> = stories
        .iter()
        .filter(|s| {
            s.status == StoryStatus::Ready
                && !run.graph_executions.iter().any(|g| g.target_id == s.id)
        })
        .collect();
    candidates.sort_by_key(|s| (s.priority, s.id.clone()));
    let mut deferred = false;
    for story in candidates {
        let mut eligible = true;
        for id in &story.dependencies {
            let dependency = all_stories
                .iter()
                .find(|s| s.id == *id)
                .ok_or("dependency missing")?;
            eligible &= dependency.status == StoryStatus::Done
                && receipt_current(run, &dependency.id, dependency.revision)?;
        }
        if !eligible {
            continue;
        }
        match store.command_expected(
            &run.id,
            &format!("daemon:child:{}", story.id),
            run.sequence,
            RunCommand::Graph {
                transition: GraphTransition::Start {
                    execution_id: format!("story:{}", story.id),
                    target_id: story.id.clone(),
                },
            },
        ) {
            Ok(_) => return Ok(Dispatch::Advanced),
            Err(error)
                if error.contains("parallel story limit")
                    || error.contains("scope overlaps")
                    || error.contains("reserved by") =>
            {
                deferred = true;
                continue;
            }
            Err(error) => return Err(error),
        }
    }
    if deferred {
        return Ok(Dispatch::Waiting);
    }
    if run
        .graph_executions
        .iter()
        .any(|g| g.target_id != run.plan_id && !g.completed)
    {
        return Ok(Dispatch::Waiting);
    }
    if ready_to_verify(run, &stories).is_ok() {
        return Ok(Dispatch::Decided((
            EdgeOutcome::Completed,
            evidence(
                graph,
                activation,
                "All included stories have current explicit integration receipts",
            ),
        )));
    }
    // Approved children wait for an operator merge, never spawn or merge automatically.
    for story in &stories {
        if story.status == StoryStatus::Done && !receipt_current(run, &story.id, story.revision)? {
            return Ok(Dispatch::Waiting);
        }
    }
    Ok(Dispatch::Decided((
        EdgeOutcome::Blocked,
        evidence(
            graph,
            activation,
            "No eligible story wave; unresolved or WontFix dependencies remain",
        ),
    )))
}

fn evidence(graph: &GraphExecution, activation: &Activation, reason: &str) -> DecisionEvidence {
    DecisionEvidence {
        actor: "daemon".into(),
        reason: reason.into(),
        references: vec![format!("activation:{}:{}", graph.id, activation.id)],
    }
}

pub(in crate::workflows::run) fn judge(
    run: &RunSnapshot,
    graph: &GraphExecution,
) -> Result<Option<(EdgeOutcome, DecisionEvidence)>, String> {
    let stories = plan_stories(run)?;
    let activation = graph.activations.last().ok_or("plan activation missing")?;
    if ready_to_verify(run, &stories).is_err() {
        return Ok(Some((
            EdgeOutcome::No,
            evidence(
                graph,
                activation,
                "Plan changed or delivery is incomplete; replan required",
            ),
        )));
    }
    let (commit, tree) = super::super::check::clean_artifact(Path::new(&run.project))?;
    let Some(receipt) = run
        .canonical_recertification
        .as_ref()
        .filter(|r| r.commit == commit && r.tree == tree)
    else {
        return Ok(None);
    };
    let passed = graph.definition.required_checks.iter().all(|check| {
        receipt.post_checks.iter().any(|r| {
            r.check_id == check.id
                && r.argv == check.argv
                && r.exit_code == 0
                && r.commit == commit
                && r.tree == tree
        })
    });
    if !passed {
        return Ok(Some((
            EdgeOutcome::No,
            evidence(graph, activation, "Deterministic final plan checks failed"),
        )));
    }
    if run.verification_fingerprint.as_deref()
        != Some(fingerprint(&run.plan_id, &stories, true)?.as_str())
    {
        return Ok(None);
    }
    Ok(Some((
        EdgeOutcome::Yes,
        evidence(
            graph,
            activation,
            "Current plan fixed point passed deterministic checks",
        ),
    )))
}

pub(in crate::workflows::run) fn drive_checks(
    store: &RunStore,
    run: &RunSnapshot,
    graph: &GraphExecution,
    activation: &Activation,
) -> Result<bool, String> {
    let stories = plan_stories(run)?;
    if ready_to_verify(run, &stories).is_err() {
        return Ok(false);
    }
    let canonical = Path::new(&run.project);
    let (commit, tree) = super::super::check::clean_artifact(canonical)?;
    if run
        .canonical_recertification
        .as_ref()
        .is_none_or(|r| r.commit != commit || r.tree != tree)
    {
        let mut checks = Vec::new();
        for check in &graph.definition.required_checks {
            checks.push(store.execute_plan_check(&run.id, check)?);
        }
        if super::super::check::clean_artifact(canonical)? != (commit.clone(), tree.clone()) {
            return Err("canonical artifact moved during final checks".into());
        }
        store.command_expected(
            &run.id,
            &format!("daemon:plan-checks:{}:{}", activation.id, run.sequence),
            run.sequence,
            RunCommand::RecordRecertification {
                receipt: super::super::CanonicalReceipt {
                    canonical_ref: run
                        .canonical_ref
                        .clone()
                        .ok_or("canonical branch missing")?,
                    commit,
                    tree,
                    post_checks: checks,
                },
            },
        )?;
        return Ok(true);
    }
    if run.verification_fingerprint.as_deref()
        != Some(fingerprint(&run.plan_id, &stories, true)?.as_str())
    {
        store.command_expected(
            &run.id,
            &format!("daemon:plan-verified:{}", run.sequence),
            run.sequence,
            RunCommand::FinalVerificationPassed,
        )?;
        return Ok(true);
    }
    Ok(false)
}
