//! Review one pull request by hand with the project's PR review workflow.
//!
//! The project's GitHub `pull_request` trigger binding identifies that workflow:
//! it pins the publication and owns the input mapping and limits. GitHub
//! supplies the exact head and base when the user asks, never the caller, so a
//! push between page load and click cannot review an old head. The request goes
//! through the same mapping as trigger admission and the same neutral start.
use super::neutral::NeutralStartRequest;
use super::neutral_triggers::trigger_request;
use crate::github_pr_api::PrApi;
use crate::workflows::{
    WorkflowStore,
    catalog::{HandlerRef, WorkflowCatalog},
    github_event::read_snapshot,
    github_trigger::{GithubAction, GithubResource, repository, snapshot_schema},
    neutral_api::refusal,
    neutral_publication::PublicationRef,
    trigger::{NeutralTriggerBinding, TriggerEvent, TriggerSource},
};
use serde::Serialize;
use std::collections::BTreeSet;

/// Identity of a pull request taken from its web URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PullRequestUrl {
    pub host: String,
    /// Lowercase `owner/name`.
    pub repository: String,
    pub number: i64,
}

/// Accepts `https://<host>/<owner>/<name>/pull/<number>` with optional trailing
/// path (`/files`), query or fragment.
pub(crate) fn parse_url(url: &str) -> Result<PullRequestUrl, String> {
    let invalid = || format!("{}{url}", refusal::PR_URL_INVALID);
    let parsed = url::Url::parse(url).map_err(|_| invalid())?;
    let host = parsed
        .host_str()
        .filter(|_| parsed.scheme() == "https")
        .ok_or_else(invalid)?
        .to_ascii_lowercase();
    let segments: Vec<&str> = parsed.path_segments().ok_or_else(invalid)?.collect();
    let [owner, name, "pull", number, ..] = segments.as_slice() else {
        return Err(invalid());
    };
    let number = number
        .parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(invalid)?;
    Ok(PullRequestUrl {
        host,
        repository: repository(&format!("{owner}/{name}")).map_err(|_| invalid())?,
        number,
    })
}

/// One binding that can review the pull request, as the UI lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct PrReviewTarget {
    pub binding_id: String,
    pub enabled: bool,
    pub publication: PublicationRef,
    /// The draft name of the publication; the publication id when unnamed.
    pub name: String,
}

/// The project's GitHub `pull_request` bindings for exactly this repository.
/// A disabled binding still counts: manual review does not need automatic intake.
pub(crate) fn bindings(
    workflows: &WorkflowStore,
    project: &str,
    repository: &str,
) -> Result<Vec<NeutralTriggerBinding>, String> {
    let prefix = format!("github:{repository}/");
    Ok(workflows
        .stored_neutral_trigger_bindings(project)?
        .into_iter()
        .filter(|b| {
            b.draft.source == TriggerSource::Github
                && b.draft.kind == "pull_request"
                && b.draft.resource_prefix == prefix
        })
        .collect())
}

pub(crate) fn targets(
    workflows: &WorkflowStore,
    project: &str,
    repository: &str,
) -> Result<Vec<PrReviewTarget>, String> {
    Ok(bindings(workflows, project, repository)?
        .into_iter()
        .map(|b| PrReviewTarget {
            name: workflows
                .get_neutral_draft(project, &b.draft.publication.id)
                .map(|d| d.name)
                .unwrap_or_else(|_| b.draft.publication.id.clone()),
            binding_id: b.draft.id,
            enabled: b.enabled,
            publication: b.draft.publication,
        })
        .collect())
}

/// Never guesses: several bindings require the caller to name one.
pub(crate) fn select(
    candidates: Vec<NeutralTriggerBinding>,
    repository: &str,
    chosen: Option<&str>,
) -> Result<NeutralTriggerBinding, String> {
    if let Some(id) = chosen {
        return candidates
            .into_iter()
            .find(|b| b.draft.id == id)
            .ok_or_else(|| format!("{}{id}", refusal::PR_REVIEW_BINDING_NOT_FOUND));
    }
    let mut candidates = candidates;
    match candidates.len() {
        0 => Err(format!("{}{repository}", refusal::PR_REVIEW_NOT_CONFIGURED)),
        1 => Ok(candidates.remove(0)),
        _ => Err(format!(
            "{}{}",
            refusal::PR_REVIEW_AMBIGUOUS,
            candidates
                .iter()
                .map(|b| b.draft.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Reads the pull request now and maps it through the binding. The request id
/// names the binding revision, head and base: asking again for the same head
/// returns the run that already reviews it instead of a second review.
pub(crate) async fn start_request(
    api: &impl PrApi,
    workflows: &WorkflowStore,
    catalog: &WorkflowCatalog,
    handlers: &BTreeSet<HandlerRef>,
    project: &str,
    binding: &NeutralTriggerBinding,
    pull: &PullRequestUrl,
) -> Result<NeutralStartRequest, String> {
    let snapshot = read_snapshot(
        api,
        &pull.repository,
        GithubResource::PullRequest,
        pull.number,
    )
    .await?;
    if !snapshot.open {
        return Err(format!("{}{}", refusal::PR_NOT_OPEN, pull.number));
    }
    let event = TriggerEvent {
        source: TriggerSource::Github,
        kind: "pull_request".into(),
        resource: snapshot.resource()?,
        generation: "manual".into(),
        event: "manual".into(),
        delivery_id: None,
        schema: snapshot_schema(GithubResource::PullRequest),
        values: snapshot.values(GithubAction::Manual),
        actor: None,
    };
    let mut request = trigger_request(workflows, catalog, handlers, project, binding, &event)?;
    request.request_id = format!(
        "pr-review:{}@{}:{}#{}:{}:{}",
        binding.draft.id,
        binding.revision,
        pull.repository,
        pull.number,
        snapshot.head.unwrap_or_default(),
        snapshot.base.unwrap_or_default(),
    );
    Ok(request)
}
