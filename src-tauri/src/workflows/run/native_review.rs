//! Review one exact staged or committed artifact; only a persisted native observation is evidence.
use super::RunStore;
use super::check::git_output;
use super::native_git::NativeFailure;
use super::native_operations::NativeContext;
use super::neutral_effects::{EffectKey, EffectOutcome};
use super::neutral_execution::NeutralResult;
use crate::workflows::catalog::content_digest;
use crate::workflows::review_verdict::CodeVerdict;
use serde::{Deserialize, Serialize};
use std::future::Future;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ReviewRequest {
    pub base: String,
    /// A committed candidate; absent only for the staged commit.create review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    pub tree: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewReceipt {
    pub workspace: String,
    pub caller: String,
    pub base: String,
    pub head: Option<String>,
    pub tree: String,
    pub diff_digest: String,
    pub verdict: CodeVerdict,
    pub body: String,
    pub reviewer_session_id: Option<String>,
    /// The review's own effect, so a graph can bind it into a later publication.
    /// Absent in receipts retained before the field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<EffectKey>,
}

fn diff(context: &NativeContext, request: &ReviewRequest) -> Result<String, String> {
    context.current(None)?;
    let path = context.workspace_path();
    let diff = if let Some(head) = &request.head {
        context.require_artifact(head, &request.tree)?;
        if git_output(
            path,
            &[
                "rev-parse",
                "--verify",
                &format!("{}^{{commit}}", request.base),
            ],
        )? != request.base
        {
            return Err("native review base is not an immutable commit".into());
        }
        git_output(
            path,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                &request.base,
                head,
                "--",
            ],
        )?
    } else {
        if git_output(path, &["rev-parse", "HEAD"])? != request.base
            || git_output(path, &["write-tree"])? != request.tree
            || !git_output(path, &["diff", "--name-only"])?.is_empty()
            || !git_output(path, &["ls-files", "--others", "--exclude-standard"])?.is_empty()
        {
            return Err("native review base/index is stale".into());
        }
        tuic_git::git::get_git_diff_blocking(
            path.to_string_lossy().into_owned(),
            Some("staged".into()),
        )?
    };
    if diff.len() > 1_000_000 || diff.contains("Binary files ") || diff.contains("GIT binary patch")
    {
        return Err("native review cannot cover this binary or oversized diff".into());
    }
    Ok(diff)
}

fn owned(
    context: &NativeContext,
    store: &RunStore,
    key: &EffectKey,
    request: &ReviewRequest,
) -> Result<(), String> {
    context.current(None)?;
    let run = store.neutral_execution(&key.fence.run)?;
    let effect = run.effect(key)?;
    if run.status != super::neutral_execution::NeutralStatus::Running
        || effect.invocation.inputs.len() != 1
        || effect
            .invocation
            .subject
            .as_ref()
            .and_then(|s| s.get("workspace"))
            != Some(
                &serde_json::json!({"reference":"workspace", "id":context.workspace_path().to_string_lossy()}),
            )
        || !effect.dispatched
        || effect.cancel_started
        || effect.observation.is_some()
        || run.deadline_ms <= super::store::now_ms()
        || !matches!(
            effect.invocation.descriptor.handler.name.as_str(),
            "native.review.change" | "native.review.bounded"
        )
        || effect.invocation.descriptor.handler.version != 1
        || effect.invocation.inputs.get("request")
            != Some(&serde_json::to_value(request).map_err(|e| e.to_string())?)
        || !effect.preparation.resources.contains(&format!(
            "native:workspace:{}",
            context.git_workspace()?.git_dir
        ))
    {
        return Err("native review lacks current owned effect".into());
    }
    Ok(())
}

/// The production callback is TUIC's no-tool ACP reviewer. Tests replace only
/// that answer boundary, retaining native diff, current scope and ledger checks.
pub(super) async fn review_with<F, Fut>(
    context: &NativeContext,
    store: &RunStore,
    key: &EffectKey,
    request: &ReviewRequest,
    reviewer: F,
) -> Result<ReviewReceipt, NativeFailure>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<(CodeVerdict, String, Option<String>), String>>,
{
    owned(context, store, key, request)?;
    let before = diff(context, request)?;
    let digest = content_digest(&before)?;
    let (verdict, body, reviewer_session_id) = if before.trim().is_empty() {
        (
            CodeVerdict::Uncertain,
            "Empty diff: no change was reviewed or approved.".into(),
            None,
        )
    } else {
        if store
            .neutral_execution(&key.fence.run)?
            .effect(key)?
            .invocation
            .descriptor
            .handler
            .name
            == super::native_review_policy::HANDLER
        {
            super::native_review_policy::reserve(context, store, key)?;
        }
        reviewer(before).await.map_err(NativeFailure::Uncertain)?
    };
    owned(context, store, key, request)?;
    if content_digest(&diff(context, request)?)? != digest {
        return Err(NativeFailure::Refused(
            "native diff changed during review".into(),
        ));
    }
    Ok(ReviewReceipt {
        workspace: context.workspace_path().to_string_lossy().into_owned(),
        caller: context.caller().into(),
        base: request.base.clone(),
        head: request.head.clone(),
        tree: request.tree.clone(),
        diff_digest: digest,
        verdict,
        body,
        reviewer_session_id,
        effect: Some(key.clone()),
    })
}

pub(super) fn require_review(
    context: &NativeContext,
    store: &RunStore,
    key: &EffectKey,
    base: &str,
    tree: &str,
) -> Result<(), String> {
    let receipt = observed(
        context,
        store,
        key,
        &ReviewRequest {
            base: base.into(),
            head: None,
            tree: tree.into(),
        },
    )?;
    if receipt.verdict != CodeVerdict::Pass {
        return Err("native review is not passing".into());
    }
    Ok(())
}

/// Returns the retained verdict for this exact current committed candidate.
/// Approval callers must separately require Pass.
pub(crate) fn observed_review(
    context: &NativeContext,
    store: &RunStore,
    key: &EffectKey,
    base: &str,
    head: &str,
    tree: &str,
) -> Result<ReviewReceipt, String> {
    observed(
        context,
        store,
        key,
        &ReviewRequest {
            base: base.into(),
            head: Some(head.into()),
            tree: tree.into(),
        },
    )
}

fn observed(
    context: &NativeContext,
    store: &RunStore,
    key: &EffectKey,
    request: &ReviewRequest,
) -> Result<ReviewReceipt, String> {
    let run = store.neutral_execution(&key.fence.run)?;
    let effect = run.effect(key)?;
    let receipt = match &effect.observation {
        Some(observation) if observation.stopped && effect.settled() => {
            match &observation.outcome {
                EffectOutcome::Completed {
                    result: NeutralResult::Completed { outcome, outputs },
                } if outcome == "ok" => outputs
                    .get("receipt")
                    .ok_or("native review receipt absent")?,
                _ => return Err("native review did not complete successfully".into()),
            }
        }
        _ => return Err("native review has no stopped observed completion".into()),
    };
    let receipt: ReviewReceipt =
        serde_json::from_value(receipt.clone()).map_err(|e| e.to_string())?;
    if !matches!(
        effect.invocation.descriptor.handler.name.as_str(),
        "native.review.change" | "native.review.bounded"
    ) || effect.invocation.descriptor.handler.version != 1
        || effect.cancel_started
        || effect.invocation.inputs.get("request")
            != Some(&serde_json::to_value(request).map_err(|e| e.to_string())?)
        || receipt.workspace != context.workspace_path().to_string_lossy()
        || receipt.caller != context.caller()
        || receipt.base != request.base
        || receipt.tree != request.tree
        || receipt.head != request.head
        || receipt.effect.as_ref().is_some_and(|own| own != key)
        || receipt.diff_digest != content_digest(&diff(context, request)?)?
    {
        return Err("native review does not cover this exact change/caller/workspace".into());
    }
    Ok(receipt)
}
