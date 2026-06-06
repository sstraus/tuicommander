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
#[derive(Debug, PartialEq)]
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
/// Empty string if HEAD can't be read.
fn resolve_head_target(git_dir: &Path) -> String {
    let head = std::fs::read_to_string(git_dir.join("HEAD")).unwrap_or_default();
    let trimmed = head.trim();
    if let Some(refpath) = trimmed.strip_prefix("ref: ") {
        match std::fs::read_to_string(git_dir.join(refpath)) {
            Ok(sha) if !sha.trim().is_empty() => format!("{refpath}={}", sha.trim()),
            _ => format!("ref: {refpath}"),
        }
    } else {
        trimmed.to_string()
    }
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
    let head_target = resolve_head_target(git_dir);
    let porcelain = crate::git_cli::git_cmd(repo_root)
        .args(["status", "--porcelain"])
        .run_silent()
        .map(|o| o.stdout)
        .unwrap_or_default();
    compute_git_fingerprint(index_size, &head_target, &porcelain)
}

/// Thread-safe wrapper for `RecommendedWatcher`.
///
/// `RecommendedWatcher` is `Send` but not `Sync`. Wrapping in `Mutex`
/// provides `Sync` so it can live in DashMap. The mutex is only locked
/// during `watch()`/`unwatch()` calls (not on the event hot path).
pub(crate) struct WatchHandle(#[allow(dead_code)] pub(crate) Mutex<RecommendedWatcher>);

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
    let git_dir = crate::git::resolve_git_dir(&repo)
        .ok_or_else(|| format!("Cannot find .git for {repo_path}"))?;
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
                match classify_path(path, &repo_for_cb, &git_dir_for_cb, &gi) {
                    EventCategory::Head => has_head = true,
                    EventCategory::GitState => has_git_state = true,
                    EventCategory::WorkingTree => has_working_tree = true,
                    EventCategory::Noise => {}
                }
            }
            drop(gi);

            // Trigger per-category delayed emits
            if has_head {
                let repo_path = repo_path_owned.clone();
                let repo = repo_for_cb.clone();
                let bus = event_bus.clone();
                #[cfg(feature = "desktop")]
                let h = handle.clone();
                emitter.trigger(&EventCategory::Head, move || {
                    tracing::info!(source = "repo_watcher", path = %repo_path, "Emit head-changed");
                    if let Some(branch) = crate::git::read_branch_from_head(&repo) {
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

    // Watch repo root recursively. On macOS (FSEvents) and Windows
    // (ReadDirectoryChangesW) this is a single OS-level registration
    // with near-zero cost — no directory traversal.
    watcher
        .watch(repo.as_path(), RecursiveMode::Recursive)
        .map_err(|e| format!("Failed to watch repo: {e}"))?;

    state
        .repo_watchers
        .insert(repo_path.to_string(), WatchHandle(Mutex::new(watcher)));
    Ok(())
}

/// Stop watching a repository.
pub(crate) fn stop_watching(repo_path: &str, state: &Arc<AppState>) {
    if state.repo_watchers.contains_key(repo_path) {
        tracing::info!(source = "repo_watcher", path = %repo_path, "Stopping watcher");
    }
    state.repo_watchers.remove(repo_path);
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

        assert_eq!(resolve_head_target(git_dir), "refs/heads/main=abc123def456");
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
        assert_eq!(resolve_head_target(git_dir), "ref: refs/heads/main");
    }

    #[test]
    fn test_resolve_head_target_detached() {
        let dir = tempfile::tempdir().unwrap();
        let git_dir = dir.path();
        std::fs::write(git_dir.join("HEAD"), "deadbeefcafe\n").unwrap();
        assert_eq!(resolve_head_target(git_dir), "deadbeefcafe");
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
