//! Filesystem API for plugins.
//!
//! Provides sandboxed read, list, and watch operations restricted to paths
//! within the user's home directory. Plugins declare `fs:read`, `fs:list`,
//! or `fs:watch` capabilities in their manifest to use these commands.

use crate::AppState;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
#[cfg(feature = "desktop")]
use tauri::{AppHandle, Emitter, State};

/// Maximum file size readable via plugin_read_file (10 MB).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Largest binary payload a plugin may explicitly request in one read.
/// Binary reads cross IPC/HTTP as Base64 and are commonly decoded into a
/// second WebView buffer, so the host still needs an upper stability bound.
const MAX_BINARY_FILE_SIZE: u64 = 512 * 1024 * 1024;

/// Maximum number of files one plugin_read_files request may ask for. Bounds how
/// long a single batch can hold a blocking thread; a directory larger than this
/// is a paging problem, not a batching one.
const MAX_BATCH_FILES: usize = 1000;

/// Total bytes one plugin_read_files request may return (64 MB). `MAX_FILE_SIZE`
/// alone does not bound a batch — `MAX_BATCH_FILES` files just under it would
/// retain ~10 GB before the response is even serialized.
const MAX_BATCH_BYTES: u64 = 64 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Path validation
// ---------------------------------------------------------------------------

/// Test-only serialization lock for tests that use filesystem operations.
/// Tests that set a home dir override acquire this lock first to prevent
/// parallel interference with tests that use the real home dir.
#[cfg(test)]
static FS_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Test-only override for the home directory used by path validation.
/// Uses RwLock to avoid deadlock (write tests set it, effective_home_dir reads it).
#[cfg(test)]
static HOME_DIR_OVERRIDE: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

/// Set home dir override. Returns a guard that clears the override on drop
/// and holds the serialization lock.
#[cfg(test)]
fn set_home_dir_override(dir: PathBuf) -> impl Drop {
    let fs_guard = FS_TEST_LOCK.lock().unwrap();
    *HOME_DIR_OVERRIDE.write().unwrap() = Some(dir);
    struct Guard(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);
    impl Drop for Guard {
        fn drop(&mut self) {
            *HOME_DIR_OVERRIDE.write().unwrap() = None;
        }
    }
    Guard(fs_guard)
}

fn effective_home_dir() -> Result<PathBuf, String> {
    #[cfg(test)]
    if let Some(dir) = HOME_DIR_OVERRIDE.read().unwrap().clone() {
        return dir
            .canonicalize()
            .map_err(|e| format!("Failed to resolve home override: {e}"));
    }
    // Canonicalized like the override above, and like the path it is compared
    // against in `validate_within_home`. On Windows `canonicalize` returns a
    // `\\?\`-prefixed path, so a plain home directory never prefixes it and
    // every plugin FS call is rejected — including reads of the home directory
    // itself. On unix the two forms usually coincide, which is why only the
    // Windows suite caught it.
    dirs::home_dir()
        .ok_or_else(|| "Cannot determine home directory".to_string())?
        .canonicalize()
        .map_err(|e| format!("Failed to resolve home directory: {e}"))
}

/// Resolve and validate that a path is within $HOME.
/// Returns the canonicalized path on success.
fn validate_within_home(raw: &str) -> Result<PathBuf, String> {
    if raw.is_empty() {
        return Err("Path is empty".into());
    }

    let path = PathBuf::from(crate::cli::expand_tilde(raw));
    if !path.is_absolute() {
        return Err("Path must be absolute".into());
    }

    // Canonicalize resolves symlinks and .. components
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path: {e}"))?;

    let home = effective_home_dir()?;

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
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plugin_read_file(
    path: String,
    plugin_id: String,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<String, String> {
    plugin_read_file_impl(&state, path, plugin_id).await
}

/// Read several files as UTF-8 text in one call, in request order.
/// Each entry is the file's content, or `null` when that path could not be read.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plugin_read_files(
    paths: Vec<String>,
    plugin_id: String,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<Vec<Option<String>>, String> {
    plugin_read_files_impl(&state, paths, plugin_id).await
}

/// Read a file's raw bytes as base64.
/// Validates the path is within $HOME, enforces the same 10 MB size limit as
/// plugin_read_file.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plugin_read_file_base64(
    path: String,
    max_bytes: Option<u64>,
    plugin_id: String,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<String, String> {
    plugin_read_file_base64_impl(&state, path, max_bytes, plugin_id).await
}

/// Atomically write raw Base64 bytes within `$HOME`.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plugin_write_file_base64(
    path: String,
    content: String,
    max_bytes: Option<u64>,
    plugin_id: String,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<(), String> {
    plugin_write_file_base64_impl(&state, path, content, max_bytes, plugin_id).await
}

/// Run a blocking filesystem closure on Tokio's blocking pool, flattening the
/// JoinError into the closure's own `Result<T, String>`. Keeps the synchronous
/// `std::fs` calls off the async worker threads.
async fn spawn_blocking_fs<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("fs task failed: {e}"))?
}

/// Read one file as UTF-8 text, confined to $HOME and capped at `max_bytes`.
/// Blocking: call it from inside `spawn_blocking_fs`.
fn read_text_capped(path: &str, max_bytes: u64) -> Result<String, String> {
    let canonical = validate_within_home(path)?;

    // Check file size before reading
    let metadata =
        std::fs::metadata(&canonical).map_err(|e| format!("Failed to stat file: {e}"))?;

    if !metadata.is_file() {
        return Err("Path is not a file".into());
    }

    if metadata.len() > max_bytes {
        return Err(format!(
            "File exceeds maximum size ({} bytes > {} bytes)",
            metadata.len(),
            max_bytes
        ));
    }

    std::fs::read_to_string(&canonical).map_err(|e| format!("Failed to read file: {e}"))
}

/// Read one file as UTF-8 text, confined to $HOME and capped at `MAX_FILE_SIZE`.
/// Blocking: call it from inside `spawn_blocking_fs`.
fn read_text_within_home(path: &str) -> Result<String, String> {
    read_text_capped(path, MAX_FILE_SIZE)
}

/// Read many files against a shared byte budget. An entry is `None` when that
/// path could not be read: a plugin listing a directory and reading every entry
/// must not lose the whole batch to one file that vanished between the list and
/// the read, or to one file too big to fit.
///
/// The budget is what bounds a batch — the per-file cap does not: `MAX_BATCH_FILES`
/// files just under it would retain three orders of magnitude more than any single
/// read is allowed to. Spending it file by file, rather than refusing the request,
/// keeps one oversized file from poisoning the ones after it.
/// Blocking: call it from inside `spawn_blocking_fs`.
fn read_files_within_budget(
    paths: Vec<String>,
    mut budget: u64,
) -> Result<Vec<Option<String>>, String> {
    if paths.len() > MAX_BATCH_FILES {
        return Err(format!(
            "Too many files in one batch ({} > {MAX_BATCH_FILES})",
            paths.len()
        ));
    }
    Ok(paths
        .iter()
        .map(|path| {
            let text = read_text_capped(path, budget.min(MAX_FILE_SIZE)).ok()?;
            budget = budget.saturating_sub(text.len() as u64);
            Some(text)
        })
        .collect())
}

fn read_files_within_home(paths: Vec<String>) -> Result<Vec<Option<String>>, String> {
    read_files_within_budget(paths, MAX_BATCH_BYTES)
}

pub(crate) async fn plugin_read_file_impl(
    state: &std::sync::Arc<crate::AppState>,
    path: String,
    plugin_id: String,
) -> Result<String, String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "fs:read")?;
    spawn_blocking_fs(move || read_text_within_home(&path)).await
}

pub(crate) async fn plugin_read_files_impl(
    state: &std::sync::Arc<crate::AppState>,
    paths: Vec<String>,
    plugin_id: String,
) -> Result<Vec<Option<String>>, String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "fs:read")?;
    spawn_blocking_fs(move || read_files_within_home(paths)).await
}

pub(crate) async fn plugin_read_file_base64_impl(
    state: &std::sync::Arc<crate::AppState>,
    path: String,
    max_bytes: Option<u64>,
    plugin_id: String,
) -> Result<String, String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "fs:read")?;
    let limit = binary_read_limit(max_bytes)?;
    spawn_blocking_fs(move || read_binary_base64_capped(&path, limit)).await
}

fn binary_read_limit(requested: Option<u64>) -> Result<u64, String> {
    match requested {
        None => Ok(MAX_FILE_SIZE),
        Some(0) => Err("maxBytes must be greater than zero".into()),
        Some(bytes) => Ok(bytes.min(MAX_BINARY_FILE_SIZE)),
    }
}

fn read_binary_base64_capped(path: &str, max_bytes: u64) -> Result<String, String> {
    use base64::Engine;

    let canonical = validate_within_home(path)?;
    let metadata =
        std::fs::metadata(&canonical).map_err(|e| format!("Failed to stat file: {e}"))?;

    if !metadata.is_file() {
        return Err("Path is not a file".into());
    }

    if metadata.len() > max_bytes {
        return Err(format!(
            "File exceeds maximum size ({} bytes > {} bytes)",
            metadata.len(),
            max_bytes
        ));
    }

    let bytes = std::fs::read(&canonical).map_err(|e| format!("Failed to read file: {e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

pub(crate) async fn plugin_write_file_base64_impl(
    state: &std::sync::Arc<crate::AppState>,
    path: String,
    content: String,
    max_bytes: Option<u64>,
    plugin_id: String,
) -> Result<(), String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "fs:write")?;
    let limit = binary_read_limit(max_bytes)?;
    spawn_blocking_fs(move || write_binary_base64_capped(&path, &content, limit)).await
}

fn write_binary_base64_capped(path: &str, content: &str, max_bytes: u64) -> Result<(), String> {
    use base64::Engine;

    let maximum_encoded_len = max_bytes.div_ceil(3).saturating_mul(4);
    if content.len() as u64 > maximum_encoded_len {
        return Err(format!(
            "Content exceeds maximum size (encoded payload > {max_bytes} decoded bytes)"
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(content)
        .map_err(|e| format!("Invalid Base64 content: {e}"))?;
    if bytes.len() as u64 > max_bytes {
        return Err(format!(
            "Content exceeds maximum size ({} bytes > {} bytes)",
            bytes.len(),
            max_bytes
        ));
    }

    let target = validate_write_target(path)?;
    crate::fs::atomic_write(&target, &bytes).map_err(|e| format!("Failed to write file: {e}"))
}

/// List filenames in a directory, optionally filtered by a glob pattern.
/// Returns filenames only (not full paths). Validates path is within $HOME.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plugin_list_directory(
    path: String,
    pattern: Option<String>,
    sort_by: Option<String>,
    plugin_id: String,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<Vec<String>, String> {
    plugin_list_directory_impl(&state, path, pattern, sort_by, plugin_id).await
}

pub(crate) async fn plugin_list_directory_impl(
    state: &std::sync::Arc<crate::AppState>,
    path: String,
    pattern: Option<String>,
    sort_by: Option<String>,
    plugin_id: String,
) -> Result<Vec<String>, String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "fs:list")?;
    plugin_list_directory_inner(path, pattern, sort_by).await
}

async fn plugin_list_directory_inner(
    path: String,
    pattern: Option<String>,
    sort_by: Option<String>,
) -> Result<Vec<String>, String> {
    spawn_blocking_fs(move || {
        let canonical = validate_within_home(&path)?;

        if !canonical.is_dir() {
            return Err("Path is not a directory".into());
        }

        let glob_pattern = pattern
            .as_deref()
            .map(|p| glob::Pattern::new(p).map_err(|e| format!("Invalid glob pattern: {e}")))
            .transpose()?;

        let entries =
            std::fs::read_dir(&canonical).map_err(|e| format!("Failed to read directory: {e}"))?;

        // Sort mode: "name" (default, alphabetical) or "mtime" (newest first).
        // mtime mode enables plugins to efficiently find recently-modified files
        // without scanning every entry (e.g. cache-keepalive picking the active JSONL).
        let sort_mode = sort_by.as_deref().unwrap_or("name");
        let mut items: Vec<(String, std::time::SystemTime)> = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(ref pat) = glob_pattern
                && !pat.matches(&name)
            {
                continue;
            }
            let mtime = if sort_mode == "mtime" {
                entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH)
            } else {
                std::time::UNIX_EPOCH
            };
            items.push((name, mtime));
        }

        match sort_mode {
            "mtime" => items.sort_by_key(|a| std::cmp::Reverse(a.1)),
            _ => items.sort_by(|a, b| a.0.cmp(&b.0)),
        }
        Ok(items.into_iter().map(|(n, _)| n).collect())
    })
    .await
}

/// Read the last `max_bytes` of a file as UTF-8 text.
/// Seeks to `file_size - max_bytes`, then skips to the next newline to avoid
/// partial lines. If the file is smaller than `max_bytes`, reads the entire file.
/// Validates path is within $HOME, same as plugin_read_file.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plugin_read_file_tail(
    path: String,
    max_bytes: u64,
    plugin_id: String,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<String, String> {
    plugin_read_file_tail_impl(&state, path, max_bytes, plugin_id).await
}

pub(crate) async fn plugin_read_file_tail_impl(
    state: &std::sync::Arc<crate::AppState>,
    path: String,
    max_bytes: u64,
    plugin_id: String,
) -> Result<String, String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "fs:read")?;
    plugin_read_file_tail_inner(path, max_bytes).await
}

async fn plugin_read_file_tail_inner(path: String, max_bytes: u64) -> Result<String, String> {
    // Clamp the tail window so a caller can't force a huge heap reservation
    // (the HTTP route exposes this without plugin-JS bounds). Matches the 10 MB
    // whole-file ceiling in `plugin_read_file_impl`.
    const MAX_TAIL_BYTES: u64 = 10 * 1024 * 1024;
    let max_bytes = max_bytes.min(MAX_TAIL_BYTES);

    spawn_blocking_fs(move || {
        use std::io::{Read, Seek, SeekFrom};

        let canonical = validate_within_home(&path)?;

        let metadata =
            std::fs::metadata(&canonical).map_err(|e| format!("Failed to stat file: {e}"))?;

        if !metadata.is_file() {
            return Err("Path is not a file".into());
        }

        let file_size = metadata.len();

        // If the file fits within max_bytes, read the whole thing
        if file_size <= max_bytes {
            return std::fs::read_to_string(&canonical)
                .map_err(|e| format!("Failed to read file: {e}"));
        }

        let mut file =
            std::fs::File::open(&canonical).map_err(|e| format!("Failed to open file: {e}"))?;

        let seek_pos = file_size - max_bytes;
        file.seek(SeekFrom::Start(seek_pos))
            .map_err(|e| format!("Failed to seek: {e}"))?;

        let mut buf = Vec::with_capacity(max_bytes as usize);
        file.read_to_end(&mut buf)
            .map_err(|e| format!("Failed to read file tail: {e}"))?;

        let text = String::from_utf8_lossy(&buf);

        // Skip partial first line (find first newline and skip past it)
        match text.find('\n') {
            Some(idx) => Ok(text[idx + 1..].to_string()),
            None => Ok(text.to_string()),
        }
    })
    .await
}

/// Maximum concurrent `fs:watch` registrations per plugin. Each live watch costs
/// one OS thread (`debounce_loop`) plus a notify watcher, so an unbounded plugin
/// re-registering without disposing leaks both per call. The cap is generous —
/// legitimate multi-directory workflows stay well under it — while bounding the
/// leak. Enforced in `plugin_watch_path` before a watcher is created; watches are
/// released on unload/uninstall by `dispose_plugin_runtime_state` (plugins.rs).
#[cfg(feature = "desktop")]
const MAX_WATCHERS_PER_PLUGIN: usize = 20;

/// Reject a new watch once a plugin already holds `MAX_WATCHERS_PER_PLUGIN` live
/// watchers. Counts the plugin's current entries in `plugin_watchers`.
#[cfg(feature = "desktop")]
fn check_watcher_cap(state: &AppState, plugin_id: &str) -> Result<(), String> {
    let count = state
        .plugin_watchers
        .iter()
        .filter(|e| e.value().0 == plugin_id)
        .count();
    if count >= MAX_WATCHERS_PER_PLUGIN {
        return Err(format!(
            "Watch limit reached: plugin '{plugin_id}' already has {MAX_WATCHERS_PER_PLUGIN} active watchers"
        ));
    }
    Ok(())
}

/// Start watching a path for filesystem changes.
/// Returns a watch_id (UUID) that can be used with plugin_unwatch.
/// Emits `plugin-fs-change-{watch_id}` Tauri events on changes. The name is
/// keyed on the watch, not the plugin: a plugin with K watches would otherwise
/// have every change delivered to all K of its callbacks.
// DESKTOP-ONLY (HTTP parity): event delivery to plugins needs AppHandle/WS — out of scope
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plugin_watch_path(
    path: String,
    plugin_id: String,
    recursive: Option<bool>,
    debounce_ms: Option<u64>,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<String, String> {
    crate::plugins::check_plugin_capability(&state, &plugin_id, "fs:watch")?;
    check_watcher_cap(&state, &plugin_id)?;
    let canonical = validate_within_home(&path)?;

    let watch_id = uuid::Uuid::new_v4().to_string();
    let event_name = format!("plugin-fs-change-{watch_id}");
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
// DESKTOP-ONLY (HTTP parity): event delivery to plugins needs AppHandle/WS — out of scope
#[cfg(feature = "desktop")]
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

#[cfg(feature = "desktop")]
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
                crate::app_logger::log_via_handle(
                    app,
                    "warn",
                    "plugin",
                    &format!("[plugin_fs] Watcher error: {e}"),
                );
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
                Ok(Err(e)) => crate::app_logger::log_via_handle(
                    app,
                    "warn",
                    "plugin",
                    &format!("[plugin_fs] Watcher error: {e}"),
                ),
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
// Write & Rename (capability-gated: fs:write, fs:rename)
// ---------------------------------------------------------------------------

/// Maximum content size writable via plugin_write_file (10 MB).
const MAX_WRITE_SIZE: usize = 10 * 1024 * 1024;

/// Write content to a file within $HOME.
/// Creates parent directories if needed. Refuses to overwrite directories.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plugin_write_file(
    path: String,
    content: String,
    plugin_id: String,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<(), String> {
    plugin_write_file_impl(&state, path, content, plugin_id).await
}

pub(crate) async fn plugin_write_file_impl(
    state: &std::sync::Arc<crate::AppState>,
    path: String,
    content: String,
    plugin_id: String,
) -> Result<(), String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "fs:write")?;
    plugin_write_file_inner(path, content).await
}

/// Core write logic, separated from the Tauri command wrapper for testability.
///
/// The whole body runs on the blocking pool like every other fs entry point here:
/// the existence probes, `canonicalize`, `create_dir_all` and the write itself are
/// all synchronous syscalls, and a plugin writing to a slow or network-mounted
/// path would otherwise stall an async worker thread.
async fn plugin_write_file_inner(path: String, content: String) -> Result<(), String> {
    if content.len() > MAX_WRITE_SIZE {
        return Err(format!(
            "Content exceeds maximum size ({} bytes > {} bytes)",
            content.len(),
            MAX_WRITE_SIZE
        ));
    }

    spawn_blocking_fs(move || {
        let file_path = validate_write_target(&path)?;
        std::fs::write(&file_path, &content).map_err(|e| format!("Failed to write file: {e}"))
    })
    .await
}

fn validate_write_target(path: &str) -> Result<PathBuf, String> {
    let file_path = PathBuf::from(path);
    if !file_path.is_absolute() {
        return Err("Path must be absolute".into());
    }

    let home = effective_home_dir()?;
    if file_path.exists() {
        let canonical = file_path
            .canonicalize()
            .map_err(|e| format!("Failed to resolve path: {e}"))?;
        if !canonical.starts_with(&home) {
            return Err("Path must be within the user's home directory".into());
        }
        if canonical.is_dir() {
            return Err("Cannot overwrite a directory".into());
        }
        return Ok(canonical);
    }

    let parent = file_path
        .parent()
        .ok_or("Cannot determine parent directory")?;
    if !parent.exists() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create parent directories: {e}"))?;
    }
    let canonical_parent = parent
        .canonicalize()
        .map_err(|e| format!("Failed to resolve parent path: {e}"))?;
    if !canonical_parent.starts_with(&home) {
        return Err("Path must be within the user's home directory".into());
    }
    Ok(file_path)
}

/// Rename/move a file within $HOME.
/// Both source and destination must be within $HOME. Source must exist.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn plugin_rename_path(
    from: String,
    to: String,
    plugin_id: String,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<(), String> {
    plugin_rename_path_impl(&state, from, to, plugin_id).await
}

pub(crate) async fn plugin_rename_path_impl(
    state: &std::sync::Arc<crate::AppState>,
    from: String,
    to: String,
    plugin_id: String,
) -> Result<(), String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "fs:rename")?;
    plugin_rename_path_inner(from, to).await
}

/// Same blocking-pool rule as [`plugin_write_file_inner`]: `validate_within_home`
/// canonicalizes, and `create_dir_all`/`rename` are synchronous syscalls.
async fn plugin_rename_path_inner(from: String, to: String) -> Result<(), String> {
    spawn_blocking_fs(move || {
        let from_path = validate_within_home(&from)?;

        let to_path = PathBuf::from(&to);
        if !to_path.is_absolute() {
            return Err("Destination path must be absolute".into());
        }

        let home = effective_home_dir()?;

        let to_parent = to_path
            .parent()
            .ok_or("Cannot determine destination parent directory")?;
        if !to_parent.exists() {
            std::fs::create_dir_all(to_parent)
                .map_err(|e| format!("Failed to create destination parent directories: {e}"))?;
        }
        let canonical_parent = to_parent
            .canonicalize()
            .map_err(|e| format!("Failed to resolve destination parent: {e}"))?;
        if !canonical_parent.starts_with(&home) {
            return Err("Destination must be within the user's home directory".into());
        }

        std::fs::rename(&from_path, &to_path).map_err(|e| format!("Failed to rename: {e}"))
    })
    .await
}

// ---------------------------------------------------------------------------
// Build-artifact scan (capability-gated: fs:scan)
//
// Wired to IPC (`scan_build_artifacts` in the invoke_handler, `lib.rs`) and to
// HTTP parity (`/api/plugins/{id}/build-artifacts/scan`, `plugin_routes.rs`).
// The `fs:scan`/`fs:delete` capability strings are registered in
// `KNOWN_CAPABILITIES` (`plugins.rs`); the PluginHost exposes `scanBuildArtifacts`
// / `deleteBuildArtifact`.
// ---------------------------------------------------------------------------

/// How a name-matched artifact dir proves it is claimed by its toolchain.
/// Generic names (`target`, `bin`, `build`, `vendor`, …) are ambiguous — they
/// only count as artifacts when a marker file sits beside them, otherwise a Go
/// sysroot `bin`, an Xcode `PIFCache/target`, or Rust `src/bin` SOURCES would
/// be reported (and deletable) as artifacts.
#[derive(Debug)]
enum ArtifactMarker {
    /// The dir name alone is unambiguous (e.g. `node_modules`, `__pycache__`).
    Always,
    /// Any of these exact filenames must exist beside the dir.
    AnyFile(&'static [&'static str]),
    /// A .NET project/solution file (`DOTNET_PROJECT_EXTS`) must exist beside it.
    DotnetProject,
}

/// One scanner/delete-guard rule: a dir name (exact, or prefix for IDE-suffixed
/// dirs like `cmake-build-debug`), the kind it maps to, and the marker required
/// beside it. First matching rule wins (`matching_rule`), so e.g. `target`
/// resolves to `rust` or `maven` depending on which project file is present.
///
/// `trim` lists the sub-paths that are pure regenerable intermediates — see
/// `TRIM PATTERNS` below. Empty means the toolchain has no safe subset and only
/// a full clean is offered.
#[derive(Debug)]
struct ArtifactRule {
    name: &'static str,
    prefix: bool,
    kind: &'static str,
    marker: ArtifactMarker,
    trim: &'static [&'static str],
}

const fn rule(name: &'static str, kind: &'static str, marker: ArtifactMarker) -> ArtifactRule {
    ArtifactRule {
        name,
        prefix: false,
        kind,
        marker,
        trim: &[],
    }
}

/// Same as `rule`, plus the trim patterns for toolchains with a known
/// intermediate/output split.
const fn trimmable_rule(
    name: &'static str,
    kind: &'static str,
    marker: ArtifactMarker,
    trim: &'static [&'static str],
) -> ArtifactRule {
    ArtifactRule {
        name,
        prefix: false,
        kind,
        marker,
        trim,
    }
}

// --- TRIM PATTERNS ---------------------------------------------------------
//
// A trim pattern is a `/`-separated list of segments, relative to the matched
// artifact dir. Segments are matched against real directory names one level at
// a time (`expand_trim_pattern`), so the separator never reaches the OS — the
// same constants work on macOS, Linux and Windows.
//
// Segment forms: `deps` (literal) · `*` (any one dir) · `*.build` (suffix).
//
// The bar for inclusion: deleting it must cost only local CPU to rebuild. A dir
// that needs the network to restore (cargo registry, SwiftPM `checkouts`,
// `repositories`) stays out — trimming must never turn into "now you're offline
// and stuck".

/// Rust `target/`. `<profile>/` holds the linked executables at its root; every
/// intermediate lives in one of these four. The `*/*/…` variants cover
/// cross-compilation, where the profile dir sits under a target triple.
/// Measured on 5 real repos: 98.2–99.8% of `target/` is these four dirs.
const RUST_TRIM: &[&str] = &[
    "*/deps",
    "*/build",
    "*/incremental",
    "*/.fingerprint",
    "*/*/deps",
    "*/*/build",
    "*/*/incremental",
    "*/*/.fingerprint",
];

/// Swift `.build/`. `index-build/` is SourceKit's separate index tree and never
/// holds the product binary; `ModuleCache`/`index` are caches; `<Module>.build/`
/// holds per-module object files. `checkouts/`/`repositories/` are dependency
/// SOURCES (network to restore) and `Modules/` holds the `.swiftmodule`
/// interfaces — all four are deliberately kept.
const SWIFT_TRIM: &[&str] = &["index-build", "*/*/ModuleCache", "*/*/index", "*/*/*.build"];

/// Maven `target/`. The packaged `*.jar`/`*.war` sits at the root and survives.
const MAVEN_TRIM: &[&str] = &[
    "classes",
    "test-classes",
    "generated-sources",
    "generated-test-sources",
    "generated-test-resources",
    "maven-status",
    "maven-archiver",
    "surefire-reports",
    "failsafe-reports",
];

/// Gradle `build/`. Keeps `libs/`, `outputs/`, `distributions/`, `install/` —
/// the four dirs Gradle publishes final artifacts into.
const GRADLE_TRIM: &[&str] = &[
    "classes",
    "tmp",
    "kotlin",
    "intermediates",
    "generated",
    "reports",
    "test-results",
    "jacoco",
];

// DEFERRED (2026-08-16) — Python `.venv` trim (`**/__pycache__`). Measured at
// only 115 MB of a 1341 MB venv (8.6%) and it needs a recursive `**` segment
// that no other toolchain wants. Revisit if a `**` form is needed anyway.

/// File extensions marking a directory as a .NET project/solution root — a
/// sibling `bin`/`obj` is then a build-artifact dir.
const DOTNET_PROJECT_EXTS: &[&str] = &["csproj", "fsproj", "vbproj", "sln", "slnx"];

const GRADLE_MARKERS: &[&str] = &[
    "build.gradle",
    "settings.gradle",
    "build.gradle.kts",
    "settings.gradle.kts",
];

/// All scanner/delete-guard rules. Kinds MUST stay in sync with the
/// build-cleaner plugin's `ALL_KINDS`/`KIND_LABELS` (`plugins/build-cleaner/main.js`).
const ARTIFACT_RULES: &[ArtifactRule] = &[
    trimmable_rule(
        "target",
        "rust",
        ArtifactMarker::AnyFile(&["Cargo.toml"]),
        RUST_TRIM,
    ),
    trimmable_rule(
        "target",
        "maven",
        ArtifactMarker::AnyFile(&["pom.xml"]),
        MAVEN_TRIM,
    ),
    rule("node_modules", "node", ArtifactMarker::Always),
    rule(".next", "jscache", ArtifactMarker::Always),
    rule(".nuxt", "jscache", ArtifactMarker::Always),
    rule(".turbo", "jscache", ArtifactMarker::Always),
    rule(".parcel-cache", "jscache", ArtifactMarker::Always),
    rule(".svelte-kit", "jscache", ArtifactMarker::Always),
    rule(".astro", "jscache", ArtifactMarker::Always),
    rule(".venv", "python", ArtifactMarker::Always),
    rule("__pycache__", "python", ArtifactMarker::Always),
    rule(".pytest_cache", "python", ArtifactMarker::Always),
    rule(".mypy_cache", "python", ArtifactMarker::Always),
    rule(".ruff_cache", "python", ArtifactMarker::Always),
    rule(".tox", "python", ArtifactMarker::Always),
    rule("obj", "dotnet", ArtifactMarker::DotnetProject),
    rule("bin", "dotnet", ArtifactMarker::DotnetProject),
    rule(".gradle", "gradle", ArtifactMarker::Always),
    trimmable_rule(
        "build",
        "gradle",
        ArtifactMarker::AnyFile(GRADLE_MARKERS),
        GRADLE_TRIM,
    ),
    rule(
        "build",
        "cmake",
        ArtifactMarker::AnyFile(&["CMakeLists.txt"]),
    ),
    rule(
        "build",
        "flutter",
        ArtifactMarker::AnyFile(&["pubspec.yaml"]),
    ),
    ArtifactRule {
        name: "cmake-build-",
        prefix: true,
        kind: "cmake",
        marker: ArtifactMarker::AnyFile(&["CMakeLists.txt"]),
        trim: &[],
    },
    trimmable_rule(
        ".build",
        "swift",
        ArtifactMarker::AnyFile(&["Package.swift"]),
        SWIFT_TRIM,
    ),
    rule("Pods", "swift", ArtifactMarker::AnyFile(&["Podfile"])),
    rule(".dart_tool", "flutter", ArtifactMarker::Always),
    rule(".terraform", "terraform", ArtifactMarker::Always),
    rule("_build", "elixir", ArtifactMarker::Always),
    rule("zig-out", "zig", ArtifactMarker::Always),
    rule(".zig-cache", "zig", ArtifactMarker::Always),
    rule(".stack-work", "haskell", ArtifactMarker::Always),
    rule("dist-newstyle", "haskell", ArtifactMarker::Always),
    rule("vendor", "php", ArtifactMarker::AnyFile(&["composer.json"])),
];

impl ArtifactRule {
    fn matches_name(&self, name: &str) -> bool {
        if self.prefix {
            name.starts_with(self.name)
        } else {
            name == self.name
        }
    }
}

impl ArtifactMarker {
    /// `files` are the names of the non-directory entries beside the candidate dir.
    fn satisfied(&self, files: &[String]) -> bool {
        match self {
            ArtifactMarker::Always => true,
            ArtifactMarker::AnyFile(names) => files.iter().any(|f| names.contains(&f.as_str())),
            ArtifactMarker::DotnetProject => files.iter().any(|f| {
                std::path::Path::new(f)
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| DOTNET_PROJECT_EXTS.contains(&e))
            }),
        }
    }
}

/// Names of the non-directory entries (files and symlinks — a symlinked
/// `Cargo.toml` still marks a workspace) directly inside `dir`.
fn file_names(dir: &std::path::Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.flatten()
        .filter(|e| e.file_type().map(|t| !t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect()
}

/// Whether any rule matches this dir name (cheap pre-check before reading the
/// parent's file list).
fn name_matches_any_rule(name: &str) -> bool {
    ARTIFACT_RULES.iter().any(|r| r.matches_name(name))
}

/// Resolve a name-matched dir to the rule that claims it, given the file names
/// beside it. `None` means no rule's marker is satisfied — not an artifact.
fn matching_rule(name: &str, files: &[String]) -> Option<&'static ArtifactRule> {
    ARTIFACT_RULES
        .iter()
        .find(|r| r.matches_name(name) && r.marker.satisfied(files))
}

/// Whether one trim-pattern segment matches a real directory name.
/// `*` matches any name; `*<suffix>` matches by suffix (Swift `<Module>.build`);
/// anything else is an exact match. A bare `*` never matches `.`/`..` because
/// those are not yielded by `read_dir`.
fn trim_segment_matches(segment: &str, name: &str) -> bool {
    match segment.strip_prefix('*') {
        Some("") => true,
        Some(suffix) => name.ends_with(suffix) && name.len() > suffix.len(),
        None => segment == name,
    }
}

/// Expand one trim pattern under `root` into the real directories it names.
///
/// Descends one segment at a time through `read_dir`, so the pattern's `/`
/// separators never reach the OS and the result is correct on Windows too.
/// Symlinked dirs are skipped at every level — a `deps -> /` symlink can never
/// widen the result — and only directories are ever yielded, so a *file* named
/// `deps` is not a trim target.
fn expand_trim_pattern(root: &std::path::Path, pattern: &str) -> Vec<PathBuf> {
    let mut current = vec![root.to_path_buf()];
    for segment in pattern.split('/') {
        let mut next = Vec::new();
        for dir in &current {
            let Ok(rd) = std::fs::read_dir(dir) else {
                continue;
            };
            for e in rd.flatten() {
                let Ok(ft) = e.file_type() else { continue };
                if !ft.is_dir() || ft.is_symlink() {
                    continue;
                }
                if trim_segment_matches(segment, &e.file_name().to_string_lossy()) {
                    next.push(e.path());
                }
            }
        }
        if next.is_empty() {
            return Vec::new();
        }
        current = next;
    }
    current
}

/// Every existing trim target inside `dir` for `rule`, deduplicated so a nested
/// match (e.g. `*/*/build` landing inside an already-matched `*/build`) is not
/// counted or deleted twice. Empty when the rule has no trim patterns.
fn trim_targets(dir: &std::path::Path, rule: &ArtifactRule) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = rule
        .trim
        .iter()
        .flat_map(|p| expand_trim_pattern(dir, p))
        .collect();
    found.sort();
    found.dedup();
    // Drop any target contained in an earlier (shorter) one. Sorted order puts
    // a parent immediately before its descendants.
    let mut out: Vec<PathBuf> = Vec::with_capacity(found.len());
    for p in found {
        if out.last().is_some_and(|kept| p.starts_with(kept)) {
            continue;
        }
        out.push(p);
    }
    out
}

/// Cap on scan-walk recursion into a repo (runaway backstop; real source trees
/// are far shallower). Symlinked dirs are never followed, so cycles are impossible.
const MAX_SCAN_DEPTH: u8 = 8;

/// Cap on size-measurement recursion within a matched artifact dir. Deeper than
/// MAX_SCAN_DEPTH because `node_modules` nests heavily; symlinks are not followed.
const MAX_SIZE_DEPTH: u8 = 64;

/// One matched build-artifact directory: its absolute path, tool kind, total
/// on-disk size, the part of that size held by regenerable intermediates
/// (`trimmable_bytes`, 0 when the toolchain has no safe subset), last-build age
/// (max mtime of direct children, as Unix secs), and the repo root it was found
/// under.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ArtifactEntry {
    pub path: String,
    pub kind: String,
    pub size_bytes: u64,
    pub trimmable_bytes: u64,
    pub last_modified_secs: u64,
    pub repo: String,
}

/// Reuse the expensive all-repo artifact walk across frontend reloads. A short
/// TTL keeps filesystem changes fresh while covering HMR's repeated plugin
/// `onload` calls. Explicit dashboard refreshes bypass ready entries.
const BUILD_ARTIFACT_SCAN_TTL: Duration = Duration::from_secs(30);
const BUILD_ARTIFACT_SCAN_MAX_READY: usize = 8;

enum ArtifactScanCacheEntry {
    Running {
        invalidated: bool,
        /// This walk began *because* something asked for fresh data — a forced
        /// refresh, or a re-run after an invalidation. A forced caller can share
        /// it; it cannot share a walk that started on its own schedule.
        fresh: bool,
    },
    Ready {
        completed_at: Instant,
        last_accessed: Instant,
        result: Vec<ArtifactEntry>,
    },
}

struct ArtifactScanCache {
    entries: parking_lot::Mutex<HashMap<Vec<PathBuf>, ArtifactScanCacheEntry>>,
    changed: parking_lot::Condvar,
    #[cfg(test)]
    waiter_count: std::sync::atomic::AtomicUsize,
}

impl ArtifactScanCache {
    fn new() -> Self {
        Self {
            entries: parking_lot::Mutex::new(HashMap::new()),
            changed: parking_lot::Condvar::new(),
            #[cfg(test)]
            waiter_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn get_or_scan<F>(
        &self,
        key: Vec<PathBuf>,
        ttl: Duration,
        force_refresh: bool,
        scan: F,
    ) -> Vec<ArtifactEntry>
    where
        F: Fn() -> Vec<ArtifactEntry>,
    {
        let mut waited_for_running = false;

        loop {
            let mut entries = self.entries.lock();
            if !waited_for_running {
                Self::prune_expired(&mut entries, ttl);
            }
            match entries.get_mut(&key) {
                Some(ArtifactScanCacheEntry::Running { invalidated, fresh }) => {
                    // A forced rescan asks for data that is fresh as of the
                    // request. A walk that started on the TTL's schedule does not
                    // answer that — it may have passed the directory before the
                    // artifact appeared — so it is invalidated: its leader
                    // discards that pass and runs again, and this wait ends on
                    // the re-run rather than on a second concurrent walk. A walk
                    // that is already a refresh is shared as it is; two clicks of
                    // Rescan must not cost two walks.
                    if force_refresh && !*fresh {
                        *invalidated = true;
                        *fresh = true;
                    }
                    waited_for_running = true;
                    #[cfg(test)]
                    self.waiter_count
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    self.changed.wait(&mut entries);
                    #[cfg(test)]
                    self.waiter_count
                        .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    continue;
                }
                Some(ArtifactScanCacheEntry::Ready {
                    completed_at,
                    last_accessed,
                    result,
                }) if waited_for_running || (!force_refresh && completed_at.elapsed() < ttl) => {
                    *last_accessed = Instant::now();
                    return result.clone();
                }
                _ => {
                    entries.insert(
                        key.clone(),
                        ArtifactScanCacheEntry::Running {
                            invalidated: false,
                            fresh: force_refresh,
                        },
                    );
                    break;
                }
            }
        }

        loop {
            // A panic must not leave the key permanently stuck in Running.
            // Preserve it so spawn_blocking still reports its normal JoinError.
            let scanned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&scan));
            let mut entries = self.entries.lock();
            match scanned {
                Ok(result) => {
                    if matches!(
                        entries.get(&key),
                        Some(ArtifactScanCacheEntry::Running {
                            invalidated: true,
                            ..
                        })
                    ) {
                        // The re-run starts now, after whatever invalidated it.
                        entries.insert(
                            key.clone(),
                            ArtifactScanCacheEntry::Running {
                                invalidated: false,
                                fresh: true,
                            },
                        );
                        drop(entries);
                        continue;
                    }

                    let now = Instant::now();
                    entries.insert(
                        key.clone(),
                        ArtifactScanCacheEntry::Ready {
                            completed_at: now,
                            last_accessed: now,
                            result: result.clone(),
                        },
                    );
                    Self::enforce_ready_bound(&mut entries);
                    self.changed.notify_all();
                    return result;
                }
                Err(payload) => {
                    entries.remove(&key);
                    self.changed.notify_all();
                    drop(entries);
                    std::panic::resume_unwind(payload);
                }
            }
        }
    }

    fn prune_expired(entries: &mut HashMap<Vec<PathBuf>, ArtifactScanCacheEntry>, ttl: Duration) {
        entries.retain(|_, entry| match entry {
            ArtifactScanCacheEntry::Running { .. } => true,
            ArtifactScanCacheEntry::Ready { completed_at, .. } => completed_at.elapsed() < ttl,
        });
    }

    fn enforce_ready_bound(entries: &mut HashMap<Vec<PathBuf>, ArtifactScanCacheEntry>) {
        while entries
            .values()
            .filter(|entry| matches!(entry, ArtifactScanCacheEntry::Ready { .. }))
            .count()
            > BUILD_ARTIFACT_SCAN_MAX_READY
        {
            let oldest = entries
                .iter()
                .filter_map(|(key, entry)| match entry {
                    ArtifactScanCacheEntry::Ready { last_accessed, .. } => {
                        Some((key.clone(), *last_accessed))
                    }
                    ArtifactScanCacheEntry::Running { .. } => None,
                })
                .min_by_key(|(_, last_accessed)| *last_accessed)
                .map(|(key, _)| key);
            if let Some(key) = oldest {
                entries.remove(&key);
            } else {
                break;
            }
        }
    }

    fn invalidate_path(&self, path: &std::path::Path) {
        let mut entries = self.entries.lock();
        entries.retain(|roots, entry| {
            if !roots.iter().any(|root| path.starts_with(root)) {
                return true;
            }
            match entry {
                ArtifactScanCacheEntry::Running { invalidated, .. } => {
                    *invalidated = true;
                    true
                }
                ArtifactScanCacheEntry::Ready { .. } => false,
            }
        });
    }

    #[cfg(test)]
    fn wait_until_waiter_is_blocked(&self) {
        let deadline = Instant::now() + Duration::from_secs(1);
        while self.waiter_count.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            assert!(Instant::now() < deadline, "expected a blocked cache waiter");
            std::thread::yield_now();
        }
    }
}

fn artifact_scan_cache() -> &'static ArtifactScanCache {
    static CACHE: OnceLock<ArtifactScanCache> = OnceLock::new();
    CACHE.get_or_init(ArtifactScanCache::new)
}

/// Recursively sum sizes of regular files under `dir`, splitting the total into
/// (everything, the part under a trim target). Does not follow symlinks (uses
/// `DirEntry` file types / non-traversing metadata), so it can't escape the tree
/// or loop. Per-dir read errors are non-fatal — a macOS TCC-protected subdir is
/// skipped, not counted, and never aborts the sum.
///
/// Both numbers come from ONE walk on purpose: on a Rust `target/` the trim
/// subtrees are ~99% of the tree, so measuring them separately would very nearly
/// double an already multi-second scan. `in_trim` latches once the walk enters a
/// target, which also removes the per-entry target lookup from the hot path.
fn measure_sizes(
    dir: &std::path::Path,
    depth: u8,
    targets: &[PathBuf],
    in_trim: bool,
) -> (u64, u64) {
    if depth == 0 {
        return (0, 0);
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return (0, 0);
    };
    let mut total = 0u64;
    let mut trimmable = 0u64;
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            let p = e.path();
            let inside = in_trim || targets.contains(&p);
            let (t, m) = measure_sizes(&p, depth - 1, targets, inside);
            total += t;
            trimmable += m;
        } else if ft.is_file()
            && let Ok(m) = e.metadata()
        {
            total += m.len();
            if in_trim {
                trimmable += m.len();
            }
        }
    }
    (total, trimmable)
}

/// Max mtime (Unix secs) among the direct children of `dir`. Dir mtime is
/// unreliable as a "last build" signal; the newest direct child is cheap and
/// closer to the truth. Returns 0 if the dir is unreadable or empty.
fn max_child_mtime_secs(dir: &std::path::Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut max = 0u64;
    for e in rd.flatten() {
        if let Ok(m) = e.metadata()
            && let Ok(mt) = m.modified()
            && let Ok(d) = mt.duration_since(std::time::UNIX_EPOCH)
        {
            max = max.max(d.as_secs());
        }
    }
    max
}

/// Measure a matched artifact dir into an `ArtifactEntry` — total size and the
/// trimmable share, in a single walk.
fn measure(dir: &std::path::Path, rule: &ArtifactRule, repo: &str) -> ArtifactEntry {
    let targets = trim_targets(dir, rule);
    let (size_bytes, trimmable_bytes) = measure_sizes(dir, MAX_SIZE_DEPTH, &targets, false);
    ArtifactEntry {
        path: dir.to_string_lossy().to_string(),
        kind: rule.kind.to_string(),
        size_bytes,
        trimmable_bytes,
        last_modified_secs: max_child_mtime_secs(dir),
        repo: repo.to_string(),
    }
}

/// Recursively find build-artifact directories under `dir`. On a match, the dir
/// is summed whole and NOT descended into (stop-at-match), so a `node_modules`
/// nested inside another is folded into the outer entry — never double counted.
/// Ambiguously-named matches (`target`, `bin`, `build`, …) require a marker
/// file beside them (`matching_rule`); unclaimed ones are walked like any
/// other dir. Skips `.git` and symlinked dirs; per-dir read errors are non-fatal.
fn walk_artifacts(dir: &std::path::Path, repo: &str, depth: u8, out: &mut Vec<ArtifactEntry>) {
    if depth == 0 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    // Sibling file names, computed lazily once per dir — only when a rule name matches.
    let mut files: Option<Vec<String>> = None;
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if !ft.is_dir() || ft.is_symlink() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        if name == ".git" {
            continue;
        }
        let p = e.path();
        if name_matches_any_rule(&name) {
            let files = files.get_or_insert_with(|| file_names(dir));
            if let Some(rule) = matching_rule(&name, files) {
                out.push(measure(&p, rule, repo));
                continue;
            }
        }
        walk_artifacts(&p, repo, depth - 1, out);
    }
}

/// Load the backend's actual registered repository roots — the `repos` map
/// key in `repositories.json` (schema owned by the frontend; see
/// `src/stores/repositories.ts`), canonicalized. Entries that fail to
/// canonicalize (moved, unmounted, never existed) are dropped, not fatal —
/// matches the silent-drop behavior for invalid caller-supplied roots
/// elsewhere in this module.
fn registered_repo_roots() -> Vec<PathBuf> {
    crate::config::registered_repo_paths()
        .into_iter()
        .filter_map(|p| PathBuf::from(p).canonicalize().ok())
        .collect()
}

/// Canonicalize, authorize, sort, and deduplicate the requested roots. The
/// resulting exact path set is the scan-cache key, so argument order and
/// duplicates share work while a changed registered path set does not.
fn normalized_scan_roots(repo_paths: &[String]) -> Vec<PathBuf> {
    let registered = registered_repo_roots();
    normalize_root_set(
        repo_paths
            .iter()
            .filter_map(|raw| validate_within_home(raw).ok())
            .filter(|root| is_within_registered_repo(root, &registered))
            .collect(),
    )
}

fn normalize_root_set(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots.sort();
    roots.dedup();
    roots
}

/// Whether `path` is itself a genuinely registered repo root, or nested
/// inside one. Guards `fs:scan`/`fs:delete` against a plugin widening its
/// containment by passing an arbitrary `repo_paths` entry (e.g. `$HOME`)
/// that was never actually registered with the app — `repo_paths` is
/// caller-supplied and must not be trusted as a containment root on its own.
fn is_within_registered_repo(path: &std::path::Path, registered: &[PathBuf]) -> bool {
    registered.iter().any(|r| path.starts_with(r))
}

/// Scan registered repo roots for build-artifact directories. Read-only; gated
/// by `fs:scan`. Each repo path is `validate_within_home`'d, then intersected
/// with the backend's actual registered repositories (`registered_repo_roots`)
/// — a caller-supplied root that isn't backed by a real registered repo is
/// skipped, not fatal, same as one that fails `$HOME` validation.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn scan_build_artifacts(
    repo_paths: Vec<String>,
    plugin_id: String,
    force_refresh: Option<bool>,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<Vec<ArtifactEntry>, String> {
    scan_build_artifacts_impl(
        &state,
        repo_paths,
        plugin_id,
        force_refresh.unwrap_or(false),
    )
    .await
}

pub(crate) async fn scan_build_artifacts_impl(
    state: &std::sync::Arc<crate::AppState>,
    repo_paths: Vec<String>,
    plugin_id: String,
    force_refresh: bool,
) -> Result<Vec<ArtifactEntry>, String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "fs:scan")?;
    scan_build_artifacts_inner(repo_paths, force_refresh).await
}

async fn scan_build_artifacts_inner(
    repo_paths: Vec<String>,
    force_refresh: bool,
) -> Result<Vec<ArtifactEntry>, String> {
    spawn_blocking_fs(move || {
        let roots = normalized_scan_roots(&repo_paths);
        let key = roots.clone();
        Ok(artifact_scan_cache().get_or_scan(
            key,
            BUILD_ARTIFACT_SCAN_TTL,
            force_refresh,
            move || {
                let mut out = Vec::new();
                for root in &roots {
                    let repo = root.to_string_lossy().to_string();
                    walk_artifacts(root, &repo, MAX_SCAN_DEPTH, &mut out);
                }
                out
            },
        ))
    })
    .await
}

// ---------------------------------------------------------------------------
// Build-artifact delete (capability-gated: fs:delete)
//
// Wired to IPC (`delete_build_artifact`) and HTTP parity
// (`/api/plugins/{id}/build-artifacts/delete`). Destructive; the guard below is
// the sharp edge.
// ---------------------------------------------------------------------------

/// Guard for a destructive `remove_dir_all`. ALL conditions must hold, or the
/// path is refused. Canonicalizes first so a symlink pointing outside a repo
/// resolves to its real location and fails containment:
///   1. basename matches a known artifact rule name (`ARTIFACT_RULES`);
///   2. strictly inside one of `repo_roots` (`starts_with` a root AND not
///      equal to it — never delete a repo root);
///   3. for ambiguous names (`target`, `bin`, `build`, …), a marker file sits
///      beside the dir (`matching_rule`) — refuses e.g. Rust `src/bin` sources.
///
/// `repo_roots` is caller-supplied (`repo_paths`) but is NOT trusted as-is:
/// the caller (`delete_build_artifact_inner`) intersects it with the backend's
/// actual registered repositories (`registered_repo_roots`) before it reaches
/// here, so a plugin cannot widen containment by passing an arbitrary root
/// (e.g. `$HOME`) that was never really registered. `$HOME` scoping is
/// enforced separately by `validate_within_home` on both the target and each
/// repo root before this runs (defense in depth).
///
/// Returns the rule that claimed the dir, so a caller that needs more than a
/// yes/no (`trim_build_artifact_inner`, which needs the trim patterns) does not
/// have to re-resolve it and risk resolving a *different* rule than the one the
/// guard approved.
fn assert_deletable(
    path: &std::path::Path,
    repo_roots: &[PathBuf],
) -> Result<&'static ArtifactRule, String> {
    let c = path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path: {e}"))?;

    let name = c.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if !name_matches_any_rule(name) {
        return Err(format!(
            "Refusing to delete: '{name}' is not a build-artifact dir"
        ));
    }

    let inside = repo_roots.iter().any(|r| c.starts_with(r) && c != *r);
    if !inside {
        return Err("Refusing to delete: path is outside all registered repos".into());
    }

    let files = c.parent().map(file_names).unwrap_or_default();
    matching_rule(name, &files).ok_or_else(|| {
        format!("Refusing to delete: '{name}' has no matching project file beside it")
    })
}

/// Delete a build-artifact directory. Destructive; gated by `fs:delete`. The
/// target and every repo root are `validate_within_home`'d and intersected
/// with the backend's actual registered repositories, then `assert_deletable`
/// enforces the artifact-name + strict-containment guard before `remove_dir_all`.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn delete_build_artifact(
    path: String,
    repo_paths: Vec<String>,
    plugin_id: String,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<(), String> {
    delete_build_artifact_impl(&state, path, repo_paths, plugin_id).await
}

pub(crate) async fn delete_build_artifact_impl(
    state: &std::sync::Arc<crate::AppState>,
    path: String,
    repo_paths: Vec<String>,
    plugin_id: String,
) -> Result<(), String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "fs:delete")?;
    delete_build_artifact_inner(path, repo_paths).await
}

/// Shared authorization for both destructive paths. Returns the canonical
/// artifact dir and the rule that claims it, or the refusal reason.
fn authorize_artifact(
    path: &str,
    repo_paths: &[String],
) -> Result<(PathBuf, &'static ArtifactRule), String> {
    // $HOME scope + canonicalization of the target.
    let canonical = validate_within_home(path)?;

    // Canonicalize each caller-supplied repo root (resolves symlinks so
    // containment is compared apples-to-apples), then keep only those that
    // are actually backed by a registered repository (`registered_repo_roots`)
    // — a plugin cannot widen containment by passing an arbitrary root
    // (e.g. `$HOME`) that was never really registered with the app. Roots
    // that fail validation or aren't registered are dropped, not fatal.
    let registered = registered_repo_roots();
    let mut roots = Vec::new();
    for r in repo_paths {
        if let Ok(rc) = validate_within_home(r)
            && is_within_registered_repo(&rc, &registered)
        {
            roots.push(rc);
        }
    }

    let rule = assert_deletable(&canonical, &roots)?;
    Ok((canonical, rule))
}

async fn delete_build_artifact_inner(path: String, repo_paths: Vec<String>) -> Result<(), String> {
    spawn_blocking_fs(move || {
        let (canonical, _rule) = authorize_artifact(&path, &repo_paths)?;

        std::fs::remove_dir_all(&canonical).map_err(|e| format!("Failed to remove: {e}"))?;
        artifact_scan_cache().invalidate_path(&canonical);
        Ok(())
    })
    .await
}

// ---------------------------------------------------------------------------
// Build-artifact trim (capability-gated: fs:delete)
//
// The non-destructive-to-outputs half of the cleaner: removes only the
// regenerable intermediates named by the matched rule's trim patterns, leaving
// the linked executables in place. This exists because a full clean of a Rust
// `target/` also deletes the binaries the user is currently RUNNING — while
// reclaiming, on measured repos, only ~1% more disk than a trim.
// ---------------------------------------------------------------------------

/// Trim a build-artifact directory: remove only its intermediates. Destructive,
/// gated by `fs:delete` (same blast-radius class as delete, on a subset).
///
/// Authorization is `delete`'s, unchanged — the target must pass `$HOME` scope,
/// registered-repo containment and the artifact-rule marker check. On top of
/// that, only paths produced by expanding the *matched rule's own* trim patterns
/// are removed, and each is re-verified to be strictly inside the artifact dir
/// before `remove_dir_all`, so a future pattern-table mistake cannot escape.
///
/// Every target is attempted even if one fails (a Windows read-only file must
/// not strand the other 4 GB); the failures are reported together.
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn trim_build_artifact(
    path: String,
    repo_paths: Vec<String>,
    plugin_id: String,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<u64, String> {
    trim_build_artifact_impl(&state, path, repo_paths, plugin_id).await
}

pub(crate) async fn trim_build_artifact_impl(
    state: &std::sync::Arc<crate::AppState>,
    path: String,
    repo_paths: Vec<String>,
    plugin_id: String,
) -> Result<u64, String> {
    crate::plugins::check_plugin_capability(state, &plugin_id, "fs:delete")?;
    trim_build_artifact_inner(path, repo_paths).await
}

/// Returns the bytes actually reclaimed. The caller patches its cached totals
/// with this instead of the `trimmable_bytes` of the last scan: a build between
/// the scan and the trim moves that number, and subtracting the stale estimate
/// publishes a total no measurement supports.
async fn trim_build_artifact_inner(path: String, repo_paths: Vec<String>) -> Result<u64, String> {
    spawn_blocking_fs(move || {
        let (canonical, rule) = authorize_artifact(&path, &repo_paths)?;

        if rule.trim.is_empty() {
            return Err(format!(
                "Refusing to trim: no intermediates are separable for kind '{}'",
                rule.kind
            ));
        }

        let targets = trim_targets(&canonical, rule);
        if targets.is_empty() {
            // Already trimmed, or built with a layout the patterns don't cover.
            // Not an error — there is simply nothing to reclaim.
            return Ok(0);
        }

        let mut reclaimed = 0u64;
        let mut failures = Vec::new();
        for target in &targets {
            // Belt and braces: `expand_trim_pattern` only ever descends from
            // `canonical` and never follows symlinks, so this cannot fail today.
            // It is here so a bad pattern (a stray `..`) is refused rather than
            // executed.
            if !target.starts_with(&canonical) || target == &canonical {
                failures.push(format!("{}: outside the artifact dir", target.display()));
                continue;
            }
            // Measured before the removal, and only counted once the removal
            // succeeded — a partial failure must not report bytes still on disk.
            let (size, _) = measure_sizes(target, MAX_SIZE_DEPTH, &[], false);
            if let Err(e) = std::fs::remove_dir_all(target) {
                failures.push(format!("{}: {e}", target.display()));
            } else {
                reclaimed += size;
            }
        }

        artifact_scan_cache().invalidate_path(&canonical);

        if failures.is_empty() {
            Ok(reclaimed)
        } else {
            Err(format!("Failed to trim {}: {}", path, failures.join("; ")))
        }
    })
    .await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{dir_outside_home, slashed};
    use std::path::Path;

    #[test]
    fn validate_rejects_empty_path() {
        assert!(validate_within_home("").is_err());
    }

    /// A directory under $HOME, because every plugin read is confined to it.
    fn temp_dir_in_home() -> tempfile::TempDir {
        tempfile::tempdir_in(dirs::home_dir().unwrap()).unwrap()
    }

    #[test]
    fn batch_read_returns_contents_in_request_order() {
        let _guard = FS_TEST_LOCK.lock().unwrap();
        let dir = temp_dir_in_home();
        let paths: Vec<String> = (0..3)
            .map(|i| {
                let p = dir.path().join(format!("{i}.md"));
                std::fs::write(&p, format!("body {i}")).unwrap();
                p.to_str().unwrap().to_string()
            })
            .collect();

        let read = read_files_within_home(paths).unwrap();

        assert_eq!(
            read,
            vec![
                Some("body 0".to_string()),
                Some("body 1".to_string()),
                Some("body 2".to_string()),
            ]
        );
    }

    #[test]
    fn batch_read_reports_unreadable_entries_as_none() {
        let _guard = FS_TEST_LOCK.lock().unwrap();
        let dir = temp_dir_in_home();
        let good = dir.path().join("good.md");
        std::fs::write(&good, "here").unwrap();

        // A missing file, a directory, and a path outside $HOME must each cost
        // the caller one None — not the whole batch.
        let read = read_files_within_home(vec![
            good.to_str().unwrap().to_string(),
            dir.path().join("missing.md").to_str().unwrap().to_string(),
            dir.path().to_str().unwrap().to_string(),
            "/etc/hosts".to_string(),
        ])
        .unwrap();

        assert_eq!(read, vec![Some("here".to_string()), None, None, None]);
    }

    #[test]
    fn batch_read_rejects_an_oversized_request() {
        let paths = vec!["/unused".to_string(); MAX_BATCH_FILES + 1];
        assert!(read_files_within_home(paths).is_err());
    }

    #[test]
    fn batch_read_accepts_exactly_the_limit() {
        // The boundary, not just past it: an off-by-one here would refuse a
        // request the plugin was told it may make.
        let paths = vec!["/unused".to_string(); MAX_BATCH_FILES];
        assert_eq!(
            read_files_within_home(paths).unwrap().len(),
            MAX_BATCH_FILES
        );
    }

    #[test]
    fn batch_read_still_enforces_the_per_file_cap() {
        let _guard = FS_TEST_LOCK.lock().unwrap();
        let dir = temp_dir_in_home();
        let big = dir.path().join("big.md");
        std::fs::write(&big, vec![b'x'; MAX_FILE_SIZE as usize + 1]).unwrap();

        // A generous budget must not let a single file past the 10 MB cap the
        // one-file read enforces — the two limits compose, they do not replace
        // each other.
        let read =
            read_files_within_budget(vec![big.to_str().unwrap().to_string()], u64::MAX).unwrap();

        assert_eq!(read, vec![None]);
    }

    #[test]
    fn batch_read_stops_spending_at_the_byte_budget() {
        let _guard = FS_TEST_LOCK.lock().unwrap();
        let dir = temp_dir_in_home();

        // The per-file cap cannot bound a batch: MAX_BATCH_FILES files just
        // under it would retain three orders of magnitude more than any single
        // read is allowed to. A budget is spent across the whole request.
        let write = |name: &str, body: &str| {
            let p = dir.path().join(name);
            std::fs::write(&p, body).unwrap();
            p.to_str().unwrap().to_string()
        };
        let paths = vec![
            write("a.md", "aaaaaa"), // 6 bytes — fits
            write("b.md", "bbbbbb"), // 6 bytes — does not fit in the 4 left
            write("c.md", "c"),      // 1 byte  — still fits, so one big file
                                     //           does not poison the rest
        ];

        let read = read_files_within_budget(paths, 10).unwrap();

        assert_eq!(
            read,
            vec![Some("aaaaaa".to_string()), None, Some("c".to_string())]
        );
    }

    #[test]
    fn binary_read_limit_preserves_default_and_clamps_custom_budget() {
        assert_eq!(binary_read_limit(None).unwrap(), MAX_FILE_SIZE);
        assert_eq!(binary_read_limit(Some(42)).unwrap(), 42);
        assert_eq!(
            binary_read_limit(Some(MAX_BINARY_FILE_SIZE + 1)).unwrap(),
            MAX_BINARY_FILE_SIZE
        );
    }

    #[test]
    fn binary_read_limit_rejects_zero() {
        assert_eq!(
            binary_read_limit(Some(0)).unwrap_err(),
            "maxBytes must be greater than zero"
        );
    }

    #[test]
    fn binary_read_uses_the_requested_budget() {
        let _guard = FS_TEST_LOCK.lock().unwrap();
        let dir = temp_dir_in_home();
        let file = dir.path().join("database.sqlite");
        std::fs::write(&file, b"SQLite format 3\0payload").unwrap();

        let error = read_binary_base64_capped(file.to_str().unwrap(), 8).unwrap_err();

        assert!(error.contains("File exceeds maximum size"), "got: {error}");
        assert!(read_binary_base64_capped(file.to_str().unwrap(), 64).is_ok());
    }

    #[test]
    fn binary_write_decodes_and_atomically_replaces_the_target() {
        use base64::Engine;

        let tmp = tempfile::tempdir().unwrap();
        let _guard = set_home_dir_override(tmp.path().to_path_buf());
        let target = tmp.path().join("database.sqlite");
        std::fs::write(&target, b"old").unwrap();
        let encoded = base64::engine::general_purpose::STANDARD.encode(b"SQLite format 3\0new");

        write_binary_base64_capped(target.to_str().unwrap(), &encoded, 64).unwrap();

        assert_eq!(std::fs::read(target).unwrap(), b"SQLite format 3\0new");
    }

    #[test]
    fn binary_write_rejects_invalid_or_oversized_base64() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = set_home_dir_override(tmp.path().to_path_buf());
        let target = tmp.path().join("database.sqlite");

        assert!(
            write_binary_base64_capped(target.to_str().unwrap(), "not base64", 64)
                .unwrap_err()
                .contains("Invalid Base64")
        );
        assert!(
            write_binary_base64_capped(target.to_str().unwrap(), "QUJDRA==", 3)
                .unwrap_err()
                .contains("maximum size")
        );
        assert!(!target.exists());
    }

    #[test]
    fn validate_rejects_relative_path() {
        assert!(validate_within_home("relative/path").is_err());
    }

    #[test]
    fn validate_rejects_outside_home() {
        let _guard = FS_TEST_LOCK.lock().unwrap();
        let home = dirs::home_dir().unwrap();
        let outside = dir_outside_home();
        if !outside.starts_with(&home) {
            assert!(validate_within_home(&outside.to_string_lossy()).is_err());
        }
    }

    #[test]
    fn validate_accepts_home_dir() {
        let _guard = FS_TEST_LOCK.lock().unwrap();
        let home = dirs::home_dir().unwrap();
        let result = validate_within_home(home.to_str().unwrap());
        assert!(result.is_ok());
    }

    #[test]
    fn validate_rejects_traversal() {
        let _guard = FS_TEST_LOCK.lock().unwrap();
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
    fn tail_reads_entire_small_file() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = set_home_dir_override(tmp.path().to_path_buf());
        let test_file = tmp.path().join("tail-small.txt");
        std::fs::write(&test_file, "line1\nline2\nline3\n").unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(plugin_read_file_tail_inner(
            test_file.to_string_lossy().to_string(),
            1024,
        ));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "line1\nline2\nline3\n");
    }

    #[test]
    fn tail_reads_last_bytes_skipping_partial_line() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = set_home_dir_override(tmp.path().to_path_buf());
        let test_file = tmp.path().join("tail-large.txt");
        let content = "line1\nline2\nline3\nline4\nline5\n";
        std::fs::write(&test_file, content).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(plugin_read_file_tail_inner(
            test_file.to_string_lossy().to_string(),
            12,
        ));
        assert!(result.is_ok());
        let text = result.unwrap();
        assert_eq!(text, "line5\n");
    }

    #[test]
    fn tail_rejects_non_file() {
        let _guard = FS_TEST_LOCK.lock().unwrap();
        let home = dirs::home_dir().unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(plugin_read_file_tail_inner(
            home.to_string_lossy().to_string(),
            1024,
        ));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a file"));
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

    // -- plugin_write_file tests --

    #[test]
    fn write_file_creates_new_file() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = set_home_dir_override(tmp.path().to_path_buf());
        let test_file = tmp.path().join("write-new.txt");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(plugin_write_file_inner(
            test_file.to_string_lossy().to_string(),
            "hello write".to_string(),
        ));
        let content = std::fs::read_to_string(&test_file).unwrap_or_default();

        assert!(result.is_ok(), "write failed: {:?}", result);
        assert_eq!(content, "hello write");
    }

    #[test]
    fn write_file_overwrites_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = set_home_dir_override(tmp.path().to_path_buf());
        let test_file = tmp.path().join("write-overwrite.txt");
        let _ = std::fs::write(&test_file, "old content");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(plugin_write_file_inner(
            test_file.to_string_lossy().to_string(),
            "new content".to_string(),
        ));
        let content = std::fs::read_to_string(&test_file).unwrap_or_default();

        assert!(result.is_ok());
        assert_eq!(content, "new content");
    }

    #[test]
    fn write_file_rejects_relative_path() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(plugin_write_file_inner(
            "relative/file.txt".to_string(),
            "content".to_string(),
        ));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("absolute"));
    }

    #[test]
    fn write_file_rejects_outside_home() {
        let _guard = FS_TEST_LOCK.lock().unwrap();
        let home = dirs::home_dir().unwrap();
        let outside = dir_outside_home();
        if !outside.starts_with(&home) {
            let target = outside.join(".tuic-test-write-outside.txt");
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(plugin_write_file_inner(
                target.to_string_lossy().to_string(),
                "content".to_string(),
            ));
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("home directory"));
        }
    }

    #[test]
    fn write_file_rejects_directory_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = set_home_dir_override(tmp.path().to_path_buf());
        let test_dir = tmp.path().join("write-dir");
        let _ = std::fs::create_dir_all(&test_dir);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(plugin_write_file_inner(
            test_dir.to_string_lossy().to_string(),
            "content".to_string(),
        ));

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("directory"));
    }

    // -- plugin_rename_path tests --

    #[test]
    fn rename_moves_file() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = set_home_dir_override(tmp.path().to_path_buf());
        let from = tmp.path().join("rename-from.txt");
        let to = tmp.path().join("rename-to.txt");
        let _ = std::fs::write(&from, "rename me");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(plugin_rename_path_inner(
            from.to_string_lossy().to_string(),
            to.to_string_lossy().to_string(),
        ));
        let content = std::fs::read_to_string(&to).unwrap_or_default();
        let from_exists = from.exists();

        assert!(result.is_ok(), "rename failed: {:?}", result);
        assert_eq!(content, "rename me");
        assert!(!from_exists);
    }

    #[test]
    fn rename_rejects_source_outside_home() {
        let _guard = FS_TEST_LOCK.lock().unwrap();
        let home = dirs::home_dir().unwrap();
        if !Path::new("/tmp").starts_with(&home) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(plugin_rename_path_inner(
                "/tmp/.tuic-test-rename.txt".to_string(),
                home.join(".tuic-test-rename-dest.txt")
                    .to_string_lossy()
                    .to_string(),
            ));
            assert!(result.is_err());
        }
    }

    #[test]
    fn rename_rejects_relative_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = set_home_dir_override(tmp.path().to_path_buf());
        let from = tmp.path().join("rename-rel.txt");
        let _ = std::fs::write(&from, "test");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(plugin_rename_path_inner(
            from.to_string_lossy().to_string(),
            "relative/dest.txt".to_string(),
        ));

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("absolute"));
    }

    // -- scan_build_artifacts tests --

    #[test]
    fn scan_build_artifacts_finds_known_dirs_only() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("Cargo.toml"), b"[package]").unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("target/a.o"), vec![0u8; 100]).unwrap();
        std::fs::create_dir_all(root.join("node_modules")).unwrap();
        std::fs::write(root.join("node_modules/pkg.js"), vec![0u8; 50]).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join(".git/HEAD"), vec![0u8; 20]).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), vec![0u8; 30]).unwrap();

        let mut out = Vec::new();
        walk_artifacts(root, "repo", MAX_SCAN_DEPTH, &mut out);

        assert_eq!(
            out.len(),
            2,
            "expected target+node_modules only, got {:?}",
            out.iter().map(|e| &e.path).collect::<Vec<_>>()
        );
        assert!(
            out.iter()
                .any(|e| e.path.ends_with("target") && e.kind == "rust")
        );
        assert!(
            out.iter()
                .any(|e| e.path.ends_with("node_modules") && e.kind == "node")
        );
        assert!(!out.iter().any(|e| e.path.contains(".git")));
    }

    /// The walk is bounded by `MAX_SCAN_DEPTH`, which is what keeps a scan off a
    /// deep source tree's tail. Nothing below the cap is read, matched or measured.
    #[test]
    fn scan_build_artifacts_stops_at_the_depth_cap() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let mut deep = root.to_path_buf();
        for i in 0..MAX_SCAN_DEPTH {
            deep = deep.join(format!("d{i}"));
        }
        // One level shallower than the cap: reachable. One level deeper: not.
        std::fs::create_dir_all(deep.join("node_modules")).unwrap();
        std::fs::write(deep.join("node_modules/deep.js"), vec![0u8; 10]).unwrap();
        let shallow = root.join("a/node_modules");
        std::fs::create_dir_all(&shallow).unwrap();
        std::fs::write(shallow.join("near.js"), vec![0u8; 20]).unwrap();

        let mut out = Vec::new();
        walk_artifacts(root, "repo", MAX_SCAN_DEPTH, &mut out);

        assert_eq!(
            out.len(),
            1,
            "only the in-range match may be reported, got {:?}",
            out.iter().map(|e| &e.path).collect::<Vec<_>>()
        );
        assert_eq!(out[0].size_bytes, 20);
    }

    #[test]
    fn scan_build_artifacts_no_double_count_nested() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let nm = root.join("node_modules");
        std::fs::create_dir_all(nm.join("dep/node_modules")).unwrap();
        std::fs::write(nm.join("outer.js"), vec![0u8; 100]).unwrap();
        std::fs::write(nm.join("dep/node_modules/inner.js"), vec![0u8; 200]).unwrap();

        let mut out = Vec::new();
        walk_artifacts(root, "repo", MAX_SCAN_DEPTH, &mut out);

        assert_eq!(
            out.len(),
            1,
            "nested node_modules must not be a separate entry"
        );
        // Outer dir is summed whole (300 bytes = outer.js + nested inner.js),
        // proving stop-at-match measures the tree but does not re-emit the nested dir.
        assert_eq!(out[0].size_bytes, 300);
    }

    #[test]
    fn scan_build_artifacts_sums_sizes_recursively() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("Cargo.toml"), b"[package]").unwrap();
        let t = root.join("target");
        std::fs::create_dir_all(t.join("debug/deps")).unwrap();
        std::fs::write(t.join("f1"), vec![0u8; 10]).unwrap();
        std::fs::write(t.join("debug/f2"), vec![0u8; 20]).unwrap();
        std::fs::write(t.join("debug/deps/f3"), vec![0u8; 30]).unwrap();

        let mut out = Vec::new();
        walk_artifacts(root, "repo", MAX_SCAN_DEPTH, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].size_bytes, 60);
    }

    #[test]
    fn scan_claims_bin_obj_only_with_dotnet_project() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // .NET project root: bin + obj beside a .csproj → both claimed.
        std::fs::write(root.join("app.csproj"), b"<Project/>").unwrap();
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin/app.dll"), vec![0u8; 10]).unwrap();
        std::fs::create_dir_all(root.join("obj")).unwrap();
        std::fs::write(root.join("obj/app.o"), vec![0u8; 10]).unwrap();
        // Go-style sysroot bin with NO marker → not claimed, but still walked:
        // a real artifact nested inside must surface.
        std::fs::create_dir_all(root.join("sysroot/bin/__pycache__")).unwrap();
        std::fs::write(root.join("sysroot/bin/python3"), vec![0u8; 10]).unwrap();
        std::fs::write(root.join("sysroot/bin/__pycache__/m.pyc"), vec![0u8; 5]).unwrap();

        let mut out = Vec::new();
        walk_artifacts(root, "repo", MAX_SCAN_DEPTH, &mut out);

        let paths: Vec<_> = out.iter().map(|e| e.path.as_str()).collect();
        assert!(
            out.iter().any(|e| slashed(&e.path).ends_with("/bin")
                && e.kind == "dotnet"
                && !e.path.contains("sysroot")),
            "got {paths:?}"
        );
        assert!(
            out.iter()
                .any(|e| slashed(&e.path).ends_with("/obj") && e.kind == "dotnet"),
            "got {paths:?}"
        );
        assert!(
            !out.iter()
                .any(|e| slashed(&e.path).ends_with("sysroot/bin")),
            "unmarked bin must not be claimed: {paths:?}"
        );
        assert!(
            out.iter().any(|e| e.path.ends_with("__pycache__")),
            "nested artifact inside unmarked bin must be found: {paths:?}"
        );
    }

    #[test]
    fn scan_ignores_target_without_cargo_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Xcode-style dir named `target` with no Cargo.toml beside it.
        std::fs::create_dir_all(root.join("PIFCache/target")).unwrap();
        std::fs::write(root.join("PIFCache/target/x"), vec![0u8; 10]).unwrap();

        let mut out = Vec::new();
        walk_artifacts(root, "repo", MAX_SCAN_DEPTH, &mut out);
        assert!(
            out.is_empty(),
            "got {:?}",
            out.iter().map(|e| &e.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn scan_classifies_target_by_project_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Maven repo: target + pom.xml → kind "maven", not "rust".
        std::fs::write(root.join("pom.xml"), b"<project/>").unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("target/app.jar"), vec![0u8; 10]).unwrap();

        let mut out = Vec::new();
        walk_artifacts(root, "repo", MAX_SCAN_DEPTH, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "maven");
    }

    #[test]
    fn scan_claims_build_dir_by_marker_only() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // Gradle module: build/ beside build.gradle → claimed as gradle.
        let gradle = root.join("app");
        std::fs::create_dir_all(gradle.join("build")).unwrap();
        std::fs::write(gradle.join("build.gradle"), b"").unwrap();
        std::fs::write(gradle.join("build/out.class"), vec![0u8; 10]).unwrap();
        // CMake project: build/ beside CMakeLists.txt → claimed as cmake.
        let cmake = root.join("native");
        std::fs::create_dir_all(cmake.join("build")).unwrap();
        std::fs::write(cmake.join("CMakeLists.txt"), b"").unwrap();
        // CLion variant: cmake-build-debug (prefix rule).
        std::fs::create_dir_all(cmake.join("cmake-build-debug")).unwrap();
        // Unmarked build/ (e.g. a JS project's committed output) → NOT claimed,
        // but a nested artifact inside must still surface.
        let plain = root.join("web");
        std::fs::create_dir_all(plain.join("build/__pycache__")).unwrap();

        let mut out = Vec::new();
        walk_artifacts(root, "repo", MAX_SCAN_DEPTH, &mut out);

        let kind_of = |suffix: &str| {
            out.iter()
                .find(|e| slashed(&e.path).ends_with(suffix))
                .map(|e| e.kind.clone())
        };
        assert_eq!(kind_of("app/build").as_deref(), Some("gradle"));
        assert_eq!(kind_of("native/build").as_deref(), Some("cmake"));
        assert_eq!(kind_of("cmake-build-debug").as_deref(), Some("cmake"));
        assert_eq!(
            kind_of("web/build"),
            None,
            "unmarked build must not be claimed"
        );
        assert_eq!(kind_of("__pycache__").as_deref(), Some("python"));
    }

    #[test]
    fn scan_claims_vendor_only_with_composer() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let php = root.join("php-app");
        std::fs::create_dir_all(php.join("vendor")).unwrap();
        std::fs::write(php.join("composer.json"), b"{}").unwrap();
        // Go-style committed vendor without composer.json → not claimed.
        let go = root.join("go-app");
        std::fs::create_dir_all(go.join("vendor")).unwrap();
        std::fs::write(go.join("go.mod"), b"module x").unwrap();

        let mut out = Vec::new();
        walk_artifacts(root, "repo", MAX_SCAN_DEPTH, &mut out);
        assert_eq!(
            out.len(),
            1,
            "got {:?}",
            out.iter().map(|e| &e.path).collect::<Vec<_>>()
        );
        assert!(slashed(&out[0].path).ends_with("php-app/vendor"));
        assert_eq!(out[0].kind, "php");
    }

    #[test]
    fn scan_finds_unconditional_cache_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        for d in [
            ".pytest_cache",
            ".next",
            ".terraform",
            "_build",
            "zig-out",
            ".stack-work",
        ] {
            std::fs::create_dir_all(root.join(d)).unwrap();
            std::fs::write(root.join(d).join("x"), vec![0u8; 1]).unwrap();
        }

        let mut out = Vec::new();
        walk_artifacts(root, "repo", MAX_SCAN_DEPTH, &mut out);
        let mut kinds: Vec<_> = out.iter().map(|e| e.kind.as_str()).collect();
        kinds.sort_unstable();
        assert_eq!(
            kinds,
            vec!["elixir", "haskell", "jscache", "python", "terraform", "zig"]
        );
    }

    #[test]
    fn scan_build_artifacts_missing_dir_is_non_fatal() {
        let mut out = Vec::new();
        walk_artifacts(
            Path::new("/nonexistent/path/xyz-tuic-test"),
            "repo",
            MAX_SCAN_DEPTH,
            &mut out,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn scan_build_artifacts_inner_validates_within_home() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = set_home_dir_override(tmp.path().to_path_buf());
        let repo = tmp.path().join("myrepo");
        std::fs::create_dir_all(repo.join("target")).unwrap();
        std::fs::write(repo.join("Cargo.toml"), b"[package]").unwrap();
        std::fs::write(repo.join("target/x"), vec![0u8; 42]).unwrap();

        // Register `repo` so it passes the registered-repo intersection.
        let _config_guard = crate::config::set_config_dir_override(tmp.path().join("cfg"));
        crate::config::replace_repositories_for_test(serde_json::json!({
            "repos": { repo.to_string_lossy().to_string(): {} }
        }))
        .unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let out = rt
            .block_on(scan_build_artifacts_inner(
                vec![
                    repo.to_string_lossy().to_string(),
                    "/outside/home/repo".to_string(), // invalid → skipped, not fatal
                ],
                false,
            ))
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].size_bytes, 42);
        assert_eq!(out[0].kind, "rust");
    }

    #[test]
    fn scan_build_artifacts_inner_rejects_unregistered_repo_path() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = set_home_dir_override(tmp.path().to_path_buf());
        let repo = tmp.path().join("myrepo");
        std::fs::create_dir_all(repo.join("target")).unwrap();
        std::fs::write(repo.join("Cargo.toml"), b"[package]").unwrap();
        std::fs::write(repo.join("target/x"), vec![0u8; 42]).unwrap();

        // No repos registered — even though `repo` is a valid $HOME-scoped
        // path, it must be skipped because it was never actually registered.
        let _config_guard = crate::config::set_config_dir_override(tmp.path().join("cfg"));
        crate::config::replace_repositories_for_test(serde_json::json!({ "repos": {} })).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let out = rt
            .block_on(scan_build_artifacts_inner(
                vec![repo.to_string_lossy().to_string()],
                false,
            ))
            .unwrap();
        assert!(out.is_empty(), "unregistered repo path must be skipped");
    }

    fn cached_artifact(label: &str) -> Vec<ArtifactEntry> {
        vec![ArtifactEntry {
            path: format!("/{label}"),
            kind: "test".into(),
            size_bytes: 1,
            trimmable_bytes: 0,
            last_modified_secs: 1,
            repo: "/repo".into(),
        }]
    }

    #[test]
    fn artifact_scan_cache_deduplicates_concurrent_scan() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let cache = Arc::new(ArtifactScanCache::new());
        let scans = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(std::sync::Barrier::new(2));
        let release = Arc::new(std::sync::Barrier::new(2));
        let key = vec![PathBuf::from("/repo")];

        let first = {
            let cache = Arc::clone(&cache);
            let scans = Arc::clone(&scans);
            let entered = Arc::clone(&entered);
            let release = Arc::clone(&release);
            let key = key.clone();
            std::thread::spawn(move || {
                cache.get_or_scan(key, Duration::from_secs(60), false, || {
                    scans.fetch_add(1, Ordering::SeqCst);
                    entered.wait();
                    release.wait();
                    cached_artifact("shared")
                })
            })
        };

        entered.wait();
        let second = {
            let cache = Arc::clone(&cache);
            let scans = Arc::clone(&scans);
            let key = key.clone();
            std::thread::spawn(move || {
                cache.get_or_scan(key, Duration::from_secs(60), false, || {
                    scans.fetch_add(1, Ordering::SeqCst);
                    cached_artifact("duplicate")
                })
            })
        };
        cache.wait_until_waiter_is_blocked();
        release.wait();

        assert_eq!(first.join().unwrap()[0].path, "/shared");
        assert_eq!(second.join().unwrap()[0].path, "/shared");
        assert_eq!(scans.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn artifact_scan_cache_reuses_fresh_ttl_result() {
        let cache = ArtifactScanCache::new();
        let key = vec![PathBuf::from("/repo")];
        let scans = std::sync::atomic::AtomicUsize::new(0);

        let first = cache.get_or_scan(key.clone(), Duration::from_secs(60), false, || {
            scans.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            cached_artifact("first")
        });
        let second = cache.get_or_scan(key, Duration::from_secs(60), false, || {
            scans.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            cached_artifact("second")
        });

        assert_eq!(first[0].path, "/first");
        assert_eq!(second[0].path, "/first");
        assert_eq!(scans.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn artifact_scan_cache_key_change_runs_new_scan() {
        let cache = ArtifactScanCache::new();
        let scans = std::sync::atomic::AtomicUsize::new(0);

        cache.get_or_scan(
            vec![PathBuf::from("/repo-a")],
            Duration::from_secs(60),
            false,
            || {
                scans.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                cached_artifact("a")
            },
        );
        let changed = cache.get_or_scan(
            vec![PathBuf::from("/repo-b")],
            Duration::from_secs(60),
            false,
            || {
                scans.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                cached_artifact("b")
            },
        );

        assert_eq!(changed[0].path, "/b");
        assert_eq!(scans.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn artifact_scan_cache_force_refresh_bypasses_ready_result() {
        let cache = ArtifactScanCache::new();
        let key = vec![PathBuf::from("/repo")];

        cache.get_or_scan(key.clone(), Duration::from_secs(60), false, || {
            cached_artifact("cached")
        });
        let refreshed = cache.get_or_scan(key, Duration::from_secs(60), true, || {
            cached_artifact("refreshed")
        });

        assert_eq!(refreshed[0].path, "/refreshed");
    }

    #[test]
    fn artifact_scan_cache_expired_result_is_pruned() {
        let cache = ArtifactScanCache::new();
        let key = vec![PathBuf::from("/repo")];

        cache.get_or_scan(key.clone(), Duration::from_secs(60), false, || {
            cached_artifact("expired")
        });
        let fresh = cache.get_or_scan(key, Duration::ZERO, false, || cached_artifact("fresh"));

        assert_eq!(fresh[0].path, "/fresh");
        assert_eq!(cache.entries.lock().len(), 1);
    }

    #[test]
    fn artifact_scan_cache_bounds_ready_entries() {
        let cache = ArtifactScanCache::new();
        let running_key = vec![PathBuf::from("/running")];
        cache.entries.lock().insert(
            running_key.clone(),
            ArtifactScanCacheEntry::Running {
                invalidated: false,
                fresh: false,
            },
        );
        for index in 0..(BUILD_ARTIFACT_SCAN_MAX_READY + 3) {
            cache.get_or_scan(
                vec![PathBuf::from(format!("/repo-{index}"))],
                Duration::from_secs(60),
                false,
                || cached_artifact(&index.to_string()),
            );
        }

        let entries = cache.entries.lock();
        assert_eq!(
            entries
                .values()
                .filter(|entry| matches!(entry, ArtifactScanCacheEntry::Ready { .. }))
                .count(),
            BUILD_ARTIFACT_SCAN_MAX_READY
        );
        assert!(matches!(
            entries.get(&running_key),
            Some(ArtifactScanCacheEntry::Running { .. })
        ));
    }

    /// A forced rescan must reflect the tree as it is when the user asks for it.
    /// Waiting for a scan already in progress and returning its result does not:
    /// that scan may have walked the directory before the artifact appeared.
    #[test]
    fn artifact_scan_cache_forced_refresh_never_returns_an_earlier_scan() {
        let cache = Arc::new(ArtifactScanCache::new());
        let entered = Arc::new(std::sync::Barrier::new(2));
        let release = Arc::new(std::sync::Barrier::new(2));
        let key = vec![PathBuf::from("/repo")];
        let scans = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        // A slow scan is already running, and it is the one that misses the
        // artifact created while it walks.
        let running = {
            let (cache, entered, release, key, scans) = (
                Arc::clone(&cache),
                Arc::clone(&entered),
                Arc::clone(&release),
                key.clone(),
                Arc::clone(&scans),
            );
            std::thread::spawn(move || {
                cache.get_or_scan(key, Duration::from_secs(60), false, || {
                    if scans.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                        entered.wait();
                        release.wait();
                        return cached_artifact("stale");
                    }
                    cached_artifact("fresh")
                })
            })
        };

        entered.wait();
        let forced = {
            let (cache, key) = (Arc::clone(&cache), key.clone());
            std::thread::spawn(move || {
                cache.get_or_scan(key, Duration::from_secs(60), true, || {
                    cached_artifact("unused")
                })
            })
        };
        cache.wait_until_waiter_is_blocked();
        release.wait();

        // The running scan is discarded and re-run, so both callers see the
        // state after the forced request — with one extra walk, not two.
        assert_eq!(running.join().unwrap()[0].path, "/fresh");
        assert_eq!(forced.join().unwrap()[0].path, "/fresh");
        assert_eq!(scans.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn artifact_scan_cache_panic_wakes_waiter_and_allows_recovery() {
        let cache = Arc::new(ArtifactScanCache::new());
        let entered = Arc::new(std::sync::Barrier::new(2));
        let release = Arc::new(std::sync::Barrier::new(2));
        let key = vec![PathBuf::from("/repo")];

        let first = {
            let cache = Arc::clone(&cache);
            let entered = Arc::clone(&entered);
            let release = Arc::clone(&release);
            let key = key.clone();
            std::thread::spawn(move || {
                cache.get_or_scan(key, Duration::from_secs(60), false, || {
                    entered.wait();
                    release.wait();
                    panic!("scan failed");
                })
            })
        };

        entered.wait();
        let waiter = {
            let cache = Arc::clone(&cache);
            let key = key.clone();
            std::thread::spawn(move || {
                cache.get_or_scan(key, Duration::from_secs(60), false, || {
                    cached_artifact("recovered")
                })
            })
        };
        cache.wait_until_waiter_is_blocked();
        release.wait();

        assert!(first.join().is_err());
        assert_eq!(waiter.join().unwrap()[0].path, "/recovered");
        assert!(matches!(
            cache.entries.lock().get(&key),
            Some(ArtifactScanCacheEntry::Ready { .. })
        ));
    }

    #[test]
    fn normalized_scan_key_ignores_order_and_duplicates() {
        let a = PathBuf::from("/repo-a");
        let b = PathBuf::from("/repo-b");
        assert_eq!(
            normalize_root_set(vec![b.clone(), a.clone(), a.clone()]),
            normalize_root_set(vec![a, b])
        );
    }

    #[test]
    fn artifact_scan_cache_concurrent_forced_waiter_shares_refresh() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let cache = Arc::new(ArtifactScanCache::new());
        let key = vec![PathBuf::from("/repo")];
        cache.get_or_scan(key.clone(), Duration::from_secs(60), false, || {
            cached_artifact("old")
        });

        let scans = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(std::sync::Barrier::new(2));
        let release = Arc::new(std::sync::Barrier::new(2));
        let first = {
            let cache = Arc::clone(&cache);
            let scans = Arc::clone(&scans);
            let entered = Arc::clone(&entered);
            let release = Arc::clone(&release);
            let key = key.clone();
            std::thread::spawn(move || {
                cache.get_or_scan(key, Duration::from_secs(60), true, || {
                    scans.fetch_add(1, Ordering::SeqCst);
                    entered.wait();
                    release.wait();
                    cached_artifact("forced")
                })
            })
        };

        entered.wait();
        let waiter = {
            let cache = Arc::clone(&cache);
            let scans = Arc::clone(&scans);
            let key = key.clone();
            std::thread::spawn(move || {
                cache.get_or_scan(key, Duration::from_secs(60), true, || {
                    scans.fetch_add(1, Ordering::SeqCst);
                    cached_artifact("duplicate-force")
                })
            })
        };
        cache.wait_until_waiter_is_blocked();
        release.wait();

        assert_eq!(first.join().unwrap()[0].path, "/forced");
        assert_eq!(waiter.join().unwrap()[0].path, "/forced");
        assert_eq!(scans.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn artifact_scan_cache_invalidation_discards_running_result() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let cache = Arc::new(ArtifactScanCache::new());
        let scans = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(std::sync::Barrier::new(2));
        let release = Arc::new(std::sync::Barrier::new(2));
        let worker = {
            let cache = Arc::clone(&cache);
            let scans = Arc::clone(&scans);
            let entered = Arc::clone(&entered);
            let release = Arc::clone(&release);
            std::thread::spawn(move || {
                cache.get_or_scan(
                    vec![PathBuf::from("/repo")],
                    Duration::from_secs(60),
                    false,
                    || {
                        let attempt = scans.fetch_add(1, Ordering::SeqCst);
                        if attempt == 0 {
                            entered.wait();
                            release.wait();
                            cached_artifact("stale")
                        } else {
                            cached_artifact("fresh")
                        }
                    },
                )
            })
        };

        entered.wait();
        cache.invalidate_path(Path::new("/repo/target"));
        release.wait();

        assert_eq!(worker.join().unwrap()[0].path, "/fresh");
        assert_eq!(scans.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn artifact_scan_cache_invalidation_removes_ready_result() {
        let cache = ArtifactScanCache::new();
        let key = vec![PathBuf::from("/repo")];
        cache.get_or_scan(key.clone(), Duration::from_secs(60), false, || {
            cached_artifact("deleted")
        });

        cache.invalidate_path(Path::new("/repo/target"));
        let result = cache.get_or_scan(key, Duration::from_secs(60), false, || {
            cached_artifact("after-delete")
        });

        assert_eq!(result[0].path, "/after-delete");
    }

    // -- trim tests --

    /// A Rust `target/` with the real cargo layout: linked executables at the
    /// root of each profile dir, everything else an intermediate.
    fn rust_target_fixture(repo: &Path) -> PathBuf {
        std::fs::write(repo.join("Cargo.toml"), b"[package]").unwrap();
        let target = repo.join("target");
        for profile in ["debug", "release"] {
            let p = target.join(profile);
            for inter in ["deps", "build", "incremental", ".fingerprint"] {
                std::fs::create_dir_all(p.join(inter)).unwrap();
                std::fs::write(p.join(inter).join("blob"), vec![0u8; 1000]).unwrap();
            }
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("myapp"), vec![0u8; 10]).unwrap();
        }
        // Cross-compilation: the profile dir sits under a target triple.
        let cross = target.join("aarch64-unknown-linux-gnu/release");
        std::fs::create_dir_all(cross.join("deps")).unwrap();
        std::fs::write(cross.join("deps/blob"), vec![0u8; 1000]).unwrap();
        std::fs::write(cross.join("myapp"), vec![0u8; 10]).unwrap();
        target
    }

    #[test]
    fn trim_segment_matches_literal_any_and_suffix() {
        assert!(trim_segment_matches("deps", "deps"));
        assert!(!trim_segment_matches("deps", "deps2"));
        assert!(trim_segment_matches("*", "anything"));
        assert!(trim_segment_matches("*.build", "MCP.build"));
        assert!(!trim_segment_matches("*.build", "other"));
        // A bare `.build` is the artifact dir itself, not a per-module dir —
        // the suffix form requires at least one character before the suffix.
        assert!(!trim_segment_matches("*.build", ".build"));
    }

    #[test]
    fn trim_targets_cover_rust_intermediates_at_both_depths() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let target = rust_target_fixture(&repo);
        let rule = matching_rule("target", &["Cargo.toml".to_string()]).unwrap();

        let targets = trim_targets(&target, rule);
        let rel: Vec<String> = targets
            .iter()
            .map(|p| {
                p.strip_prefix(&target)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();

        for expected in [
            "debug/deps",
            "debug/build",
            "debug/incremental",
            "debug/.fingerprint",
            "release/deps",
            "aarch64-unknown-linux-gnu/release/deps",
        ] {
            assert!(
                rel.contains(&expected.to_string()),
                "missing {expected} in {rel:?}"
            );
        }
        // The linked executable is never a target.
        assert!(!rel.iter().any(|p| p.ends_with("myapp")), "{rel:?}");
    }

    // Unix-only like the other symlink tests here: creating one on Windows
    // needs Developer Mode or admin, which an unprivileged runner lacks. The
    // guard being tested (`ft.is_symlink()`) is platform-independent.
    #[cfg(unix)]
    #[test]
    fn trim_targets_never_follow_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let target = rust_target_fixture(&repo);
        // An escape attempt: a symlinked `deps` pointing outside the repo.
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let linked = target.join("evil");
        std::fs::create_dir_all(&linked).unwrap();
        std::os::unix::fs::symlink(&outside, linked.join("deps")).unwrap();

        let rule = matching_rule("target", &["Cargo.toml".to_string()]).unwrap();
        let targets = trim_targets(&target, rule);

        assert!(
            !targets.iter().any(|p| p.starts_with(&linked)),
            "symlinked deps must not be a trim target: {targets:?}"
        );
        assert!(
            targets.iter().all(|p| p.starts_with(&target)),
            "every target must stay inside the artifact dir: {targets:?}"
        );
    }

    #[test]
    fn trim_targets_drop_nested_duplicates() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("Cargo.toml"), b"[package]").unwrap();
        // `debug/build` matches `*/build`; the `build` inside it would also
        // match `*/*/build`. Only the outer one may be reported.
        let target = repo.join("target");
        std::fs::create_dir_all(target.join("debug/build/build")).unwrap();
        let rule = matching_rule("target", &["Cargo.toml".to_string()]).unwrap();

        let targets = trim_targets(&target, rule);
        assert_eq!(targets, vec![target.join("debug/build")], "{targets:?}");
    }

    #[test]
    fn measure_splits_total_and_trimmable_in_one_walk() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let target = rust_target_fixture(&repo);
        let rule = matching_rule("target", &["Cargo.toml".to_string()]).unwrap();

        let entry = measure(&target, rule, "repo");
        // 9 intermediate blobs of 1000 B; 3 executables of 10 B.
        assert_eq!(entry.trimmable_bytes, 9000);
        assert_eq!(entry.size_bytes, 9030);
    }

    #[test]
    fn measure_reports_zero_trimmable_for_kinds_without_a_split() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let nm = repo.join("node_modules");
        std::fs::create_dir_all(nm.join("dep")).unwrap();
        std::fs::write(nm.join("dep/index.js"), vec![0u8; 100]).unwrap();
        let rule = matching_rule("node_modules", &[]).unwrap();

        let entry = measure(&nm, rule, "repo");
        assert_eq!(entry.size_bytes, 100);
        assert_eq!(
            entry.trimmable_bytes, 0,
            "node_modules has no separable intermediates"
        );
    }

    #[test]
    fn trim_removes_intermediates_and_keeps_executables() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let target = rust_target_fixture(&repo);
        let rule = matching_rule("target", &["Cargo.toml".to_string()]).unwrap();

        for t in trim_targets(&target, rule) {
            std::fs::remove_dir_all(&t).unwrap();
        }

        assert!(
            target.join("debug/myapp").exists(),
            "executable must survive"
        );
        assert!(target.join("release/myapp").exists());
        assert!(
            target
                .join("aarch64-unknown-linux-gnu/release/myapp")
                .exists()
        );
        assert!(!target.join("debug/deps").exists());
        assert!(!target.join("debug/incremental").exists());
        assert!(!target.join("release/.fingerprint").exists());
    }

    #[test]
    fn swift_trim_keeps_the_product_and_dependency_sources() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("Package.swift"), b"// swift-tools-version:5.9").unwrap();
        let build = repo.join(".build");
        let profile = build.join("arm64-apple-macosx/debug");
        for d in ["ModuleCache", "index", "MCP.build", "Modules"] {
            std::fs::create_dir_all(profile.join(d)).unwrap();
        }
        std::fs::write(profile.join("MyTool"), vec![0u8; 10]).unwrap();
        std::fs::create_dir_all(build.join("index-build/debug")).unwrap();
        std::fs::create_dir_all(build.join("checkouts/swift-nio")).unwrap();
        std::fs::create_dir_all(build.join("repositories/swift-nio.git")).unwrap();

        let rule = matching_rule(".build", &["Package.swift".to_string()]).unwrap();
        let targets = trim_targets(&build, rule);
        let rel: Vec<String> = targets
            .iter()
            .map(|p| {
                p.strip_prefix(&build)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();

        assert!(rel.contains(&"index-build".to_string()), "{rel:?}");
        assert!(
            rel.contains(&"arm64-apple-macosx/debug/ModuleCache".to_string()),
            "{rel:?}"
        );
        assert!(
            rel.contains(&"arm64-apple-macosx/debug/MCP.build".to_string()),
            "{rel:?}"
        );
        // Dependency SOURCES need the network to restore, and `Modules` holds
        // the .swiftmodule interfaces — all three stay.
        for kept in [
            "checkouts",
            "repositories",
            "arm64-apple-macosx/debug/Modules",
        ] {
            assert!(
                !rel.iter().any(|p| p == kept),
                "{kept} must be kept: {rel:?}"
            );
        }
    }

    #[test]
    fn maven_trim_keeps_the_packaged_jar() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("pom.xml"), b"<project/>").unwrap();
        let target = repo.join("target");
        for d in [
            "classes",
            "test-classes",
            "generated-sources",
            "maven-status",
        ] {
            std::fs::create_dir_all(target.join(d)).unwrap();
            std::fs::write(target.join(d).join("blob"), vec![0u8; 100]).unwrap();
        }
        std::fs::write(target.join("app-1.0.jar"), vec![0u8; 7]).unwrap();

        let rule = matching_rule("target", &["pom.xml".to_string()]).unwrap();
        assert_eq!(rule.kind, "maven");
        let entry = measure(&target, rule, "repo");
        assert_eq!(entry.trimmable_bytes, 400);
        assert_eq!(entry.size_bytes, 407, "the jar is not trimmable");

        for t in trim_targets(&target, rule) {
            std::fs::remove_dir_all(&t).unwrap();
        }
        assert!(target.join("app-1.0.jar").exists());
    }

    #[test]
    fn gradle_trim_keeps_libs_and_outputs() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("build.gradle"), b"").unwrap();
        let build = repo.join("build");
        for d in ["classes", "tmp", "kotlin", "intermediates", "reports"] {
            std::fs::create_dir_all(build.join(d)).unwrap();
        }
        for d in ["libs", "outputs", "distributions", "install"] {
            std::fs::create_dir_all(build.join(d)).unwrap();
        }

        let rule = matching_rule("build", &["build.gradle".to_string()]).unwrap();
        assert_eq!(rule.kind, "gradle");
        let rel: Vec<String> = trim_targets(&build, rule)
            .iter()
            .map(|p| {
                p.strip_prefix(&build)
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();

        assert_eq!(rel.len(), 5, "{rel:?}");
        for kept in ["libs", "outputs", "distributions", "install"] {
            assert!(
                !rel.iter().any(|p| p == kept),
                "{kept} must be kept: {rel:?}"
            );
        }
    }

    /// The failure mode to fear is a trim silently escalating into a full clean
    /// on a kind that has no separable intermediates. Two independent things
    /// prevent it, and this asserts the second one over the WHOLE rule table:
    /// `trim_build_artifact_inner` returns early on an empty `trim` list, and
    /// an empty `trim` list can never expand to a target to delete.
    #[test]
    fn no_rule_without_trim_patterns_can_ever_yield_a_target() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("anything");
        std::fs::create_dir_all(dir.join("deps/nested")).unwrap();
        std::fs::create_dir_all(dir.join("classes")).unwrap();
        std::fs::create_dir_all(dir.join("index-build")).unwrap();

        for rule in ARTIFACT_RULES.iter().filter(|r| r.trim.is_empty()) {
            assert!(
                trim_targets(&dir, rule).is_empty(),
                "{} ({}) has no trim patterns but produced targets",
                rule.name,
                rule.kind
            );
        }
        // …and the kinds that DO have patterns are not accidentally empty.
        assert!(
            ARTIFACT_RULES.iter().filter(|r| !r.trim.is_empty()).count() >= 4,
            "expected trim rules for rust, maven, gradle and swift"
        );
    }

    #[test]
    fn kinds_without_trim_rules_report_no_targets() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let venv = repo.join(".venv/lib/python3.12/site-packages");
        std::fs::create_dir_all(&venv).unwrap();
        let rule = matching_rule(".venv", &[]).unwrap();

        assert!(rule.trim.is_empty(), ".venv has no trim rules yet");
        assert_eq!(
            trim_targets(&repo.join(".venv"), rule),
            [] as [std::path::PathBuf; 0]
        );
    }

    // -- delete_build_artifact tests --

    #[test]
    fn delete_build_artifact_accepts_target_inside_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let target = repo.join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(repo.join("Cargo.toml"), b"[package]").unwrap();
        let roots = vec![repo.canonicalize().unwrap()];

        assert!(assert_deletable(&target, &roots).is_ok());
    }

    #[test]
    fn delete_build_artifact_rejects_outside_all_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        // A real `target` dir that lives OUTSIDE the registered repo root.
        let stray = tmp.path().join("elsewhere/target");
        std::fs::create_dir_all(&stray).unwrap();
        let roots = vec![repo.canonicalize().unwrap()];

        let err = assert_deletable(&stray, &roots).unwrap_err();
        assert!(err.contains("outside"), "got: {err}");
    }

    #[test]
    fn delete_build_artifact_rejects_non_artifact_name() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let src = repo.join("src"); // not a known artifact dir
        std::fs::create_dir_all(&src).unwrap();
        let roots = vec![repo.canonicalize().unwrap()];

        let err = assert_deletable(&src, &roots).unwrap_err();
        assert!(err.contains("artifact"), "got: {err}");
    }

    #[test]
    fn delete_build_artifact_rejects_bin_without_dotnet_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        // Rust convention: `src/bin` holds SOURCE files — must never be deletable.
        let bin = repo.join("src/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("main.rs"), b"fn main() {}").unwrap();
        let roots = vec![repo.canonicalize().unwrap()];

        let err = assert_deletable(&bin, &roots).unwrap_err();
        assert!(err.contains("project file"), "got: {err}");
    }

    #[test]
    fn delete_build_artifact_accepts_bin_with_csproj_beside_it() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let bin = repo.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(repo.join("app.csproj"), b"<Project/>").unwrap();
        let roots = vec![repo.canonicalize().unwrap()];

        assert!(assert_deletable(&bin, &roots).is_ok());
    }

    #[test]
    fn delete_build_artifact_respects_new_kind_markers() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        // Gradle build/ with marker → deletable; bare build/ → refused.
        let marked = repo.join("app");
        std::fs::create_dir_all(marked.join("build")).unwrap();
        std::fs::write(marked.join("settings.gradle"), b"").unwrap();
        let bare = repo.join("web");
        std::fs::create_dir_all(bare.join("build")).unwrap();
        let roots = vec![repo.canonicalize().unwrap()];

        assert!(assert_deletable(&marked.join("build"), &roots).is_ok());
        let err = assert_deletable(&bare.join("build"), &roots).unwrap_err();
        assert!(err.contains("project file"), "got: {err}");
    }

    #[test]
    fn delete_build_artifact_rejects_repo_root_itself() {
        let tmp = tempfile::tempdir().unwrap();
        // Repo root whose own name happens to be a known artifact name — the
        // guard must still refuse to delete the registered root (c == root).
        let repo = tmp.path().join("target");
        std::fs::create_dir_all(&repo).unwrap();
        let root = repo.canonicalize().unwrap();

        let err = assert_deletable(&root, std::slice::from_ref(&root)).unwrap_err();
        assert!(err.contains("outside"), "got: {err}");
    }

    #[cfg(unix)]
    #[test]
    fn delete_build_artifact_rejects_symlink_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        // Real `target` outside the repo; a symlink inside the repo points to it.
        let outside = tmp.path().join("outside/target");
        std::fs::create_dir_all(&outside).unwrap();
        let link = repo.join("target");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let roots = vec![repo.canonicalize().unwrap()];

        // canonicalize() resolves the symlink to `outside`, which is not inside
        // the repo root → rejected despite the artifact-name basename matching.
        let err = assert_deletable(&link, &roots).unwrap_err();
        assert!(err.contains("outside"), "got: {err}");
    }

    #[test]
    fn delete_build_artifact_inner_removes_real_target() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = set_home_dir_override(tmp.path().to_path_buf());
        let repo = tmp.path().join("repo");
        let target = repo.join("target");
        std::fs::create_dir_all(target.join("debug")).unwrap();
        std::fs::write(repo.join("Cargo.toml"), b"[package]").unwrap();
        std::fs::write(target.join("debug/artifact.o"), vec![0u8; 10]).unwrap();

        // Register `repo` so it passes the registered-repo intersection.
        let _config_guard = crate::config::set_config_dir_override(tmp.path().join("cfg"));
        crate::config::replace_repositories_for_test(serde_json::json!({
            "repos": { repo.to_string_lossy().to_string(): {} }
        }))
        .unwrap();

        let cache_key = vec![repo.canonicalize().unwrap()];
        artifact_scan_cache().get_or_scan(
            cache_key.clone(),
            Duration::from_secs(60),
            false,
            || cached_artifact("before-delete"),
        );
        assert!(
            artifact_scan_cache()
                .entries
                .lock()
                .contains_key(&cache_key)
        );

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(delete_build_artifact_inner(
            target.to_string_lossy().to_string(),
            vec![repo.to_string_lossy().to_string()],
        ));
        assert!(result.is_ok(), "delete failed: {:?}", result);
        assert!(!target.exists(), "target should be removed");
        assert!(repo.exists(), "repo root must survive");
        assert!(
            !artifact_scan_cache()
                .entries
                .lock()
                .contains_key(&cache_key),
            "successful deletion must invalidate the affected cached scan"
        );
    }

    /// The panel patches its cached total with what the trim returns. That
    /// number must be what left the disk, not what the last scan estimated:
    /// a build between the scan and the trim moves the estimate, and
    /// subtracting it publishes a total no measurement supports.
    #[test]
    fn trim_build_artifact_inner_reports_the_bytes_it_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = set_home_dir_override(tmp.path().to_path_buf());
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let target = rust_target_fixture(&repo);

        let _config_guard = crate::config::set_config_dir_override(tmp.path().join("cfg"));
        crate::config::replace_repositories_for_test(serde_json::json!({
            "repos": { repo.to_string_lossy().to_string(): {} }
        }))
        .unwrap();

        let (before, _) = measure_sizes(&target, MAX_SIZE_DEPTH, &[], false);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let reclaimed = rt
            .block_on(trim_build_artifact_inner(
                target.to_string_lossy().to_string(),
                vec![repo.to_string_lossy().to_string()],
            ))
            .expect("trim failed");
        let (after, _) = measure_sizes(&target, MAX_SIZE_DEPTH, &[], false);

        assert!(reclaimed > 0, "the fixture has intermediates to reclaim");
        assert_eq!(
            reclaimed,
            before - after,
            "the reported bytes must be the bytes that left the disk"
        );

        // Nothing left to separate: a second trim reclaims nothing rather than
        // reporting the estimate again.
        let again = rt
            .block_on(trim_build_artifact_inner(
                target.to_string_lossy().to_string(),
                vec![repo.to_string_lossy().to_string()],
            ))
            .expect("second trim failed");
        assert_eq!(again, 0);
    }

    #[test]
    fn delete_build_artifact_inner_rejects_outside_home() {
        let _guard = FS_TEST_LOCK.lock().unwrap();
        let home = dirs::home_dir().unwrap();
        if !Path::new("/tmp").starts_with(&home) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(delete_build_artifact_inner(
                "/tmp/.tuic-test-delete/target".to_string(),
                vec!["/tmp/.tuic-test-delete".to_string()],
            ));
            assert!(result.is_err());
        }
    }

    /// SEC-1: a plugin must not be able to widen `fs:delete` containment by
    /// passing an unregistered `$HOME` subdir (or `$HOME` itself) as a
    /// `repo_path`. Only genuinely registered repos (`repositories.json`)
    /// may act as containment roots — a caller-supplied root that was never
    /// registered is dropped, so the delete is refused even though the
    /// target is a real, correctly-named build-artifact dir under `$HOME`.
    #[test]
    fn delete_build_artifact_inner_rejects_unregistered_repo_path() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = set_home_dir_override(tmp.path().to_path_buf());

        // A legitimately registered repo that does NOT contain the target.
        let registered_repo = tmp.path().join("registered-repo");
        std::fs::create_dir_all(&registered_repo).unwrap();
        let _config_guard = crate::config::set_config_dir_override(tmp.path().join("cfg"));
        crate::config::replace_repositories_for_test(serde_json::json!({
            "repos": { registered_repo.to_string_lossy().to_string(): {} }
        }))
        .unwrap();

        // A real `node_modules` dir sitting under an UNregistered $HOME subdir.
        let unregistered_project = tmp.path().join("unregistered-project");
        let target = unregistered_project.join("node_modules");
        std::fs::create_dir_all(&target).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        // The plugin lies about repo_paths, claiming the entire $HOME is a
        // "repo root" to widen containment.
        let result = rt.block_on(delete_build_artifact_inner(
            target.to_string_lossy().to_string(),
            vec![tmp.path().to_string_lossy().to_string()],
        ));

        assert!(
            result.is_err(),
            "must reject: $HOME was never actually registered as a repo"
        );
        assert!(target.exists(), "artifact must survive the rejected delete");
    }

    // -- fs:watch per-plugin cap (story 157) --

    #[cfg(feature = "desktop")]
    fn add_watchers(state: &AppState, plugin_id: &str, n: usize) {
        use notify::{Config, RecommendedWatcher, Watcher};
        for _ in 0..n {
            let (tx, _rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
            let w = RecommendedWatcher::new(tx, Config::default()).unwrap();
            state
                .plugin_watchers
                .insert(uuid::Uuid::new_v4().to_string(), (plugin_id.to_string(), w));
        }
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn watcher_cap_allows_under_limit() {
        let state = crate::state::tests_support::make_test_app_state();
        add_watchers(&state, "p", MAX_WATCHERS_PER_PLUGIN - 1);
        assert!(check_watcher_cap(&state, "p").is_ok());
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn watcher_cap_rejects_at_limit_with_clear_error() {
        let state = crate::state::tests_support::make_test_app_state();
        add_watchers(&state, "p", MAX_WATCHERS_PER_PLUGIN);
        let err = check_watcher_cap(&state, "p").unwrap_err();
        assert!(err.contains("Watch limit reached"), "got: {err}");
        assert!(
            err.contains(&MAX_WATCHERS_PER_PLUGIN.to_string()),
            "got: {err}"
        );
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn watcher_cap_is_counted_per_plugin() {
        let state = crate::state::tests_support::make_test_app_state();
        // Another plugin at its own limit must not consume this plugin's budget.
        add_watchers(&state, "other", MAX_WATCHERS_PER_PLUGIN);
        assert!(check_watcher_cap(&state, "p").is_ok());
        // Fill "p" to its limit; "other" being full is irrelevant to "p".
        add_watchers(&state, "p", MAX_WATCHERS_PER_PLUGIN);
        assert!(check_watcher_cap(&state, "p").is_err());
        // "other" is still independently over its own limit.
        assert!(check_watcher_cap(&state, "other").is_err());
    }
}
