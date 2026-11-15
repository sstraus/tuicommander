//! Deterministic facts about a stash, and the one guarded way to drop it
//! after a "Check" verdict. The model only classifies; whether a stash may be
//! dropped is decided here, from git, never from the model's words.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::git_cli::git_cmd;

/// Longest diff passed to the prompt, in bytes (a prompt variable is capped at 50 000). A longer one is cut and marked.
const MAX_PROMPT_DIFF_BYTES: usize = 30_000;

/// Namespace that keeps the commit of every stash dropped by a check.
const ARCHIVE_PREFIX: &str = "refs/archive/stash/";

/// One file the stash changes, and whether HEAD already holds the stash's version of it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StashFileFact {
    pub path: String,
    pub in_head: bool,
}

/// Everything the prompt and the drop guard know about one stash.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StashFacts {
    pub stash_ref: String,
    pub hash: String,
    pub message: String,
    pub head_branch: String,
    /// Branch the stash was made on, parsed from its message.
    pub branch: Option<String>,
    pub branch_exists: bool,
    pub branch_merged_into_head: bool,
    /// The stash also holds untracked files. Those are never compared, so
    /// such a stash is never proven redundant.
    pub has_untracked: bool,
    pub files: Vec<StashFileFact>,
    /// Every changed file has the stash's content in HEAD, and there is at least one.
    pub all_in_head: bool,
    pub diff: String,
}

/// Outcome of a drop request after a check.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StashCheckOutcome {
    pub dropped: bool,
    pub reason: String,
    pub archive_ref: Option<String>,
    /// Command that puts the dropped stash back in the stash list.
    pub restore: Option<String>,
}

fn git_out(repo: &Path, args: &[&str]) -> Result<String, String> {
    git_cmd(repo)
        .args(args)
        .run()
        .map(|o| o.stdout)
        .map_err(|e| format!("git {} failed: {e}", args.first().copied().unwrap_or("")))
}

fn git_ok(repo: &Path, args: &[&str]) -> bool {
    git_cmd(repo).args(args).run_silent().is_some()
}

/// `On <branch>: ...` or `WIP on <branch>: ...`.
fn branch_from_message(message: &str) -> Option<String> {
    let rest = message
        .strip_prefix("WIP on ")
        .or_else(|| message.strip_prefix("On "))?;
    let name = rest.split(':').next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn validate_stash_ref(stash_ref: &str) -> Result<(), String> {
    let inner = stash_ref
        .strip_prefix("stash@{")
        .and_then(|r| r.strip_suffix('}'));
    match inner {
        Some(n) if n.parse::<u32>().is_ok() => Ok(()),
        _ => Err(format!("Invalid stash ref: '{stash_ref}'")),
    }
}

pub fn stash_facts_blocking(path: String, stash_ref: String) -> Result<StashFacts, String> {
    validate_stash_ref(&stash_ref)?;
    let repo = PathBuf::from(path);
    let hash = git_out(&repo, &["rev-parse", "--verify", &stash_ref])?
        .trim()
        .to_string();
    let message = git_out(&repo, &["log", "-1", "--format=%s", &hash])?
        .trim()
        .to_string();
    let base = format!("{hash}^1");
    let head_branch = git_out(&repo, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();

    let names = git_out(
        &repo,
        &["diff", "--name-only", "--no-renames", "-z", &base, &hash],
    )?;
    let files: Vec<StashFileFact> = names
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(|p| StashFileFact {
            path: p.to_string(),
            // Equal content, mode included. An error reads as "differs": the safe side.
            in_head: git_ok(&repo, &["diff", "--quiet", &hash, "HEAD", "--", p]),
        })
        .collect();
    let all_in_head = !files.is_empty() && files.iter().all(|f| f.in_head);

    let branch = branch_from_message(&message);
    let (branch_exists, branch_merged_into_head) = match &branch {
        Some(b) => {
            let full = format!("refs/heads/{b}");
            let exists = git_ok(&repo, &["rev-parse", "--verify", "-q", &full]);
            let merged = exists && git_ok(&repo, &["merge-base", "--is-ancestor", &full, "HEAD"]);
            (exists, merged)
        }
        None => (false, false),
    };
    let has_untracked = git_ok(
        &repo,
        &["rev-parse", "--verify", "-q", &format!("{hash}^3")],
    );

    let mut diff = git_out(&repo, &["diff", "--no-color", &base, &hash])?;
    if diff.len() > MAX_PROMPT_DIFF_BYTES {
        let mut cut = MAX_PROMPT_DIFF_BYTES;
        while !diff.is_char_boundary(cut) {
            cut -= 1;
        }
        diff.truncate(cut);
        diff.push_str("\n[diff truncated]\n");
    }

    Ok(StashFacts {
        stash_ref,
        hash,
        message,
        head_branch,
        branch,
        branch_exists,
        branch_merged_into_head,
        has_untracked,
        files,
        all_in_head,
        diff,
    })
}

/// The only rule that may drop a stash: the model said redundant or obsolete
/// AND git proves every changed file already has the stash's content in HEAD.
fn drop_refusal(facts: &StashFacts, verdict: &str) -> Option<String> {
    if !matches!(verdict, "redundant" | "obsolete") {
        return Some(format!("verdict \"{verdict}\" never drops a stash"));
    }
    if facts.has_untracked {
        return Some("the stash holds untracked files, which are not compared".into());
    }
    if facts.files.is_empty() {
        return Some("the stash changes no tracked file, nothing to prove".into());
    }
    let missing: Vec<&str> = facts
        .files
        .iter()
        .filter(|f| !f.in_head)
        .map(|f| f.path.as_str())
        .collect();
    if !missing.is_empty() {
        return Some(format!(
            "not proven: HEAD lacks the stash's version of {}",
            missing.join(", ")
        ));
    }
    None
}

/// Drop `stash_ref` only if `verdict` and git both allow it. The stash commit
/// is kept under `refs/archive/stash/<hash>` first; a failed archive aborts the drop.
pub fn stash_check_drop_blocking(
    path: String,
    stash_ref: String,
    expected_hash: String,
    verdict: String,
) -> Result<StashCheckOutcome, String> {
    let facts = stash_facts_blocking(path.clone(), stash_ref.clone())?;
    if facts.hash != expected_hash {
        return Err(format!(
            "{stash_ref} changed since the check (now {}); run the check again",
            facts.hash
        ));
    }
    if let Some(reason) = drop_refusal(&facts, &verdict) {
        return Ok(StashCheckOutcome {
            dropped: false,
            reason,
            archive_ref: None,
            restore: None,
        });
    }
    let repo = PathBuf::from(path);
    let archive_ref = format!("{ARCHIVE_PREFIX}{}", facts.hash);
    git_out(&repo, &["update-ref", &archive_ref, &facts.hash])?;
    let kept = git_out(&repo, &["rev-parse", "--verify", &archive_ref])?;
    if kept.trim() != facts.hash {
        return Err(format!(
            "could not archive {} under {archive_ref}",
            facts.hash
        ));
    }
    git_out(&repo, &["stash", "drop", &stash_ref])?;
    let restore = format!(
        "git stash store -m {} {}",
        shell_quote(&facts.message),
        facts.hash
    );
    Ok(StashCheckOutcome {
        dropped: true,
        reason: "every changed file already has the stash's content in HEAD".into(),
        archive_ref: Some(archive_ref),
        restore: Some(restore),
    })
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::setup_test_repo_with_commit;

    fn git(repo: &Path, args: &[&str]) -> String {
        git_out(repo, args).unwrap()
    }

    fn stash_file(repo: &Path, name: &str, content: &str) {
        std::fs::write(repo.join(name), content).unwrap();
        git(repo, &["add", name]);
        git(repo, &["stash", "push", "-m", "wip"]);
    }

    fn list(repo: &Path) -> String {
        git(repo, &["stash", "list"])
    }

    // Catches: a stash with a hunk missing from HEAD is dropped automatically.
    #[test]
    fn stash_with_a_hunk_missing_from_head_is_never_dropped() {
        let (_t, repo) = setup_test_repo_with_commit();
        stash_file(&repo, "initial.txt", "changed only in the stash\n");
        let facts =
            stash_facts_blocking(repo.to_string_lossy().into(), "stash@{0}".into()).unwrap();
        assert!(!facts.all_in_head);
        let out = stash_check_drop_blocking(
            repo.to_string_lossy().into(),
            "stash@{0}".into(),
            facts.hash.clone(),
            "redundant".into(),
        )
        .unwrap();
        assert!(!out.dropped, "{out:?}");
        assert!(list(&repo).contains("stash@{0}"));
        assert!(out.archive_ref.is_none());
    }

    // Catches: "still useful" or "unsure" dropping a stash that is provably in HEAD.
    #[test]
    fn other_verdicts_never_drop_even_when_head_has_everything() {
        let (_t, repo) = setup_test_repo_with_commit();
        stash_file(&repo, "initial.txt", "same\n");
        std::fs::write(repo.join("initial.txt"), "same\n").unwrap();
        git(&repo, &["commit", "-qam", "land it"]);
        let hash = stash_facts_blocking(repo.to_string_lossy().into(), "stash@{0}".into())
            .unwrap()
            .hash;
        for verdict in ["useful", "unsure", ""] {
            let out = stash_check_drop_blocking(
                repo.to_string_lossy().into(),
                "stash@{0}".into(),
                hash.clone(),
                verdict.into(),
            )
            .unwrap();
            assert!(!out.dropped, "{verdict}: {out:?}");
        }
        assert!(list(&repo).contains("stash@{0}"));
    }

    // Catches: an untracked file inside the stash being ignored by the proof.
    #[test]
    fn stash_holding_untracked_files_is_never_proven_redundant() {
        let (_t, repo) = setup_test_repo_with_commit();
        std::fs::write(repo.join("initial.txt"), "same\n").unwrap();
        std::fs::write(repo.join("new.txt"), "only here\n").unwrap();
        git(&repo, &["stash", "push", "-u", "-m", "wip"]);
        std::fs::write(repo.join("initial.txt"), "same\n").unwrap();
        git(&repo, &["commit", "-qam", "land it"]);
        let facts =
            stash_facts_blocking(repo.to_string_lossy().into(), "stash@{0}".into()).unwrap();
        assert!(facts.has_untracked);
        let out = stash_check_drop_blocking(
            repo.to_string_lossy().into(),
            "stash@{0}".into(),
            facts.hash,
            "redundant".into(),
        )
        .unwrap();
        assert!(!out.dropped, "{out:?}");
    }

    // Catches: dropping a different stash after the list shifted between check and drop.
    #[test]
    fn a_shifted_stash_list_aborts_the_drop() {
        let (_t, repo) = setup_test_repo_with_commit();
        stash_file(&repo, "initial.txt", "one\n");
        let old = stash_facts_blocking(repo.to_string_lossy().into(), "stash@{0}".into())
            .unwrap()
            .hash;
        stash_file(&repo, "initial.txt", "two\n");
        let err = stash_check_drop_blocking(
            repo.to_string_lossy().into(),
            "stash@{0}".into(),
            old,
            "redundant".into(),
        )
        .unwrap_err();
        assert!(err.contains("changed since the check"), "{err}");
        assert_eq!(list(&repo).lines().count(), 2);
    }
}
