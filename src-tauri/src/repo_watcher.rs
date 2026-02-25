use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

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
pub(crate) fn start_watching(repo_path: &str, app_handle: &AppHandle) -> Result<(), String> {
    let state = app_handle.state::<Arc<AppState>>();

    // Don't double-watch
    if state.repo_watchers.contains_key(repo_path) {
        return Ok(());
    }

    let repo = PathBuf::from(repo_path);
    let git_dir =
        crate::git::resolve_git_dir(&repo).ok_or_else(|| format!("Cannot find .git for {repo_path}"))?;

    let repo_path_owned = repo_path.to_string();
    let handle = app_handle.clone();

    let mut debouncer = new_debouncer(
        Duration::from_millis(DEBOUNCE_MS),
        move |events: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
            let events = match events {
                Ok(evts) => evts,
                Err(e) => {
                    eprintln!("[repo_watcher] watcher error for {repo_path_owned}: {e}");
                    return;
                }
            };

            // Only care about data-change events with relevant paths
            let dominated = events.iter().any(|e| {
                matches!(e.kind, DebouncedEventKind::Any) && is_relevant_git_path(&e.path)
            });

            if !dominated {
                return;
            }

            let _ = handle.emit(
                "repo-changed",
                RepoChangedPayload {
                    repo_path: repo_path_owned.clone(),
                },
            );
        },
    )
    .map_err(|e| format!("Failed to create repo watcher: {e}"))?;

    // Watch .git/ non-recursively for sentinel files (index, MERGE_HEAD, etc.)
    // and .git/refs/ recursively for branch/tag changes.
    // This avoids receiving hundreds of noise events from .git/objects/ during
    // fetch/gc operations that would all be filtered out anyway.
    debouncer
        .watcher()
        .watch(git_dir.as_path(), RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to watch .git/: {e}"))?;

    let refs_dir = git_dir.join("refs");
    if refs_dir.is_dir() {
        debouncer
            .watcher()
            .watch(refs_dir.as_path(), RecursiveMode::Recursive)
            .map_err(|e| format!("Failed to watch .git/refs/: {e}"))?;
    }

    // Watch .git/worktrees/ non-recursively to detect external worktree add/remove.
    // We only care about directory-level changes (new/removed entries), not file
    // modifications within each worktree's admin dir.
    let worktrees_dir = git_dir.join("worktrees");
    if worktrees_dir.is_dir() {
        debouncer
            .watcher()
            .watch(worktrees_dir.as_path(), RecursiveMode::NonRecursive)
            .map_err(|e| format!("Failed to watch .git/worktrees/: {e}"))?;
    }

    state.repo_watchers.insert(repo_path.to_string(), debouncer);
    Ok(())
}

/// Stop watching a repository's `.git/` directory.
pub(crate) fn stop_watching(repo_path: &str, app_handle: &AppHandle) {
    let state = app_handle.state::<Arc<AppState>>();
    // Dropping the Debouncer stops the watcher automatically
    state.repo_watchers.remove(repo_path);
}

// --- Tauri commands ---

#[tauri::command]
pub(crate) fn start_repo_watcher(repo_path: String, app_handle: AppHandle) -> Result<(), String> {
    start_watching(&repo_path, &app_handle)
}

#[tauri::command]
pub(crate) fn stop_repo_watcher(repo_path: String, app_handle: AppHandle) {
    stop_watching(&repo_path, &app_handle);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
