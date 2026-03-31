use ignore::gitignore::Gitignore;
use notify::RecursiveMode;
use notify_debouncer_full::new_debouncer;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

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

/// Debounce interval — longer than HEAD watcher because git writes multiple
/// files per operation (e.g. commit updates index, refs, and logs).
const DEBOUNCE_MS: u64 = 500;

/// Payload emitted when a repo's `.git/` directory changes in a meaningful way.
#[derive(Clone, serde::Serialize)]
pub(crate) struct RepoChangedPayload {
    pub repo_path: String,
}

/// Check whether a changed path within `.git/` is relevant to panels.
/// We only care about changes that affect repo state visible to users:
/// index (staging), refs (commits/branches), merge/rebase state, and
/// worktree entries (external worktree creation/removal).
/// Note: HEAD is handled by `head_watcher` exclusively to avoid redundant events.
pub(crate) fn is_relevant_git_path(path: &Path) -> bool {
    // refs/ directory: any change under it (commits, branch creates/deletes, tags)
    // Use path components instead of string contains to work on both Unix and Windows
    if path.components().any(|c| c.as_os_str() == "refs") {
        return true;
    }

    // worktrees/ directory: changes here mean external worktree add/remove
    if path.components().any(|c| c.as_os_str() == "worktrees") {
        return true;
    }

    // Sentinel files: match by filename only to avoid false positives
    // (e.g. .git/logs/HEAD should NOT trigger, only .git/HEAD itself)
    // HEAD is NOT included — it's handled by head_watcher to avoid double-firing
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        matches!(
            name,
            "index" | "MERGE_HEAD" | "REBASE_HEAD" | "CHERRY_PICK_HEAD" | "REVERT_HEAD"
        )
    } else {
        false
    }
}

/// Start watching `.git/` recursively for a repository.
/// Emits `"repo-changed"` when relevant files change.
/// Sends events to both the broadcast channel (for SSE/WebSocket consumers) and Tauri IPC
/// (for desktop backward compat). The `app_handle` is optional for browser-only mode.
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
    let git_dir =
        crate::git::resolve_git_dir(&repo).ok_or_else(|| format!("Cannot find .git for {repo_path}"))?;

    let repo_path_owned = repo_path.to_string();
    let handle = app_handle.cloned();
    let event_bus = state.event_bus.clone();

    // Track whether .git/worktrees/ is being watched so the callback can
    // dynamically add the watch when the directory first appears (e.g. when
    // an external tool like Claude Code creates a git worktree).
    let worktrees_dir_for_check = git_dir.join("worktrees");
    let worktrees_watched = Arc::new(AtomicBool::new(worktrees_dir_for_check.is_dir()));
    let worktrees_watched_cb = Arc::clone(&worktrees_watched);
    let git_dir_cb = git_dir.clone();
    let state_cb = Arc::clone(state);

    let mut debouncer = new_debouncer(
        Duration::from_millis(DEBOUNCE_MS),
        None,
        move |events: Result<Vec<notify_debouncer_full::DebouncedEvent>, Vec<notify::Error>>| {
            let events = match events {
                Ok(evts) => evts,
                Err(errs) => {
                    if let Some(ref handle) = handle {
                        crate::app_logger::log_via_handle(handle, "warn", "app", &format!("[repo_watcher] watcher error for {repo_path_owned}: {errs:?}"));
                    }
                    return;
                }
            };

            // Only care about events with relevant paths
            let dominated = events.iter().any(|e| {
                e.event.paths.iter().any(|p| is_relevant_git_path(p))
            });

            if !dominated {
                return;
            }

            // Dynamically watch .git/worktrees/ when it first appears.
            // At startup the directory may not exist yet; external tools (e.g. Claude Code)
            // create it later via `git worktree add`. The non-recursive watch on .git/
            // detects the new `worktrees/` entry, and here we add a dedicated watch so
            // subsequent add/remove operations inside it are also detected.
            if !worktrees_watched_cb.load(Ordering::Relaxed) {
                let wt_dir = git_dir_cb.join("worktrees");
                if wt_dir.is_dir()
                    && let Some(mut debouncer_ref) = state_cb.repo_watchers.get_mut(&repo_path_owned)
                    && debouncer_ref.watch(wt_dir.as_path(), RecursiveMode::NonRecursive).is_ok()
                {
                    worktrees_watched_cb.store(true, Ordering::Relaxed);
                }
            }

            // Invalidate backend git caches so the next IPC call fetches fresh data
            state_cb.invalidate_repo_caches(&repo_path_owned);

            // Broadcast to SSE/WebSocket consumers
            let _ = event_bus.send(AppEvent::RepoChanged {
                repo_path: repo_path_owned.clone(),
            });
            // Tauri IPC for desktop backward compat
            if let Some(ref handle) = handle {
                let _ = handle.emit(
                    "repo-changed",
                    RepoChangedPayload {
                        repo_path: repo_path_owned.clone(),
                    },
                );
            }
        },
    )
    .map_err(|e| format!("Failed to create repo watcher: {e}"))?;

    // Watch .git/ non-recursively for sentinel files (index, MERGE_HEAD, etc.)
    // and .git/refs/ recursively for branch/tag changes.
    // This avoids receiving hundreds of noise events from .git/objects/ during
    // fetch/gc operations that would all be filtered out anyway.
    debouncer
        .watch(git_dir.as_path(), RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to watch .git/: {e}"))?;

    let refs_dir = git_dir.join("refs");
    if refs_dir.is_dir() {
        debouncer
            .watch(refs_dir.as_path(), RecursiveMode::Recursive)
            .map_err(|e| format!("Failed to watch .git/refs/: {e}"))?;
    }

    // Watch .git/worktrees/ non-recursively to detect external worktree add/remove.
    // We only care about directory-level changes (new/removed entries), not file
    // modifications within each worktree's admin dir.
    let worktrees_dir = git_dir.join("worktrees");
    if worktrees_dir.is_dir() {
        debouncer
            .watch(worktrees_dir.as_path(), RecursiveMode::NonRecursive)
            .map_err(|e| format!("Failed to watch .git/worktrees/: {e}"))?;
    }

    state.repo_watchers.insert(repo_path.to_string(), debouncer);
    Ok(())
}

/// Stop watching a repository's `.git/` directory.
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
    use std::path::PathBuf;

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

    // --- Legacy tests kept for is_relevant_git_path (still used by current watcher) ---

    #[test]
    fn test_relevant_git_paths() {
        // Should match
        assert!(is_relevant_git_path(&PathBuf::from("/repo/.git/index")));
        assert!(is_relevant_git_path(&PathBuf::from("/repo/.git/MERGE_HEAD")));
        assert!(is_relevant_git_path(&PathBuf::from("/repo/.git/REBASE_HEAD")));
        assert!(is_relevant_git_path(&PathBuf::from("/repo/.git/CHERRY_PICK_HEAD")));
        assert!(is_relevant_git_path(&PathBuf::from("/repo/.git/REVERT_HEAD")));
        assert!(is_relevant_git_path(&PathBuf::from(
            "/repo/.git/refs/heads/main"
        )));
        assert!(is_relevant_git_path(&PathBuf::from(
            "/repo/.git/refs/tags/v1.0"
        )));

        // worktrees/ directory: detect external worktree add/remove
        assert!(is_relevant_git_path(&PathBuf::from(
            "/repo/.git/worktrees/my-wt"
        )));
        assert!(is_relevant_git_path(&PathBuf::from(
            "/repo/.git/worktrees/my-wt/gitdir"
        )));

        // HEAD is handled by head_watcher — should NOT match here
        assert!(!is_relevant_git_path(&PathBuf::from("/repo/.git/HEAD")));

        // Should NOT match (noise we want to filter out)
        assert!(!is_relevant_git_path(&PathBuf::from("/repo/.git/config")));
        assert!(!is_relevant_git_path(&PathBuf::from(
            "/repo/.git/objects/ab/cdef1234"
        )));
        assert!(!is_relevant_git_path(&PathBuf::from(
            "/repo/.git/hooks/pre-commit"
        )));
        assert!(!is_relevant_git_path(&PathBuf::from(
            "/repo/.git/description"
        )));
        assert!(!is_relevant_git_path(&PathBuf::from(
            "/repo/.git/info/exclude"
        )));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_relevant_git_paths_windows_separators() {
        // On Windows, PathBuf parses backslashes as path separators,
        // so components() correctly finds "refs" in the path.
        assert!(is_relevant_git_path(&PathBuf::from(
            "C:\\repo\\.git\\refs\\heads\\main"
        )));
        assert!(is_relevant_git_path(&PathBuf::from(
            "C:\\repo\\.git\\index"
        )));
        assert!(!is_relevant_git_path(&PathBuf::from(
            "C:\\repo\\.git\\objects\\ab\\cdef1234"
        )));
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
}
