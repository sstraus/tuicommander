use ignore::gitignore::Gitignore;
use notify::RecursiveMode;
use notify_debouncer_full::new_debouncer;
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::task::AbortHandle;

use crate::state::AppEvent;
use crate::AppState;

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
        if let Some(name) = rel.file_name().and_then(|n| n.to_str()) {
            if matches!(
                name,
                "index" | "MERGE_HEAD" | "REBASE_HEAD" | "CHERRY_PICK_HEAD" | "REVERT_HEAD"
            ) && rel.parent().is_some_and(|p| p == Path::new(""))
            {
                return EventCategory::GitState;
            }
        }

        // Everything else under .git/ is noise (objects, config, hooks, logs, etc.)
        return EventCategory::Noise;
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

/// Per-category debounce delays. The fs-level debouncer runs at HEAD's rate
/// (the fastest), and CategoryEmitter applies these app-level delays so
/// slower categories don't over-fire.
const HEAD_DEBOUNCE: Duration = Duration::from_millis(200);
const GIT_STATE_DEBOUNCE: Duration = Duration::from_millis(500);
const WORKING_TREE_DEBOUNCE: Duration = Duration::from_millis(1500);

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
    head: Mutex<Option<AbortHandle>>,
    git_state: Mutex<Option<AbortHandle>>,
    working_tree: Mutex<Option<AbortHandle>>,
}

impl CategoryEmitter {
    pub(crate) fn new() -> Self {
        Self {
            head: Mutex::new(None),
            git_state: Mutex::new(None),
            working_tree: Mutex::new(None),
        }
    }

    /// Schedule a delayed emit for the given category. If a pending emit
    /// exists for the same category, it is cancelled first (trailing debounce).
    pub(crate) fn trigger<F>(
        &self,
        category: &EventCategory,
        rt: &tokio::runtime::Handle,
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
        let delay = category.delay();
        let mut guard = slot.lock();
        if let Some(handle) = guard.take() {
            handle.abort();
        }
        let join_handle = rt.spawn(async move {
            tokio::time::sleep(delay).await;
            emit_fn();
        });
        *guard = Some(join_handle.abort_handle());
    }
}

/// Base debounce for the fs-level debouncer — set to HEAD's rate (fastest).
/// Per-category app-level debounce is handled by `CategoryEmitter`.
const BASE_DEBOUNCE_MS: u64 = 200;

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

/// Start a unified watcher for a repository. Watches the repo root recursively
/// with a single debouncer, classifies events into Head/GitState/WorkingTree/Noise,
/// and applies per-category trailing debounce via `CategoryEmitter`.
///
/// Replaces the previous separate head_watcher + repo_watcher pair.
pub(crate) fn start_watching(
    repo_path: &str,
    app_handle: Option<&AppHandle>,
    state: &Arc<AppState>,
) -> Result<(), String> {
    // Don't double-watch
    if state.repo_watchers.contains_key(repo_path) {
        return Ok(());
    }

    let repo = PathBuf::from(repo_path);
    let git_dir = crate::git::resolve_git_dir(&repo)
        .ok_or_else(|| format!("Cannot find .git for {repo_path}"))?;
    let gitignore = Arc::new(parking_lot::RwLock::new(build_gitignore(&repo)));

    let repo_path_owned = repo_path.to_string();
    let handle = app_handle.cloned();
    let event_bus = state.event_bus.clone();
    let state_cb = Arc::clone(state);
    let emitter = Arc::new(CategoryEmitter::new());
    let rt = tokio::runtime::Handle::current();

    let repo_for_cb = repo.clone();
    let git_dir_for_cb = git_dir.clone();
    let gitignore_cb = Arc::clone(&gitignore);

    let mut debouncer = new_debouncer(
        Duration::from_millis(BASE_DEBOUNCE_MS),
        None,
        move |events: Result<Vec<notify_debouncer_full::DebouncedEvent>, Vec<notify::Error>>| {
            let events = match events {
                Ok(evts) => evts,
                Err(errs) => {
                    if let Some(ref handle) = handle {
                        crate::app_logger::log_via_handle(handle, "warn", "app", &format!("[repo_watcher] error for {repo_path_owned}: {errs:?}"));
                    }
                    return;
                }
            };

            // Check if .gitignore itself changed — rebuild matcher if so
            let gitignore_changed = events.iter().any(|e| {
                e.event.paths.iter().any(|p| {
                    p.file_name()
                        .is_some_and(|n| n == ".gitignore")
                })
            });
            if gitignore_changed {
                *gitignore_cb.write() = build_gitignore(&repo_for_cb);
            }

            // Classify all event paths and collect which categories fired
            let gi = gitignore_cb.read();
            let mut has_head = false;
            let mut has_git_state = false;
            let mut has_working_tree = false;

            for event in &events {
                for path in &event.event.paths {
                    match classify_path(path, &repo_for_cb, &git_dir_for_cb, &gi) {
                        EventCategory::Head => has_head = true,
                        EventCategory::GitState => has_git_state = true,
                        EventCategory::WorkingTree => has_working_tree = true,
                        EventCategory::Noise => {}
                    }
                }
            }
            drop(gi);

            // Trigger per-category delayed emits
            if has_head {
                let repo_path = repo_path_owned.clone();
                let repo = repo_for_cb.clone();
                let bus = event_bus.clone();
                let h = handle.clone();
                emitter.trigger(&EventCategory::Head, &rt, move || {
                    if let Some(branch) = crate::git::read_branch_from_head(&repo) {
                        let _ = bus.send(AppEvent::HeadChanged {
                            repo_path: repo_path.clone(),
                            branch: branch.clone(),
                        });
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
                let h = handle.clone();
                let st = Arc::clone(&state_cb);
                emitter.trigger(&EventCategory::GitState, &rt, move || {
                    st.invalidate_repo_caches(&repo_path);
                    let _ = bus.send(AppEvent::RepoChanged {
                        repo_path: repo_path.clone(),
                    });
                    if let Some(ref handle) = h {
                        let _ = handle.emit(
                            "repo-changed",
                            RepoChangedPayload { repo_path },
                        );
                    }
                });
            }

            if has_working_tree {
                let repo_path = repo_path_owned.clone();
                let bus = event_bus.clone();
                let h = handle.clone();
                let st = Arc::clone(&state_cb);
                emitter.trigger(&EventCategory::WorkingTree, &rt, move || {
                    st.invalidate_repo_caches(&repo_path);
                    let _ = bus.send(AppEvent::RepoChanged {
                        repo_path: repo_path.clone(),
                    });
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

    // Watch repo root recursively — single watcher covers .git/ and working tree
    debouncer
        .watch(repo.as_path(), RecursiveMode::Recursive)
        .map_err(|e| format!("Failed to watch repo: {e}"))?;

    state.repo_watchers.insert(repo_path.to_string(), debouncer);
    Ok(())
}

/// Stop watching a repository.
pub(crate) fn stop_watching(repo_path: &str, state: &Arc<AppState>) {
    // Dropping the Debouncer stops the watcher automatically
    state.repo_watchers.remove(repo_path);
}

// --- Tauri commands ---

#[tauri::command]
pub(crate) fn start_repo_watcher(repo_path: String, app_handle: AppHandle) -> Result<(), String> {
    let state = app_handle.state::<Arc<AppState>>();
    start_watching(&repo_path, Some(&app_handle), &state)
}

#[tauri::command]
pub(crate) fn stop_repo_watcher(repo_path: String, app_handle: AppHandle) {
    let state = app_handle.state::<Arc<AppState>>();
    stop_watching(&repo_path, &state);
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
        assert_eq!(EventCategory::WorkingTree.delay(), Duration::from_millis(1500));
        assert_eq!(EventCategory::Noise.delay(), Duration::ZERO);
    }

    #[tokio::test]
    async fn test_emitter_fires_after_delay() {
        let emitter = CategoryEmitter::new();
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);
        let rt = tokio::runtime::Handle::current();

        emitter.trigger(&EventCategory::Head, &rt, move || {
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
        let emitter = CategoryEmitter::new();
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let rt = tokio::runtime::Handle::current();

        // Trigger Head twice in quick succession — only the second should fire
        let c1 = Arc::clone(&counter);
        emitter.trigger(&EventCategory::Head, &rt, move || {
            c1.fetch_add(1, Ordering::Relaxed);
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let c2 = Arc::clone(&counter);
        emitter.trigger(&EventCategory::Head, &rt, move || {
            c2.fetch_add(10, Ordering::Relaxed);
        });

        // Wait for second debounce to complete
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Only the second trigger should have fired (value 10, not 1 or 11)
        assert_eq!(counter.load(Ordering::Relaxed), 10);
    }

    #[tokio::test]
    async fn test_emitter_noise_is_ignored() {
        let emitter = CategoryEmitter::new();
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);
        let rt = tokio::runtime::Handle::current();

        emitter.trigger(&EventCategory::Noise, &rt, move || {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_emitter_independent_categories() {
        let emitter = CategoryEmitter::new();
        let head_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let git_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let rt = tokio::runtime::Handle::current();

        let hc = Arc::clone(&head_count);
        emitter.trigger(&EventCategory::Head, &rt, move || {
            hc.fetch_add(1, Ordering::Relaxed);
        });

        let gc = Arc::clone(&git_count);
        emitter.trigger(&EventCategory::GitState, &rt, move || {
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
}
