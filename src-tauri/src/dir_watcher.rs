use notify::RecursiveMode;
use notify_debouncer_full::new_debouncer;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppEvent;
use crate::AppState;

/// Debounce interval for directory content changes.
const DEBOUNCE_MS: u64 = 500;

/// Payload emitted when a watched directory's contents change.
#[derive(Clone, serde::Serialize)]
pub(crate) struct DirChangedPayload {
    pub dir_path: String,
}

/// Start watching a directory non-recursively for content changes.
/// Emits `"dir-changed"` when files are created, deleted, or renamed.
pub(crate) fn start_watching(
    dir_path: &str,
    app_handle: Option<&AppHandle>,
    state: &Arc<AppState>,
) -> Result<(), String> {
    let path = PathBuf::from(dir_path);
    if !path.is_dir() {
        return Err(format!("Directory does not exist: {dir_path}"));
    }

    // Remove previous watcher for this path (idempotent restart)
    state.dir_watchers.remove(dir_path);

    let dir_path_owned = dir_path.to_string();
    let handle = app_handle.cloned();
    let event_bus = state.event_bus.clone();

    let mut debouncer = new_debouncer(
        Duration::from_millis(DEBOUNCE_MS),
        None,
        move |events: Result<Vec<notify_debouncer_full::DebouncedEvent>, Vec<notify::Error>>| {
            let events = match events {
                Ok(evts) => evts,
                Err(errs) => {
                    if let Some(ref handle) = handle {
                        crate::app_logger::log_via_handle(
                            handle,
                            "warn",
                            "app",
                            &format!("[dir_watcher] watcher error for {dir_path_owned}: {errs:?}"),
                        );
                    }
                    return;
                }
            };

            if events.is_empty() {
                return;
            }

            // Broadcast to SSE/WebSocket consumers
            let _ = event_bus.send(AppEvent::DirChanged {
                dir_path: dir_path_owned.clone(),
            });
            // Tauri IPC for desktop
            if let Some(ref handle) = handle {
                let _ = handle.emit(
                    "dir-changed",
                    DirChangedPayload {
                        dir_path: dir_path_owned.clone(),
                    },
                );
            }
        },
    )
    .map_err(|e| format!("Failed to create dir watcher: {e}"))?;

    debouncer
        .watch(path.as_path(), RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to watch directory: {e}"))?;

    state.dir_watchers.insert(dir_path.to_string(), debouncer);
    Ok(())
}

/// Stop watching a directory.
pub(crate) fn stop_watching(dir_path: &str, state: &Arc<AppState>) {
    // Dropping the Debouncer stops the watcher automatically
    state.dir_watchers.remove(dir_path);
}

// --- Tauri commands ---

#[tauri::command]
pub(crate) fn start_dir_watcher(path: String, app_handle: AppHandle) -> Result<(), String> {
    let state = app_handle.state::<Arc<AppState>>();
    start_watching(&path, Some(&app_handle), &state)
}

#[tauri::command]
pub(crate) fn stop_dir_watcher(path: String, app_handle: AppHandle) {
    let state = app_handle.state::<Arc<AppState>>();
    stop_watching(&path, &state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_test_state() -> Arc<AppState> {
        // Reuse the minimal AppState construction pattern
        Arc::new(crate::state::tests_support::make_test_app_state())
    }

    #[test]
    fn test_watch_nonexistent_dir_returns_error() {
        let state = make_test_state();
        let result = start_watching("/tmp/nonexistent-dir-watcher-test-12345", None, &state);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_watcher_stops_on_drop() {
        let tmp = TempDir::new().unwrap();
        let dir_path = tmp.path().to_str().unwrap().to_string();
        let state = make_test_state();

        start_watching(&dir_path, None, &state).unwrap();
        assert!(state.dir_watchers.contains_key(&dir_path));

        stop_watching(&dir_path, &state);
        assert!(!state.dir_watchers.contains_key(&dir_path));
    }

    #[test]
    fn test_replace_watcher() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();
        let path1 = tmp1.path().to_str().unwrap().to_string();
        let path2 = tmp2.path().to_str().unwrap().to_string();
        let state = make_test_state();

        start_watching(&path1, None, &state).unwrap();
        assert!(state.dir_watchers.contains_key(&path1));

        // Starting a new path doesn't affect the old one
        start_watching(&path2, None, &state).unwrap();
        assert!(state.dir_watchers.contains_key(&path1));
        assert!(state.dir_watchers.contains_key(&path2));

        // Re-watching the same path replaces the watcher (no error, no double entry)
        start_watching(&path1, None, &state).unwrap();
        assert_eq!(state.dir_watchers.len(), 2);
    }

    /// FSEvents on macOS has inherent latency (~2s), making event emission tests
    /// flaky in CI. The watcher integration is covered by manual testing.
    /// This test verifies the event bus wiring without relying on FSEvents timing.
    #[test]
    fn test_event_bus_wiring() {
        let state = make_test_state();
        let mut rx = state.event_bus.subscribe();

        // Manually send a DirChanged event to verify the bus works
        let _ = state.event_bus.send(AppEvent::DirChanged {
            dir_path: "/test/path".to_string(),
        });

        match rx.try_recv() {
            Ok(AppEvent::DirChanged { dir_path }) => {
                assert_eq!(dir_path, "/test/path");
            }
            other => panic!("Expected DirChanged, got {:?}", other),
        }
    }

    #[test]
    fn test_payload_serialization() {
        let payload = DirChangedPayload {
            dir_path: "/home/user/project/src".to_string(),
        };
        let json = serde_json::to_string(&payload).expect("should serialize");
        assert!(json.contains("dir_path"));
        assert!(json.contains("/home/user/project/src"));
    }
}
