//! Exact-commit review events through the existing account-bound publication path.
use super::super::native_git::NativeFailure;
use super::GitHubRequest;
use crate::github_pr_api::{PrApi, RepoApi};
use serde_json::{Value, json};

/// The GitHub review event a native review publishes.
#[derive(
    Clone, Copy, Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema, PartialEq, Eq,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum ReviewVerdict {
    Comment,
    Approve,
    RequestChanges,
}
impl ReviewVerdict {
    fn state(self) -> &'static str {
        match self {
            Self::Comment => "COMMENTED",
            Self::Approve => "APPROVED",
            Self::RequestChanges => "CHANGES_REQUESTED",
        }
    }
}

pub(super) async fn publish(
    api: &impl PrApi,
    number: u64,
    commit: &str,
    base: &str,
    body: &str,
    verdict: ReviewVerdict,
) -> Result<Value, NativeFailure> {
    let endpoint = format!("pulls/{number}");
    let head = api.rest("GET", &endpoint, Value::Null).await?;
    if head["head"]["sha"] != commit || head["base"]["sha"] != base || head["state"] != "open" {
        return Err(NativeFailure::Refused(
            "GitHub review head is stale or PR is closed".into(),
        ));
    }
    // GitHub pins the review itself with commit_id, including when the head
    // advances between the read and POST. Never silently publish on the new head.
    let created = api
        .rest(
            "POST",
            &format!("{endpoint}/reviews"),
            json!({"commit_id":commit,"body":body,"event":verdict}),
        )
        .await
        .map_err(NativeFailure::Uncertain)?;
    let id = created["id"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or_else(|| NativeFailure::Uncertain("GitHub review id missing".into()))?;
    let observed = api
        .rest("GET", &format!("{endpoint}/reviews/{id}"), Value::Null)
        .await
        .map_err(NativeFailure::Uncertain)?;
    let head = api
        .rest("GET", &endpoint, Value::Null)
        .await
        .map_err(NativeFailure::Uncertain)?;
    if observed["id"] != id
        || observed["commit_id"] != commit
        || observed["state"] != verdict.state()
        || observed["body"] != body
        || observed["html_url"].as_str().is_none_or(str::is_empty)
        || head["head"]["sha"] != commit
        || head["base"]["sha"] != base
    {
        return Err(NativeFailure::Uncertain(
            "GitHub exact-head review read-back differs".into(),
        ));
    }
    Ok(observed)
}

pub(in crate::workflows::run) fn verify_open_pr(
    observed: &Value,
    base: &str,
    title: &str,
    body: &str,
) -> Result<(), NativeFailure> {
    let body_matches = observed["body"] == body || (body.is_empty() && observed["body"].is_null());
    if observed["state"] != "open"
        || observed["base"]["ref"] != base
        || observed["title"] != title
        || !body_matches
    {
        return Err(NativeFailure::Uncertain(
            "GitHub PR read-back differs".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;

// This action retains its own exact-effect approval; a passing review grants none.
pub(super) async fn mark_ready(
    api: &impl PrApi,
    number: u64,
    commit: &str,
    pr: &Value,
) -> Result<(), NativeFailure> {
    if pr["number"] != number
        || pr["state"] != "open"
        || pr["draft"] != true
        || pr["head"]["sha"] != commit
    {
        return Err(NativeFailure::Refused(
            "MarkReady requires the exact open draft PR".into(),
        ));
    }
    let id = pr["node_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| NativeFailure::Refused("PR node id missing".into()))?;
    let result = api.graphql("mutation($id:ID!){markPullRequestReadyForReview(input:{pullRequestId:$id}){pullRequest{id isDraft}}}",json!({"id":id})).await.map_err(NativeFailure::Uncertain)?;
    if result.get("errors").is_some_and(|e| !e.is_null()) {
        return Err(NativeFailure::Uncertain(
            "MarkReady mutation returned errors".into(),
        ));
    }
    let observed = api
        .rest("GET", &format!("pulls/{number}"), Value::Null)
        .await
        .map_err(NativeFailure::Uncertain)?;
    if observed["node_id"] != id
        || observed["draft"] != false
        || observed["state"] != "open"
        || observed["head"]["sha"] != commit
        || observed["base"]["sha"] != pr["base"]["sha"]
    {
        return Err(NativeFailure::Uncertain(
            "MarkReady exact PR read-back differs".into(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn publish_approved(
    api: &impl PrApi,
    bound: &super::NativeBinding,
    key: &super::EffectKey,
    request: &super::GitHubRequest,
    number: u64,
    body: &str,
    verdict: ReviewVerdict,
    base: &str,
    evidence: &super::EffectKey,
) -> Result<Value, NativeFailure> {
    if evidence.fence.run != key.fence.run {
        return Err(NativeFailure::Refused(
            "review belongs to another run".into(),
        ));
    }
    let receipt = super::super::native_review::observed_review(
        &bound.context,
        &bound.store,
        evidence,
        base,
        &request.commit,
        &request.tree,
    )?;
    use crate::workflows::review_verdict::CodeVerdict;
    if body != receipt.body
        || receipt.reviewer_session_id.is_none()
        || matches!(
            (verdict, receipt.verdict),
            (
                ReviewVerdict::Approve,
                CodeVerdict::Findings | CodeVerdict::Uncertain
            ) | (
                ReviewVerdict::RequestChanges,
                CodeVerdict::Pass | CodeVerdict::Uncertain
            )
        )
    {
        return Err(NativeFailure::Refused(
            "publication verdict/body differs from independent review evidence".into(),
        ));
    }
    publish(api, number, &request.commit, base, body, verdict).await
}

pub(super) async fn pr(api: &RepoApi<'_>, number: u64, r: &GitHubRequest) -> Result<Value, String> {
    let v = api
        .rest("GET", &format!("pulls/{number}"), Value::Null)
        .await?;
    if v["head"]["sha"] != r.commit || v["head"]["repo"]["full_name"] != r.repository {
        return Err("GitHub PR head/repository differs from approved artifact".into());
    }
    Ok(v)
}
pub(super) async fn branch_head(api: &RepoApi<'_>, branch: &str) -> Result<String, String> {
    let v = api
        .rest("GET", &format!("git/ref/heads/{branch}"), Value::Null)
        .await?;
    v["object"]["sha"]
        .as_str()
        .map(str::to_owned)
        .ok_or("GitHub ref response lacks commit".into())
}

pub(super) fn source_base(
    source: Option<&super::super::neutral_trigger_store::TriggerIntent>,
    observed: &Value,
) -> Result<(), NativeFailure> {
    if let Some(source) = source
        && source.event.kind == "pull_request"
        && source.event.values.get("base") != observed.pointer("/base/sha")
    {
        return Err(NativeFailure::Refused(
            "PR base differs from source generation".into(),
        ));
    }
    Ok(())
}
