//! Accepted issue material becomes one owned plan and a graph-owned story proposal.
//! This operation never dispatches a child or hides story creation inside a handoff.
use super::native_git::NativeFailure;
use super::native_host::NativeBinding;
use super::native_issue_source::IssueSource;
use super::native_issue_triage::{owned, require_triage};
use super::native_story_create::CreateRequest;
use super::neutral_effects::{EffectKey, EffectOutcome};
use super::neutral_execution::NeutralResult;
use crate::stories::{NewPlan, NewStory, Plan, StoryBackend, StoryOrigin, StoryType};
use crate::workflows::review_verdict::TriageOutcome;
use serde::{Deserialize, Serialize};

pub(super) const HANDLER: &str = "native.issue.prepare";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub source: IssueSource,
    pub read: super::native_issue_read::IssueReadReceipt,
    pub triage: EffectKey,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub effect: EffectKey,
    pub source: IssueSource,
    pub triage: EffectKey,
    pub plan: Plan,
    pub create: CreateRequest,
}

fn material(
    bound: &NativeBinding,
    key: &EffectKey,
    request: &Request,
) -> Result<(NewPlan, String), String> {
    let triage = require_triage(bound, key, &request.triage, &request.source)?;
    if triage.assessment.decision.outcome != TriageOutcome::Accepted {
        return Err("only accepted issue triage can prepare delivery".into());
    }
    if serde_json::to_vec(&request.read)
        .map_err(|e| e.to_string())?
        .len()
        > 262_144
    {
        return Err("issue preparation material exceeds byte limit".into());
    }
    let run = bound.store.neutral_execution(&key.fence.run)?;
    let exact = serde_json::to_value(&request.read).map_err(|e| e.to_string())?;
    if !run.effects.values().any(|effect| {
        effect.invocation.descriptor.handler.name == "native.issue.read"
            && matches!(&effect.observation, Some(super::neutral_effects::EffectObservation { outcome: EffectOutcome::Completed { result: NeutralResult::Completed { outputs, .. } }, stopped: true }) if outputs.get("receipt") == Some(&exact))
    }) {
        return Err("issue preparation needs original same-run stopped read evidence".into());
    }
    let read = &request.read;
    if read.source != request.source {
        return Err("issue read differs from accepted source".into());
    }
    request.source.require_live(&read.issue)?;
    let url = format!(
        "https://github.com/{}/issues/{}",
        request.source.repository, request.source.number
    );
    // Keep the source link even when title/body consume the material budget.
    let material_limit = 20_000usize
        .checked_sub(url.chars().count() + 2)
        .ok_or("issue source URL exceeds the problem character limit")?;
    let material: String = format!(
        "{}\n\n{}",
        read.issue["title"].as_str().unwrap_or_default(),
        read.issue["body"].as_str().unwrap_or_default()
    )
    .chars()
    .take(material_limit)
    .collect();
    let problem = format!("{material}\n\n{url}");
    Ok((
        NewPlan {
            project: bound
                .context
                .workspace_path()
                .to_string_lossy()
                .into_owned(),
            title: format!(
                "Issue {}#{}",
                request.source.repository, request.source.number
            ),
            source: format!("Issue run {}: {url}", key.fence.run),
        },
        problem,
    ))
}
fn proposal(key: &EffectKey, request: &Request, plan: Plan, problem: String) -> Receipt {
    let url = format!(
        "https://github.com/{}/issues/{}",
        request.source.repository, request.source.number
    );
    Receipt {
        effect: key.clone(),
        source: request.source.clone(),
        triage: request.triage.clone(),
        create: CreateRequest {
            review: "issue_delivery".into(),
            cap: 1,
            source_story: None,
            source_revision: None,
            evidence: vec![url.clone()],
            proposal: "accepted_issue".into(),
            problem: Some(problem),
            story: NewStory {
                plan_id: plan.id.clone(),
                title: format!(
                    "Resolve {} issue #{}",
                    request.source.repository, request.source.number
                ),
                criteria: vec![format!("Resolve {url} as the issue states")],
                priority: 2,
                story_type: Some(StoryType::Task),
                origin: StoryOrigin::GitHubIssue {
                    number: request.source.number as u64,
                },
                file_scope: vec![],
            },
        },
        plan,
    }
}

pub(super) fn prepare(
    bound: &NativeBinding,
    key: &EffectKey,
    request: &Request,
    backend: &dyn StoryBackend,
) -> Result<Receipt, NativeFailure> {
    owned(bound, key, HANDLER, request)?;
    bound
        .context
        .authorize(&bound.store, key, HANDLER, request, None)?;
    let (input, problem) = material(bound, key, request)?;
    let plan = backend
        .create_plan_once(&key.fence.run, "accepted_issue", input)
        .map_err(NativeFailure::Uncertain)?;
    Ok(proposal(key, request, plan, problem))
}

pub(super) fn reconcile(
    bound: &NativeBinding,
    effect: &super::neutral_effects::NeutralEffect,
    backend: &dyn StoryBackend,
) -> Result<Receipt, String> {
    let run = bound.store.neutral_execution(&effect.key.fence.run)?;
    if run.effect(&effect.key)? != effect || effect.invocation.descriptor.handler.name != HANDLER {
        return Err("issue preparation recovery lacks original effect".into());
    }
    if matches!(
        effect.observation.as_ref().map(|o| &o.outcome),
        Some(EffectOutcome::Uncertain { .. })
    ) {
        return Err(
            "original issue preparation remains uncertain; no write or result relabelled".into(),
        );
    }
    let request: Request = serde_json::from_value(effect.invocation.inputs["request"].clone())
        .map_err(|e| e.to_string())?;
    let (input, problem) = material(bound, &effect.key, &request)?;
    let plan = backend
        .existing_plan_for_proposal(&effect.key.fence.run, "accepted_issue", &input)?
        .ok_or("original issue plan outcome unproven; no write repeated")?;
    Ok(proposal(&effect.key, &request, plan, problem))
}
