//! One source-bound handoff delegates execution to the installed child owner.
use super::native_git::NativeFailure;
use super::native_host::NativeBinding;
use super::native_issue_service::{IssueChildReceipt, IssueChildRequest, IssueServices};
use super::native_issue_triage::{owned, require_triage};
use super::neutral_effects::EffectKey;
use crate::github_pr_api::{PrApi, RepoApi};
use crate::workflows::review_verdict::TriageOutcome;
use serde_json::Value;

pub(super) fn validate(request: &IssueChildRequest) -> Result<(), String> {
    request.source.validate()?;
    if request.publication.id.is_empty()
        || request.publication.revision <= 0
        || request.inputs.len() > 100
    {
        return Err("handoff requires an exact bounded pinned publication".into());
    }
    Ok(())
}
pub(super) fn require_child(
    request: &IssueChildRequest,
    receipt: &IssueChildReceipt,
) -> Result<(), String> {
    if receipt.source != request.source
        || receipt.triage != request.triage
        || receipt.publication != request.publication
        || receipt.child.trim().is_empty()
        || receipt.child.len() > 256
        || receipt
            .evidence
            .as_ref()
            .is_some_and(|text| text.len() > 32_768)
    {
        return Err("child receipt differs from original source/publication/triage".into());
    }
    Ok(())
}
pub(super) async fn handoff(
    bound: &NativeBinding,
    key: &EffectKey,
    request: &IssueChildRequest,
    services: &dyn IssueServices,
) -> Result<IssueChildReceipt, NativeFailure> {
    services.child_dispatch().map_err(NativeFailure::Refused)?;
    validate(request)?;
    let triage = require_triage(bound, key, &request.triage, &request.source)?;
    if triage.assessment.decision.outcome != TriageOutcome::Accepted {
        return Err("only accepted issue triage can dispatch a child"
            .to_owned()
            .into());
    }
    bound
        .context
        .authorize(&bound.store, key, "native.issue.handoff", request, None)?;
    let state = bound.context.current(None)?;
    let api = RepoApi::resolve(&state, &bound.context.workspace_path().to_string_lossy()).await?;
    if api.account.id != request.source.account
        || format!("{}/{}", api.owner, api.repo).to_ascii_lowercase() != request.source.repository
    {
        return Err("handoff account/repository differs from trusted workspace"
            .to_owned()
            .into());
    }
    let issue = api
        .rest(
            "GET",
            &format!("issues/{}", request.source.number),
            Value::Null,
        )
        .await?;
    request.source.require_live(&issue)?;
    handoff_owned(bound, key, request, services).await
}
pub(super) async fn handoff_owned(
    bound: &NativeBinding,
    key: &EffectKey,
    request: &IssueChildRequest,
    services: &dyn IssueServices,
) -> Result<IssueChildReceipt, NativeFailure> {
    services.child_dispatch().map_err(NativeFailure::Refused)?;
    validate(request)?;
    let triage = require_triage(bound, key, &request.triage, &request.source)?;
    if triage.assessment.decision.outcome != TriageOutcome::Accepted {
        return Err("only accepted issue triage can dispatch a child"
            .to_owned()
            .into());
    }
    bound
        .context
        .authorize(&bound.store, key, "native.issue.handoff", request, None)?;
    let run = bound.store.neutral_execution(&key.fence.run)?;
    // The child must be in the immutable closure retained at start; no latest lookup.
    if !run.run.closure.children.iter().any(|publication| {
        publication.project == request.publication.project
            && publication.id == request.publication.id
            && publication.revision == request.publication.revision
    }) || request.publication.project != run.run.closure.root.project
    {
        return Err(
            "issue child publication absent from retained project closure"
                .to_owned()
                .into(),
        );
    }
    owned(bound, key, "native.issue.handoff", request)?;
    let receipt = services
        .handoff(bound, key, request)
        .await
        .map_err(NativeFailure::Uncertain)?;
    require_child(request, &receipt).map_err(NativeFailure::Uncertain)?;
    Ok(receipt)
}
