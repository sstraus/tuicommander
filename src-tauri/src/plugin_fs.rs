//! Filesystem API for plugins.
//!
//! Provides sandboxed read, list, and watch operations restricted to paths
//! within the user's home directory. Plugins declare `fs:read`, `fs:list`,
//! or `fs:watch` capabilities in their manifest to use these commands.

use crate::AppState;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// Maximum file size readable via plugin_read_file (10 MB).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Path validation
// ---------------------------------------------------------------------------

/// Resolve and validate that a path is within $HOME.
/// Returns the canonicalized path on success.
fn validate_within_home(raw: &str) -> Result<PathBuf, String> {
    if raw.is_empty() {
        return Err("Path is empty".into());
    }

    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err("Path must be absolute".into());
    }

    // Canonicalize resolves symlinks and .. components
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path: {e}"))?;

    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;

    if !canonical.starts_with(&home) {
        return Err("Path must be within the user's home directory".into());
    }

    Ok(canonical)
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Read a file's content as UTF-8 text.
/// Validates the path is within $HOME, enforces a 10 MB size limit.
#[tauri::command]
pub async fn plugin_read_file(
    path: String,
    _plugin_id: String,
) -> Result<String, String> {
    let canonical = validate_within_home(&path)?;

    // Check file size before reading
    let metadata = std::fs::metadata(&canonical)
        .map_err(|e| format!("Failed to stat file: {e}"))?;

    if !metadata.is_file() {
        return Err("Path is not a file".into());
    }

    if metadata.len() > MAX_FILE_SIZE {
        return Err(format!(
            "File exceeds maximum size ({} bytes > {} bytes)",
            metadata.len(),
            MAX_FILE_SIZE
        ));
    }

    std::fs::read_to_string(&canonical)
        .map_err(|e| format!("Failed to read file: {e}"))
}

/// List filenames in a directory, optionally filtered by a glob pattern.
/// Returns filenames only (not full paths). Validates path is within $HOME.
#[tauri::command]
pub async fn plugin_list_directory(
    path: String,
    pattern: Option<String>,
    _plugin_id: String,
) -> Result<Vec<String>, String> {
    let canonical = validate_within_home(&path)?;

    if !canonical.is_dir() {
        return Err("Path is not a directory".into());
    }

    let glob_pattern = pattern
        .as_deref()
        .map(|p| glob::Pattern::new(p).map_err(|e| format!("Invalid glob pattern: {e}")))
        .transpose()?;

    let entries = std::fs::read_dir(&canonical)
        .map_err(|e| format!("Failed to read directory: {e}"))?;

    let mut names = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(ref pat) = glob_pattern {
            if !pat.matches(&name) {
                continue;
            }
        }
        names.push(name);
    }

    names.sort();
    Ok(names)
}

/// Start watching a path for filesystem changes.
/// Returns a watch_id (UUID) that can be used with plugin_unwatch.
/// Emits `plugin-fs-change-{plugin_id}` Tauri events on changes.
#[tauri::command]
pub async fn plugin_watch_path(
    path: String,
    plugin_id: String,
    recursive: Option<bool>,
    debounce_ms: Option<u64>,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<String, String> {
    let canonical = validate_within_home(&path)?;

    let watch_id = uuid::Uuid::new_v4().to_string();
    let event_name = format!("plugin-fs-change-{plugin_id}");
    let debounce = std::time::Duration::from_millis(debounce_ms.unwrap_or(300));
    let mode = if recursive.unwrap_or(false) {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };

    // Channel for debouncing: collect events, emit after quiet period
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<Event>>();

    let mut watcher = RecommendedWatcher::new(tx, notify::Config::default())
        .map_err(|e| format!("Failed to create watcher: {e}"))?;

    watcher
        .watch(&canonical, mode)
        .map_err(|e| format!("Failed to watch path: {e}"))?;

    // Store watcher in AppState for cleanup
    let wid = watch_id.clone();
    state
        .plugin_watchers
        .insert(wid.clone(), (plugin_id.clone(), watcher));

    // Spawn debounce thread that emits Tauri events
    let app_handle = app.clone();
    std::thread::spawn(move || {
        debounce_loop(rx, debounce, &event_name, &app_handle);
    });

    Ok(watch_id)
}

/// Stop watching a previously registered path.
#[tauri::command]
pub async fn plugin_unwatch(
    watch_id: String,
    _plugin_id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // Remove drops the watcher, which stops the notify thread
    match state.plugin_watchers.remove(&watch_id) {
        Some(_) => Ok(()),
        None => Err(format!("Watch ID not found: {watch_id}")),
    }
}

// ---------------------------------------------------------------------------
// Debounce loop
// ---------------------------------------------------------------------------

/// Collect notify events and emit batched Tauri events after a quiet period.
fn debounce_loop(
    rx: std::sync::mpsc::Receiver<notify::Result<Event>>,
    debounce: std::time::Duration,
    event_name: &str,
    app: &AppHandle,
) {
    use std::collections::HashMap;

    loop {
        // Block until first event (or channel close)
        let first = match rx.recv() {
            Ok(Ok(event)) => event,
            Ok(Err(e)) => {
                eprintln!("[plugin_fs] Watcher error: {e}");
                continue;
            }
            Err(_) => break, // Channel closed — watcher was dropped
        };

        // Collect events during the debounce window
        let mut events_by_path: HashMap<PathBuf, String> = HashMap::new();
        classify_event(&first, &mut events_by_path);

        let deadline = std::time::Instant::now() + debounce;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match rx.recv_timeout(remaining) {
                Ok(Ok(event)) => classify_event(&event, &mut events_by_path),
                Ok(Err(e)) => eprintln!("[plugin_fs] Watcher error: {e}"),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }

        // Emit batched changes
        let changes: Vec<serde_json::Value> = events_by_path
            .into_iter()
            .map(|(path, kind)| {
                serde_json::json!({
                    "type": kind,
                    "path": path.to_string_lossy(),
                })
            })
            .collect();

        if !changes.is_empty() {
            let _ = app.emit(event_name, changes);
        }
    }
}

/// Map a notify event to a simplified type string and collect by path.
fn classify_event(event: &Event, map: &mut std::collections::HashMap<PathBuf, String>) {
    let kind = match event.kind {
        notify::EventKind::Create(_) => "create",
        notify::EventKind::Modify(_) => "modify",
        notify::EventKind::Remove(_) => "delete",
        _ => return,
    };

    for path in &event.paths {
        map.insert(path.clone(), kind.to_string());
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn validate_rejects_empty_path() {
        assert!(validate_within_home("").is_err());
    }

    #[test]
    fn validate_rejects_relative_path() {
        assert!(validate_within_home("relative/path").is_err());
    }

    #[test]
    fn validate_rejects_outside_home() {
        // /tmp is typically not inside $HOME
        // Skip this test if $HOME happens to be /tmp (unlikely)
        let home = dirs::home_dir().unwrap();
        if !Path::new("/tmp").starts_with(&home) {
            assert!(validate_within_home("/tmp").is_err());
        }
    }

    #[test]
    fn validate_accepts_home_dir() {
        let home = dirs::home_dir().unwrap();
        let result = validate_within_home(home.to_str().unwrap());
        assert!(result.is_ok());
    }

    #[test]
    fn validate_rejects_traversal() {
        let home = dirs::home_dir().unwrap();
        let traversal = format!("{}/../../../etc/passwd", home.display());
        assert!(validate_within_home(&traversal).is_err());
    }

    #[test]
    fn classify_create_event() {
        let mut map = std::collections::HashMap::new();
        let event = Event {
            kind: notify::EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/test/file.txt")],
            attrs: Default::default(),
        };
        classify_event(&event, &mut map);
        assert_eq!(map.get(Path::new("/test/file.txt")).unwrap(), "create");
    }

    #[test]
    fn classify_modify_event() {
        let mut map = std::collections::HashMap::new();
        let event = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![PathBuf::from("/test/file.txt")],
            attrs: Default::default(),
        };
        classify_event(&event, &mut map);
        assert_eq!(map.get(Path::new("/test/file.txt")).unwrap(), "modify");
    }

    #[test]
    fn classify_remove_event() {
        let mut map = std::collections::HashMap::new();
        let event = Event {
            kind: notify::EventKind::Remove(notify::event::RemoveKind::File),
            paths: vec![PathBuf::from("/test/file.txt")],
            attrs: Default::default(),
        };
        classify_event(&event, &mut map);
        assert_eq!(map.get(Path::new("/test/file.txt")).unwrap(), "delete");
    }

    #[test]
    fn classify_ignores_access_event() {
        let mut map = std::collections::HashMap::new();
        let event = Event {
            kind: notify::EventKind::Access(notify::event::AccessKind::Read),
            paths: vec![PathBuf::from("/test/file.txt")],
            attrs: Default::default(),
        };
        classify_event(&event, &mut map);
        assert!(map.is_empty());
    }

    #[test]
    fn classify_last_event_wins() {
        let mut map = std::collections::HashMap::new();
        let create = Event {
            kind: notify::EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/test/file.txt")],
            attrs: Default::default(),
        };
        let modify = Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![PathBuf::from("/test/file.txt")],
            attrs: Default::default(),
        };
        classify_event(&create, &mut map);
        classify_event(&modify, &mut map);
        assert_eq!(map.get(Path::new("/test/file.txt")).unwrap(), "modify");
    }
}
