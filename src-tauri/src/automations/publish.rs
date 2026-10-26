//! Draft PR publication for event runs. The allowlist is checked here, at the
//! only place a PR is created, independently of how the run was admitted.
use super::model::EventIntake;
use crate::github_pr_api::PrApi;

pub(crate) struct DraftPr<'a> {
    /// The repository the run was admitted for.
    pub repository: &'a str,
    pub head: &'a str,
    pub base: &'a str,
    pub issue_number: i64,
}

/// A branch name must not look like an option or carry whitespace and control
/// characters; a detached checkout reports `HEAD`, which names no branch.
fn plain_ref(value: &str) -> bool {
    !value.is_empty()
        && value != "HEAD"
        && !value.starts_with('-')
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Create a draft PR through the GitHub HTTP layer. `api` is bound to one
/// repository (`bound_repository`, resolved from the workspace remote); the PR
/// is created only when that repository is the admitted one and is allowlisted.
pub(crate) async fn publish_draft_pr(
    intake: &EventIntake,
    api: &impl PrApi,
    bound_repository: &str,
    request: &DraftPr<'_>,
) -> Result<String, String> {
    if !intake.allows(request.repository) || !intake.allows(bound_repository) {
        return Err(format!(
            "Refusing to publish: {} is not in the event intake allowlist",
            request.repository
        ));
    }
    if !request.repository.eq_ignore_ascii_case(bound_repository) {
        return Err("Refusing to publish: workspace remote is not the admitted repository".into());
    }
    if !plain_ref(request.head) || !plain_ref(request.base) {
        return Err("Refusing to publish: invalid head or base branch".into());
    }
    let created = crate::github::create_pr_with(
        api,
        &format!("Resolve issue #{}", request.issue_number),
        &format!(
            "Closes #{}\n\nDraft opened by a TUICommander event run.",
            request.issue_number
        ),
        request.base,
        request.head,
        true,
    )
    .await?;
    Ok(created.url)
}

/// Trimmed stdout of `git` in `workspace`; used to name the branch to publish.
pub(crate) fn run_git(workspace: &std::path::Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .current_dir(workspace)
        .args(args)
        .output()
        .map_err(|e| format!("git unavailable: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(format!("git {} failed", args.join(" ")))
    }
}

#[cfg(test)]
mod tests;
