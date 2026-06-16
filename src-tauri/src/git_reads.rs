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

    /// Compute staged / changed counts via gix `status`, mapped onto the same
    /// porcelain-v2 semantics the CLI path uses (a path modified in both index
    /// and worktree counts toward both, matching git's "1 MM"). Returns `None`
    /// — signalling a CLI fallback — for sparse-checkout / submodule repos
    /// (where gix status diverges) or on any gix error.
    /// gix status/diff diverge from git on sparse-checkout and submodule repos
    /// (per the porting plan), so those defer to the CLI. Shared by
    /// `gix_status_counts` and `gix_diff_stats_worktree`.
    fn gix_repo_unsupported(grepo: &gix::Repository) -> bool {
        grepo
            .config_snapshot()
            .boolean("core.sparseCheckout")
            .unwrap_or(false)
            || grepo
                .submodules()
                .ok()
                .flatten()
                .is_some_and(|mut it| it.next().is_some())
    }

    fn gix_status_counts(&self, repo: &Path) -> Option<StatusCounts> {
        use gix::status::Item;
        use gix::status::index_worktree::Item as IwItem;
        use gix::status::plumbing::index_as_worktree::EntryStatus;

        let grepo = self.repo(repo).ok()?;

        // gix status is unsupported on sparse checkouts and diverges around
        // submodules — defer those to the CLI.
        if Self::gix_repo_unsupported(&grepo) {
            return None;
        }

        let platform = grepo.status(gix::progress::Discard).ok()?;
        let iter = platform
            .into_iter(std::iter::empty::<gix::bstr::BString>())
            .ok()?;

        let mut staged = 0u32;
        let mut changed = 0u32;
        let mut conflict = false;
        for item in iter {
            // One unreadable status entry must not discard the whole pass (and
            // the counts already accumulated) into a silent CLI fallback — skip
            // the bad entry and keep going, but make the failure observable.
            let Ok(item) = item.inspect_err(|e| {
                tracing::warn!(
                    repo = %repo.display(),
                    error = %e,
                    "gix status_counts: skipping unreadable status entry"
                );
            }) else {
                continue;
            };
            match item {
                // HEAD ↔ index difference = a staged change (a tracked rename is
                // a single Rewrite, matching porcelain's one "2 R." entry).
                Item::TreeIndex(_) => staged += 1,
                Item::IndexWorktree(iw) => match iw {
                    IwItem::Modification { status, .. } => match status {
                        EntryStatus::Conflict { .. } => {
                            conflict = true;
                            changed += 1;
                        }
                        EntryStatus::Change(_) | EntryStatus::IntentToAdd => changed += 1,
                        // No real change — just a stat refresh; do not count.
                        EntryStatus::NeedsUpdate(_) => {}
                    },
                    IwItem::DirectoryContents { entry, .. } => {
                        if matches!(entry.status, gix::dir::entry::Status::Untracked) {
                            changed += 1;
                        }
                    }
                    // Worktree-side rewrite (off by default); treat as one change.
                    IwItem::Rewrite { .. } => changed += 1,
                },
            }
        }

        let status = if conflict {
            "conflict"
        } else if staged > 0 || changed > 0 {
            "dirty"
        } else {
            "clean"
        }
        .to_string();
        Some(StatusCounts {
            status,
            staged,
            changed,
        })
    }

    /// Unstaged `git diff --shortstat` (worktree vs index) via gix: sum
    /// per-file added/removed line counts. Returns `None` (→ CLI fallback) for
    /// sparse-checkout / submodule repos or on any gix error. Binary files are
    /// excluded from the line totals, matching git.
    fn gix_diff_stats_worktree(&self, repo: &Path) -> Option<DiffStats> {
        use gix::bstr::ByteSlice;
        use gix::status::Item;
        use gix::status::index_worktree::Item as IwItem;
        use gix::status::plumbing::index_as_worktree::EntryStatus;

        let grepo = self.repo(repo).ok()?;
        if Self::gix_repo_unsupported(&grepo) {
            return None;
        }

        let platform = grepo.status(gix::progress::Discard).ok()?;
        let iter = platform
            .into_iter(std::iter::empty::<gix::bstr::BString>())
            .ok()?;

        let mut added = 0i64;
        let mut removed = 0i64;
        for item in iter {
            // One unreadable status entry must not abort the whole diff pass —
            // skip it (and warn) rather than discarding accumulated counts.
            let Ok(item) = item.inspect_err(|e| {
                tracing::warn!(
                    repo = %repo.display(),
                    error = %e,
                    "gix diff_stats: skipping unreadable status entry"
                );
            }) else {
                continue;
            };
            // Only unstaged changes to tracked files (git diff, no --cached);
            // untracked entries and purely-staged files are not counted.
            let Item::IndexWorktree(IwItem::Modification {
                entry,
                rela_path,
                status,
                ..
            }) = item
            else {
                continue;
            };
            if !matches!(status, EntryStatus::Change(_)) {
                continue;
            }
            // old = the blob recorded in the index; new = the worktree file
            // (empty when the tracked file was deleted from the worktree).
            let old = match grepo.find_object(entry.id) {
                Ok(obj) => obj.data.clone(),
                Err(e) => {
                    tracing::warn!(
                        repo = %repo.display(),
                        error = %e,
                        "gix diff_stats: skipping entry with unreadable index blob"
                    );
                    continue;
                }
            };
            let path = repo.join(rela_path.to_str_lossy().as_ref());
            let new = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                // A tracked file deleted from the worktree reads as NotFound;
                // empty is the correct "new" side (git counts the deletion).
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                // A real I/O error (permissions, etc.) is NOT a deletion — count
                // nothing for this entry rather than fabricating removed lines.
                Err(e) => {
                    tracing::warn!(
                        repo = %repo.display(),
                        path = %path.display(),
                        error = %e,
                        "gix diff_stats: skipping entry with unreadable worktree file"
                    );
                    continue;
                }
            };
            if let Some((a, r)) = count_diff_lines(&old, &new) {
                added += i64::from(a);
                removed += i64::from(r);
            }
        }
        Some(crate::git::DiffStats::from_counts(
            added.min(i64::from(i32::MAX)) as i32,
            removed.min(i64::from(i32::MAX)) as i32,
        ))
    }
}

impl GixGitReads {
    /// Build the per-commit ref decoration map matching `git log %D`: refs
    /// under refs/heads, refs/remotes, refs/tags pointing at each commit, in
    /// git's order (reverse-refname), with tags prefixed `tag: ` and HEAD
    /// combined as `HEAD -> branch` (or a leading `HEAD` when detached).
    fn gix_decorations(
        grepo: &gix::Repository,
    ) -> std::collections::HashMap<gix::ObjectId, Vec<String>> {
        use gix::bstr::ByteSlice;
        let mut entries: Vec<(String, gix::ObjectId, String)> = Vec::new();
        if let Ok(platform) = grepo.references()
            && let Ok(iter) = platform.all()
        {
            for r in iter.filter_map(Result::ok) {
                let mut reference = r;
                let full = reference.name().as_bstr().to_str_lossy().into_owned();
                let is_tag = full.starts_with("refs/tags/");
                if !(full.starts_with("refs/heads/") || full.starts_with("refs/remotes/") || is_tag)
                {
                    continue;
                }
                let short = reference.name().shorten().to_str_lossy().into_owned();
                if short.ends_with("/HEAD") {
                    continue; // skip origin/HEAD and similar symbolic pointers
                }
                let Ok(id) = reference.peel_to_id() else {
                    continue;
                };
                let label = if is_tag {
                    format!("tag: {short}")
                } else {
                    short
                };
                entries.push((full, id.detach(), label));
            }
        }
        // git renders decorations in reverse-refname order.
        entries.sort_by(|a, b| b.0.cmp(&a.0));
        let mut map: std::collections::HashMap<gix::ObjectId, Vec<String>> =
            std::collections::HashMap::new();
        for (_full, oid, label) in entries {
            map.entry(oid).or_default().push(label);
        }

        // HEAD: combine into "HEAD -> branch" at the front, or a leading "HEAD"
        // when detached.
        match grepo.head_ref() {
            Ok(Some(mut href)) => {
                let branch = href.name().shorten().to_str_lossy().into_owned();
                if let Ok(id) = href.peel_to_id() {
                    let list = map.entry(id.detach()).or_default();
                    list.retain(|l| l != &branch);
                    list.insert(0, format!("HEAD -> {branch}"));
                }
            }
            Ok(None) => {
                if let Ok(id) = grepo.head_id() {
                    map.entry(id.detach())
                        .or_default()
                        .insert(0, "HEAD".to_string());
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "gix decorations: head_ref unreadable; HEAD decoration omitted");
            }
        }
        map
    }

    /// Return commits reachable from `tips` in git `--topo-order`, truncated to
    /// `count`. Kahn's algorithm with the ready-set ordered by commit-date
    /// (newest first), matching git's priority-queue topo sort (deterministic
    /// for distinct commit times). Returns `(oid, parent_oids)` per commit.
    fn gix_topo_order(
        grepo: &gix::Repository,
        tips: impl IntoIterator<Item = gix::ObjectId>,
        count: usize,
    ) -> Option<Vec<(gix::ObjectId, Vec<gix::ObjectId>)>> {
        use gix::revision::walk::Sorting;
        use gix::traverse::commit::simple::CommitTimeOrder;
        use std::collections::HashMap;

        let walk = grepo
            .rev_walk(tips)
            .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
            .all()
            .ok()?;

        let mut parents: HashMap<gix::ObjectId, Vec<gix::ObjectId>> = HashMap::new();
        let mut time: HashMap<gix::ObjectId, i64> = HashMap::new();
        let mut indegree: HashMap<gix::ObjectId, i32> = HashMap::new();
        for info in walk {
            let info = info.ok()?;
            let ps: Vec<gix::ObjectId> = info.parent_ids().map(|id| id.detach()).collect();
            time.insert(info.id, info.commit_time.unwrap_or(0));
            indegree.entry(info.id).or_insert(1);
            parents.insert(info.id, ps);
        }
        // indegree[c] = 1 + (# of in-set children). Bump parents per child.
        for ps in parents.values() {
            for p in ps {
                if let Some(d) = indegree.get_mut(p) {
                    *d += 1;
                }
            }
        }

        // Ready set: commits with no unemitted children (indegree == 1), as a
        // max-heap by commit-time. Tie-break by oid for determinism.
        let mut ready: std::collections::BinaryHeap<(i64, gix::ObjectId)> = indegree
            .iter()
            .filter(|(_, d)| **d == 1)
            .map(|(oid, _)| (time[oid], *oid))
            .collect();

        let mut out = Vec::new();
        while let Some((_, oid)) = ready.pop() {
            let ps = parents.get(&oid).cloned().unwrap_or_default();
            out.push((oid, ps.clone()));
            if out.len() >= count {
                break;
            }
            for p in ps {
                if let Some(d) = indegree.get_mut(&p) {
                    *d -= 1;
                    if *d == 1 {
                        ready.push((time[&p], p));
                    }
                }
            }
        }
        Some(out)
    }
}

/// Count added/removed lines between two blobs with git's slider heuristics.
/// Returns `None` for binary content (which git excludes from `--shortstat`).
fn count_diff_lines(old: &[u8], new: &[u8]) -> Option<(u32, u32)> {
    if is_binary(old) || is_binary(new) {
        return None;
    }
    use gix::diff::blob::{Algorithm, InternedInput, diff_with_slider_heuristics};
    let input = InternedInput::new(old, new);
    // git's default diff algorithm is Myers, with the indent (slider) heuristic on.
    let diff = diff_with_slider_heuristics(Algorithm::Myers, &input);
    Some((diff.count_additions(), diff.count_removals()))
}

/// git's binary heuristic: a NUL byte within the first 8000 bytes.
fn is_binary(data: &[u8]) -> bool {
    data.iter().take(8000).any(|&b| b == 0)
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
                    Err(e) => {
                        // Degrade to "no tracking token" like git, but don't do
                        // it silently — an error here is invisible otherwise.
                        tracing::warn!(
                            repo = %repo.display(),
                            branch = %name,
                            upstream = %u,
                            error = %e,
                            "ahead_behind failed; upstream tracking shown as in-sync"
                        );
                        (None, None)
                    }
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

    // commit_log / graph_commits: gix has no built-in topological sort, so we
    // reproduce git's `--topo-order` ourselves (Kahn seeded by commit-date, see
    // gix_topo_order) plus git's `%D` ref decoration (gix_decorations).
    fn commit_log(
        &self,
        repo: &Path,
        count: Option<u32>,
        after: Option<String>,
    ) -> Result<Vec<CommitLogEntry>, String> {
        use gix::bstr::ByteSlice;
        let grepo = self.repo(repo)?;
        let n = count
            .unwrap_or(crate::git::COMMIT_LOG_DEFAULT_COUNT)
            .min(crate::git::COMMIT_LOG_MAX_COUNT) as usize;
        let tip = match &after {
            Some(h) => grepo
                .rev_parse_single(h.as_str())
                .map_err(|e| e.to_string())?
                .detach(),
            None => grepo.head_id().map_err(|e| e.to_string())?.detach(),
        };
        let decos = Self::gix_decorations(&grepo);
        let topo = Self::gix_topo_order(&grepo, [tip], n).ok_or("gix topo walk failed")?;

        let mut out = Vec::with_capacity(topo.len());
        for (oid, parents) in topo {
            let commit = grepo.find_commit(oid).map_err(|e| e.to_string())?;
            let author = commit.author().map_err(|e| e.to_string())?;
            let author_name = author.name.to_str_lossy().into_owned();
            let author_date = author
                .time()
                .map_err(|e| e.to_string())?
                .format(gix::date::time::format::ISO8601_STRICT)
                .map_err(|e| e.to_string())?;
            // git's strict-ISO (%aI) renders a zero offset as `Z`; gix emits
            // `+00:00`. Same instant — normalize to git's `Z` for byte parity.
            let author_date = author_date
                .strip_suffix("+00:00")
                .map_or(author_date.clone(), |s| format!("{s}Z"));
            let msg = commit.message().map_err(|e| e.to_string())?;
            let subject = msg.summary().to_str_lossy().into_owned();
            let body = msg
                .body()
                .map(|b| b.to_str_lossy().trim().to_string())
                .filter(|s| !s.is_empty());
            out.push(CommitLogEntry {
                hash: oid.to_string(),
                parents: parents.iter().map(ToString::to_string).collect(),
                refs: decos.get(&oid).cloned().unwrap_or_default(),
                author_name,
                author_date,
                subject,
                body,
            });
        }
        Ok(out)
    }

    fn graph_commits(&self, repo: &Path, count: u32) -> Result<Vec<RawCommit>, String> {
        let grepo = self.repo(repo)?;
        let head = grepo.head_id().map_err(|e| e.to_string())?.detach();
        let decos = Self::gix_decorations(&grepo);
        let topo =
            Self::gix_topo_order(&grepo, [head], count as usize).ok_or("gix topo walk failed")?;
        Ok(topo
            .into_iter()
            .map(|(oid, parents)| RawCommit {
                hash: oid.to_string(),
                parents: parents.iter().map(ToString::to_string).collect(),
                refs: decos.get(&oid).cloned().unwrap_or_default(),
            })
            .collect())
    }

    fn ahead_behind(&self, repo: &Path, left: &str, right: &str) -> Result<(u32, u32), String> {
        let grepo = self.repo(repo)?;
        let l = grepo
            .rev_parse_single(left)
            .map_err(|e| e.to_string())?
            .detach();
        let r = grepo
            .rev_parse_single(right)
            .map_err(|e| e.to_string())?
            .detach();

        // ahead = commits reachable from `left` but not `right`; behind = the
        // reverse. Counting is order-independent, so the lack of topo-order in
        // gix's walk is irrelevant here. Hiding the other side prunes the shared
        // history, which also yields the correct result with no common ancestor.
        let count_excl = |tip, hide| -> Result<u32, String> {
            Ok(grepo
                .rev_walk([tip])
                .with_hidden([hide])
                .all()
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .count()
                .try_into()
                .unwrap_or(u32::MAX))
        };
        Ok((count_excl(l, r)?, count_excl(r, l)?))
    }

    fn worktree_paths(&self, repo: &Path) -> Result<HashMap<String, String>, String> {
        use gix::bstr::ByteSlice;
        let grepo = self.repo(repo)?;
        let mut map = HashMap::new();

        let branch_of = |r: &gix::Repository| -> Option<String> {
            r.head_ref()
                .ok()
                .flatten()
                .map(|href| href.name().shorten().to_str_lossy().into_owned())
        };
        // `git worktree list` reports resolved real paths (symlinks followed,
        // e.g. macOS /var -> /private/var). Match that so paths are byte-equal.
        let real = |p: &Path| -> String {
            std::fs::canonicalize(p)
                .unwrap_or_else(|_| p.to_path_buf())
                .to_string_lossy()
                .into_owned()
        };

        // Main worktree (gix's worktrees() lists only LINKED ones). Standard
        // non-bare layout: the main work dir is the `.git` directory's parent.
        if let Some(main_wd) = grepo.common_dir().parent()
            && main_wd.exists()
            && let Ok(main_repo) = gix::open(main_wd)
            && let Some(branch) = branch_of(&main_repo)
        {
            map.insert(branch, real(main_wd));
        }

        // Linked worktrees on a branch whose directory still exists.
        for proxy in grepo.worktrees().map_err(|e| e.to_string())? {
            let Ok(base) = proxy.base() else { continue };
            if !base.exists() {
                continue;
            }
            let Ok(wt_repo) = proxy.into_repo() else {
                continue;
            };
            if let Some(branch) = branch_of(&wt_repo) {
                map.insert(branch, real(&base));
            }
        }
        Ok(map)
    }

    // status_counts via gix: classify status items into the panel's staged /
    // changed counts (the values the UI consumes — not the porcelain text).
    // Sparse-checkout / submodule repos fall back to the CLI (gix status
    // diverges there, per the plan), as does any gix error.
    fn status_counts(&self, repo: &Path) -> StatusCounts {
        self.gix_status_counts(repo)
            .unwrap_or_else(|| crate::git::status_counts_cli(repo))
    }

    // diff_stats: the hot fan-out mode is the unstaged worktree-vs-index diff
    // (scope=None) — served by gix (per-blob imara line counts). The staged
    // (--cached) and commit (hash^..hash) modes are click-time, not hot, and
    // reduce to tree↔tree diffs; they stay on the CLI. Any gix error (or
    // sparse/submodule) falls back to the CLI.
    fn diff_stats(&self, repo: &Path, scope: Option<&str>) -> DiffStats {
        if scope.is_none()
            && let Some(stats) = self.gix_diff_stats_worktree(repo)
        {
            return stats;
        }
        crate::git::get_diff_stats_impl(&repo.to_string_lossy(), scope)
    }

    fn blame(&self, repo: &Path, file: &str) -> Result<Vec<BlameLine>, String> {
        use gix::bstr::ByteSlice;
        // Same friendly "not tracked" error contract as the CLI adapter.
        crate::git::ensure_file_tracked(repo, file)?;
        let grepo = self.repo(repo)?;

        // gix blame, like `git blame` without -C/-M, does not follow content
        // across a file rename — it stops at the rename boundary. When the
        // file's history contains a rename, git's per-line attribution differs,
        // so fall back to the CLI (which is what the panel expects).
        if crate::git::file_history_has_rename(repo, file) {
            return crate::git::blame_cli(repo, file);
        }

        let head = grepo.head_id().map_err(|e| e.to_string())?.detach();
        let outcome = grepo
            .blame_file(file.into(), head, Default::default())
            .map_err(|e| e.to_string())?;

        let mut out = Vec::new();
        for (entry, lines) in outcome.entries_with_lines() {
            let commit = grepo
                .find_commit(entry.commit_id)
                .map_err(|e| e.to_string())?;
            let sig = commit.author().map_err(|e| e.to_string())?;
            let author = sig.name.to_str_lossy().trim().to_string();
            let author_time = sig.time().map_err(|e| e.to_string())?.seconds;
            let summary = commit
                .message()
                .map_err(|e| e.to_string())?
                .summary()
                .to_str_lossy()
                .trim()
                .to_string();
            let hash = entry.commit_id.to_string();
            for (i, line) in lines.iter().enumerate() {
                // gix keeps the line terminator in the token; `git blame
                // --porcelain` content omits it. Strip a trailing LF (and CR).
                let raw = line.to_str_lossy();
                let content = raw
                    .strip_suffix('\n')
                    .map(|s| s.strip_suffix('\r').unwrap_or(s))
                    .unwrap_or(&raw)
                    .to_string();
                out.push(BlameLine {
                    hash: hash.clone(),
                    author: author.clone(),
                    author_time,
                    summary: summary.clone(),
                    line_number: entry.start_in_blamed_file + i as u32 + 1,
                    content,
                });
            }
        }
        Ok(out)
    }
}

/// Which backend serves a given read op.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Backend {
    // Every op currently defaults to `Gix` (each gix adapter falls back to the
    // CLI internally for its unsupported edge cases). `Cli` is retained as the
    // per-op rollback lever: set any field in `PerOpBackend::default` back to
    // `Cli` to instantly route that op through the CLI adapter again.
    #[allow(dead_code)]
    Cli,
    Gix,
}

/// Per-op backend selection. Every op currently uses `Gix` (all 8 parity tests
/// green); set a field to `Backend::Cli` to roll that op back — a one-line change.
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
            // Flipped to gix (Step 8, revised): topo-order + %D decoration are
            // reproduced in-process. Parity: shootout_commit_log / shootout_graph.
            commit_log: Backend::Gix,
            graph_commits: Backend::Gix,
            // Flipped to gix in Step 9 (parity test: shootout_ahead_behind).
            ahead_behind: Backend::Gix,
            // Flipped to gix in Step 10 (parity test: shootout_worktrees).
            worktree_paths: Backend::Gix,
            // Flipped to gix in Step 11 (parity test: shootout_status_counts);
            // sparse-checkout / submodule repos fall back to CLI inside the adapter.
            status_counts: Backend::Gix,
            // Flipped in Step 12 (parity test: shootout_diff_stats): the hot
            // worktree mode is gix; staged/commit modes fall back to CLI.
            diff_stats: Backend::Gix,
            // Flipped to gix in Step 13 (parity test: shootout_blame); renamed
            // files fall back to CLI inside the gix adapter.
            blame: Backend::Gix,
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

    /// Like `run_git`, but pins author+committer date to `ts` (RFC3339) so
    /// commits get distinct, deterministic timestamps for topo-order tests.
    pub(crate) fn run_git_at(dir: &Path, ts: &str, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .env("GIT_AUTHOR_DATE", ts)
            .env("GIT_COMMITTER_DATE", ts)
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

    // --- is_binary / count_diff_lines unit tests (git's NUL-within-8000 rule) ---

    #[test]
    fn is_binary_nul_at_byte_7999_is_true() {
        // git scans the first 8000 bytes; byte index 7999 is the last one scanned.
        let mut data = vec![b'a'; 8000];
        data[7999] = 0;
        assert!(is_binary(&data), "NUL at byte 7999 must be detected");
    }

    #[test]
    fn is_binary_nul_at_byte_8000_is_false() {
        // Byte index 8000 is the first one NOT scanned, so it's treated as text.
        let mut data = vec![b'a'; 8001];
        data[8000] = 0;
        assert!(
            !is_binary(&data),
            "NUL at byte 8000 is beyond the 8000-byte scan"
        );
    }

    #[test]
    fn count_diff_lines_returns_none_for_binary() {
        let bin = [b'a', 0, b'b'];
        assert_eq!(count_diff_lines(&bin, b"abc"), None, "binary old → None");
        assert_eq!(count_diff_lines(b"abc", &bin), None, "binary new → None");
    }

    #[test]
    fn count_diff_lines_counts_additions_and_removals() {
        // old has 2 lines, new replaces line 2 and appends two → 1 removal, 3 additions.
        let (add, rem) = count_diff_lines(b"a\nb\n", b"a\nB\nc\nd\n").expect("text diff");
        assert_eq!((add, rem), (3, 1));
        // identical content → no changes.
        assert_eq!(count_diff_lines(b"x\ny\n", b"x\ny\n"), Some((0, 0)));
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

    /// Step 9: gix ahead_behind == `git rev-list --left-right --count`, including
    /// the no-common-ancestor case; rev-parse resolves the same OIDs.
    #[test]
    fn shootout_ahead_behind() {
        let (_guard, repo) = fixture_repo();
        let cli = CliGitReads;
        let gix = GixGitReads::new();

        for (l, r) in [
            ("main", "feature"),
            ("feature", "main"),
            ("main", "origin/main"),
            ("origin/main", "main"),
            ("main", "main"),
        ] {
            let a = cli.ahead_behind(&repo, l, r);
            let b = gix.ahead_behind(&repo, l, r);
            assert_eq!(a.as_ref().ok(), b.as_ref().ok(), "ahead_behind {l}...{r}");
        }
        // Sanity on a known pair.
        assert_eq!(
            gix.ahead_behind(&repo, "main", "origin/main").unwrap(),
            (1, 0)
        );

        // No common ancestor: an orphan branch with its own root.
        run_git(&repo, &["checkout", "--orphan", "orphan"]);
        run_git(&repo, &["rm", "-rf", "--cached", "."]);
        std::fs::write(repo.join("o.txt"), "o\n").unwrap();
        run_git(&repo, &["add", "o.txt"]);
        run_git(&repo, &["commit", "-m", "orphan root", "--no-verify"]);
        let a = cli.ahead_behind(&repo, "main", "orphan");
        let b = gix.ahead_behind(&repo, "main", "orphan");
        assert_eq!(
            a.as_ref().ok(),
            b.as_ref().ok(),
            "ahead_behind with no common ancestor"
        );

        // rev-parse parity: same OID for HEAD.
        let gix_head = gix
            .repo(&repo)
            .unwrap()
            .rev_parse_single("HEAD")
            .unwrap()
            .detach()
            .to_string();
        let cli_head = run_git(&repo, &["rev-parse", "HEAD"]).trim().to_string();
        assert_eq!(gix_head, cli_head);
    }

    /// Step 10: gix worktree_paths == CLI, including the main worktree (gix
    /// omits it) and a linked worktree; detached/missing worktrees excluded.
    #[test]
    fn shootout_worktrees() {
        let (_guard, repo) = fixture_repo();
        let wt_dir = tempfile::tempdir().unwrap();
        let wt = wt_dir.path().join("linked");
        run_git(
            &repo,
            &["worktree", "add", "-b", "wt-branch", wt.to_str().unwrap()],
        );

        let cli = CliGitReads;
        let gix = GixGitReads::new();
        let a = cli.worktree_paths(&repo).unwrap();
        let b = gix.worktree_paths(&repo).unwrap();
        assert_eq!(a, b, "worktree_paths gix != cli\ncli={a:#?}\ngix={b:#?}");
        assert!(a.contains_key("main") && a.contains_key("wt-branch"));
    }

    /// Init a fresh, empty repo with one committed `a.txt` and return guard+path.
    fn clean_repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().to_path_buf();
        run_git(&p, &["init", "-b", "main"]);
        run_git(&p, &["config", "user.email", "t@t"]);
        run_git(&p, &["config", "user.name", "T"]);
        run_git(&p, &["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(p.join("a.txt"), "a\n").unwrap();
        run_git(&p, &["add", "a.txt"]);
        run_git(&p, &["commit", "-m", "init", "--no-verify"]);
        (dir, p)
    }

    /// Step 11: gix status_counts == CLI on the values the panel consumes
    /// (status string + staged/changed) across clean/staged/unstaged/untracked/
    /// renamed; sparse-checkout repos fall back to CLI inside the gix adapter.
    #[test]
    fn shootout_status_counts() {
        let cli = CliGitReads;
        let gix = GixGitReads::new();
        let eq = |label: &str, p: &std::path::Path| {
            assert_eq!(
                cli.status_counts(p),
                gix.status_counts(p),
                "status_counts {label}: gix != cli"
            );
        };

        // clean
        let (_g0, clean) = clean_repo();
        eq("clean", &clean);
        assert_eq!(gix.status_counts(&clean).status, "clean");

        // staged modification, then an unstaged edit on top (porcelain "1 MM").
        let (_g1, staged) = clean_repo();
        std::fs::write(staged.join("a.txt"), "a2\n").unwrap();
        run_git(&staged, &["add", "a.txt"]);
        eq("staged", &staged);
        assert_eq!(gix.status_counts(&staged).staged, 1);
        std::fs::write(staged.join("a.txt"), "a3\n").unwrap();
        eq("staged+unstaged", &staged);
        let sc = gix.status_counts(&staged);
        assert_eq!(sc.staged, 1);
        assert!(sc.changed >= 1);

        // untracked only
        let (_g2, untracked) = clean_repo();
        std::fs::write(untracked.join("new.txt"), "n\n").unwrap();
        eq("untracked", &untracked);
        assert_eq!(gix.status_counts(&untracked).changed, 1);

        // unstaged-only modification (no staging)
        let (_g2b, unstaged) = clean_repo();
        std::fs::write(unstaged.join("a.txt"), "changed\n").unwrap();
        eq("unstaged", &unstaged);

        // staged rename → porcelain "2 R.": one staged entry.
        let (_g3, renamed) = clean_repo();
        run_git(&renamed, &["mv", "a.txt", "b.txt"]);
        eq("staged-rename", &renamed);
        assert_eq!(gix.status_counts(&renamed).staged, 1);

        // sparse-checkout repo: gix adapter falls back to CLI (must agree).
        let (_g4, sparse) = clean_repo();
        run_git(&sparse, &["sparse-checkout", "init"]);
        eq("sparse-checkout", &sparse);
    }

    /// Step 12: gix diff_stats == CLI for the unstaged worktree mode (the hot
    /// fan-out path) across add/remove/mixed/multiple-file/delete/binary cases;
    /// staged and commit modes are served by the CLI and must still agree.
    #[test]
    fn shootout_diff_stats() {
        let cli = CliGitReads;
        let gix = GixGitReads::new();
        let wt_eq = |label: &str, p: &std::path::Path| {
            assert_eq!(
                serde_json::to_value(cli.diff_stats(p, None)).unwrap(),
                serde_json::to_value(gix.diff_stats(p, None)).unwrap(),
                "diff_stats worktree {label}: gix != cli"
            );
        };

        let (_g, repo) = clean_repo();
        std::fs::write(repo.join("a.txt"), "a\nb\nc\n").unwrap();
        std::fs::write(repo.join("bin.dat"), [0u8, 1, 2, 0, 3]).unwrap();
        run_git(&repo, &["add", "."]);
        run_git(&repo, &["commit", "-am", "seed", "--no-verify"]);

        wt_eq("clean", &repo);

        // add lines
        std::fs::write(repo.join("a.txt"), "a\nb\nc\nd\ne\n").unwrap();
        wt_eq("added-lines", &repo);

        // remove lines
        std::fs::write(repo.join("a.txt"), "a\n").unwrap();
        wt_eq("removed-lines", &repo);

        // mixed change across two files + a deleted tracked file
        std::fs::write(repo.join("a.txt"), "A\nb\nc\nNEW\n").unwrap();
        std::fs::write(repo.join("b.txt"), "x\ny\n").unwrap();
        run_git(&repo, &["add", "b.txt"]);
        run_git(&repo, &["commit", "-m", "add b", "--no-verify"]);
        std::fs::write(repo.join("b.txt"), "x\ny\nz\n").unwrap();
        wt_eq("mixed-two-files", &repo);

        // binary file modified → excluded from line counts (both sides agree)
        std::fs::write(repo.join("bin.dat"), [0u8, 9, 9, 9, 0, 9]).unwrap();
        wt_eq("binary-modified", &repo);

        // deleted tracked file (worktree)
        std::fs::remove_file(repo.join("a.txt")).unwrap();
        wt_eq("deleted-file", &repo);

        // staged + commit modes (router → CLI) still agree.
        run_git(&repo, &["add", "-A"]);
        run_git(&repo, &["commit", "-m", "more", "--no-verify"]);
        let head = run_git(&repo, &["rev-parse", "HEAD"]).trim().to_string();
        for scope in [Some(head.as_str()), Some("staged")] {
            assert_eq!(
                serde_json::to_value(git_reads().diff_stats(&repo, scope)).unwrap(),
                serde_json::to_value(crate::git::get_diff_stats_impl(
                    &repo.to_string_lossy(),
                    scope
                ))
                .unwrap(),
                "diff_stats scope={scope:?}"
            );
        }
    }

    /// Step 13: gix blame == CLI on a non-renamed file (line→commit, author,
    /// time, content); a file with rename history falls back to the CLI.
    #[test]
    fn shootout_blame() {
        let (_g, repo) = clean_repo();
        // a.txt: extend across two commits so >1 commit attributes lines.
        std::fs::write(repo.join("a.txt"), "a\nb\n").unwrap();
        run_git(&repo, &["commit", "-am", "two lines", "--no-verify"]);
        std::fs::write(repo.join("a.txt"), "a\nb\nc\n").unwrap();
        run_git(&repo, &["commit", "-am", "three lines", "--no-verify"]);

        let cli = CliGitReads;
        let gix = GixGitReads::new();
        assert!(!crate::git::file_history_has_rename(&repo, "a.txt"));
        let a = cli.blame(&repo, "a.txt");
        let b = gix.blame(&repo, "a.txt");
        assert_ok_json_eq(&a, &b, "blame (no rename)");
        assert_eq!(b.unwrap().len(), 3);

        // Renamed-history file: git follows the rename, gix does not, so the
        // gix adapter must fall back to the CLI and match exactly.
        run_git(&repo, &["mv", "a.txt", "renamed.txt"]);
        run_git(&repo, &["commit", "-m", "rename a->renamed", "--no-verify"]);
        std::fs::write(repo.join("renamed.txt"), "a\nb\nc\nd\n").unwrap();
        run_git(&repo, &["commit", "-am", "append d", "--no-verify"]);
        assert!(crate::git::file_history_has_rename(&repo, "renamed.txt"));
        let a = cli.blame(&repo, "renamed.txt");
        let b = gix.blame(&repo, "renamed.txt"); // routes to CLI internally
        assert_ok_json_eq(&a, &b, "blame (renamed → CLI fallback)");
    }

    /// A merge history with DISTINCT commit dates + branch/tag/remote/HEAD refs,
    /// so `git --topo-order` is deterministic (date tie-break) and `%D`
    /// decoration is exercised. Returns (guard, path).
    ///
    /// ```text
    ///   A(t1) ── B(t2) ───────── M(t5)   [main, HEAD]
    ///     └────── C(t3) ── D(t4) ─┘       (feature)
    /// ```
    fn topo_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        use super::test_fixtures::run_git_at;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().to_path_buf();
        run_git(&p, &["init", "-b", "main"]);
        run_git(&p, &["config", "user.email", "t@t"]);
        run_git(&p, &["config", "user.name", "T"]);
        run_git(&p, &["config", "core.hooksPath", "/dev/null"]);

        std::fs::write(p.join("a"), "a\n").unwrap();
        run_git(&p, &["add", "a"]);
        run_git_at(
            &p,
            "2026-01-01T00:00:01Z",
            &["commit", "-m", "A", "--no-verify"],
        );
        let a = run_git(&p, &["rev-parse", "HEAD"]).trim().to_string();
        run_git(&p, &["update-ref", "refs/tags/v1", &a]); // tag on A

        std::fs::write(p.join("b"), "b\n").unwrap();
        run_git(&p, &["add", "b"]);
        run_git_at(
            &p,
            "2026-01-01T00:00:02Z",
            &["commit", "-m", "B", "--no-verify"],
        );
        let b = run_git(&p, &["rev-parse", "HEAD"]).trim().to_string();
        run_git(&p, &["update-ref", "refs/remotes/origin/main", &b]); // remote on B

        run_git(&p, &["checkout", "-q", "-b", "feature", &a]);
        std::fs::write(p.join("c"), "c\n").unwrap();
        run_git(&p, &["add", "c"]);
        run_git_at(
            &p,
            "2026-01-01T00:00:03Z",
            &["commit", "-m", "C", "--no-verify"],
        );
        std::fs::write(p.join("d"), "d\n").unwrap();
        run_git(&p, &["add", "d"]);
        run_git_at(
            &p,
            "2026-01-01T00:00:04Z",
            &["commit", "-m", "D", "--no-verify"],
        );

        run_git(&p, &["checkout", "-q", "main"]);
        run_git_at(
            &p,
            "2026-01-01T00:00:05Z",
            &["merge", "--no-ff", "feature", "-m", "M"],
        );
        (dir, p)
    }

    /// Step 8 (revised): gix graph_commits == CLI byte-for-byte — same topo OID
    /// order, parents, and `%D` ref decoration — on a merge history.
    #[test]
    fn shootout_graph() {
        let (_g, repo) = topo_fixture();
        let cli = CliGitReads;
        let gix = GixGitReads::new();
        let a = cli.graph_commits(&repo, 200).unwrap();
        let b = gix.graph_commits(&repo, 200).unwrap();
        assert_eq!(
            format!("{a:?}"),
            format!("{b:?}"),
            "graph_commits gix != cli"
        );
        // sanity: the merge + decorations are present.
        assert!(
            a.iter().any(|c| c.parents.len() == 2),
            "merge commit present"
        );
        assert!(
            a.iter().any(|c| c.refs.iter().any(|r| r == "HEAD -> main")),
            "HEAD -> main decoration present"
        );
        assert!(a.iter().any(|c| c.refs.iter().any(|r| r == "tag: v1")));
    }

    /// Step 8 (revised): gix commit_log == CLI byte-for-byte (hash/parents/refs/
    /// author/date/subject/body, topo order) — full history and with `after`.
    #[test]
    fn shootout_commit_log() {
        let (_g, repo) = topo_fixture();
        let cli = CliGitReads;
        let gix = GixGitReads::new();

        let json = |v: &Vec<CommitLogEntry>| serde_json::to_value(v).unwrap();
        let a = cli.commit_log(&repo, None, None).unwrap();
        let b = gix.commit_log(&repo, None, None).unwrap();
        assert_eq!(json(&a), json(&b), "commit_log gix != cli");
        assert!(a.iter().any(|c| c.parents.len() == 2));

        // with `after` = the second commit on main (B), and a small count.
        let b_oid = run_git(&repo, &["rev-parse", "main~1^1"]) // a parent of M's history
            .trim()
            .to_string();
        let a2 = cli.commit_log(&repo, Some(2), Some(b_oid.clone())).unwrap();
        let b2 = gix.commit_log(&repo, Some(2), Some(b_oid)).unwrap();
        assert_eq!(json(&a2), json(&b2), "commit_log(after,count) gix != cli");
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
        let log_b =
            crate::git::get_commit_log_impl(repo.to_string_lossy().into_owned(), None, None)
                .unwrap();
        assert_eq!(
            serde_json::to_value(&log_a).unwrap(),
            serde_json::to_value(&log_b).unwrap()
        );

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
