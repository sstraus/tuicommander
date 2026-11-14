//! Reuse the account-aware GitHub API; no credential or alternate HTTP client.
use super::github_trigger::{GithubResource, GithubSnapshot};
use crate::github_pr_api::PrApi;
use serde_json::Value;
use std::collections::BTreeSet;

pub(crate) async fn read_snapshot(
    api: &impl PrApi,
    repository: &str,
    kind: GithubResource,
    number: i64,
) -> Result<GithubSnapshot, String> {
    if number <= 0 {
        return Err("GitHub resource number must be positive".into());
    }
    let endpoint = match kind {
        GithubResource::Issue => "issues",
        GithubResource::PullRequest => "pulls",
    };
    let value = api
        .rest("GET", &format!("{endpoint}/{number}"), Value::Null)
        .await?;
    let snapshot = from_rest(repository, kind, &value)?;
    if snapshot.number != number {
        return Err("GitHub response number differs from requested resource".into());
    }
    Ok(snapshot)
}
/// REST API version 2026-03-10: issues/get and pulls/get. Sender is deliberately
/// absent: REST author/user does not identify the actor of the latest update.
pub(crate) fn from_rest(
    repository: &str,
    kind: GithubResource,
    value: &Value,
) -> Result<GithubSnapshot, String> {
    let string = |pointer: &str| {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("GitHub snapshot missing {pointer}"))
    };
    if kind == GithubResource::Issue && value.get("pull_request").is_some() {
        return Err("pull request cannot masquerade as an issue trigger".into());
    }
    let state = string("/state")?;
    if !matches!(state.as_str(), "open" | "closed") {
        return Err("unsupported GitHub resource state".into());
    }
    let title = string("/title")?;
    let body = match value.get("body") {
        Some(Value::String(body)) => body.as_str(),
        Some(Value::Null) => "",
        _ => return Err("GitHub snapshot missing body".into()),
    };
    let labels = value["labels"]
        .as_array()
        .ok_or("GitHub snapshot missing labels")?
        .iter()
        .map(|label| {
            label["name"]
                .as_str()
                .map(str::to_owned)
                .ok_or("GitHub label missing name")
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let snapshot = GithubSnapshot {
        repository: super::github_trigger::repository(repository)?,
        number: value["number"]
            .as_i64()
            .ok_or("GitHub snapshot missing number")?,
        kind,
        open: state == "open",
        title: title.clone(),
        material_digest: tuic_git::github::requirements_digest(&title, body),
        labels,
        head: if kind == GithubResource::PullRequest {
            Some(string("/head/sha")?)
        } else {
            None
        },
        base: if kind == GithubResource::PullRequest {
            Some(string("/base/sha")?)
        } else {
            None
        },
        updated_at: string("/updated_at")?,
        actor: None,
    };
    snapshot.validate()?;
    Ok(snapshot)
}

/// A truncated source list cannot prove that every eligible resource was seen.
pub(crate) async fn open_snapshots(
    api: &impl PrApi,
    kind: GithubResource,
    repository: &str,
) -> Result<Vec<GithubSnapshot>, String> {
    let endpoint = match kind {
        GithubResource::Issue => "issues",
        GithubResource::PullRequest => "pulls",
    };
    let mut snapshots = Vec::new();
    for page in 1..=10 {
        let list = api
            .rest(
                "GET",
                &format!("{endpoint}?state=open&per_page=100&page={page}"),
                Value::Null,
            )
            .await?;
        let list = list
            .as_array()
            .ok_or("GitHub open resource list is not an array")?;
        if list.len() > 100 {
            return Err("GitHub open resource page exceeds bound".into());
        }
        for resource in list {
            // GitHub's issues endpoint includes PRs; those have their own source.
            if kind == GithubResource::Issue && resource.get("pull_request").is_some() {
                continue;
            }
            snapshots.push(from_rest(repository, kind, resource)?);
        }
        if list.len() < 100 {
            return Ok(snapshots);
        }
    }
    Err("GitHub trigger discovery exceeded complete-page bound".into())
}

pub(crate) struct PollSnapshots {
    pub snapshots: Vec<GithubSnapshot>,
    pub unavailable: Vec<i64>,
}
/// Complete list payloads are authoritative; only vanished known-open resources
/// need an individual GET to distinguish closure from deletion/transfer.
pub(crate) async fn poll_snapshots(
    api: &impl PrApi,
    repository: &str,
    kind: GithubResource,
    known: &[i64],
) -> Result<PollSnapshots, String> {
    let mut snapshots = open_snapshots(api, kind, repository).await?;
    let listed = snapshots.iter().map(|s| s.number).collect::<BTreeSet<_>>();
    let missing = known.iter().copied().collect::<BTreeSet<_>>();
    let mut unavailable = Vec::new();
    for number in missing.difference(&listed) {
        match read_snapshot(api, repository, kind, *number).await {
            Ok(snapshot) => snapshots.push(snapshot),
            Err(error) => {
                // PrApi preserves the installed http_error_message status prefix.
                // All unknown errors remain retryable; body text is not status.
                let retired = error.starts_with("GitHub API error (404 ")
                    || error.starts_with("GitHub API error (410 ")
                    || error == "GitHub response number differs from requested resource";
                tracing::warn!(source="workflows",number,%error,retired,"Neutral GitHub resource read failed; transient candidates remain for retry");
                if retired {
                    unavailable.push(*number);
                }
            }
        }
    }
    Ok(PollSnapshots {
        snapshots,
        unavailable,
    })
}
