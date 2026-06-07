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
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

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

/// In-process gix adapter. Implements the same port as the CLI adapter; ops are
/// flipped to gix one at a time in later steps, each gated by a parity test.
///
/// Repository handles are cached: opening a `gix::Repository` reads config,
/// refs and the object DB layout, so we keep a `ThreadSafeRepository` per path
/// and cheaply derive a thread-local `Repository` per call.
pub(crate) struct GixGitReads {
    handles: moka::sync::Cache<PathBuf, gix::ThreadSafeRepository>,
}

impl GixGitReads {
    pub(crate) fn new() -> Self {
        Self {
            handles: moka::sync::Cache::builder().max_capacity(64).build(),
        }
    }

    /// Open (or reuse a cached) repository handle for `repo` and return a
    /// thread-local `Repository` for use on the current thread.
    pub(crate) fn repo(&self, repo: &Path) -> Result<gix::Repository, String> {
        let tsr = self
            .handles
            .try_get_with(repo.to_path_buf(), || {
                // Box the (large) open error to keep the Result small.
                gix::ThreadSafeRepository::open(repo).map_err(Box::new)
            })
            .map_err(|e: Arc<Box<gix::open::Error>>| e.to_string())?;
        Ok(tsr.to_thread_local())
    }
}

impl GitReads for GixGitReads {
    fn branches_detail(&self, repo: &Path) -> Result<Vec<BranchDetail>, String> {
        use gix::bstr::ByteSlice;
        let grepo = self.repo(repo)?;

        // Current branch short name (None when HEAD is detached).
        let head_short: Option<String> = grepo
            .head_ref()
            .map_err(|e| e.to_string())?
            .map(|r| r.name().shorten().to_str_lossy().into_owned());

        let merged = crate::git::merged_branch_set(repo);

        let refs = grepo.references().map_err(|e| e.to_string())?;
        let mut branches: Vec<BranchDetail> = Vec::new();
        for r in refs.all().map_err(|e| e.to_string())? {
            let Ok(mut reference) = r else { continue };

            let full = reference.name().as_bstr().to_str_lossy().into_owned();
            if !(full.starts_with("refs/heads/") || full.starts_with("refs/remotes/")) {
                continue;
            }
            let name = reference.name().shorten().to_str_lossy().into_owned();
            // Skip the synthetic origin/HEAD (and any */HEAD) pointer.
            if name == "origin/HEAD" || name.ends_with("/HEAD") {
                continue;
            }
            let is_remote = full.starts_with("refs/remotes/");
            let is_current = !is_remote && head_short.as_deref() == Some(name.as_str());

            // %(upstream:short) — local branches only.
            let upstream: Option<String> = if is_remote {
                None
            } else {
                reference
                    .remote_tracking_ref_name(gix::remote::Direction::Fetch)
                    .and_then(|res| res.ok())
                    .map(|full_ref| full_ref.shorten().to_str_lossy().into_owned())
            };

            // ahead/behind vs upstream. Deferred to the ahead_behind backend
            // (CLI until Step 9). Replicate git's %(upstream:track) quirk: a
            // count of 0 renders as no token, i.e. None.
            let (ahead, behind) = match &upstream {
                Some(u) => match git_reads().ahead_behind(repo, &name, u) {
                    Ok((a, b)) => ((a > 0).then_some(a), (b > 0).then_some(b)),
                    Err(_) => (None, None),
                },
                None => (None, None),
            };

            let commit = reference.peel_to_commit().map_err(|e| e.to_string())?;
            let last_commit_date = Some(
                commit
                    .time()
                    .map_err(|e| e.to_string())?
                    .format(gix::date::time::format::ISO8601)
                    .map_err(|e| e.to_string())?,
            );
            let last_commit_author = {
                let a = commit.author().map_err(|e| e.to_string())?;
                let s = a.name.to_str_lossy().trim().to_string();
                (!s.is_empty()).then_some(s)
            };
            let last_commit_message = {
                let msg = commit.message().map_err(|e| e.to_string())?;
                let s = msg.summary().to_str_lossy().trim().to_string();
                (!s.is_empty()).then_some(s)
            };

            let is_merged = merged.contains(&name);
            branches.push(BranchDetail {
                is_main: crate::git::is_main_branch(&name),
                name,
                is_current,
                is_remote,
                is_merged,
                ahead,
                behind,
                upstream,
                last_commit_date,
                last_commit_message,
                last_commit_author,
                base_ahead: None,
                base_behind: None,
                base_branch: None,
            });
        }

        crate::git::apply_base_ahead_behind_and_sort(repo, &mut branches);
        Ok(branches)
    }

    fn commit_log(
        &self,
        repo: &Path,
        _count: Option<u32>,
        _after: Option<String>,
    ) -> Result<Vec<CommitLogEntry>, String> {
        let _repo = self.repo(repo)?;
        unimplemented!("gix commit_log — Step 8")
    }

    fn graph_commits(&self, repo: &Path, _count: u32) -> Result<Vec<RawCommit>, String> {
        let _repo = self.repo(repo)?;
        unimplemented!("gix graph_commits — Step 8")
    }

    fn ahead_behind(&self, repo: &Path, _left: &str, _right: &str) -> Result<(u32, u32), String> {
        let _repo = self.repo(repo)?;
        unimplemented!("gix ahead_behind — Step 9")
    }

    fn worktree_paths(&self, repo: &Path) -> Result<HashMap<String, String>, String> {
        let _repo = self.repo(repo)?;
        unimplemented!("gix worktree_paths — Step 10")
    }

    fn status_counts(&self, repo: &Path) -> StatusCounts {
        let _repo = self.repo(repo);
        unimplemented!("gix status_counts — Step 11")
    }

    fn diff_stats(&self, repo: &Path, _scope: Option<&str>) -> DiffStats {
        let _repo = self.repo(repo);
        unimplemented!("gix diff_stats — Step 12")
    }

    fn blame(&self, repo: &Path, _file: &str) -> Result<Vec<BlameLine>, String> {
        let _repo = self.repo(repo)?;
        unimplemented!("gix blame — Step 13")
    }
}

/// Which backend serves a given read op.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Backend {
    Cli,
    Gix,
}

/// Per-op backend selection. Every op defaults to `Cli`; flipping an op to gix
/// (after its parity test is green) is a one-line change here.
#[derive(Clone, Copy)]
struct PerOpBackend {
    branches_detail: Backend,
    commit_log: Backend,
    graph_commits: Backend,
    ahead_behind: Backend,
    worktree_paths: Backend,
    status_counts: Backend,
    diff_stats: Backend,
    blame: Backend,
}

impl Default for PerOpBackend {
    fn default() -> Self {
        Self {
            // Flipped to gix in Step 7 (parity test: shootout_branches).
            branches_detail: Backend::Gix,
            commit_log: Backend::Cli,
            graph_commits: Backend::Cli,
            ahead_behind: Backend::Cli,
            worktree_paths: Backend::Cli,
            status_counts: Backend::Cli,
            diff_stats: Backend::Cli,
            blame: Backend::Cli,
        }
    }
}

/// Routes each read op to its configured backend (CLI or gix).
pub(crate) struct GitReadsRouter {
    cli: CliGitReads,
    gix: GixGitReads,
    backend: PerOpBackend,
}

impl GitReadsRouter {
    fn new() -> Self {
        Self {
            cli: CliGitReads,
            gix: GixGitReads::new(),
            backend: PerOpBackend::default(),
        }
    }

    pub(crate) fn branches_detail(&self, repo: &Path) -> Result<Vec<BranchDetail>, String> {
        match self.backend.branches_detail {
            Backend::Cli => self.cli.branches_detail(repo),
            Backend::Gix => self.gix.branches_detail(repo),
        }
    }

    pub(crate) fn commit_log(
        &self,
        repo: &Path,
        count: Option<u32>,
        after: Option<String>,
    ) -> Result<Vec<CommitLogEntry>, String> {
        match self.backend.commit_log {
            Backend::Cli => self.cli.commit_log(repo, count, after),
            Backend::Gix => self.gix.commit_log(repo, count, after),
        }
    }

    pub(crate) fn graph_commits(&self, repo: &Path, count: u32) -> Result<Vec<RawCommit>, String> {
        match self.backend.graph_commits {
            Backend::Cli => self.cli.graph_commits(repo, count),
            Backend::Gix => self.gix.graph_commits(repo, count),
        }
    }

    pub(crate) fn ahead_behind(
        &self,
        repo: &Path,
        left: &str,
        right: &str,
    ) -> Result<(u32, u32), String> {
        match self.backend.ahead_behind {
            Backend::Cli => self.cli.ahead_behind(repo, left, right),
            Backend::Gix => self.gix.ahead_behind(repo, left, right),
        }
    }

    pub(crate) fn worktree_paths(&self, repo: &Path) -> Result<HashMap<String, String>, String> {
        match self.backend.worktree_paths {
            Backend::Cli => self.cli.worktree_paths(repo),
            Backend::Gix => self.gix.worktree_paths(repo),
        }
    }

    pub(crate) fn status_counts(&self, repo: &Path) -> StatusCounts {
        match self.backend.status_counts {
            Backend::Cli => self.cli.status_counts(repo),
            Backend::Gix => self.gix.status_counts(repo),
        }
    }

    pub(crate) fn diff_stats(&self, repo: &Path, scope: Option<&str>) -> DiffStats {
        match self.backend.diff_stats {
            Backend::Cli => self.cli.diff_stats(repo, scope),
            Backend::Gix => self.gix.diff_stats(repo, scope),
        }
    }

    pub(crate) fn blame(&self, repo: &Path, file: &str) -> Result<Vec<BlameLine>, String> {
        match self.backend.blame {
            Backend::Cli => self.cli.blame(repo, file),
            Backend::Gix => self.gix.blame(repo, file),
        }
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
        let second = git(&["rev-parse", "HEAD"]).trim().to_string();

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

        // A real remote (url=self) gives a default fetch refspec so upstream
        // tracking resolves. origin/main points at the older 2nd commit, so
        // `main` is 1 ahead / 0 behind its upstream (exercises the track quirk).
        git(&["remote", "add", "origin", "."]);
        git(&["update-ref", "refs/remotes/origin/main", &second]);
        git(&["branch", "--set-upstream-to=origin/main", "main"]);

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
    use super::test_fixtures::{fixture_repo, run_git};
    use super::*;

    /// Run a port op through BOTH adapters and return `(cli_result, gix_result)`.
    macro_rules! shootout {
        ($repo:expr, $method:ident ( $($arg:expr),* )) => {{
            let cli = CliGitReads;
            let gix = GixGitReads::new();
            let a = GitReads::$method(&cli, $repo $(, $arg)*);
            let b = GitReads::$method(&gix, $repo $(, $arg)*);
            (a, b)
        }};
    }

    /// Assert two `Result<T: Serialize, String>` are byte-equal once serialized,
    /// requiring both to be `Ok`.
    fn assert_ok_json_eq<T: serde::Serialize>(
        a: &Result<T, String>,
        b: &Result<T, String>,
        what: &str,
    ) {
        let ja = serde_json::to_value(a.as_ref().unwrap_or_else(|e| panic!("cli {what}: {e}")));
        let jb = serde_json::to_value(b.as_ref().unwrap_or_else(|e| panic!("gix {what}: {e}")));
        assert_eq!(ja.unwrap(), jb.unwrap(), "{what}: gix != cli");
    }

    /// Step 7: gix branches_detail == CLI on local+remote branches, an upstream
    /// with non-zero ahead, packed refs, and detached HEAD.
    #[test]
    fn shootout_branches() {
        let (_guard, repo) = fixture_repo();

        // Sanity: the gix path actually produces the expected upstream/ahead.
        let gix = GixGitReads::new();
        let bs = gix.branches_detail(&repo).unwrap();
        let main = bs.iter().find(|b| b.name == "main").expect("main present");
        assert_eq!(main.upstream.as_deref(), Some("origin/main"));
        assert_eq!(main.ahead, Some(1), "main is 1 ahead of origin/main");
        assert_eq!(main.behind, None);
        assert!(bs.iter().any(|b| b.is_remote && b.name == "origin/main"));

        // Loose refs.
        let (a, b) = shootout!(&repo, branches_detail());
        assert_ok_json_eq(&a, &b, "branches_detail (loose refs)");

        // Packed refs.
        run_git(&repo, &["pack-refs", "--all"]);
        let (a, b) = shootout!(&repo, branches_detail());
        assert_ok_json_eq(&a, &b, "branches_detail (packed refs)");

        // Detached HEAD: no branch is current.
        let head = run_git(&repo, &["rev-parse", "HEAD"]).trim().to_string();
        run_git(&repo, &["checkout", "--detach", &head]);
        let (a, b) = shootout!(&repo, branches_detail());
        assert_ok_json_eq(&a, &b, "branches_detail (detached HEAD)");
        assert!(
            !b.unwrap().iter().any(|br| br.is_current),
            "no current branch when detached"
        );
    }

    /// Step 6: gix can open the fixture repo and the handle cache reuses the
    /// `ThreadSafeRepository` for the same path.
    #[test]
    fn gix_open_caches_handle() {
        let (_guard, repo) = fixture_repo();
        let gix = GixGitReads::new();

        let r1 = gix.repo(&repo).expect("gix should open the fixture repo");
        // `path()` is the `.git` directory of the fixture.
        assert!(
            r1.path().ends_with(".git"),
            "unexpected git dir: {:?}",
            r1.path()
        );

        // A second open for the same path reuses the cached handle.
        let r2 = gix.repo(&repo).expect("reuse cached handle");
        assert_eq!(r1.path(), r2.path());
        gix.handles.run_pending_tasks();
        assert_eq!(gix.handles.entry_count(), 1);
    }

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
