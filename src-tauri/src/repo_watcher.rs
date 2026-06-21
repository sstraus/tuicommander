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
/// Pure function: no I/O, no side effects. The `gitignore` matcher is used
/// to filter out ignored working-tree files.
pub(crate) fn classify_path(
    path: &Path,
    repo_root: &Path,
    git_dir: &Path,
    gitignore: &Gitignore,
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
                "index" | "MERGE_HEAD" | "REBASE_HEAD" | "CHERRY_PICK_HEAD" | "REVERT_HEAD"
            )
            && rel.parent().is_some_and(|p| p == Path::new(""))
        {
            return EventCategory::GitState;
        }

        // Everything else under .git/ is noise (objects, config, hooks, logs, etc.)
        return EventCategory::Noise;
    }

    // Always-excluded directories — noise regardless of .gitignore
    if let Ok(rel) = path.strip_prefix(repo_root)
        && let Some(first) = rel.components().next()
    {
        let name = first.as_os_str();
        if crate::fs::ALWAYS_EXCLUDED_DIRS
            .iter()
            .any(|d| name == std::ffi::OsStr::new(d))
        {
            return EventCategory::Noise;
        }
    }

    // Path is outside .git/ — check gitignore
    if let Ok(rel) = path.strip_prefix(repo_root) {
        let is_dir = path.is_dir();
        if gitignore
            .matched_path_or_any_parents(rel, is_dir)
            .is_ignore()
        {
            return EventCategory::Noise;
        }
        return EventCategory::WorkingTree;
    }

    // Path outside repo root entirely — shouldn't happen, treat as noise
    EventCategory::Noise
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
        let slot = match category {
            EventCategory::Head => &self.head,
            EventCategory::GitState => &self.git_state,
            EventCategory::WorkingTree => &self.working_tree,
            EventCategory::Noise => return,
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

/// Payload emitted when a repo's `.git/` directory changes in a meaningful way.
#[derive(Clone, serde::Serialize)]
pub(crate) struct RepoChangedPayload {
    pub repo_path: String,
}

/// Payload emitted when a repo's HEAD changes (branch switch).
#[derive(Clone, serde::Serialize)]
pub(crate) struct HeadChangedPayload {
    pub repo_path: String,
    pub branch: String,
}

/// Build a `Gitignore` matcher from the repo's `.gitignore` file.
/// Returns an empty matcher if no `.gitignore` exists.
fn build_gitignore(repo_root: &Path) -> Gitignore {
    let gitignore_path = repo_root.join(".gitignore");
    if gitignore_path.exists() {
        let mut builder = ignore::gitignore::GitignoreBuilder::new(repo_root);
        builder.add(&gitignore_path);
        builder.build().unwrap_or_else(|_| Gitignore::empty())
    } else {
        Gitignore::empty()
    }
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
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    index_size.hash(&mut hasher);
    head_target.hash(&mut hasher);
    porcelain_status.hash(&mut hasher);
    hasher.finish()
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
fn repo_git_fingerprint(repo_root: &Path, git_dir: &Path) -> u64 {
    let index_size = std::fs::metadata(git_dir.join("index"))
        .map(|m| m.len())
        .unwrap_or(0);
    let head_target = resolve_head_target(git_dir).unwrap_or_default();
    let porcelain = crate::git_cli::git_cmd(repo_root)
        .args(["status", "--porcelain"])
        .run_silent()
        .map(|o| o.stdout)
        .unwrap_or_default();
    compute_git_fingerprint(index_size, &head_target, &porcelain)
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
    // Read only on Linux (dynamic add-watch); elsewhere it's kept alive to keep
    // the watcher running but never accessed.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) watcher: Mutex<RecommendedWatcher>,
    #[cfg(target_os = "linux")]
    watched_dirs: Mutex<std::collections::HashSet<PathBuf>>,
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
fn is_ignorable_access(kind: &notify::EventKind) -> bool {
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
    let gitignore = Arc::new(parking_lot::RwLock::new(build_gitignore(&repo)));

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

    let repo_for_cb = repo.clone();
    let git_dir_for_cb = git_dir.clone();
    let gitignore_cb = Arc::clone(&gitignore);

    let mut watcher = notify::recommended_watcher(
        move |result: Result<notify::Event, notify::Error>| {
            let event = match result {
                Ok(e) => e,
                Err(err) => {
                    tracing::warn!(source = "repo_watcher", path = %repo_path_owned, "Watcher error: {err}");
                    return;
                }
            };

            // Drop read-only access events (open/read/close-nowrite) before any
            // work: file reads spray thousands per second per repo and pinned a
            // core processing them on this (notify event) thread (issue #84).
            if is_ignorable_access(&event.kind) {
                return;
            }

            // Check if .gitignore itself changed — rebuild matcher if so
            let gitignore_changed = event.paths.iter().any(|p| {
                p.file_name().is_some_and(|n| n == ".gitignore")
            });
            if gitignore_changed {
                *gitignore_cb.write() = build_gitignore(&repo_for_cb);
            }

            // Classify all event paths and collect which categories fired
            let gi = gitignore_cb.read();
            let mut has_head = false;
            let mut has_git_state = false;
            let mut has_working_tree = false;

            for path in &event.paths {
                let category = classify_path(path, &repo_for_cb, &git_dir_for_cb, &gi);
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
                emitter.trigger(&EventCategory::GitState, move || {
                    // Skip the emit (and cache invalidation) when the meaningful git
                    // state is unchanged. A no-op `.git` touch — e.g. a non-writing
                    // status refreshing the index stat cache — leaves the fingerprint
                    // identical, so we avoid the redundant ~20-panel frontend cascade.
                    let fp = repo_git_fingerprint(&repo, &git_dir);
                    if st.repo_git_fingerprints.get(&repo_path).map(|v| *v) == Some(fp) {
                        tracing::debug!(source = "repo_watcher", path = %repo_path, "Skip repo-changed (git-state unchanged)");
                        return;
                    }
                    st.repo_git_fingerprints.insert(repo_path.clone(), fp);
                    tracing::debug!(source = "repo_watcher", path = %repo_path, "Emit repo-changed (git-state)");
                    st.invalidate_repo_caches(&repo_path);
                    let _ = bus.send(AppEvent::RepoChanged {
                        repo_path: repo_path.clone(),
                    });
                    #[cfg(feature = "desktop")]
                    if let Some(ref handle) = h {
                        let _ = handle.emit(
                            "repo-changed",
                            RepoChangedPayload { repo_path },
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
                    st.invalidate_repo_caches(&repo_path);
                    let _ = bus.send(AppEvent::RepoChanged {
                        repo_path: repo_path.clone(),
                    });
                    #[cfg(feature = "desktop")]
                    if let Some(ref handle) = h {
                        let _ = handle.emit(
                            "repo-changed",
                            RepoChangedPayload { repo_path },
                        );
                    }
                });
            }
        },
    )
    .map_err(|e| format!("Failed to create repo watcher: {e}"))?;

    // macOS (FSEvents) / Windows (ReadDirectoryChangesW): a single recursive
    // registration is an OS-level operation with near-zero cost — no directory
    // traversal — so we watch the whole repo root in one call.
    #[cfg(not(target_os = "linux"))]
    watcher
        .watch(repo.as_path(), RecursiveMode::Recursive)
        .map_err(|e| format!("Failed to watch repo: {e}"))?;

    // Linux (inotify): a recursive watch makes `notify` walk the entire tree and
    // add a watch per directory — including `node_modules`, `target`, and
    // `.git/objects` — so every churn in those subtrees floods our callback and
    // pins CPU (issue #82). Split the watch instead:
    //   1. working tree — one non-recursive watch per directory, pruning the
    //      always-excluded dirs and gitignored paths up front; new dirs created
    //      after launch are picked up dynamically in the callback;
    //   2. `.git` — targeted watches (root non-recursive for HEAD/index/
    //      sentinels/packed-refs, `refs` and `worktrees` recursive) so we never
    //      watch `objects`/`logs`/`hooks`, the high-churn part of `.git`.
    #[cfg(target_os = "linux")]
    let watched_dirs = {
        let mut set = std::collections::HashSet::new();
        let mut watch_failures = 0usize;
        for dir in collect_working_tree_dirs(&repo) {
            if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
                watch_failures += 1;
                tracing::debug!(source = "repo_watcher", path = %dir.display(), "Failed to watch working-tree dir: {e}");
            } else {
                set.insert(dir);
            }
        }
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
                tracing::warn!(source = "repo_watcher", path = %refs_dir.display(), "Failed to watch .git/refs: {e}");
            }
            let worktrees_dir = git_dir.join("worktrees");
            if worktrees_dir.is_dir()
                && let Err(e) = watcher.watch(&worktrees_dir, RecursiveMode::Recursive)
            {
                tracing::warn!(source = "repo_watcher", path = %worktrees_dir.display(), "Failed to watch .git/worktrees: {e}");
            }
        }
        Mutex::new(set)
    };

    let handle = RepoWatchHandle {
        watcher: Mutex::new(watcher),
        #[cfg(target_os = "linux")]
        watched_dirs,
    };
    state
        .repo_watchers
        .insert(repo_path.to_string(), Arc::new(handle));
    Ok(())
}

/// Stop watching a repository and retire its repo-local semantic caches, so a
/// later restart starts cold instead of suppressing the first real change with
/// stale state (issue #82).
pub(crate) fn stop_watching(repo_path: &str, state: &Arc<AppState>) {
    if state.repo_watchers.remove(repo_path).is_some() {
        tracing::info!(source = "repo_watcher", path = %repo_path, "Stopping watcher");
    }
    state.repo_head_targets.remove(repo_path);
    state.repo_git_fingerprints.remove(repo_path);
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

    /// Build an empty Gitignore matcher (matches nothing).
    fn empty_gitignore() -> Gitignore {
        let gi = Gitignore::empty();
        gi
    }

    /// Build a Gitignore matcher from pattern strings.
    fn gitignore_from_patterns(repo_root: &Path, patterns: &[&str]) -> Gitignore {
        let mut builder = ignore::gitignore::GitignoreBuilder::new(repo_root);
        for pat in patterns {
            builder.add_line(None, pat).unwrap();
        }
        builder.build().unwrap()
    }

    #[test]
    fn test_classify_head() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = empty_gitignore();

        assert_eq!(
            classify_path(Path::new("/repo/.git/HEAD"), root, git, &gi),
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
            classify_path(Path::new("/repo/.git"), root, git, &gi),
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
            classify_path(Path::new("/repo/.git/index"), root, git, &gi),
            EventCategory::GitState
        );
        // refs
        assert_eq!(
            classify_path(Path::new("/repo/.git/refs/heads/main"), root, git, &gi),
            EventCategory::GitState
        );
        assert_eq!(
            classify_path(Path::new("/repo/.git/refs/tags/v1.0"), root, git, &gi),
            EventCategory::GitState
        );
        // sentinel files
        assert_eq!(
            classify_path(Path::new("/repo/.git/MERGE_HEAD"), root, git, &gi),
            EventCategory::GitState
        );
        assert_eq!(
            classify_path(Path::new("/repo/.git/REBASE_HEAD"), root, git, &gi),
            EventCategory::GitState
        );
        assert_eq!(
            classify_path(Path::new("/repo/.git/CHERRY_PICK_HEAD"), root, git, &gi),
            EventCategory::GitState
        );
        assert_eq!(
            classify_path(Path::new("/repo/.git/REVERT_HEAD"), root, git, &gi),
            EventCategory::GitState
        );
        // worktrees
        assert_eq!(
            classify_path(Path::new("/repo/.git/worktrees/my-wt"), root, git, &gi),
            EventCategory::GitState
        );
    }

    #[test]
    fn test_classify_working_tree() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = empty_gitignore();

        assert_eq!(
            classify_path(Path::new("/repo/src/main.rs"), root, git, &gi),
            EventCategory::WorkingTree
        );
        assert_eq!(
            classify_path(Path::new("/repo/README.md"), root, git, &gi),
            EventCategory::WorkingTree
        );
    }

    #[test]
    fn test_classify_noise_git_internals() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = empty_gitignore();

        assert_eq!(
            classify_path(Path::new("/repo/.git/objects/ab/cdef"), root, git, &gi),
            EventCategory::Noise
        );
        assert_eq!(
            classify_path(Path::new("/repo/.git/config"), root, git, &gi),
            EventCategory::Noise
        );
        assert_eq!(
            classify_path(Path::new("/repo/.git/hooks/pre-commit"), root, git, &gi),
            EventCategory::Noise
        );
        assert_eq!(
            classify_path(Path::new("/repo/.git/logs/HEAD"), root, git, &gi),
            EventCategory::Noise
        );
        assert_eq!(
            classify_path(Path::new("/repo/.git/description"), root, git, &gi),
            EventCategory::Noise
        );
        assert_eq!(
            classify_path(Path::new("/repo/.git/info/exclude"), root, git, &gi),
            EventCategory::Noise
        );
    }

    #[test]
    fn test_classify_noise_gitignored() {
        let root = Path::new("/repo");
        let git = Path::new("/repo/.git");
        let gi = gitignore_from_patterns(root, &["node_modules/", "*.log"]);

        assert_eq!(
            classify_path(Path::new("/repo/node_modules/foo/bar.js"), root, git, &gi),
            EventCategory::Noise
        );
        assert_eq!(
            classify_path(Path::new("/repo/debug.log"), root, git, &gi),
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
            classify_path(Path::new("/repo/.git/index"), root, git, &gi),
            EventCategory::GitState
        );
        // .git/some_subdir/index → Noise (not directly under .git/)
        assert_eq!(
            classify_path(Path::new("/repo/.git/some_subdir/index"), root, git, &gi),
            EventCategory::Noise
        );
    }

    #[test]
    fn test_payload_serialization() {
        let payload = RepoChangedPayload {
            repo_path: "/home/user/my-repo".to_string(),
        };
        let json = serde_json::to_string(&payload).expect("should serialize");
        assert!(json.contains("repo_path"));
        assert!(json.contains("/home/user/my-repo"));
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
        let a = compute_git_fingerprint(1024, "refs/heads/main=abc123", " M src/main.rs\n");
        let b = compute_git_fingerprint(1024, "refs/heads/main=abc123", " M src/main.rs\n");
        assert_eq!(a, b);
    }

    #[test]
    fn test_fingerprint_changed_file_differs() {
        let clean = compute_git_fingerprint(1024, "refs/heads/main=abc123", "");
        let dirty = compute_git_fingerprint(1024, "refs/heads/main=abc123", " M src/main.rs\n");
        assert_ne!(clean, dirty);
    }

    #[test]
    fn test_fingerprint_branch_switch_differs() {
        let on_main = compute_git_fingerprint(1024, "refs/heads/main=abc123", "");
        let on_feat = compute_git_fingerprint(1024, "refs/heads/feature=def456", "");
        assert_ne!(on_main, on_feat);
    }

    #[test]
    fn test_fingerprint_commit_changes_head_sha() {
        // Same branch, new commit → resolved HEAD sha changes even if porcelain matches.
        let before = compute_git_fingerprint(1024, "refs/heads/main=abc123", "");
        let after = compute_git_fingerprint(1024, "refs/heads/main=zzz999", "");
        assert_ne!(before, after);
    }

    #[test]
    fn test_fingerprint_index_size_differs() {
        let small = compute_git_fingerprint(512, "refs/heads/main=abc123", "");
        let large = compute_git_fingerprint(2048, "refs/heads/main=abc123", "");
        assert_ne!(small, large);
    }

    #[test]
    fn test_fingerprint_ignores_index_mtime_noop_touch() {
        // mtime is NOT an input — a bare `touch .git/index` (size/head/status all
        // unchanged) yields the identical fingerprint, so the emit is skipped.
        let before = compute_git_fingerprint(1024, "refs/heads/main=abc123", " M a.txt\n");
        let after_noop_touch =
            compute_git_fingerprint(1024, "refs/heads/main=abc123", " M a.txt\n");
        assert_eq!(before, after_noop_touch);
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
