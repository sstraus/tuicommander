use crate::AppState;
use crate::state::AppEvent;
use ignore::gitignore::Gitignore;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
#[cfg(feature = "desktop")]
use tauri::{AppHandle, Emitter, Manager};

/// Classification of a filesystem event path for per-category debounce.
#[derive(Debug, PartialEq, Clone, Copy)]
pub(crate) enum EventCategory {
    /// `.git/HEAD` — branch switches, 200ms debounce
    Head,
    /// `.git/index`, `.git/refs/`, sentinel files — 500ms debounce
    GitState,
    /// Non-.git, non-gitignored files — 1500ms debounce
    WorkingTree,
    /// `.git/objects`, `.git/config`, gitignored files — skip entirely
    Noise,
}

/// Classify a filesystem event path into an `EventCategory`.
///
/// Pure function: no I/O, no side effects. The `ignores` matcher is used
/// to filter out ignored working-tree files. `worktree_roots` holds the working
/// tree roots of the repo's linked worktrees (see `linked_worktree_roots`);
/// they are matched before `repo_root` so a worktree stored *inside* the repo
/// (`.worktrees/`, `.claude/worktrees/` — both typically gitignored) is still
/// classified as a working-tree change rather than being dropped as noise.
pub(crate) fn classify_path(
    path: &Path,
    repo_root: &Path,
    git_dir: &Path,
    worktree_roots: &[PathBuf],
    ignores: &IgnoreSet,
) -> EventCategory {
    // Check if the path is inside .git/
    if let Ok(rel) = path.strip_prefix(git_dir) {
        let rel_str = rel.to_string_lossy();

        // The `.git` entry itself was created or removed (runtime `git init` /
        // deinit) — a meaningful state change that flips the repo's git-ness.
        // On Linux this is the only signal, since `.git`'s contents aren't
        // sub-watched until the watcher restarts post-transition.
        if rel_str.is_empty() {
            return EventCategory::GitState;
        }

        // .git/HEAD (exactly, not .git/logs/HEAD or similar)
        if rel_str == "HEAD" {
            return EventCategory::Head;
        }

        // .git/refs/** — branch/tag changes
        if rel_str.starts_with("refs") {
            return EventCategory::GitState;
        }

        // .git/worktrees/** — external worktree add/remove
        if rel_str.starts_with("worktrees") {
            return EventCategory::GitState;
        }

        // Sentinel files directly under .git/
        if let Some(name) = rel.file_name().and_then(|n| n.to_str())
            && matches!(
                name,
                "index"
                    | "packed-refs"
                    | "config"
                    | "MERGE_HEAD"
                    | "REBASE_HEAD"
                    | "CHERRY_PICK_HEAD"
                    | "REVERT_HEAD"
            )
            && rel.parent().is_some_and(|p| p == Path::new(""))
        {
            return EventCategory::GitState;
        }

        // Everything else under .git/ is noise (objects, hooks, logs, etc.)
        return EventCategory::Noise;
    }

    // Linked worktrees first: one stored inside the repo (`.worktrees/`,
    // `.claude/worktrees/`) is normally gitignored, so the `repo_root` rules
    // below would drop its edits as noise.
    for wt_root in worktree_roots {
        if let Ok(rel) = path.strip_prefix(wt_root) {
            return classify_in_working_tree(rel, path, ignores);
        }
    }

    if let Ok(rel) = path.strip_prefix(repo_root) {
        return classify_in_working_tree(rel, path, ignores);
    }

    // Path outside repo root entirely — shouldn't happen, treat as noise
    EventCategory::Noise
}

/// Whether a changed path is an ignore source the matcher was built from, so the
/// matcher has to be rebuilt.
///
/// Two sources qualify. A `.gitignore` counts only when it is not already noise:
/// a cargo build writes several vendored `.gitignore` files under `target/`, and
/// each one would otherwise trigger the (walking) rebuild. `.git/info/exclude`
/// counts unconditionally — it lives inside `.git`, so it is noise by
/// construction, and gating on that would mean its rules stayed cached for the
/// lifetime of the watcher.
fn ignore_source_changed(
    path: &Path,
    repo_root: &Path,
    git_dir: &Path,
    worktrees: &[PathBuf],
    ignores: &IgnoreSet,
) -> bool {
    if path == git_dir.join("info").join("exclude") {
        return true;
    }
    path.file_name().is_some_and(|n| n == ".gitignore")
        && classify_path(path, repo_root, git_dir, worktrees, ignores) != EventCategory::Noise
}

/// Classify a path already known to sit under a working-tree root, given its
/// path relative to that root. Shared by the main checkout and every linked
/// worktree so both obey the same exclusion rules.
///
/// The `ignores` matcher is always the main checkout's — linked worktrees
/// share the tracked `.gitignore`, and a branch that diverges on it only shifts
/// what we treat as noise, never what git reports.
fn classify_in_working_tree(rel: &Path, path: &Path, ignores: &IgnoreSet) -> EventCategory {
    // Always-excluded directories — noise regardless of .gitignore. Covers a
    // linked worktree's `.git` *file*, whose real state lives in the admin dir
    // under the main `.git` (already watched and classified there).
    //
    // EVERY component is checked, not just the first: a nested git repo
    // (`plugins/.git`, a submodule, a vendored checkout) is not this repo's
    // `git_dir`, so the `.git` test above misses it, and its `objects/`/`index`/
    // `logs/` churn used to arrive here as a working-tree change on every
    // command the inner repo ran. Same for build output one level down
    // (`src-tauri/target/`, `frontend/node_modules/`).
    //
    // The list holds no name a project uses for tracked source — `build/` and
    // `out/` are not in it. Whether those are generated is `.gitignore`'s answer,
    // and the check below asks it (parents included).
    if rel.components().any(|c| {
        let name = c.as_os_str();
        crate::fs::ALWAYS_EXCLUDED_DIRS
            .iter()
            .any(|d| name == std::ffi::OsStr::new(d))
    }) {
        return EventCategory::Noise;
    }

    if ignores.is_ignored(rel, path.is_dir()) {
        return EventCategory::Noise;
    }
    EventCategory::WorkingTree
}

/// Working-tree roots of the repo's linked worktrees, resolved from
/// `.git/worktrees/*/gitdir` — each holds the absolute path of that worktree's
/// `.git` file, whose parent is the working-tree root.
///
/// A linked worktree usually lives OUTSIDE the repo root (the `Sibling` and
/// `AppDir` storage strategies), so the root's watch never sees it. Without its
/// own watch an agent editing files there produces no event at all, and the
/// branch's diff badge in the sidebar stays stale until the user selects it.
/// The git-state fingerprint is no fallback either: it is computed from the
/// main checkout's index and porcelain status, so a worktree-local change
/// leaves it identical and the emit is suppressed.
fn linked_worktree_roots(git_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(git_dir.join("worktrees")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path().join("gitdir")).ok())
        .filter_map(|gitdir| {
            PathBuf::from(gitdir.trim())
                .parent()
                .map(|p| p.to_path_buf())
        })
        .filter(|root| root.is_dir())
        // Canonicalized because these roots are prefix-matched against event
        // paths, and the backends report resolved paths (on macOS FSEvents
        // reports `/private/var/…` for a `/var/…` worktree).
        .map(|root| root.canonicalize().unwrap_or(root))
        .collect()
}

/// Per-category debounce delays. CategoryEmitter applies these app-level
/// delays so slower categories don't over-fire.
const HEAD_DEBOUNCE: Duration = Duration::from_millis(200);
const GIT_STATE_DEBOUNCE: Duration = Duration::from_millis(500);
const WORKING_TREE_DEBOUNCE: Duration = Duration::from_millis(1500);
const COLD_WORKING_TREE_DEBOUNCE: Duration = Duration::from_secs(15);

impl EventCategory {
    /// The debounce delay for this category.
    fn delay(&self) -> Duration {
        match self {
            Self::Head => HEAD_DEBOUNCE,
            Self::GitState => GIT_STATE_DEBOUNCE,
            Self::WorkingTree => WORKING_TREE_DEBOUNCE,
            Self::Noise => Duration::ZERO,
        }
    }
}

/// Per-category trailing debounce emitter.
///
/// When an event arrives for a category, any pending timer for that category
/// is cancelled and a new delayed emit is spawned. The event fires N ms
/// after the *last* event in the burst.
pub(crate) struct CategoryEmitter {
    rt: tokio::runtime::Handle,
    head: Mutex<Option<tokio::task::AbortHandle>>,
    git_state: Mutex<Option<tokio::task::AbortHandle>>,
    working_tree: Mutex<Option<tokio::task::AbortHandle>>,
}

impl CategoryEmitter {
    pub(crate) fn new(rt: tokio::runtime::Handle) -> Self {
        Self {
            rt,
            head: Mutex::new(None),
            git_state: Mutex::new(None),
            working_tree: Mutex::new(None),
        }
    }

    /// Schedule a delayed emit with the category's default debounce delay.
    pub(crate) fn trigger<F>(&self, category: &EventCategory, emit_fn: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.trigger_with_delay(category, category.delay(), emit_fn);
    }

    /// Drop a pending emit for `category` without firing it. No-op when nothing
    /// is pending, and never touches another category.
    ///
    /// Used to dedupe `repo-changed`: a GitState emit already sent the event a
    /// pending WorkingTree emit was going to send, so the second one is pure
    /// duplication — one logical change, two frontend cascades.
    pub(crate) fn cancel(&self, category: &EventCategory) {
        if let Some(slot) = self.slot(category)
            && let Some(handle) = slot.lock().take()
        {
            handle.abort();
        }
    }

    /// The pending-emit slot for a category. `Noise` has none — it never emits.
    fn slot(&self, category: &EventCategory) -> Option<&Mutex<Option<tokio::task::AbortHandle>>> {
        match category {
            EventCategory::Head => Some(&self.head),
            EventCategory::GitState => Some(&self.git_state),
            EventCategory::WorkingTree => Some(&self.working_tree),
            EventCategory::Noise => None,
        }
    }

    /// Schedule a delayed emit with an explicit delay. If a pending emit
    /// exists for the same category, it is cancelled first (trailing debounce).
    pub(crate) fn trigger_with_delay<F>(
        &self,
        category: &EventCategory,
        delay: Duration,
        emit_fn: F,
    ) where
        F: FnOnce() + Send + 'static,
    {
        let Some(slot) = self.slot(category) else {
            return;
        };
        let mut guard = slot.lock();
        if let Some(handle) = guard.take() {
            handle.abort();
        }
        let join_handle = self.rt.spawn(async move {
            tokio::time::sleep(delay).await;
            emit_fn();
        });
        *guard = Some(join_handle.abort_handle());
    }
}

/// What kind of change a `repo-changed` reports.
///
/// The watcher already knows this — it debounces `.git/` writes and working-tree
/// writes as separate categories — it just used to throw the answer away, so
/// every consumer had to assume the worst. On a live repo the two are not close
/// to balanced: a 4000-line log sample held 360 working-tree emits against 3
/// git-state ones, so a panel that only reads committed history was re-running
/// `git log` on ~99% of events that could not possibly have changed its answer.
///
/// A `GitState` emit cancels the pending `WorkingTree` one (`git add`, `git
/// commit` and `git checkout` all write both), so `GitState` is not "only .git
/// changed" — it is "at least .git changed". That asymmetry is why the coarse
/// revision counter must keep bumping on both, and only the narrow one is
/// git-state-gated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RepoChangeKind {
    /// `.git/` changed: HEAD, refs, the index, worktree admin. Committed
    /// history, branches and stashes may all differ.
    GitState,
    /// Only files in the working tree changed. Nothing that `git log` reads can
    /// have moved.
    WorkingTree,
}

/// Payload emitted when a repo's `.git/` directory changes in a meaningful way.
#[derive(Clone, serde::Serialize)]
pub(crate) struct RepoChangedPayload {
    pub repo_path: String,
    pub kind: RepoChangeKind,
}

/// Payload emitted when a repo's HEAD changes (branch switch).
#[derive(Clone, serde::Serialize)]
pub(crate) struct HeadChangedPayload {
    pub repo_path: String,
    pub branch: String,
}

/// Every ignore source git consults for a repo, layered so a deeper rule wins:
/// the user's global ignore file (`core.excludesFile`), the root `.gitignore`
/// together with `.git/info/exclude`, and one matcher per nested `.gitignore`.
///
/// Reading only the root `.gitignore` — the previous behaviour — left the other
/// three sources invisible, so every write under a nested-ignored path
/// (`plugins/*/data/`, `.git/info/exclude`'s `src-tauri/history/`) reached the
/// emitter as a working-tree change and fed the `repo-changed` storm.
pub(crate) struct IgnoreSet {
    /// `(anchor, matcher)` shallowest first. The anchor is relative to a
    /// working-tree root, not absolute, so the same set matches paths in the
    /// main checkout and in every linked worktree.
    layers: Vec<(PathBuf, Gitignore)>,
}

impl IgnoreSet {
    /// A matcher that ignores nothing.
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self { layers: Vec::new() }
    }

    /// Whether `rel` — a path relative to a working-tree root — is ignored.
    ///
    /// Layers are applied shallowest first so a deeper `.gitignore` overrides a
    /// shallower rule, including via a negation. Git would not even descend into
    /// a directory an outer rule already ignored, so a deep negation under one is
    /// honoured here where git would not: that classifies a path as a working
    /// tree change instead of noise, which costs an event rather than losing one.
    fn is_ignored(&self, rel: &Path, is_dir: bool) -> bool {
        let mut ignored = false;
        for (anchor, gi) in &self.layers {
            let Ok(sub) = rel.strip_prefix(anchor) else {
                continue;
            };
            if sub.as_os_str().is_empty() {
                continue;
            }
            match gi.matched_path_or_any_parents(sub, is_dir) {
                ignore::Match::Ignore(_) => ignored = true,
                ignore::Match::Whitelist(_) => ignored = false,
                ignore::Match::None => {}
            }
        }
        ignored
    }
}

/// Build the repo's `IgnoreSet` from all of git's ignore sources.
///
/// Discovering the nested `.gitignore` files needs a walk, so this is far from
/// free — it runs at watcher registration and only when a `.gitignore` that is
/// itself not ignored changes, never per filesystem event. The rebuild path runs
/// on notify's event thread, so a large repo stalls event delivery for the
/// duration; that is why the caller gates it on the changed `.gitignore` not
/// already being noise, which excludes the vendored ones cargo writes under
/// `target/` during a build.
fn build_ignore(repo_root: &Path, git_dir: &Path) -> IgnoreSet {
    let mut layers: Vec<(PathBuf, Gitignore)> = Vec::new();

    // The user's global ignore file. Its patterns are unanchored by convention
    // (`*.swp`, `**/.claude/settings.local.json`), so matching them against a
    // repo-relative path is what git does too.
    let (global, err) = Gitignore::global();
    if let Some(e) = err {
        tracing::debug!(
            source = "repo_watcher",
            "Global gitignore not fully read: {e}"
        );
    }
    if global.num_ignores() > 0 || global.num_whitelists() > 0 {
        layers.push((PathBuf::new(), global));
    }

    // `.git/info/exclude` and the root `.gitignore` are both repo-root relative,
    // but they are NOT one matcher: git ranks a per-directory `.gitignore` above
    // `$GIT_DIR/info/exclude`, so a root `!generated.txt` must beat an
    // `info/exclude` entry for the same path. Later layers win here, so exclude
    // goes first.
    for source in [
        git_dir.join("info").join("exclude"),
        repo_root.join(".gitignore"),
    ] {
        let mut builder = ignore::gitignore::GitignoreBuilder::new(repo_root);
        builder.add(source);
        if let Ok(gi) = builder.build()
            && (gi.num_ignores() > 0 || gi.num_whitelists() > 0)
        {
            layers.push((PathBuf::new(), gi));
        }
    }

    // Nested `.gitignore` files, deepest last so their rules override.
    let mut nested: Vec<(PathBuf, Gitignore)> = nested_gitignores(repo_root);
    nested.sort_by_key(|(anchor, _)| anchor.components().count());
    layers.extend(nested);

    IgnoreSet { layers }
}

/// Every `.gitignore` below the repo root, paired with its directory relative to
/// that root. The walk itself honours ignore rules, so a `.gitignore` inside an
/// already-ignored directory is never returned — it could not matter anyway.
fn nested_gitignores(repo_root: &Path) -> Vec<(PathBuf, Gitignore)> {
    ignore::WalkBuilder::new(repo_root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(false)
        .filter_entry(|e| !crate::fs::is_always_excluded_dir(e))
        .build()
        .flatten()
        .filter(|e| e.depth() > 0 && e.file_name() == std::ffi::OsStr::new(".gitignore"))
        .filter_map(|e| {
            let dir = e.path().parent()?;
            let anchor = dir.strip_prefix(repo_root).ok()?.to_path_buf();
            let mut builder = ignore::gitignore::GitignoreBuilder::new(dir);
            builder.add(e.path());
            let gi = builder.build().ok()?;
            (gi.num_ignores() > 0 || gi.num_whitelists() > 0).then_some((anchor, gi))
        })
        .collect()
}

/// Fold the meaningful git-state inputs into a single u64 fingerprint.
///
/// Deliberately EXCLUDES `.git/index` mtime: a bare `touch .git/index` — or a
/// `--no-optional-locks` status that rewrites the index only to refresh its stat
/// cache — bumps mtime without changing the logical state, and must NOT be treated
/// as a change. Index *size*, the resolved HEAD target, and the porcelain status
/// together capture every meaningful change (stage/unstage, commit, branch switch)
/// while staying stable across those no-op mtime touches.
pub(crate) fn compute_git_fingerprint(
    index_size: u64,
    head_target: &str,
    porcelain_status: &str,
    worktree_admin_names: &[String],
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    index_size.hash(&mut hasher);
    head_target.hash(&mut hasher);
    porcelain_status.hash(&mut hasher);
    worktree_admin_names.hash(&mut hasher);
    hasher.finish()
}

/// Sorted names of the repo's linked-worktree admin dirs (`.git/worktrees/*`).
///
/// Part of the fingerprint because adding or removing a worktree touches nothing
/// else it covers: HEAD, the index and the working tree are all unchanged, so a
/// worktree-only change used to be swallowed by the "git-state unchanged" guard
/// and the sidebar kept showing rows for worktrees that no longer existed.
/// Sorted so directory-iteration order can't produce a spurious change.
fn worktree_admin_names(git_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(git_dir.join("worktrees")) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// Resolve HEAD to a stable target string (cheap file reads, no subprocess):
/// - attached HEAD → `"<refpath>=<sha>"` (resolving the loose ref), or `"ref: <refpath>"`
///   if the ref is packed/unreadable (still distinguishes branches);
/// - detached HEAD → the raw commit SHA.
///
/// `None` if `.git/HEAD` itself can't be read — callers must NOT treat that as a
/// stable target (caching an empty sentinel poisons the dedup cache and would
/// suppress the next real HEAD move; see `head_target_changed`).
fn resolve_head_target(git_dir: &Path) -> Option<String> {
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let trimmed = head.trim();
    Some(if let Some(refpath) = trimmed.strip_prefix("ref: ") {
        match std::fs::read_to_string(git_dir.join(refpath)) {
            Ok(sha) if !sha.trim().is_empty() => format!("{refpath}={}", sha.trim()),
            _ => format!("ref: {refpath}"),
        }
    } else {
        trimmed.to_string()
    })
}

/// Compute the current git-state fingerprint for a repo. Gathers the cheap inputs
/// — index size, resolved HEAD, and the porcelain status via the existing
/// `--no-optional-locks` (non-writing) read path — and folds them with
/// `compute_git_fingerprint`. Runs on the post-debounce emit task, not the
/// FSEvents hot path.
///
/// DEFERRED (2026-09-05) — story 693-1711 asked for this read to come from the
/// shared `git_reads().status_counts()` instead of a fork. It cannot: that read
/// returns only the clean/dirty/conflict verdict and the staged/changed counts,
/// and swapping *which* file is staged leaves all three — plus the index size
/// and HEAD — byte-identical (measured). The fingerprint would go blind to a
/// stage swap and suppress the emit, which is worse than the fork it saves. See
/// `test_real_fingerprint_moves_across_stage_commit_conflict_and_clean`, whose
/// last leg fails the moment the counts are substituted. A conversion needs a
/// per-file digest carried on `StatusCounts` first.
fn repo_git_fingerprint(repo_root: &Path, git_dir: &Path) -> u64 {
    let index_size = std::fs::metadata(git_dir.join("index"))
        .map(|m| m.len())
        .unwrap_or(0);
    let head_target = resolve_head_target(git_dir).unwrap_or_default();
    let porcelain = crate::git_cli::git_cmd(repo_root)
        .args(["--no-optional-locks", "status", "--porcelain"])
        .run_silent()
        .map(|o| o.stdout)
        .unwrap_or_default();
    compute_git_fingerprint(
        index_size,
        &head_target,
        &porcelain,
        &worktree_admin_names(git_dir),
    )
}

/// Decide whether a `head-changed` emit should fire for `repo_path` given the
/// freshly resolved HEAD `target`, updating the per-repo cache as a side effect.
///
/// Returns `false` when the target is unchanged since the last emit — the guard
/// that suppresses the Linux inotify storm where `.git/HEAD` events recur without
/// the resolved HEAD actually moving (issue #82). Cold start (empty cache) returns
/// `true`, mirroring the GitState fingerprint guard which also emits on first sight.
fn head_target_changed(
    cache: &dashmap::DashMap<String, String>,
    repo_path: &str,
    target: &str,
) -> bool {
    if cache.get(repo_path).is_some_and(|v| *v == target) {
        return false;
    }
    cache.insert(repo_path.to_string(), target.to_string());
    true
}

/// Collect every working-tree directory that should receive a watch, pruning
/// the always-excluded dirs (`.git`, `node_modules`, `target`, …) and any
/// gitignored paths via `ignore::WalkBuilder`. Used by the Linux watch path to
/// register one non-recursive inotify watch per surviving directory instead of
/// a single recursive watch that would also cover the pruned subtrees (issue
/// #82). The repo root is always included.
///
/// Not cfg-gated so it can be unit-tested on any platform; only its caller in
/// `start_watching` is Linux-specific (hence dead on non-Linux non-test builds).
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn collect_working_tree_dirs(repo_root: &Path) -> Vec<PathBuf> {
    ignore::WalkBuilder::new(repo_root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .parents(false)
        .filter_entry(|e| !crate::fs::is_always_excluded_dir(e))
        .build()
        .flatten()
        .filter(|e| e.file_type().is_some_and(|ft| ft.is_dir()))
        .map(|e| e.path().to_path_buf())
        .collect()
}

/// Thread-safe wrapper for `RecommendedWatcher`.
///
/// `RecommendedWatcher` is `Send` but not `Sync`. Wrapping in `Mutex`
/// provides `Sync` so it can live in DashMap. The mutex is only locked
/// during `watch()`/`unwatch()` calls (not on the event hot path).
pub(crate) struct WatchHandle(#[allow(dead_code)] pub(crate) Mutex<RecommendedWatcher>);

/// Repo-watcher handle: the live `notify` watcher plus, on Linux, the set of
/// working-tree directories we've already registered a non-recursive watch for.
///
/// Stored behind `Arc` in `AppState.repo_watchers`. The Linux event callback
/// clones the `Arc` (dropping the `DashMap` ref immediately) before calling the
/// blocking `watch()` on the watcher mutex — so `stop_watching`'s map removal
/// never stalls behind an in-flight add-watch, and the watcher is never dropped
/// while a `DashMap` shard lock is held. `watched_dirs` dedupes the add-watch
/// requests that create-event bursts would otherwise fire repeatedly; it is
/// dropped with the handle, so a stopped+restarted watcher starts cold.
pub(crate) struct RepoWatchHandle {
    pub(crate) watcher: Mutex<RecommendedWatcher>,
    /// Linux only: the directories holding a non-recursive watch. Stays empty on
    /// macOS/Windows, where one recursive registration covers a whole root.
    watched_dirs: Mutex<std::collections::HashSet<PathBuf>>,
    /// Linked-worktree roots that currently hold a watch. Diffed against disk by
    /// `sync_worktree_watches` so worktrees added or removed at runtime are
    /// picked up without restarting the watcher. Held across the `watch()`
    /// syscalls, which also serializes concurrent syncs — never touched on the
    /// event hot path, which reads `worktree_snapshot` instead.
    worktree_roots: Mutex<std::collections::HashSet<PathBuf>>,
    /// Lock-cheap copy of the registered roots for `classify_path`, shared with
    /// the event callback. Refreshed at the end of each sync.
    worktree_snapshot: Arc<parking_lot::RwLock<Vec<PathBuf>>>,
    status_watch_complete: Arc<std::sync::atomic::AtomicBool>,
}

/// Register the watches for one working-tree root: the main checkout at startup,
/// or a linked worktree as it appears. On macOS/Windows a single recursive
/// registration; on Linux one non-recursive watch per surviving directory, since
/// a recursive one would also cover `node_modules`/`target` and flood the
/// callback (issue #82). Returns the number of directories it failed to watch.
fn watch_working_tree_root(
    watcher: &mut RecommendedWatcher,
    root: &Path,
    watched: &mut std::collections::HashSet<PathBuf>,
) -> usize {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = watched;
        if let Err(e) = watcher.watch(root, RecursiveMode::Recursive) {
            tracing::warn!(source = "repo_watcher", path = %root.display(), "Failed to watch working tree: {e}");
            return 1;
        }
        0
    }

    #[cfg(target_os = "linux")]
    {
        let mut failures = 0usize;
        for dir in collect_working_tree_dirs(root) {
            if !watched.insert(dir.clone()) {
                continue;
            }
            if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
                watched.remove(&dir);
                failures += 1;
                tracing::debug!(source = "repo_watcher", path = %dir.display(), "Failed to watch working-tree dir: {e}");
            }
        }
        failures
    }
}

/// Drop the watches registered for a working-tree root. `unwatch` errors are
/// expected and ignored: the usual reason a root disappears is that the
/// directory was deleted, which already invalidated the watch.
fn unwatch_working_tree_root(
    watcher: &mut RecommendedWatcher,
    root: &Path,
    watched: &mut std::collections::HashSet<PathBuf>,
) {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = watched;
        let _ = watcher.unwatch(root);
    }

    #[cfg(target_os = "linux")]
    {
        let gone: Vec<PathBuf> = watched
            .iter()
            .filter(|d| d.starts_with(root))
            .cloned()
            .collect();
        for dir in gone {
            watched.remove(&dir);
            let _ = watcher.unwatch(&dir);
        }
    }
}

/// Make the registered linked-worktree watches match what's on disk.
///
/// Called once at registration and again after every git-state change: adding or
/// removing a worktree writes under `.git/worktrees`, which classifies as
/// `GitState`, so the sync rides the emit that change already produces. Cheap
/// when nothing moved — a `read_dir` plus a set comparison, syscalls only on a
/// delta.
fn sync_worktree_watches(state: &Arc<AppState>, repo_path: &str, git_dir: &Path) {
    let Some(handle) = state
        .repo_watchers
        .get(repo_path)
        .map(|r| r.value().clone())
    else {
        return;
    };
    let desired: std::collections::HashSet<PathBuf> =
        linked_worktree_roots(git_dir).into_iter().collect();

    let mut registered = handle.worktree_roots.lock();
    if *registered == desired {
        return;
    }

    {
        // `watched_dirs` before `watcher`, the order the Linux add-watch path
        // uses (one lock at a time there, so neither can wedge the other).
        let mut watched = handle.watched_dirs.lock();
        let mut watcher = handle.watcher.lock();
        for root in desired.difference(&registered) {
            tracing::debug!(source = "repo_watcher", repo = %repo_path, path = %root.display(), "Watching linked worktree");
            let failures = watch_working_tree_root(&mut watcher, root, &mut watched);
            crate::monitoring_status::CACHE.set_watching(
                Path::new(repo_path),
                root,
                failures == 0
                    && handle
                        .status_watch_complete
                        .load(std::sync::atomic::Ordering::Acquire),
            );
            if !handle
                .status_watch_complete
                .load(std::sync::atomic::Ordering::Acquire)
            {
                crate::monitoring_status::CACHE.stop(Path::new(repo_path));
            }
        }
        for root in registered.difference(&desired) {
            tracing::debug!(source = "repo_watcher", repo = %repo_path, path = %root.display(), "Dropping linked-worktree watch");
            crate::monitoring_status::CACHE.invalidate_checkout(root);
            crate::monitoring_status::CACHE.set_watching(Path::new(repo_path), root, false);
            unwatch_working_tree_root(&mut watcher, root, &mut watched);
        }
    }

    *handle.worktree_snapshot.write() = desired.iter().cloned().collect();
    *registered = desired;
}

/// Whether a filesystem event denotes a newly created working-tree directory
/// that needs its own non-recursive watch on Linux (issue #82). Pure so it can
/// be unit-tested without a live inotify backend: gates on the event kind
/// (`Create(Folder)`, reliably set by inotify via `IN_ISDIR`) rather than a
/// racy `path.is_dir()` stat that may lose to a rename/delete.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn is_new_watchable_dir(kind: &notify::EventKind, category: EventCategory) -> bool {
    matches!(
        kind,
        notify::EventKind::Create(notify::event::CreateKind::Folder)
    ) && category == EventCategory::WorkingTree
}

/// Whether an event is a read-only access event that carries no state change and
/// must be ignored before any classification (issue #84).
///
/// On Linux, inotify reports `IN_ACCESS`/`IN_OPEN`/`IN_CLOSE_NOWRITE` for every
/// file *read*. Any process reading the working tree — TUIC's own periodic
/// `git status`, the user's editor, language-server indexing — sprays thousands
/// of these per second per repo (verified: a `git status` every 200 ms produced
/// ~3000 events/s, 99.7% of them `Access`). `recommended_watcher` runs the
/// callback on notify's event thread, so classifying + gitignore-matching each
/// one pinned a core per repo and ultimately SIGABRTed — the emit-dedup fix
/// (#82) silenced the downstream emit but not this per-event work.
///
/// `Access(Close(Write))` is kept: it signals a *completed write*. Real
/// modifications also arrive as `Modify`/`Create`/`Remove`, which are never
/// dropped, so no change is missed.
pub(crate) fn is_ignorable_access(kind: &notify::EventKind) -> bool {
    use notify::event::{AccessKind, AccessMode};
    match kind {
        notify::EventKind::Access(AccessKind::Close(AccessMode::Write)) => false,
        notify::EventKind::Access(_) => true,
        _ => false,
    }
}

/// Start a watcher for a repository using raw `notify::RecommendedWatcher`.
///
/// On macOS/Windows, FSEvents/ReadDirectoryChangesW handle recursive watching
/// at the OS level with near-zero cost. Events are classified via `classify_path`
/// and fed to `CategoryEmitter` for per-category trailing debounce.
///
/// Unlike the previous `notify-debouncer-full` approach, this does NOT perform
/// a synchronous walkdir+stat scan at registration time.
pub(crate) fn start_watching(repo_path: &str, state: &Arc<AppState>) -> Result<(), String> {
    if state.repo_watchers.contains_key(repo_path) {
        return Ok(());
    }
    tracing::info!(source = "repo_watcher", path = %repo_path, "Starting watcher");

    let repo = PathBuf::from(repo_path);
    // A registered directory may not (yet) be a git repo — watch it anyway so a
    // runtime `git init` is detected: the `.git` creation event classifies as
    // GitState and triggers the frontend's non-git→git transition probe, which
    // restarts this watcher with the real `.git` present. Fall back to the
    // conventional `.git` location for path classification; the Linux `.git`
    // sub-watches below are skipped until it actually exists.
    let git_dir = crate::git::resolve_git_dir(&repo).unwrap_or_else(|| repo.join(".git"));
    let gitignore = Arc::new(parking_lot::RwLock::new(build_ignore(&repo, &git_dir)));
    // Filled in by `sync_worktree_watches` once the handle is registered; the
    // event callback reads it to classify paths inside linked worktrees.
    let worktree_snapshot: Arc<parking_lot::RwLock<Vec<PathBuf>>> =
        Arc::new(parking_lot::RwLock::new(Vec::new()));

    let repo_path_owned = repo_path.to_string();
    #[cfg(feature = "desktop")]
    let handle = state.app_handle.read().clone();
    let event_bus = state.event_bus.clone();
    let state_cb = Arc::clone(state);
    let rt_handle = {
        #[cfg(feature = "desktop")]
        {
            tauri::async_runtime::handle().inner().clone()
        }
        #[cfg(not(feature = "desktop"))]
        {
            tokio::runtime::Handle::current()
        }
    };
    // Linux dynamically adds non-recursive watches for new working-tree dirs
    // from the event callback; it needs a runtime handle to offload the
    // (blocking, must-not-run-on-event-loop-thread) `watch()` call.
    #[cfg(target_os = "linux")]
    let rt_for_cb = rt_handle.clone();
    let emitter = Arc::new(CategoryEmitter::new(rt_handle));
    // The base-checkout fingerprint cannot see a sibling ref/HEAD move. Keep
    // one pending bit across the debounce rather than reading every checkout.
    let shared_git_changed = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let status_watch_complete = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let status_complete_cb = Arc::clone(&status_watch_complete);
    let repo_for_cb = repo.clone();
    let git_dir_for_cb = git_dir.clone();
    let gitignore_cb = Arc::clone(&gitignore);
    let worktrees_cb = Arc::clone(&worktree_snapshot);

    let mut watcher = notify::recommended_watcher(
        move |result: Result<notify::Event, notify::Error>| {
            let event = match result {
                Ok(e) => e,
                Err(err) => {
                    tracing::warn!(source = "repo_watcher", path = %repo_path_owned, "Watcher error: {err}");
                    status_complete_cb.store(false, std::sync::atomic::Ordering::Release);
                    crate::monitoring_status::CACHE.stop(&repo_for_cb);
                    tuic_git::worktree::set_monitoring_watch_active(&repo_for_cb, false);
                    return;
                }
            };

            if !crate::monitoring_status::CACHE.event(&repo_for_cb, &event) {
                status_complete_cb.store(false, std::sync::atomic::Ordering::Release);
            }

            // Drop read-only access events (open/read/close-nowrite) before any
            // work: file reads spray thousands per second per repo and pinned a
            // core processing them on this (notify event) thread (issue #84).
            if is_ignorable_access(&event.kind) {
                return;
            }

            if event.need_rescan() {
                tuic_git::worktree::set_monitoring_watch_active(&repo_for_cb, false);
            }
            let git_paths: Vec<_> = event.paths.iter()
                .filter(|path| path.starts_with(&git_dir_for_cb)).cloned().collect();
            if !git_paths.is_empty() {
                tuic_git::worktree::monitoring_paths_changed(&repo_for_cb, &git_paths);
            }
            if git_paths.iter().any(|path| {
                path == &git_dir_for_cb.join("packed-refs")
                    || path == &git_dir_for_cb.join("config")
                    || (path.starts_with(git_dir_for_cb.join("refs"))
                        && path.extension().is_none_or(|extension| extension != "lock"))
                    || (path.starts_with(git_dir_for_cb.join("worktrees"))
                        && path.file_name().is_some_and(|name| name == "HEAD"))
            }) {
                shared_git_changed.store(true, std::sync::atomic::Ordering::Relaxed);
            }

            // Rebuild the matcher when an ignore source that matters changes.
            let gitignore_changed = {
                let gi = gitignore_cb.read();
                let worktrees = worktrees_cb.read();
                event.paths.iter().any(|p| {
                    ignore_source_changed(p, &repo_for_cb, &git_dir_for_cb, &worktrees, &gi)
                })
            };
            if gitignore_changed {
                *gitignore_cb.write() = build_ignore(&repo_for_cb, &git_dir_for_cb);
            }

            // Classify all event paths and collect which categories fired
            let gi = gitignore_cb.read();
            let worktrees = worktrees_cb.read();
            let mut has_head = false;
            let mut has_git_state = false;
            let mut has_working_tree = false;

            for path in &event.paths {
                let category =
                    classify_path(path, &repo_for_cb, &git_dir_for_cb, &worktrees, &gi);
                match category {
                    EventCategory::Head => has_head = true,
                    EventCategory::GitState => has_git_state = true,
                    EventCategory::WorkingTree => has_working_tree = true,
                    EventCategory::Noise => {}
                }

                // Linux watches each working-tree dir non-recursively (issue #82),
                // so a newly created directory needs its own watch or its contents
                // go unobserved. Offload the add to a blocking task: notify's
                // inotify `watch()` must NOT run on this (event-loop) thread — it
                // would block on a reply the same thread is supposed to deliver.
                // The task clones the handle `Arc` and drops the `DashMap` ref
                // before locking, so `stop_watching` never stalls behind it.
                #[cfg(target_os = "linux")]
                if is_new_watchable_dir(&event.kind, category) {
                    let st = Arc::clone(&state_cb);
                    let rp = repo_path_owned.clone();
                    let new_dir = path.clone();
                    rt_for_cb.spawn_blocking(move || {
                        let Some(h) = st.repo_watchers.get(&rp).map(|r| r.value().clone()) else {
                            return;
                        };
                        // Dedupe create-event bursts: only the first request for a
                        // dir schedules the syscall.
                        if !h.watched_dirs.lock().insert(new_dir.clone()) {
                            return;
                        }
                        if let Err(e) = h.watcher.lock().watch(&new_dir, RecursiveMode::NonRecursive)
                        {
                            h.watched_dirs.lock().remove(&new_dir);
                            tracing::warn!(source = "repo_watcher", path = %new_dir.display(), "Failed to watch new dir: {e}");
                        }
                    });
                }
            }
            drop(gi);
            drop(worktrees);

            // Trigger per-category delayed emits
            if has_head {
                let repo_path = repo_path_owned.clone();
                let repo = repo_for_cb.clone();
                let git_dir = git_dir_for_cb.clone();
                let bus = event_bus.clone();
                let st = Arc::clone(&state_cb);
                #[cfg(feature = "desktop")]
                let h = handle.clone();
                emitter.trigger(&EventCategory::Head, move || {
                    // Semantic dedupe: only emit when the resolved HEAD target
                    // actually moved. On Linux, inotify re-fires `.git/HEAD`
                    // events without the branch/SHA changing (issue #82);
                    // suppressing those here stops the emit loop and the
                    // downstream IPC cascade that pinned CPU and aborted.
                    match resolve_head_target(&git_dir) {
                        Some(target)
                            if !head_target_changed(
                                &st.repo_head_targets,
                                &repo_path,
                                &target,
                            ) =>
                        {
                            st.repo_head_emits_suppressed
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            tracing::debug!(source = "repo_watcher", path = %repo_path, "Skip head-changed (HEAD target unchanged)");
                            return;
                        }
                        // HEAD momentarily unreadable (rebase/gc/fetch in flight):
                        // don't dedupe — fall through and let the emit attempt
                        // proceed rather than caching an empty sentinel.
                        None => tracing::debug!(source = "repo_watcher", path = %repo_path, "HEAD unreadable; emitting head-changed without dedupe"),
                        Some(_) => {}
                    }
                    if let Some(branch) = crate::git::read_branch_from_head(&repo) {
                        tracing::debug!(source = "repo_watcher", path = %repo_path, "Emit head-changed");
                        let _ = bus.send(AppEvent::HeadChanged {
                            repo_path: repo_path.clone(),
                            branch: branch.clone(),
                        });
                        #[cfg(feature = "desktop")]
                        if let Some(ref handle) = h {
                            let _ = handle.emit(
                                "head-changed",
                                HeadChangedPayload { repo_path, branch },
                            );
                        }
                    }
                });
            }

            if has_git_state {
                let repo_path = repo_path_owned.clone();
                let bus = event_bus.clone();
                #[cfg(feature = "desktop")]
                let h = handle.clone();
                let st = Arc::clone(&state_cb);
                let repo = repo_for_cb.clone();
                let git_dir = git_dir_for_cb.clone();
                let em = Arc::clone(&emitter);
                let shared_changed = Arc::clone(&shared_git_changed);
                emitter.trigger(&EventCategory::GitState, move || {
                    // Skip the emit (and cache invalidation) when the meaningful git
                    // state is unchanged. A no-op `.git` touch — e.g. a non-writing
                    // status refreshing the index stat cache — leaves the fingerprint
                    // identical, so we avoid the redundant ~20-panel frontend cascade.
                    let shared_changed = shared_changed.swap(false, std::sync::atomic::Ordering::Relaxed);
                    let fp = repo_git_fingerprint(&repo, &git_dir);
                    if !shared_changed && st.repo_git_fingerprints.get(&repo_path).map(|v| *v) == Some(fp) {
                        tracing::debug!(source = "repo_watcher", path = %repo_path, "Skip repo-changed (git-state unchanged)");
                        return;
                    }
                    st.repo_git_fingerprints.insert(repo_path.clone(), fp);
                    // The worktree admin set is part of the fingerprint, so a
                    // worktree added or removed (by us or by an outside agent)
                    // always lands here — catch its watches up before emitting.
                    sync_worktree_watches(&st, &repo_path, &git_dir);
                    tracing::debug!(source = "repo_watcher", path = %repo_path, "Emit repo-changed (git-state)");
                    st.invalidate_repo_caches_from_watch(&repo_path);
                    // Dedupe: a pending working-tree emit would send this exact
                    // event again in another second. `git add`, `git commit` and
                    // `git checkout` all write `.git` *and* the working tree, so
                    // one logical change scheduled two identical cascades.
                    //
                    // Only on an emit that actually fires — the fingerprint skip
                    // above returns early, because a worktree-local edit leaves
                    // the main checkout's fingerprint identical and the pending
                    // working-tree emit is then the ONLY one that will report it.
                    em.cancel(&EventCategory::WorkingTree);
                    // GitState, not "only .git changed": the cancel above means a
                    // `git add` or `git commit` that also touched the working tree
                    // reports here and nowhere else. Consumers that need
                    // working-tree news must therefore react to this kind too.
                    let _ = bus.send(AppEvent::RepoChanged {
                        repo_path: repo_path.clone(),
                        kind: RepoChangeKind::GitState,
                    });
                    #[cfg(feature = "desktop")]
                    if let Some(ref handle) = h {
                        let _ = handle.emit(
                            "repo-changed",
                            RepoChangedPayload {
                                repo_path,
                                kind: RepoChangeKind::GitState,
                            },
                        );
                    }
                });
            }

            if has_working_tree && !has_git_state {
                let repo_path = repo_path_owned.clone();
                let bus = event_bus.clone();
                #[cfg(feature = "desktop")]
                let h = handle.clone();
                let st = Arc::clone(&state_cb);
                let wt_delay = if st.hot_repo_paths.read().contains(&repo_path) {
                    WORKING_TREE_DEBOUNCE
                } else {
                    COLD_WORKING_TREE_DEBOUNCE
                };
                emitter.trigger_with_delay(&EventCategory::WorkingTree, wt_delay, move || {
                    tracing::debug!(source = "repo_watcher", path = %repo_path, "Emit repo-changed (working-tree)");
                    st.invalidate_repo_caches_from_watch(&repo_path);
                    let _ = bus.send(AppEvent::RepoChanged {
                        repo_path: repo_path.clone(),
                        kind: RepoChangeKind::WorkingTree,
                    });
                    #[cfg(feature = "desktop")]
                    if let Some(ref handle) = h {
                        let _ = handle.emit(
                            "repo-changed",
                            RepoChangedPayload {
                                repo_path,
                                kind: RepoChangeKind::WorkingTree,
                            },
                        );
                    }
                });
            }
        },
    )
    .map_err(|e| format!("Failed to create repo watcher: {e}"))?;

    // Watch the main checkout. macOS (FSEvents) / Windows
    // (ReadDirectoryChangesW): one recursive registration, an OS-level operation
    // with near-zero cost. Linux (inotify): one non-recursive watch per
    // surviving directory — a recursive watch makes `notify` walk the whole tree
    // and add a watch per directory, including `node_modules`, `target` and
    // `.git/objects`, whose churn floods the callback and pins CPU (issue #82).
    let mut watched_dirs = std::collections::HashSet::new();
    let watch_failures = watch_working_tree_root(&mut watcher, repo.as_path(), &mut watched_dirs);
    // Only claim path-complete Git watching for a canonical main checkout.
    // Linked roots, partial registrations and watcher loss retain signature checks.
    #[allow(unused_mut)]
    let mut monitoring_watch_complete = watch_failures == 0
        && git_dir == repo.join(".git")
        && std::fs::canonicalize(&repo).ok().as_ref() == Some(&repo);

    #[cfg(not(target_os = "linux"))]
    if watch_failures > 0 {
        return Err(format!("Failed to watch repo {}", repo.display()));
    }

    #[cfg(target_os = "linux")]
    {
        // Surface partial watching instead of degrading silently: on Linux this
        // is almost always inotify watch exhaustion (one watch per dir), which
        // leaves those subtrees unmonitored with no user-visible signal.
        if watch_failures > 0 {
            tracing::warn!(
                source = "repo_watcher",
                repo = %repo.display(),
                failures = watch_failures,
                "Could not register {watch_failures} inotify watch(es) — changes in those dirs won't refresh panels. \
                 The kernel inotify limit may be exhausted; raise /proc/sys/fs/inotify/max_user_watches."
            );
        }
        // `.git` gets targeted watches (root non-recursive for HEAD/index/
        // sentinels/packed-refs, `refs` and `worktrees` recursive) so we never
        // watch `objects`/`logs`/`hooks`, the high-churn part of `.git`.
        //
        // Non-git directories have no `.git` to sub-watch yet. The working-tree
        // watches above include the repo root (WalkBuilder yields it first), so
        // the `.git` *creation* event is still caught and classified as GitState;
        // the frontend then restarts this watcher, re-entering here with `.git`
        // present to register the targeted sub-watches.
        if git_dir.is_dir() {
            watcher
                .watch(&git_dir, RecursiveMode::NonRecursive)
                .map_err(|e| format!("Failed to watch .git: {e}"))?;
            let refs_dir = git_dir.join("refs");
            if let Err(e) = watcher.watch(&refs_dir, RecursiveMode::Recursive) {
                monitoring_watch_complete = false;
                tracing::warn!(source = "repo_watcher", path = %refs_dir.display(), "Failed to watch .git/refs: {e}");
            }
            let worktrees_dir = git_dir.join("worktrees");
            if worktrees_dir.is_dir()
                && let Err(e) = watcher.watch(&worktrees_dir, RecursiveMode::Recursive)
            {
                tracing::warn!(source = "repo_watcher", path = %worktrees_dir.display(), "Failed to watch .git/worktrees: {e}");
            }
        }
    }

    // DEFERRED (2026-10-21) — Linux prunes ignored/excluded directories, which
    // can contain tracked files. Keep status fresh there; do not widen watches
    // for this macOS Defender fix.
    let status_complete = status_watch_supported(monitoring_watch_complete);
    if !status_complete {
        status_watch_complete.store(false, std::sync::atomic::Ordering::Release);
    }
    crate::monitoring_status::CACHE.set_watching(
        &repo,
        &repo,
        status_watch_complete.load(std::sync::atomic::Ordering::Acquire),
    );
    let handle = RepoWatchHandle {
        watcher: Mutex::new(watcher),
        watched_dirs: Mutex::new(watched_dirs),
        worktree_roots: Mutex::new(std::collections::HashSet::new()),
        worktree_snapshot,
        status_watch_complete,
    };
    if !handle
        .status_watch_complete
        .load(std::sync::atomic::Ordering::Acquire)
    {
        crate::monitoring_status::CACHE.stop(&repo);
    }
    state
        .repo_watchers
        .insert(repo_path.to_string(), Arc::new(handle));
    // Linked worktrees get their own watches — the handle must be registered
    // first, since the sync reaches it through `state.repo_watchers`.
    sync_worktree_watches(state, repo_path, &git_dir);
    tuic_git::worktree::set_monitoring_watch_active(&repo, monitoring_watch_complete);
    Ok(())
}

fn status_watch_supported(complete: bool) -> bool {
    complete && !cfg!(target_os = "linux")
}

/// Stop watching a repository and retire its repo-local semantic caches, so a
/// later restart starts cold instead of suppressing the first real change with
/// stale state (issue #82).
///
/// Also releases the repo's BM25 content index — the heaviest per-repo
/// allocation we hold. Every caller of this means "no longer in use": the user
/// removed the repo, parked it, or the refresh coordinator retired it. An
/// in-flight build keeps its own `Arc` and simply writes into an orphan.
pub(crate) fn stop_watching(repo_path: &str, state: &Arc<AppState>) {
    crate::monitoring_status::CACHE.stop(Path::new(repo_path));
    tuic_git::worktree::set_monitoring_watch_active(Path::new(repo_path), false);
    if state.repo_watchers.remove(repo_path).is_some() {
        tracing::info!(source = "repo_watcher", path = %repo_path, "Stopping watcher");
    }
    state.repo_head_targets.remove(repo_path);
    state.repo_git_fingerprints.remove(repo_path);
    state.content_indices.remove(repo_path);
}

// --- Tauri commands ---

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn start_repo_watcher(repo_path: String, app_handle: AppHandle) -> Result<(), String> {
    let state = app_handle.state::<Arc<AppState>>();
    start_watching(&repo_path, &state)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn stop_repo_watcher(repo_path: String, app_handle: AppHandle) {
    let state = app_handle.state::<Arc<AppState>>();
    stop_watching(&repo_path, &state);
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn set_hot_repos(paths: Vec<String>, state: tauri::State<'_, std::sync::Arc<AppState>>) {
    let mut hot = state.hot_repo_paths.write();
    hot.clear();
    hot.extend(paths);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    /// Build an empty matcher (matches nothing).
    fn empty_gitignore() -> IgnoreSet {
        IgnoreSet::empty()
    }

    /// Build a single-layer `IgnoreSet` from repo-root-relative pattern strings.
    fn gitignore_from_patterns(repo_root: &Path, patterns: &[&str]) -> IgnoreSet {
        let mut builder = ignore::gitignore::GitignoreBuilder::new(repo_root);
        for pat in patterns {
            builder.add_line(None, pat).unwrap();
        }
        IgnoreSet {
            layers: vec![(PathBuf::new(), builder.build().unwrap())],
        }
    }

    /// Catches enabling snapshot reuse on Linux's status-incomplete pruned watches.
    #[test]
    fn monitoring_status_requires_complete_platform_watches() {
        assert!(!status_watch_supported(false));
        assert_eq!(status_watch_supported(true), !cfg!(target_os = "linux"));
    }

    #[test]
    fn test_classify_head() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = empty_gitignore();

        assert_eq!(
            classify_path(Path::new("/repo/.git/HEAD"), root, git, &[], &gi),
            EventCategory::Head
        );
    }

    #[test]
    fn test_classify_git_dir_itself() {
        // The `.git` entry itself being created/removed (runtime `git init` /
        // deinit) is a GitState change — the only signal Linux gets, since
        // `.git`'s contents aren't sub-watched on a non-git directory.
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = empty_gitignore();

        assert_eq!(
            classify_path(Path::new("/repo/.git"), root, git, &[], &gi),
            EventCategory::GitState
        );
    }

    #[test]
    fn test_classify_git_state() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = empty_gitignore();

        // index
        assert_eq!(
            classify_path(Path::new("/repo/.git/index"), root, git, &[], &gi),
            EventCategory::GitState
        );
        // refs
        assert_eq!(
            classify_path(Path::new("/repo/.git/refs/heads/main"), root, git, &[], &gi),
            EventCategory::GitState
        );
        assert_eq!(
            classify_path(Path::new("/repo/.git/refs/tags/v1.0"), root, git, &[], &gi),
            EventCategory::GitState
        );
        // sentinel files
        assert_eq!(
            classify_path(Path::new("/repo/.git/MERGE_HEAD"), root, git, &[], &gi),
            EventCategory::GitState
        );
        assert_eq!(
            classify_path(Path::new("/repo/.git/REBASE_HEAD"), root, git, &[], &gi),
            EventCategory::GitState
        );
        assert_eq!(
            classify_path(
                Path::new("/repo/.git/CHERRY_PICK_HEAD"),
                root,
                git,
                &[],
                &gi
            ),
            EventCategory::GitState
        );
        assert_eq!(
            classify_path(Path::new("/repo/.git/REVERT_HEAD"), root, git, &[], &gi),
            EventCategory::GitState
        );
        // worktrees
        assert_eq!(
            classify_path(Path::new("/repo/.git/worktrees/my-wt"), root, git, &[], &gi),
            EventCategory::GitState
        );
    }

    #[test]
    fn test_classify_working_tree() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = empty_gitignore();

        assert_eq!(
            classify_path(Path::new("/repo/src/main.rs"), root, git, &[], &gi),
            EventCategory::WorkingTree
        );
        assert_eq!(
            classify_path(Path::new("/repo/README.md"), root, git, &[], &gi),
            EventCategory::WorkingTree
        );
    }

    /// A linked worktree lives outside the repo root, so without its roots the
    /// classifier drops every edit an agent makes there — the stale-diff-badge
    /// bug: the sidebar only caught up when the user selected the branch.
    #[test]
    fn test_classify_linked_worktree_outside_repo_root() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = empty_gitignore();
        let worktrees = vec![PathBuf::from("/repo__wt/feat")];

        assert_eq!(
            classify_path(Path::new("/repo__wt/feat/src/main.rs"), root, git, &[], &gi),
            EventCategory::Noise,
            "without the worktree roots the path is outside everything we watch"
        );
        assert_eq!(
            classify_path(
                Path::new("/repo__wt/feat/src/main.rs"),
                root,
                git,
                &worktrees,
                &gi
            ),
            EventCategory::WorkingTree
        );
    }

    /// A worktree stored inside the repo (`.worktrees/`, `.claude/worktrees/`)
    /// is normally gitignored, so the repo-root rules would classify it as
    /// noise. The worktree roots are matched first precisely to prevent that.
    #[test]
    fn test_classify_gitignored_worktree_inside_repo_root() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = gitignore_from_patterns(root, &[".worktrees/"]);
        let worktrees = vec![PathBuf::from("/repo/.worktrees/feat")];

        assert_eq!(
            classify_path(Path::new("/repo/.worktrees/feat/a.rs"), root, git, &[], &gi),
            EventCategory::Noise,
            "gitignored under the repo root"
        );
        assert_eq!(
            classify_path(
                Path::new("/repo/.worktrees/feat/a.rs"),
                root,
                git,
                &worktrees,
                &gi
            ),
            EventCategory::WorkingTree
        );
    }

    #[test]
    fn test_classify_worktree_noise() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = gitignore_from_patterns(root, &["*.log"]);
        let worktrees = vec![PathBuf::from("/repo__wt/feat")];

        // The worktree's `.git` is a file pointing at the admin dir under the
        // main `.git`, which is watched and classified there.
        assert_eq!(
            classify_path(Path::new("/repo__wt/feat/.git"), root, git, &worktrees, &gi),
            EventCategory::Noise
        );
        // Always-excluded dirs and gitignored files follow the same rules as the
        // main checkout — build churn must not wake the sidebar.
        assert_eq!(
            classify_path(
                Path::new("/repo__wt/feat/node_modules/x/i.js"),
                root,
                git,
                &worktrees,
                &gi
            ),
            EventCategory::Noise
        );
        assert_eq!(
            classify_path(
                Path::new("/repo__wt/feat/debug.log"),
                root,
                git,
                &worktrees,
                &gi
            ),
            EventCategory::Noise
        );
        // `.gitignore` itself is not `.git`.
        assert_eq!(
            classify_path(
                Path::new("/repo__wt/feat/.gitignore"),
                root,
                git,
                &worktrees,
                &gi
            ),
            EventCategory::WorkingTree
        );
    }

    /// `.git/worktrees/**` keeps classifying as GitState even with the roots
    /// known — that's the admin side, and it's what drives the watch sync.
    #[test]
    fn test_classify_worktree_admin_dir_still_git_state() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = empty_gitignore();
        let worktrees = vec![PathBuf::from("/repo__wt/feat")];

        assert_eq!(
            classify_path(
                Path::new("/repo/.git/worktrees/feat/gitdir"),
                root,
                git,
                &worktrees,
                &gi
            ),
            EventCategory::GitState
        );
    }

    // Catches: packing refs or changing integration configuration never wakes monitoring.
    #[test]
    fn monitoring_dependency_files_are_git_state_not_noise() {
        let root = Path::new("/repo");
        let git = root.join(".git");
        let gi = empty_gitignore();
        for name in ["packed-refs", "config"] {
            assert_eq!(
                classify_path(&git.join(name), root, &git, &[], &gi),
                EventCategory::GitState,
                "{name}"
            );
        }
    }

    #[test]
    fn test_classify_noise_git_internals() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = empty_gitignore();

        assert_eq!(
            classify_path(Path::new("/repo/.git/objects/ab/cdef"), root, git, &[], &gi),
            EventCategory::Noise
        );
        assert_eq!(
            classify_path(
                Path::new("/repo/.git/hooks/pre-commit"),
                root,
                git,
                &[],
                &gi
            ),
            EventCategory::Noise
        );
        assert_eq!(
            classify_path(Path::new("/repo/.git/logs/HEAD"), root, git, &[], &gi),
            EventCategory::Noise
        );
        assert_eq!(
            classify_path(Path::new("/repo/.git/description"), root, git, &[], &gi),
            EventCategory::Noise
        );
        assert_eq!(
            classify_path(Path::new("/repo/.git/info/exclude"), root, git, &[], &gi),
            EventCategory::Noise
        );
    }

    /// A nested git repo (`plugins/.git` here — a real one in this repo) is not
    /// this repo's git dir, so `strip_prefix(git_dir)` misses it. It must still
    /// be noise: its `objects/`, `index` and `logs/` churn on every command the
    /// inner repo runs, and every one of those writes used to reach the emitter
    /// as a working-tree change.
    #[test]
    fn test_classify_nested_git_repo_is_noise() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = empty_gitignore();

        assert_eq!(
            classify_path(
                Path::new("/repo/plugins/.git/objects/ab/cdef"),
                root,
                git,
                &[],
                &gi
            ),
            EventCategory::Noise
        );
        assert_eq!(
            classify_path(Path::new("/repo/plugins/.git/index"), root, git, &[], &gi),
            EventCategory::Noise
        );
        // Same rule for every always-excluded dir at any depth, not just the
        // first path component: build output nested one level down is still
        // build output.
        assert_eq!(
            classify_path(
                Path::new("/repo/src-tauri/target/debug/x.d"),
                root,
                git,
                &[],
                &gi
            ),
            EventCategory::Noise
        );
        assert_eq!(
            classify_path(
                Path::new("/repo/frontend/app/node_modules/x/i.js"),
                root,
                git,
                &[],
                &gi
            ),
            EventCategory::Noise
        );
        // A tracked file that merely *contains* an excluded name as a suffix is
        // untouched — only whole components count.
        assert_eq!(
            classify_path(Path::new("/repo/src/build_helper.rs"), root, git, &[], &gi),
            EventCategory::WorkingTree
        );
    }

    /// `build/` and `out/` are names projects really use for tracked source — a
    /// monorepo package, a directory of release scripts. Matching the bare name
    /// classified every edit under them as noise, so no panel ever refreshed for
    /// them. Only `.gitignore` decides whether such a directory is generated.
    #[test]
    fn test_classify_tracked_build_and_out_dirs_are_working_tree() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = empty_gitignore();

        for tracked in [
            "/repo/build/release.sh",
            "/repo/packages/build/src/index.ts",
            "/repo/out/schema.json",
            "/repo/src/out/protocol.rs",
        ] {
            assert_eq!(
                classify_path(Path::new(tracked), root, git, &[], &gi),
                EventCategory::WorkingTree,
                "{tracked} is tracked source, not build output"
            );
        }

        // Generated ones stay noise — via `.gitignore`, the source of truth.
        let ignored = gitignore_from_patterns(root, &["build/", "out/"]);
        assert_eq!(
            classify_path(Path::new("/repo/build/main.o"), root, git, &[], &ignored),
            EventCategory::Noise
        );
        assert_eq!(
            classify_path(Path::new("/repo/out/bundle.js"), root, git, &[], &ignored),
            EventCategory::Noise
        );
    }

    /// Git ranks a per-directory `.gitignore` above `$GIT_DIR/info/exclude`, so a
    /// root un-ignore must beat an `info/exclude` entry for the same path. Both
    /// sources are repo-root relative; folding them into one matcher made the
    /// later-added `info/exclude` win instead, hiding files git reports.
    #[test]
    fn test_root_gitignore_overrides_info_exclude() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git_dir = root.join(".git");
        std::fs::create_dir_all(git_dir.join("info")).unwrap();
        std::fs::write(git_dir.join("info/exclude"), "*.gen.ts\n").unwrap();
        std::fs::write(root.join(".gitignore"), "!api.gen.ts\n").unwrap();

        let gi = build_ignore(root, &git_dir);
        let classify = |rel: &str| classify_path(&root.join(rel), root, &git_dir, &[], &gi);

        assert_eq!(
            classify("api.gen.ts"),
            EventCategory::WorkingTree,
            "root .gitignore un-ignores what info/exclude hid"
        );
        assert_eq!(
            classify("other.gen.ts"),
            EventCategory::Noise,
            "info/exclude still applies to everything the root file says nothing about"
        );
    }

    /// The matcher is rebuilt only when an ignore source it was built from
    /// changes. `.git/info/exclude` is one of those sources, and it lives inside
    /// `.git` — so the noise gate that protects the `.gitignore` case must not
    /// apply to it, or its rules stay cached until the watcher restarts.
    #[test]
    fn test_ignore_source_changed_covers_info_exclude() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = empty_gitignore();
        let changed = |p: &str| ignore_source_changed(Path::new(p), root, git, &[], &gi);

        assert!(changed("/repo/.git/info/exclude"));
        assert!(changed("/repo/.gitignore"));
        assert!(changed("/repo/src/.gitignore"));
        // Vendored ignore files under an excluded dir stay out: a cargo build
        // writes several, and each would trigger a walking rebuild.
        assert!(!changed("/repo/target/debug/pkg/.gitignore"));
        assert!(!changed("/repo/node_modules/pkg/.gitignore"));
        // Unrelated `.git` internals must not rebuild anything.
        assert!(!changed("/repo/.git/index"));
        assert!(!changed("/repo/.git/info/attributes"));
        assert!(!changed("/repo/src/main.rs"));
    }

    /// The watcher must honour every ignore source git honours. Reading only the
    /// root `.gitignore` left nested `.gitignore` files, `.git/info/exclude` and
    /// the user's global ignore file invisible, so writes under those paths
    /// reached the emitter as working-tree changes.
    #[test]
    fn test_ignore_set_covers_nested_and_info_exclude() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git_dir = root.join(".git");
        std::fs::create_dir_all(git_dir.join("info")).unwrap();
        std::fs::write(root.join(".gitignore"), "/root-only.txt\n").unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/.gitignore"), "data/\n").unwrap();
        std::fs::create_dir_all(root.join("sub/data")).unwrap();
        std::fs::write(root.join("sub/data/x.json"), "{}").unwrap();
        std::fs::write(git_dir.join("info/exclude"), "history/\n").unwrap();
        std::fs::create_dir_all(root.join("history")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();

        let gi = build_ignore(root, &git_dir);
        let classify = |rel: &str| classify_path(&root.join(rel), root, &git_dir, &[], &gi);

        assert_eq!(
            classify("root-only.txt"),
            EventCategory::Noise,
            "root .gitignore"
        );
        assert_eq!(
            classify("sub/data/x.json"),
            EventCategory::Noise,
            "nested sub/.gitignore"
        );
        assert_eq!(
            classify("history/2026.md"),
            EventCategory::Noise,
            ".git/info/exclude"
        );
        // A nested pattern must not leak upwards: `data/` under `sub/` says
        // nothing about a `data/` at the repo root.
        assert_eq!(classify("data/x.json"), EventCategory::WorkingTree);
        assert_eq!(classify("src/main.rs"), EventCategory::WorkingTree);
    }

    /// `core.excludesFile` (the user's global ignore) is a third source git
    /// consults. `GIT_CONFIG_GLOBAL` is process-wide, which is safe here because
    /// `cargo nextest` runs each test in its own process.
    #[test]
    fn test_ignore_set_covers_global_excludes_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git_dir = root.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        // Canonicalised, because the Windows CI runner's temp directory is an
        // 8.3 short name (`C:\Users\RUNNER~1\…`) and `ignore`'s config reader
        // expands *every* `~` in `core.excludesFile` to the home directory, not
        // just a leading one. The path it then looks for does not exist, so the
        // global ignore is silently empty and every file reads as a working-tree
        // change. `long_form` also drops the `\\?\` prefix canonicalising adds,
        // which git config cannot read, and gives forward slashes — a git config
        // *value* treats `\` as the start of an escape sequence.
        let global_ignore = crate::fs::long_form(dir.path()).join("global-ignore");
        std::fs::write(&global_ignore, "*.swp\n").unwrap();
        let gitconfig = dir.path().join("gitconfig");
        std::fs::write(
            &gitconfig,
            format!(
                "[core]\n\texcludesFile = {}\n",
                crate::fs::portable_spelling(&global_ignore.to_string_lossy())
            ),
        )
        .unwrap();
        unsafe { std::env::set_var("GIT_CONFIG_GLOBAL", &gitconfig) };

        let gi = build_ignore(root, &git_dir);
        assert_eq!(
            classify_path(&root.join("src/main.rs.swp"), root, &git_dir, &[], &gi),
            EventCategory::Noise,
            "the global ignore file must be honoured"
        );
        assert_eq!(
            classify_path(&root.join("src/main.rs"), root, &git_dir, &[], &gi),
            EventCategory::WorkingTree
        );
    }

    /// Every path under `root`, with its bytes, as one comparable snapshot.
    /// Small trees only — this is a test fixture, not a scanner.
    #[cfg(test)]
    fn tree_snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    out.push((path.clone(), Vec::new()));
                    walk(&path, out);
                } else {
                    out.push((path.clone(), std::fs::read(&path).unwrap_or_default()));
                }
            }
        }
        let mut out = Vec::new();
        walk(root, &mut out);
        out.sort();
        out
    }

    /// Nothing Progress writes belongs in the user's source tree.
    ///
    /// The previous design wrote `.tuic/progress.sqlite3` plus WAL/SHM sidecars
    /// at the project root and had to register them in `.git/info/exclude` so
    /// the watcher would not read its own runtime state as a working-tree
    /// change. Merely opening the panel created that directory in all 39
    /// registered repositories. The store now writes one file in the config
    /// directory, so the exclude contract is not tightened — it is gone, and
    /// this asserts the stronger property that replaced it: a report leaves the
    /// project directory byte-identical.
    #[test]
    fn progress_store_writes_never_touch_the_repository() {
        use std::process::Command;
        let config = tempfile::tempdir().unwrap();
        let _config_guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().to_path_buf();
        let out = Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git init: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        let before = tree_snapshot(&repo);
        let store = crate::progress::ProgressStore::open().unwrap();
        let project = repo.to_string_lossy().to_string();
        store
            .record(
                &project,
                &crate::progress::NewProgressEntry {
                    kind: crate::progress::ProgressKind::Done,
                    text: "Watcher-storm regression coverage recorded.".to_string(),
                    step: None,
                    agent_name: None,
                },
            )
            .unwrap();
        store.delete(&project, &[1]).unwrap();
        store.mark_viewed_for_pty(&project, None).unwrap();

        assert_eq!(
            tree_snapshot(&repo),
            before,
            "a Progress write left something inside the repository"
        );
        assert!(
            store.database_path().starts_with(config.path()),
            "the journal must live in the config directory, not under any repo"
        );
    }

    #[test]
    fn test_classify_noise_gitignored() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = gitignore_from_patterns(root, &["node_modules/", "*.log"]);

        assert_eq!(
            classify_path(
                Path::new("/repo/node_modules/foo/bar.js"),
                root,
                git,
                &[],
                &gi
            ),
            EventCategory::Noise
        );
        assert_eq!(
            classify_path(Path::new("/repo/debug.log"), root, git, &[], &gi),
            EventCategory::Noise
        );
    }

    #[test]
    fn test_classify_sentinel_only_at_git_root() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = empty_gitignore();

        // .git/index → GitState
        assert_eq!(
            classify_path(Path::new("/repo/.git/index"), root, git, &[], &gi),
            EventCategory::GitState
        );
        // .git/some_subdir/index → Noise (not directly under .git/)
        assert_eq!(
            classify_path(
                Path::new("/repo/.git/some_subdir/index"),
                root,
                git,
                &[],
                &gi
            ),
            EventCategory::Noise
        );
    }

    #[test]
    fn test_payload_serialization() {
        let payload = RepoChangedPayload {
            repo_path: "/home/user/my-repo".to_string(),
            kind: RepoChangeKind::GitState,
        };
        let json = serde_json::to_string(&payload).expect("should serialize");
        assert!(json.contains("repo_path"));
        assert!(json.contains("/home/user/my-repo"));
    }

    /// The wire spelling is the contract the frontend narrows on, so it is
    /// asserted rather than left to whatever `rename_all` happens to produce.
    #[test]
    fn a_change_kind_spells_itself_in_kebab_case() {
        let git = serde_json::to_value(RepoChangeKind::GitState).unwrap();
        let work = serde_json::to_value(RepoChangeKind::WorkingTree).unwrap();
        assert_eq!(git, serde_json::json!("git-state"));
        assert_eq!(work, serde_json::json!("working-tree"));
    }

    /// The desktop Tauri payload and the SSE payload are the SAME object for
    /// the same event — the frontend store reads one field name on both
    /// transports. A field added to one and not the other is the failure this
    /// catches.
    #[test]
    fn the_tauri_payload_and_the_sse_payload_agree() {
        for kind in [RepoChangeKind::GitState, RepoChangeKind::WorkingTree] {
            let tauri = serde_json::to_value(RepoChangedPayload {
                repo_path: "/repo".to_string(),
                kind,
            })
            .unwrap();
            let sse = crate::mcp_http::sse_routes::event_payload_for_test(
                &crate::state::AppEvent::RepoChanged {
                    repo_path: "/repo".to_string(),
                    kind,
                },
            );
            assert_eq!(tauri, sse, "transports disagree for {kind:?}");
        }
    }

    // --- CategoryEmitter tests ---

    #[test]
    fn test_category_delays() {
        assert_eq!(EventCategory::Head.delay(), Duration::from_millis(200));
        assert_eq!(EventCategory::GitState.delay(), Duration::from_millis(500));
        assert_eq!(
            EventCategory::WorkingTree.delay(),
            Duration::from_millis(1500)
        );
        assert_eq!(EventCategory::Noise.delay(), Duration::ZERO);
    }

    #[tokio::test]
    async fn test_emitter_fires_after_delay() {
        let emitter = CategoryEmitter::new(tokio::runtime::Handle::current());
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        emitter.trigger(&EventCategory::Head, move || {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        });

        // Should not have fired yet
        assert_eq!(counter.load(Ordering::Relaxed), 0);

        // Wait for debounce + margin
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_emitter_trailing_debounce_resets_timer() {
        let emitter = CategoryEmitter::new(tokio::runtime::Handle::current());
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        // Trigger Head twice in quick succession — only the second should fire
        let c1 = Arc::clone(&counter);
        emitter.trigger(&EventCategory::Head, move || {
            c1.fetch_add(1, Ordering::Relaxed);
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let c2 = Arc::clone(&counter);
        emitter.trigger(&EventCategory::Head, move || {
            c2.fetch_add(10, Ordering::Relaxed);
        });

        // Wait for second debounce to complete
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Only the second trigger should have fired (value 10, not 1 or 11)
        assert_eq!(counter.load(Ordering::Relaxed), 10);
    }

    /// A GitState emit sends the exact same `RepoChanged` a pending WorkingTree
    /// emit would send a second later, so one logical change (a `git add`, a
    /// `git checkout`, a commit — all of which write `.git` *and* the working
    /// tree) produced two identical events and two full frontend cascades.
    /// Cancelling the pending one is the dedupe.
    #[tokio::test]
    async fn test_emitter_cancel_drops_a_pending_emit() {
        let emitter = CategoryEmitter::new(tokio::runtime::Handle::current());
        let wt = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let head = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let c = Arc::clone(&wt);
        emitter.trigger_with_delay(
            &EventCategory::WorkingTree,
            Duration::from_millis(80),
            move || {
                c.fetch_add(1, Ordering::Relaxed);
            },
        );
        let h = Arc::clone(&head);
        emitter.trigger(&EventCategory::Head, move || {
            h.fetch_add(1, Ordering::Relaxed);
        });

        emitter.cancel(&EventCategory::WorkingTree);

        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            wt.load(Ordering::Relaxed),
            0,
            "the cancelled working-tree emit must not fire"
        );
        assert_eq!(
            head.load(Ordering::Relaxed),
            1,
            "cancelling one category must leave the others pending"
        );
    }

    /// Cancelling with nothing pending is a no-op, and must not stop the *next*
    /// emit for that category — the GitState path calls it unconditionally after
    /// every emit it makes.
    #[tokio::test]
    async fn test_emitter_cancel_with_nothing_pending_is_harmless() {
        let emitter = CategoryEmitter::new(tokio::runtime::Handle::current());
        emitter.cancel(&EventCategory::WorkingTree);

        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = Arc::clone(&counter);
        emitter.trigger_with_delay(
            &EventCategory::WorkingTree,
            Duration::from_millis(50),
            move || {
                c.fetch_add(1, Ordering::Relaxed);
            },
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_emitter_noise_is_ignored() {
        let emitter = CategoryEmitter::new(tokio::runtime::Handle::current());
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        emitter.trigger(&EventCategory::Noise, move || {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_emitter_independent_categories() {
        let emitter = CategoryEmitter::new(tokio::runtime::Handle::current());
        let head_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let git_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let hc = Arc::clone(&head_count);
        emitter.trigger(&EventCategory::Head, move || {
            hc.fetch_add(1, Ordering::Relaxed);
        });

        let gc = Arc::clone(&git_count);
        emitter.trigger(&EventCategory::GitState, move || {
            gc.fetch_add(1, Ordering::Relaxed);
        });

        // After 300ms, Head should have fired but GitState shouldn't yet
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(head_count.load(Ordering::Relaxed), 1);
        assert_eq!(git_count.load(Ordering::Relaxed), 0);

        // After 600ms total, GitState should also have fired
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(git_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_cold_debounce_constant() {
        assert_eq!(COLD_WORKING_TREE_DEBOUNCE, Duration::from_secs(15));
        assert_eq!(
            COLD_WORKING_TREE_DEBOUNCE.as_millis() / WORKING_TREE_DEBOUNCE.as_millis(),
            10
        );
    }

    // --- git-state fingerprint (skip-emit-when-unchanged) ---

    #[test]
    fn test_fingerprint_same_state_is_equal() {
        let a = compute_git_fingerprint(1024, "refs/heads/main=abc123", " M src/main.rs\n", &[]);
        let b = compute_git_fingerprint(1024, "refs/heads/main=abc123", " M src/main.rs\n", &[]);
        assert_eq!(a, b);
    }

    #[test]
    fn test_fingerprint_changed_file_differs() {
        let clean = compute_git_fingerprint(1024, "refs/heads/main=abc123", "", &[]);
        let dirty =
            compute_git_fingerprint(1024, "refs/heads/main=abc123", " M src/main.rs\n", &[]);
        assert_ne!(clean, dirty);
    }

    #[test]
    fn test_fingerprint_branch_switch_differs() {
        let on_main = compute_git_fingerprint(1024, "refs/heads/main=abc123", "", &[]);
        let on_feat = compute_git_fingerprint(1024, "refs/heads/feature=def456", "", &[]);
        assert_ne!(on_main, on_feat);
    }

    #[test]
    fn test_fingerprint_commit_changes_head_sha() {
        // Same branch, new commit → resolved HEAD sha changes even if porcelain matches.
        let before = compute_git_fingerprint(1024, "refs/heads/main=abc123", "", &[]);
        let after = compute_git_fingerprint(1024, "refs/heads/main=zzz999", "", &[]);
        assert_ne!(before, after);
    }

    #[test]
    fn test_fingerprint_index_size_differs() {
        let small = compute_git_fingerprint(512, "refs/heads/main=abc123", "", &[]);
        let large = compute_git_fingerprint(2048, "refs/heads/main=abc123", "", &[]);
        assert_ne!(small, large);
    }

    #[test]
    fn test_fingerprint_ignores_index_mtime_noop_touch() {
        // mtime is NOT an input — a bare `touch .git/index` (size/head/status all
        // unchanged) yields the identical fingerprint, so the emit is skipped.
        let before = compute_git_fingerprint(1024, "refs/heads/main=abc123", " M a.txt\n", &[]);
        let after_noop_touch =
            compute_git_fingerprint(1024, "refs/heads/main=abc123", " M a.txt\n", &[]);
        assert_eq!(before, after_noop_touch);
    }

    /// The ghost-worktree regression: removing a worktree leaves HEAD, the index
    /// and the working tree untouched, so before the worktree set joined the
    /// fingerprint the emit was skipped and the sidebar row survived forever.
    #[test]
    fn test_fingerprint_worktree_removed_differs() {
        let two = compute_git_fingerprint(
            1024,
            "refs/heads/main=abc123",
            "",
            &["feat-a".to_string(), "feat-b".to_string()],
        );
        let one =
            compute_git_fingerprint(1024, "refs/heads/main=abc123", "", &["feat-a".to_string()]);
        assert_ne!(two, one, "a removed worktree must change the fingerprint");
    }

    #[test]
    fn test_fingerprint_worktree_added_differs() {
        let none = compute_git_fingerprint(1024, "refs/heads/main=abc123", "", &[]);
        let one =
            compute_git_fingerprint(1024, "refs/heads/main=abc123", "", &["feat-a".to_string()]);
        assert_ne!(none, one);
    }

    #[test]
    fn test_fingerprint_worktree_set_is_order_insensitive() {
        // `worktree_admin_names` sorts, so readdir order can never fake a change.
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path();
        std::fs::create_dir_all(git_dir.join("worktrees/zeta")).unwrap();
        std::fs::create_dir_all(git_dir.join("worktrees/alpha")).unwrap();
        assert_eq!(worktree_admin_names(git_dir), vec!["alpha", "zeta"]);
    }

    /// End-to-end guard on a real repo: `git worktree add` then `git worktree
    /// remove` must move the fingerprint both ways. HEAD, the index and the
    /// working tree are identical before and after, so this is exactly the case
    /// the "git-state unchanged" skip used to swallow — leaving the sidebar with
    /// rows for worktrees that no longer exist.
    #[test]
    fn test_real_worktree_add_remove_moves_fingerprint() {
        use std::process::Command;
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str], cwd: &Path| {
            let out = Command::new("git")
                .args(args)
                .current_dir(cwd)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-b", "main"], &repo);
        git(&["config", "user.email", "t@t.com"], &repo);
        git(&["config", "user.name", "T"], &repo);
        std::fs::write(repo.join("a.txt"), "hi\n").unwrap();
        git(&["add", "a.txt"], &repo);
        git(&["commit", "-m", "init"], &repo);

        let git_dir = repo.join(".git");
        let before = repo_git_fingerprint(&repo, &git_dir);

        let wt = dir.path().join("wt-feat");
        git(
            &["worktree", "add", "-b", "feat", wt.to_str().unwrap()],
            &repo,
        );
        let with_worktree = repo_git_fingerprint(&repo, &git_dir);
        assert_ne!(
            before, with_worktree,
            "adding a worktree must be visible to the git-state guard"
        );

        git(&["worktree", "remove", wt.to_str().unwrap()], &repo);
        let after_remove = repo_git_fingerprint(&repo, &git_dir);
        assert_ne!(
            with_worktree, after_remove,
            "removing a worktree must be visible to the git-state guard — \
             this is the ghost-row regression"
        );
        assert_eq!(
            before, after_remove,
            "back to the original worktree set → back to the original fingerprint"
        );
    }

    /// Every git-state transition the watcher must report has to move the
    /// fingerprint: stage, commit, conflict, and back to clean. The last leg is
    /// the demanding one — swapping *which* file is staged leaves the staged and
    /// changed counts, the index size and HEAD all identical, so only a per-file
    /// status read tells the two states apart. A fingerprint folded from counts
    /// alone goes blind there and the emit is skipped, leaving the git panel
    /// showing the file that is no longer staged.
    #[test]
    fn test_real_fingerprint_moves_across_stage_commit_conflict_and_clean() {
        use std::process::Command;
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let run = |args: &[&str]| {
            Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap()
        };
        let git = |args: &[&str]| {
            let out = run(args);
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        let write = |name: &str, body: &str| std::fs::write(repo.join(name), body).unwrap();

        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        write("a.txt", "one\n");
        write("b.txt", "one\n");
        git(&["add", "."]);
        git(&["commit", "-m", "init"]);

        let git_dir = repo.join(".git");
        let fp = || repo_git_fingerprint(&repo, &git_dir);

        let clean = fp();
        write("a.txt", "two\n");
        let dirty = fp();
        assert_ne!(
            clean, dirty,
            "a working-tree edit must move the fingerprint"
        );

        git(&["add", "a.txt"]);
        let staged = fp();
        assert_ne!(dirty, staged, "staging must move the fingerprint");

        git(&["commit", "-m", "two"]);
        let committed = fp();
        assert_ne!(staged, committed, "committing must move the fingerprint");

        // Conflict: a sibling commit on the same file, merged back.
        git(&["checkout", "-b", "feat", "HEAD~1"]);
        write("a.txt", "feat\n");
        git(&["commit", "-am", "feat"]);
        git(&["checkout", "main"]);
        assert!(
            !run(&["merge", "feat"]).status.success(),
            "the merge must conflict for this leg to test anything"
        );
        let conflict = fp();
        assert_ne!(committed, conflict, "a conflict must move the fingerprint");

        git(&["merge", "--abort"]);
        let resolved = fp();
        assert_ne!(
            conflict, resolved,
            "leaving the conflict must move the fingerprint back"
        );

        // Same counts, different file: staged a.txt -> staged b.txt.
        write("a.txt", "swap-a\n");
        write("b.txt", "swap-b\n");
        git(&["add", "a.txt"]);
        let staged_a = fp();
        git(&["restore", "--staged", "a.txt"]);
        git(&["add", "b.txt"]);
        let staged_b = fp();
        assert_ne!(
            staged_a, staged_b,
            "staging a different file must move the fingerprint even though the \
             staged/changed counts and the index size are unchanged"
        );
    }

    #[test]
    fn test_worktree_admin_names_empty_without_worktrees_dir() {
        // A repo with no linked worktrees (no `.git/worktrees`) must not error and
        // must hash the same as one whose worktrees were all removed.
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            worktree_admin_names(dir.path()),
            [] as [std::string::String; 0]
        );
    }

    /// End-to-end proof of the fix: an edit inside a linked worktree, made by
    /// anyone (an agent driven from another repo, in the original report), must
    /// reach the frontend as a `RepoChanged` for the parent repo. Before the
    /// worktree watches existed this produced no event at all and the branch's
    /// sidebar diff badge only caught up when the user selected the branch.
    #[tokio::test]
    async fn test_edit_in_linked_worktree_emits_repo_changed() {
        use std::process::Command;
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str], cwd: &Path| {
            let out = Command::new("git")
                .args(args)
                .current_dir(cwd)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-b", "main"], &repo);
        git(&["config", "user.email", "t@t.com"], &repo);
        git(&["config", "user.name", "T"], &repo);
        std::fs::write(repo.join("a.txt"), "hi\n").unwrap();
        git(&["add", "a.txt"], &repo);
        git(&["commit", "-m", "init"], &repo);
        let wt = dir.path().join("wt-feat");
        git(
            &["worktree", "add", "-b", "feat", wt.to_str().unwrap()],
            &repo,
        );

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let repo_path = repo.to_string_lossy().to_string();
        // Hot (a repo with open terminals) so the working-tree debounce is 1.5s
        // instead of the 15s cold one — the delay is not what's under test.
        state.hot_repo_paths.write().insert(repo_path.clone());
        let mut rx = state.event_bus.subscribe();
        start_watching(&repo_path, &state).unwrap();

        std::fs::write(wt.join("a.txt"), "edited by an agent\n").unwrap();

        let waited = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                match rx.recv().await {
                    Ok(AppEvent::RepoChanged { repo_path: p, kind }) if p == repo_path => {
                        return kind;
                    }
                    Ok(_) => continue,
                    Err(e) => panic!("event bus closed: {e}"),
                }
            }
        })
        .await;
        assert!(
            waited.is_ok(),
            "an edit inside the linked worktree must emit repo-changed for {repo_path}"
        );
        // And it must say so. Reporting this as GitState would put the panels
        // that only read committed history straight back to re-running `git log`
        // for a file nobody committed, which is the whole finding.
        assert_eq!(waited.unwrap(), RepoChangeKind::WorkingTree);

        stop_watching(&repo_path, &state);
    }

    /// The other half of the same contract, and the one the narrowing depends on
    /// being right: a commit writes `.git`, so it must report GitState. If this
    /// ever came through as WorkingTree, the history panels would stop
    /// refreshing on the one event that actually changes their answer — a
    /// silent staleness far worse than the redundant fetch being removed.
    ///
    /// Note the commit also rewrites the working tree (git updates the index and
    /// stat cache). Earlier edits may already have emitted WorkingTree before
    /// the commit finishes; skip those and wait for the commit notification.
    #[tokio::test]
    async fn a_commit_reports_git_state_not_working_tree_even_after_preceding_edits() {
        use std::process::Command;
        let dir = tempfile::tempdir().unwrap();
        // Canonicalize: on macOS the temp dir is `/var/...`, a symlink to
        // `/private/var/...`, and FSEvents reports the resolved path. Watching
        // the unresolved one makes `classify_path` strip_prefix fail for every
        // event, so the whole repo reads as noise and nothing is ever emitted.
        let repo = dir.path().canonicalize().unwrap().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.com"]);
        git(&["config", "user.name", "T"]);
        std::fs::write(repo.join("a.txt"), "hi\n").unwrap();
        git(&["add", "a.txt"]);
        git(&["commit", "-m", "init"]);
        let sibling = repo.parent().unwrap().join("watcher-sibling");
        git(&[
            "worktree",
            "add",
            "-b",
            "watcher-sibling",
            sibling.to_str().unwrap(),
        ]);

        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let repo_path = repo.to_string_lossy().to_string();
        state.hot_repo_paths.write().insert(repo_path.clone());
        let mut rx = state.event_bus.subscribe();
        start_watching(&repo_path, &state).unwrap();
        // Let the platform watcher arm before touching anything: FSEvents drops
        // writes that land between the registration call and the stream opening,
        // and this test makes exactly one burst of them.
        tokio::time::sleep(Duration::from_millis(500)).await;
        let before = tuic_git::worktree::monitoring_ref_key(&repo, "main").unwrap();

        std::fs::write(repo.join("b.txt"), "second\n").unwrap();
        git(&["add", "b.txt"]);
        git(&["commit", "-m", "second"]);

        let kind = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                match rx.recv().await {
                    Ok(AppEvent::RepoChanged { repo_path: p, kind })
                        if p == repo_path && kind == RepoChangeKind::GitState =>
                    {
                        return kind;
                    }
                    Ok(_) => continue,
                    Err(e) => panic!("event bus closed: {e}"),
                }
            }
        })
        .await
        .expect("a commit must emit GitState after any preceding working-tree notifications");

        assert_eq!(kind, RepoChangeKind::GitState);
        // Catches: watcher delivery refreshes panels but leaves the ref snapshot stale.
        let after = tuic_git::worktree::monitoring_ref_key(&repo, "main").unwrap();
        assert_ne!(before, after);

        // Catches: the base fingerprint suppresses later linked-checkout commits.
        for _ in 0..2 {
            let before = tuic_git::worktree::monitoring_ref_key(&repo, "watcher-sibling").unwrap();
            git(&[
                "-C",
                sibling.to_str().unwrap(),
                "commit",
                "--allow-empty",
                "-m",
                "sibling",
            ]);
            tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    match rx.recv().await {
                        Ok(AppEvent::RepoChanged { repo_path: p, kind: RepoChangeKind::GitState }) if p == repo_path => {
                            let after = tuic_git::worktree::monitoring_ref_key(&repo, "watcher-sibling").unwrap();
                            if after != before { break; }
                        }
                        Ok(_) => {}
                        Err(error) => panic!("event bus closed: {error}"),
                    }
                }
            }).await.expect("a linked-checkout commit must wake monitoring even when the base fingerprint is unchanged");
        }

        stop_watching(&repo_path, &state);
    }

    /// A repo's BM25 content index is the heaviest per-repo allocation we hold
    /// (every text file's tokens). `stop_watching` runs when the user removes a
    /// repo, parks it, or the refresh coordinator retires it — all of which mean
    /// "no longer in use" — so the index must be released there, alongside the
    /// semantic caches. Without this the map only ever grows for the life of the
    /// process.
    #[test]
    fn test_stop_watching_reaps_the_content_index() {
        let dir = tempfile::tempdir().unwrap();
        let repo_path = dir.path().to_string_lossy().to_string();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.content_indices.insert(
            repo_path.clone(),
            Arc::new(parking_lot::RwLock::new(
                crate::content_index::ContentIndex::empty(dir.path().to_path_buf()),
            )),
        );
        state
            .repo_head_targets
            .insert(repo_path.clone(), "t".into());

        stop_watching(&repo_path, &state);

        assert!(
            !state.content_indices.contains_key(&repo_path),
            "a repo that is no longer watched must not keep its content index"
        );
        assert!(!state.repo_head_targets.contains_key(&repo_path));
    }

    #[test]
    fn test_linked_worktree_roots_empty_without_worktrees_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            linked_worktree_roots(dir.path()),
            [] as [std::path::PathBuf; 0]
        );
    }

    /// The watch target for a linked worktree comes from `.git/worktrees/*/gitdir`
    /// — resolve it against real git rather than trusting the layout by memory,
    /// and check it disappears again once the worktree is removed.
    #[test]
    fn test_linked_worktree_roots_resolves_real_worktree() {
        use std::process::Command;
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str], cwd: &Path| {
            let out = Command::new("git")
                .args(args)
                .current_dir(cwd)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-b", "main"], &repo);
        git(&["config", "user.email", "t@t.com"], &repo);
        git(&["config", "user.name", "T"], &repo);
        std::fs::write(repo.join("a.txt"), "hi\n").unwrap();
        git(&["add", "a.txt"], &repo);
        git(&["commit", "-m", "init"], &repo);

        let git_dir = repo.join(".git");
        assert_eq!(
            linked_worktree_roots(&git_dir),
            [] as [std::path::PathBuf; 0]
        );

        let wt = dir.path().join("wt-feat");
        git(
            &["worktree", "add", "-b", "feat", wt.to_str().unwrap()],
            &repo,
        );
        assert_eq!(
            linked_worktree_roots(&git_dir),
            vec![wt.canonicalize().unwrap()],
            "the worktree's working-tree root must be watchable"
        );

        git(&["worktree", "remove", wt.to_str().unwrap()], &repo);
        assert!(
            linked_worktree_roots(&git_dir).is_empty(),
            "a removed worktree must drop out so its watch is released"
        );
    }

    #[test]
    fn test_resolve_head_target_attached_resolves_ref_sha() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::create_dir_all(git_dir.join("refs/heads")).unwrap();
        std::fs::write(git_dir.join("refs/heads/main"), "abc123def456\n").unwrap();

        assert_eq!(
            resolve_head_target(git_dir).as_deref(),
            Some("refs/heads/main=abc123def456")
        );
    }

    #[test]
    fn test_resolve_head_target_branch_switch_changes() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path();
        std::fs::create_dir_all(git_dir.join("refs/heads")).unwrap();
        std::fs::write(git_dir.join("refs/heads/main"), "aaa\n").unwrap();
        std::fs::write(git_dir.join("refs/heads/feature"), "bbb\n").unwrap();

        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        let on_main = resolve_head_target(git_dir);
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feature\n").unwrap();
        let on_feat = resolve_head_target(git_dir);

        assert_ne!(on_main, on_feat);
    }

    #[test]
    fn test_resolve_head_target_packed_ref_falls_back_to_ref_path() {
        // Loose ref absent (packed) → fall back to "ref: <path>", still distinguishes branches.
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(
            resolve_head_target(git_dir).as_deref(),
            Some("ref: refs/heads/main")
        );
    }

    #[test]
    fn test_resolve_head_target_detached() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path();
        std::fs::write(git_dir.join("HEAD"), "deadbeefcafe\n").unwrap();
        assert_eq!(
            resolve_head_target(git_dir).as_deref(),
            Some("deadbeefcafe")
        );
    }

    #[test]
    fn test_resolve_head_target_unreadable_is_none() {
        // No HEAD file → None, so the caller skips dedupe instead of caching "".
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(resolve_head_target(dir.path()), None);
    }

    // --- head-changed semantic dedupe (issue #82 storm guard) ---

    #[test]
    fn test_head_target_changed_suppresses_repeats() {
        let cache: dashmap::DashMap<String, String> = dashmap::DashMap::new();
        let repo = "/repo";
        // Cold start (empty cache) emits once, mirroring the GitState guard.
        assert!(head_target_changed(&cache, repo, "refs/heads/main=aaa"));
        // Identical target burst → suppressed. This is the storm guard: the
        // Linux inotify churn that re-fires `.git/HEAD` without HEAD moving.
        assert!(!head_target_changed(&cache, repo, "refs/heads/main=aaa"));
        assert!(!head_target_changed(&cache, repo, "refs/heads/main=aaa"));
        // Real branch switch → emit again, then its repeat is suppressed.
        assert!(head_target_changed(&cache, repo, "refs/heads/feature=bbb"));
        assert!(!head_target_changed(&cache, repo, "refs/heads/feature=bbb"));
    }

    #[test]
    fn test_head_target_changed_is_per_repo() {
        let cache: dashmap::DashMap<String, String> = dashmap::DashMap::new();
        assert!(head_target_changed(&cache, "/a", "t1"));
        // Different repo, same target string → still emits (per-repo keying).
        assert!(head_target_changed(&cache, "/b", "t1"));
        // Repeats now suppressed independently per repo.
        assert!(!head_target_changed(&cache, "/a", "t1"));
        assert!(!head_target_changed(&cache, "/b", "t1"));
    }

    #[test]
    fn test_collect_working_tree_dirs_prunes_excluded_and_gitignored() {
        // Build a repo tree: src/sub kept; node_modules, .git, target pruned;
        // a gitignored dir (build/) pruned via .gitignore.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for p in [
            "src/sub",
            "node_modules/pkg",
            ".git/objects",
            "target/debug",
            "build/out",
        ] {
            std::fs::create_dir_all(root.join(p)).unwrap();
        }
        std::fs::write(root.join(".gitignore"), "build/\n").unwrap();

        let dirs = collect_working_tree_dirs(root);
        let has = |rel: &str| dirs.iter().any(|d| d == &root.join(rel));

        // Kept: repo root + real source dirs.
        assert!(has(""), "repo root should be watched");
        assert!(has("src"));
        assert!(has("src/sub"));
        // Pruned: always-excluded dirs and their children.
        assert!(!has("node_modules"));
        assert!(!has("node_modules/pkg"));
        assert!(!has(".git"));
        assert!(!has(".git/objects"));
        assert!(!has("target"));
        assert!(!has("target/debug"));
        // Pruned: gitignored dir.
        assert!(!has("build"));
        assert!(!has("build/out"));
    }

    #[test]
    fn test_is_new_watchable_dir() {
        use notify::EventKind;
        use notify::event::{CreateKind, ModifyKind};
        // A folder created in the working tree needs its own watch.
        assert!(is_new_watchable_dir(
            &EventKind::Create(CreateKind::Folder),
            EventCategory::WorkingTree
        ));
        // A file create is not a directory to watch.
        assert!(!is_new_watchable_dir(
            &EventKind::Create(CreateKind::File),
            EventCategory::WorkingTree
        ));
        // A folder under an excluded/gitignored path (classified Noise) is skipped.
        assert!(!is_new_watchable_dir(
            &EventKind::Create(CreateKind::Folder),
            EventCategory::Noise
        ));
        // Non-create events never schedule a watch, even for working-tree dirs.
        assert!(!is_new_watchable_dir(
            &EventKind::Modify(ModifyKind::Any),
            EventCategory::WorkingTree
        ));
    }

    #[test]
    fn test_is_ignorable_access() {
        use notify::EventKind;
        use notify::event::{AccessKind, AccessMode, CreateKind, ModifyKind, RemoveKind};
        // Read-only access noise — git status / editors / LSPs reading files.
        assert!(is_ignorable_access(&EventKind::Access(AccessKind::Read)));
        assert!(is_ignorable_access(&EventKind::Access(AccessKind::Open(
            AccessMode::Read
        ))));
        assert!(is_ignorable_access(&EventKind::Access(AccessKind::Close(
            AccessMode::Read
        ))));
        assert!(is_ignorable_access(&EventKind::Access(AccessKind::Any)));
        // Close(Write) is a real completed write — kept.
        assert!(!is_ignorable_access(&EventKind::Access(AccessKind::Close(
            AccessMode::Write
        ))));
        // Modify / Create / Remove are never access-ignored.
        assert!(!is_ignorable_access(&EventKind::Modify(ModifyKind::Any)));
        assert!(!is_ignorable_access(&EventKind::Create(CreateKind::File)));
        assert!(!is_ignorable_access(&EventKind::Remove(RemoveKind::File)));
    }

    #[test]
    fn test_head_target_changed_detached_sha_transition() {
        // Detached HEAD: target is the raw SHA. A different SHA is a real move
        // (emits); the same SHA repeating is suppressed.
        let cache: dashmap::DashMap<String, String> = dashmap::DashMap::new();
        let repo = "/repo";
        assert!(head_target_changed(&cache, repo, "deadbeef"));
        assert!(!head_target_changed(&cache, repo, "deadbeef"));
        assert!(head_target_changed(&cache, repo, "cafef00d"));
    }

    #[tokio::test]
    async fn test_trigger_with_delay_uses_explicit_duration() {
        let emitter = CategoryEmitter::new(tokio::runtime::Handle::current());
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = Arc::clone(&counter);

        emitter.trigger_with_delay(
            &EventCategory::WorkingTree,
            Duration::from_millis(50),
            move || {
                c.fetch_add(1, Ordering::Relaxed);
            },
        );

        assert_eq!(counter.load(Ordering::Relaxed), 0);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }
}
