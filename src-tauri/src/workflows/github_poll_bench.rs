//! Test-only door for the provider gateway benchmark: one trigger-source pass
//! over the production `RepoApi`, as `github_poll::observe` runs it after a poll
//! tick. `github_event` is private to this module tree, so the benchmark enters
//! here.

use super::{
    github_event::{from_rest, poll_snapshots},
    github_trigger::GithubResource,
};
use crate::{github::Origin, github_pr_api::RepoApi, state::AppState};
use serde_json::Value;

/// `issues` and `pulls` are the open resources in REST shape. With
/// `reuse_polled` they stand for the lists a poll tick already downloaded;
/// without it the source reads the REST pages itself. Returns the snapshot count.
pub(crate) async fn source_pass(
    state: &AppState,
    path: &str,
    issues: &[Value],
    pulls: &[Value],
    reuse_polled: bool,
) -> Result<usize, String> {
    let origin = Origin::background("workflow.github_poll.observe");
    let api = RepoApi::resolve(state, path, origin).await?;
    let repository = format!("{}/{}", api.owner, api.repo).to_ascii_lowercase();
    let mut snapshots = 0;
    for (kind, open) in [
        (GithubResource::Issue, issues),
        (GithubResource::PullRequest, pulls),
    ] {
        let known: Vec<i64> = open.iter().filter_map(|v| v["number"].as_i64()).collect();
        let listed = if reuse_polled {
            Some(
                open.iter()
                    .map(|value| from_rest(&repository, kind, value))
                    .collect::<Result<Vec<_>, _>>()?,
            )
        } else {
            None
        };
        snapshots += poll_snapshots(&api, &repository, kind, &known, listed)
            .await?
            .snapshots
            .len();
    }
    Ok(snapshots)
}
