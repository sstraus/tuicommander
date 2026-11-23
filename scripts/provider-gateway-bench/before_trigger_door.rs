//! Story 3151, tree before the provider gateway: one trigger-source pass over
//! the production `RepoApi`. Copied into `workflows/github_poll_bench.rs` of the
//! measured tree by `scripts/provider-gateway-bench-before.sh`; never compiled
//! in this tree.

use super::{github_event::poll_snapshots, github_trigger::GithubResource};
use crate::{github_pr_api::RepoApi, state::AppState};
use serde_json::Value;

/// `issues` and `pulls` are the open resources the source already knows.
/// Returns the snapshot count.
pub(crate) async fn source_pass(
    state: &AppState,
    path: &str,
    issues: &[Value],
    pulls: &[Value],
) -> Result<usize, String> {
    let api = RepoApi::resolve(state, path).await?;
    let repository = format!("{}/{}", api.owner, api.repo).to_ascii_lowercase();
    let mut snapshots = 0;
    for (kind, open) in [
        (GithubResource::Issue, issues),
        (GithubResource::PullRequest, pulls),
    ] {
        let known: Vec<i64> = open.iter().filter_map(|v| v["number"].as_i64()).collect();
        snapshots += poll_snapshots(&api, &repository, kind, &known)
            .await?
            .snapshots
            .len();
    }
    Ok(snapshots)
}
