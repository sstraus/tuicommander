//! Atomic deletion of a branch whose tip is preserved by a remote-tracking ref.

use crate::git_cli::git_cmd;
use std::path::Path;

/// Capture the containing ref and its OID together, then lock that proof while
/// compare-deleting the branch. A concurrent fetch must not invalidate the proof.
pub(crate) fn delete_remote_contained_branch(
    repo: &Path,
    branch: &str,
    tip: &str,
) -> Result<String, String> {
    let refs = git_cmd(repo)
        .args([
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            &format!("--contains={tip}"),
            "refs/remotes/",
        ])
        .run()
        .map_err(|error| format!("could not inspect remote containment: {error}"))?;
    let (proof_ref, proof_oid) = refs
        .stdout
        .lines()
        .next()
        .and_then(|line| line.split_once(' '))
        .ok_or_else(|| "remote refs no longer contain the captured branch tip".to_string())?;
    git_cmd(repo)
        .args(["update-ref", "--stdin"])
        .run_with_input(&format!(
            "start\nverify {proof_ref} {proof_oid}\noption no-deref\ndelete refs/heads/{branch} {tip}\ncommit\n"
        ))
        .map_err(|error| error.to_string())?;
    Ok(proof_ref.to_string())
}
