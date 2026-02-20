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
/// index (staging), refs (commits/branches), HEAD, and merge/rebase state.
pub(crate) fn is_relevant_git_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();

    // refs/ directory: any change under it (commits, branch creates/deletes, tags)
    if path_str.contains("/refs/") {
        return true;
    }

    // Sentinel files: match by filename only to avoid false positives
    // (e.g. .git/logs/HEAD should NOT trigger, only .git/HEAD itself)
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        matches!(
            name,
            "index" | "HEAD" | "MERGE_HEAD" | "REBASE_HEAD" | "CHERRY_PICK_HEAD" | "REVERT_HEAD"
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
            let Ok(events) = events else { return };

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

    // Watch the .git/ directory recursively
    debouncer
        .watcher()
        .watch(git_dir.as_path(), RecursiveMode::Recursive)
        .map_err(|e| format!("Failed to watch .git/: {e}"))?;

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
        assert!(is_relevant_git_path(&PathBuf::from("/repo/.git/HEAD")));
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
    fn test_payload_serialization() {
        let payload = RepoChangedPayload {
            repo_path: "/home/user/my-repo".to_string(),
        };
        let json = serde_json::to_string(&payload).expect("should serialize");
        assert!(json.contains("repo_path"));
        assert!(json.contains("/home/user/my-repo"));
    }
}
