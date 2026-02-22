use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

/// Debounce interval to coalesce rapid HEAD writes (e.g. rebase, merge)
const DEBOUNCE_MS: u64 = 200;

/// Payload emitted when a repo's HEAD changes
#[derive(Clone, serde::Serialize)]
pub(crate) struct HeadChangedPayload {
    pub repo_path: String,
    pub branch: String,
}

/// Resolve the actual HEAD file path for a repo, handling linked worktrees.
fn resolve_head_path(repo_path: &Path) -> Option<PathBuf> {
    let git_dir = crate::git::resolve_git_dir(repo_path)?;
    let head = git_dir.join("HEAD");
    if head.exists() { Some(head) } else { None }
}

/// Read the current branch from .git/HEAD (no subprocess).
fn read_current_branch(repo_path: &Path) -> Option<String> {
    crate::git::read_branch_from_head(repo_path)
}

/// Start watching .git/HEAD for a repository. Emits `"head-changed"` when the branch changes.
pub(crate) fn start_watching(
    repo_path: &str,
    app_handle: &AppHandle,
) -> Result<(), String> {
    let state = app_handle.state::<Arc<AppState>>();

    // Don't double-watch
    if state.head_watchers.contains_key(repo_path) {
        return Ok(());
    }

    let repo = PathBuf::from(repo_path);
    let head_path =
        resolve_head_path(&repo).ok_or_else(|| format!("Cannot find HEAD for {repo_path}"))?;

    let repo_path_owned = repo_path.to_string();
    let repo_clone = repo.clone();
    let handle = app_handle.clone();

    let mut debouncer = new_debouncer(
        Duration::from_millis(DEBOUNCE_MS),
        move |events: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
            let events = match events {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("Error: Head watcher error: {e:?}");
                    return;
                }
            };

            // Only care about data-change events (content writes)
            let dominated = events
                .iter()
                .any(|e| matches!(e.kind, DebouncedEventKind::Any));

            if !dominated {
                return;
            }

            if let Some(branch) = read_current_branch(&repo_clone) {
                let _ = handle.emit(
                    "head-changed",
                    HeadChangedPayload {
                        repo_path: repo_path_owned.clone(),
                        branch,
                    },
                );
            }
        },
    )
    .map_err(|e| format!("Failed to create watcher: {e}"))?;

    // Watch the HEAD file itself (not recursive — it's a single file)
    debouncer
        .watcher()
        .watch(head_path.as_path(), RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to watch HEAD: {e}"))?;

    state.head_watchers.insert(repo_path.to_string(), debouncer);
    Ok(())
}

/// Stop watching a repository's HEAD file.
pub(crate) fn stop_watching(repo_path: &str, app_handle: &AppHandle) {
    let state = app_handle.state::<Arc<AppState>>();
    // Dropping the Debouncer stops the watcher automatically
    state.head_watchers.remove(repo_path);
}

// --- Tauri commands ---

#[tauri::command]
pub(crate) fn start_head_watcher(
    repo_path: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    start_watching(&repo_path, &app_handle)
}

#[tauri::command]
pub(crate) fn stop_head_watcher(
    repo_path: String,
    app_handle: AppHandle,
) {
    stop_watching(&repo_path, &app_handle);
}
