//! Triage consumes a live admitted issue and an installed scoped report owner.
use super::native_git::NativeFailure;
use super::native_host::NativeBinding;
use super::native_issue_read::IssueReadReceipt;
use super::native_issue_service::{IssuePolicy, IssueProfile, IssueServices, ScopedTriage};
use super::native_issue_source::IssueSource;
use super::neutral_effects::{EffectKey, EffectOutcome};
use super::neutral_execution::{NeutralResult, NeutralStatus};
use crate::github_pr_api::{PrApi, RepoApi};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IssueTriageRequest {
    pub source: IssueSource,
    pub profile: IssueProfile,
    pub policy: IssuePolicy,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IssueTriageReceipt {
    pub assessment: ScopedTriage,
    pub policy: IssuePolicy,
    pub effect: EffectKey,
}
impl IssueTriageRequest {
    pub(super) fn validate(&self) -> Result<(), String> {
        self.source.validate()?;
        self.profile.validate()?;
        self.policy.validate()
    }
}
pub(super) fn owned(
    bound: &NativeBinding,
    key: &EffectKey,
    handler: &str,
    request: &impl Serialize,
) -> Result<(), String> {
    bound.context.current(None)?;
    let run = bound.store.neutral_execution(&key.fence.run)?;
    let effect = run.effect(key)?;
    if run.status != NeutralStatus::Running
        || run.deadline_ms <= super::store::now_ms()
        || !effect.dispatched
        || effect.cancel_started
        || effect.observation.is_some()
        || effect.invocation.descriptor.handler.name != handler
        || effect.invocation.descriptor.handler.version != 1
        || effect.invocation.inputs.get("request")
            != Some(&serde_json::to_value(request).map_err(|e| e.to_string())?)
    {
        return Err("issue operation lacks exact current effect ownership".into());
    }
    Ok(())
}
pub(super) async fn triage(
    bound: &NativeBinding,
    key: &EffectKey,
    request: &IssueTriageRequest,
    services: &dyn IssueServices,
) -> Result<IssueTriageReceipt, NativeFailure> {
    request.validate()?;
    request.source.require(bound, key)?;
    let state = bound.context.current(None)?;
    let api = RepoApi::resolve(&state, &bound.context.workspace_path().to_string_lossy()).await?;
    if api.account.id != request.source.account
        || format!("{}/{}", api.owner, api.repo).to_ascii_lowercase() != request.source.repository
    {
        return Err("triage account/repository differs from trusted workspace"
            .to_owned()
            .into());
    }
    triage_with(bound, key, request, services, &api).await
}
pub(super) async fn triage_with(
    bound: &NativeBinding,
    key: &EffectKey,
    request: &IssueTriageRequest,
    services: &dyn IssueServices,
    api: &impl PrApi,
) -> Result<IssueTriageReceipt, NativeFailure> {
    request.validate()?;
    request.source.require(bound, key)?;
    owned(bound, key, "native.issue.triage", request)?;
    let issue = api
        .rest(
            "GET",
            &format!("issues/{}", request.source.number),
            Value::Null,
        )
        .await?;
    request.source.require_live(&issue)?;
    let assessment = services
        .triage(
            bound,
            key,
            &IssueReadReceipt::new(request.source.clone(), issue),
            &request.profile,
        )
        .await
        .map_err(NativeFailure::Uncertain)?;
    assessment.decision.validate()?;
    if assessment.source != request.source
        || assessment.profile != request.profile
        || assessment.report.trim().is_empty()
        || assessment.report.len() > 1024
    {
        return Err("scoped triage report differs from exact source/profile"
            .to_owned()
            .into());
    }
    // A model turn is not permission to publish requirements edited during it.
    let latest = api
        .rest(
            "GET",
            &format!("issues/{}", request.source.number),
            Value::Null,
        )
        .await?;
    request.source.require_live(&latest)?;
    request.source.require(bound, key)?;
    owned(bound, key, "native.issue.triage", request)?;
    Ok(IssueTriageReceipt {
        assessment,
        policy: request.policy.clone(),
        effect: key.clone(),
    })
}
/// Only a successful stopped native triage observation in this intake can
/// authorize policy effects/handoff. Model or graph JSON is not a verdict.
pub(super) fn require_triage(
    bound: &NativeBinding,
    key: &EffectKey,
    reference: &EffectKey,
    source: &IssueSource,
) -> Result<IssueTriageReceipt, String> {
    source.require(bound, key)?;
    if key.fence.run != reference.fence.run {
        return Err("triage evidence belongs to another intake".into());
    }
    let run = bound.store.neutral_execution(&key.fence.run)?;
    let effect = run.effect(reference)?;
    if !effect.settled() || effect.invocation.descriptor.handler.name != "native.issue.triage" {
        return Err("source-bound stopped triage evidence unavailable".into());
    }
    let outputs = match effect
        .observation
        .as_ref()
        .map(|observation| &observation.outcome)
    {
        Some(EffectOutcome::Completed {
            result: NeutralResult::Completed { outputs, .. },
        }) => outputs,
        _ => return Err("triage did not complete successfully".into()),
    };
    let receipt: IssueTriageReceipt = serde_json::from_value(
        outputs
            .get("receipt")
            .ok_or("triage receipt missing")?
            .clone(),
    )
    .map_err(|e| e.to_string())?;
    let request: IssueTriageRequest = serde_json::from_value(
        effect
            .invocation
            .inputs
            .get("request")
            .ok_or("triage request missing")?
            .clone(),
    )
    .map_err(|e| e.to_string())?;
    if &receipt.assessment.source != source
        || receipt.effect != *reference
        || request.source != *source
        || request.profile != receipt.assessment.profile
        || request.policy != receipt.policy
    {
        return Err("triage receipt source/profile/effect mismatch".into());
    }
    receipt.assessment.decision.validate()?;
    Ok(receipt)
}
