//! `GitReads` port — a reversible abstraction over git **read** operations.
//!
//! Every read op the panels need goes through this trait. The CLI adapter
//! ([`CliGitReads`]) delegates to the existing `git_cmd`-based implementations,
//! so introducing the port is a no-behavior-change refactor. A future `gix`
//! adapter (added in later steps) implements the same trait, letting individual
//! ops be flipped from CLI to in-process gix one at a time — gated behind
//! byte-for-byte parity tests — while everything else stays on the CLI.
//!
//! Writes/auth, the displayed unified diff/patch, stash and reflog are NOT part
//! of this port: they stay on the CLI forever.

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use crate::git::{BlameLine, BranchDetail, CommitLogEntry, DiffStats};
use crate::git_graph::RawCommit;

/// Staged / changed working-tree counts derived from `status --porcelain=v2`.
/// `status` is "clean" | "dirty" | "conflict". Untracked entries count toward
/// `changed` (matches the git panel's prior behavior).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusCounts {
    pub status: String,
    pub staged: u32,
    pub changed: u32,
}

/// In-process read operations over a git repository.
///
/// Implementors must produce results indistinguishable from the git CLI — the
/// shootout parity tests assert structural equality of both adapters' output on
/// fixture repos before any op is flipped to gix.
pub(crate) trait GitReads: Send + Sync {
    fn branches_detail(&self, repo: &Path) -> Result<Vec<BranchDetail>, String>;
    fn commit_log(
        &self,
        repo: &Path,
        count: Option<u32>,
        after: Option<String>,
    ) -> Result<Vec<CommitLogEntry>, String>;
    fn graph_commits(&self, repo: &Path, count: u32) -> Result<Vec<RawCommit>, String>;
    fn ahead_behind(&self, repo: &Path, left: &str, right: &str) -> Result<(u32, u32), String>;
    fn worktree_paths(&self, repo: &Path) -> Result<HashMap<String, String>, String>;
    fn status_counts(&self, repo: &Path) -> StatusCounts;
    fn diff_stats(&self, repo: &Path, scope: Option<&str>) -> DiffStats;
    fn blame(&self, repo: &Path, file: &str) -> Result<Vec<BlameLine>, String>;
}

/// CLI adapter — delegates every op to the existing `git_cmd`-based functions.
pub(crate) struct CliGitReads;

impl GitReads for CliGitReads {
    fn branches_detail(&self, repo: &Path) -> Result<Vec<BranchDetail>, String> {
        crate::git::get_branches_detail_impl(repo)
    }

    fn commit_log(
        &self,
        repo: &Path,
        count: Option<u32>,
        after: Option<String>,
    ) -> Result<Vec<CommitLogEntry>, String> {
        crate::git::get_commit_log_impl(repo.to_string_lossy().into_owned(), count, after)
    }

    fn graph_commits(&self, repo: &Path, count: u32) -> Result<Vec<RawCommit>, String> {
        crate::git_graph::raw_commits_cli(repo, count)
    }

    fn ahead_behind(&self, repo: &Path, left: &str, right: &str) -> Result<(u32, u32), String> {
        crate::git::ahead_behind_cli(repo, left, right)
    }

    fn worktree_paths(&self, repo: &Path) -> Result<HashMap<String, String>, String> {
        crate::worktree::get_worktree_paths(repo.to_string_lossy().into_owned())
    }

    fn status_counts(&self, repo: &Path) -> StatusCounts {
        crate::git::status_counts_cli(repo)
    }

    fn diff_stats(&self, repo: &Path, scope: Option<&str>) -> DiffStats {
        crate::git::get_diff_stats_impl(&repo.to_string_lossy(), scope)
    }

    fn blame(&self, repo: &Path, file: &str) -> Result<Vec<BlameLine>, String> {
        crate::git::blame_cli(repo, file)
    }
}

/// Routes each read op to a backend. Today every op is served by the CLI
/// adapter; later steps add a gix adapter and flip ops one at a time.
pub(crate) struct GitReadsRouter {
    cli: CliGitReads,
}

impl GitReadsRouter {
    fn new() -> Self {
        Self { cli: CliGitReads }
    }

    pub(crate) fn branches_detail(&self, repo: &Path) -> Result<Vec<BranchDetail>, String> {
        self.cli.branches_detail(repo)
    }

    pub(crate) fn commit_log(
        &self,
        repo: &Path,
        count: Option<u32>,
        after: Option<String>,
    ) -> Result<Vec<CommitLogEntry>, String> {
        self.cli.commit_log(repo, count, after)
    }

    pub(crate) fn graph_commits(&self, repo: &Path, count: u32) -> Result<Vec<RawCommit>, String> {
        self.cli.graph_commits(repo, count)
    }

    pub(crate) fn ahead_behind(
        &self,
        repo: &Path,
        left: &str,
        right: &str,
    ) -> Result<(u32, u32), String> {
        self.cli.ahead_behind(repo, left, right)
    }

    pub(crate) fn worktree_paths(&self, repo: &Path) -> Result<HashMap<String, String>, String> {
        self.cli.worktree_paths(repo)
    }

    pub(crate) fn status_counts(&self, repo: &Path) -> StatusCounts {
        self.cli.status_counts(repo)
    }

    pub(crate) fn diff_stats(&self, repo: &Path, scope: Option<&str>) -> DiffStats {
        self.cli.diff_stats(repo, scope)
    }

    pub(crate) fn blame(&self, repo: &Path, file: &str) -> Result<Vec<BlameLine>, String> {
        self.cli.blame(repo, file)
    }
}

static ROUTER: OnceLock<GitReadsRouter> = OnceLock::new();

/// Global read-ops router. All git read call sites go through this.
pub(crate) fn git_reads() -> &'static GitReadsRouter {
    ROUTER.get_or_init(GitReadsRouter::new)
}

#[cfg(test)]
pub(crate) mod test_fixtures {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use tempfile::TempDir;

    /// Run a git command in `dir`, panicking with stderr on failure.
    pub(crate) fn run_git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
            .output()
            .unwrap_or_else(|e| panic!("git {args:?} failed to spawn: {e}"));
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Build a representative fixture repo and return its TempDir guard + path.
    ///
    /// Layout: `main` with 3 commits; a `feature` branch 1 commit ahead; a
    /// simulated `refs/remotes/origin/main`; plus a staged, an unstaged, and an
    /// untracked change in the working tree. Enough to exercise every read op.
    pub(crate) fn fixture_repo() -> (TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let git = |args: &[&str]| run_git(&path, args);

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["config", "core.hooksPath", "/dev/null"]);

        std::fs::write(path.join("a.txt"), "a1\na2\na3\n").unwrap();
        git(&["add", "a.txt"]);
        git(&["commit", "-m", "feat: first", "--no-verify"]);

        std::fs::write(path.join("b.txt"), "b1\n").unwrap();
        git(&["add", "b.txt"]);
        git(&["commit", "-m", "feat: second", "--no-verify"]);

        // Feature branch, 1 commit ahead of main.
        git(&["checkout", "-b", "feature"]);
        std::fs::write(path.join("c.txt"), "c1\n").unwrap();
        git(&["add", "c.txt"]);
        git(&["commit", "-m", "feat: feature commit", "--no-verify"]);

        // Back to main, one more commit so the branches diverge.
        git(&["checkout", "main"]);
        std::fs::write(path.join("d.txt"), "d1\n").unwrap();
        git(&["add", "d.txt"]);
        git(&["commit", "-m", "feat: third on main", "--no-verify"]);

        // Simulate a remote branch pointing at main's tip.
        let head = git(&["rev-parse", "HEAD"]);
        git(&["update-ref", "refs/remotes/origin/main", head.trim()]);

        // Dirty working tree: staged edit, unstaged edit, untracked file.
        std::fs::write(path.join("a.txt"), "a1\na2-staged\na3\n").unwrap();
        git(&["add", "a.txt"]);
        std::fs::write(path.join("b.txt"), "b1\nb2-unstaged\n").unwrap();
        std::fs::write(path.join("untracked.txt"), "u\n").unwrap();

        (dir, path)
    }
}

#[cfg(test)]
mod tests {
    use super::test_fixtures::fixture_repo;
    use super::*;

    /// Step 5: the CLI adapter is wired to the legacy direct functions and
    /// produces identical results for every op on a fixture repo. This also
    /// validates the shared fixture harness used by the gix shootout tests.
    #[test]
    fn cli_parity_with_legacy() {
        let (_guard, repo) = fixture_repo();
        let cli = CliGitReads;

        // branches_detail
        let json = |b: &Vec<BranchDetail>| serde_json::to_value(b).unwrap();
        assert_eq!(
            json(&cli.branches_detail(&repo).unwrap()),
            json(&crate::git::get_branches_detail_impl(&repo).unwrap()),
        );

        // commit_log
        let log_a = cli.commit_log(&repo, None, None).unwrap();
        let log_b = crate::git::get_commit_log_impl(
            repo.to_string_lossy().into_owned(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(serde_json::to_value(&log_a).unwrap(), serde_json::to_value(&log_b).unwrap());

        // graph_commits (RawCommit has Debug, not Serialize)
        let g_a = cli.graph_commits(&repo, 200).unwrap();
        let g_b = crate::git_graph::raw_commits_cli(&repo, 200).unwrap();
        assert_eq!(format!("{g_a:?}"), format!("{g_b:?}"));

        // ahead_behind: feature is 1 ahead, 1 behind main.
        assert_eq!(
            cli.ahead_behind(&repo, "main", "feature").unwrap(),
            crate::git::ahead_behind_cli(&repo, "main", "feature").unwrap(),
        );
        assert_eq!(cli.ahead_behind(&repo, "main", "feature").unwrap(), (1, 1));

        // worktree_paths
        assert_eq!(
            cli.worktree_paths(&repo).unwrap(),
            crate::worktree::get_worktree_paths(repo.to_string_lossy().into_owned()).unwrap(),
        );

        // status_counts: staged a.txt, unstaged b.txt, untracked untracked.txt
        let sc = cli.status_counts(&repo);
        assert_eq!(sc, crate::git::status_counts_cli(&repo));
        assert_eq!(sc.status, "dirty");
        assert_eq!(sc.staged, 1);
        assert!(sc.changed >= 2, "unstaged + untracked: {sc:?}");

        // diff_stats
        assert_eq!(
            serde_json::to_value(cli.diff_stats(&repo, None)).unwrap(),
            serde_json::to_value(crate::git::get_diff_stats_impl(
                &repo.to_string_lossy(),
                None
            ))
            .unwrap(),
        );

        // blame on a committed, tracked file
        let bl_a = cli.blame(&repo, "a.txt").unwrap();
        let bl_b = crate::git::blame_cli(&repo, "a.txt").unwrap();
        assert_eq!(
            serde_json::to_value(&bl_a).unwrap(),
            serde_json::to_value(&bl_b).unwrap(),
        );
        assert!(!bl_a.is_empty());
    }
}
