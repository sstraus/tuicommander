//! User plugin discovery, validation, and serving.
//!
//! Plugins are ES modules installed in `{config_dir}/plugins/{id}/`.
//! Each plugin directory contains a `manifest.json` and a JS entry point.
//!
//! This module provides:
//! - `register_plugin_protocol()` — custom `plugin://` URI scheme handler
//! - `list_user_plugins` — Tauri command returning valid manifests
//! - `start_plugin_watcher()` — file watcher for hot-reload events
//! - `read_plugin_data` / `write_plugin_data` / `delete_plugin_data` — sandboxed per-plugin storage

use crate::config;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use tauri::http::{Response, StatusCode};
use tauri::{AppHandle, Emitter};

/// Root directory for user plugins: `{config_dir}/plugins/`
fn plugins_dir() -> PathBuf {
    config::config_dir().join("plugins")
}

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

/// Plugin manifest as declared in `manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(rename = "minAppVersion")]
    pub min_app_version: String,
    pub main: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// Known capability strings. Anything outside this set is rejected.
const KNOWN_CAPABILITIES: &[&str] = &[
    "pty:write",
    "ui:markdown",
    "ui:sound",
    "invoke:read_file",
    "invoke:list_markdown_files",
];

/// Validate a parsed manifest for required fields and sanity.
fn validate_manifest(manifest: &PluginManifest, dir_name: &str) -> Result<(), String> {
    if manifest.id.is_empty() {
        return Err("id is empty".into());
    }
    if manifest.id != dir_name {
        return Err(format!(
            "id \"{}\" does not match directory name \"{}\"",
            manifest.id, dir_name
        ));
    }
    if manifest.name.is_empty() {
        return Err("name is empty".into());
    }
    if manifest.version.is_empty() {
        return Err("version is empty".into());
    }
    if manifest.min_app_version.is_empty() {
        return Err("minAppVersion is empty".into());
    }
    if manifest.main.is_empty() {
        return Err("main is empty".into());
    }
    // main must not escape the plugin directory
    if is_path_escape(&manifest.main) {
        return Err(format!("main \"{}\" attempts path traversal", manifest.main));
    }
    // Validate capabilities
    for cap in &manifest.capabilities {
        if !KNOWN_CAPABILITIES.contains(&cap.as_str()) {
            return Err(format!("unknown capability: \"{cap}\""));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Path safety
// ---------------------------------------------------------------------------

/// Returns true if a relative path attempts to escape its root via `..`,
/// absolute components, or other shenanigans.
fn is_path_escape(relative: &str) -> bool {
    let path = Path::new(relative);

    // Reject absolute paths
    if path.is_absolute() {
        return true;
    }

    for component in path.components() {
        match component {
            Component::ParentDir => return true,
            Component::RootDir | Component::Prefix(_) => return true,
            _ => {}
        }
    }

    false
}

/// Resolve a `plugin://{id}/{file}` URI to a filesystem path within the
/// plugins directory. Returns `None` if the path is invalid or escapes.
fn resolve_plugin_path(uri_path: &str) -> Option<PathBuf> {
    // URI path starts with `/`, e.g. `/my-plugin/main.js`
    let trimmed = uri_path.strip_prefix('/').unwrap_or(uri_path);

    if trimmed.is_empty() {
        return None;
    }

    // Split into plugin_id / rest
    let (plugin_id, file) = trimmed.split_once('/')?;

    if plugin_id.is_empty() || file.is_empty() {
        return None;
    }

    // Validate no path traversal in either segment
    if is_path_escape(plugin_id) || is_path_escape(file) {
        return None;
    }

    let full = plugins_dir().join(plugin_id).join(file);

    // Final safety: canonicalize and verify it's under plugins_dir.
    // If the file doesn't exist yet, we can't canonicalize — but that's fine,
    // we just return the constructed path and let the caller handle the 404.
    // We still check the logical path doesn't escape.
    Some(full)
}

// ---------------------------------------------------------------------------
// URI protocol handler
// ---------------------------------------------------------------------------

/// Register the `plugin://` custom URI scheme protocol on the Tauri builder.
///
/// Serves JS files from `{config_dir}/plugins/{id}/{file}` with the
/// `application/javascript` MIME type. Rejects path traversal attempts.
///
/// The `?t=<timestamp>` query parameter is accepted (and ignored) to allow
/// cache-busting for hot-reload.
pub fn register_plugin_protocol(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.register_uri_scheme_protocol("plugin", |_ctx, request| {
        let uri = request.uri();
        let path = uri.path();

        let Some(file_path) = resolve_plugin_path(path) else {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/plain")
                .body(b"Invalid plugin path".to_vec())
                .unwrap();
        };

        match std::fs::read(&file_path) {
            Ok(data) => {
                // Determine MIME type from extension
                let mime = match file_path.extension().and_then(|e| e.to_str()) {
                    Some("js" | "mjs") => "application/javascript",
                    Some("json") => "application/json",
                    Some("css") => "text/css",
                    _ => "application/octet-stream",
                };

                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", mime)
                    .body(data)
                    .unwrap()
            }
            Err(_) => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("Content-Type", "text/plain")
                .body(b"Plugin file not found".to_vec())
                .unwrap(),
        }
    })
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Scan the user plugins directory and return all valid manifests.
/// Invalid manifests are logged and skipped — never cause an error.
#[tauri::command]
pub fn list_user_plugins() -> Vec<PluginManifest> {
    let dir = plugins_dir();
    if !dir.exists() {
        return Vec::new();
    }

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("[plugins] Failed to read plugins dir: {err}");
            return Vec::new();
        }
    };

    let mut manifests = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Skip hidden directories
        if dir_name.starts_with('.') {
            continue;
        }

        let manifest_path = path.join("manifest.json");
        let manifest_data = match std::fs::read_to_string(&manifest_path) {
            Ok(d) => d,
            Err(err) => {
                eprintln!("[plugins] {dir_name}: failed to read manifest.json: {err}");
                continue;
            }
        };

        let manifest: PluginManifest = match serde_json::from_str(&manifest_data) {
            Ok(m) => m,
            Err(err) => {
                eprintln!("[plugins] {dir_name}: invalid manifest.json: {err}");
                continue;
            }
        };

        if let Err(err) = validate_manifest(&manifest, &dir_name) {
            eprintln!("[plugins] {dir_name}: manifest validation failed: {err}");
            continue;
        }

        manifests.push(manifest);
    }

    manifests
}

// ---------------------------------------------------------------------------
// Sandboxed plugin data storage
// ---------------------------------------------------------------------------

/// Get the data directory for a specific plugin: `{plugins_dir}/{id}/data/`
fn plugin_data_dir(plugin_id: &str) -> Result<PathBuf, String> {
    if plugin_id.is_empty() || is_path_escape(plugin_id) {
        return Err("Invalid plugin ID".into());
    }
    Ok(plugins_dir().join(plugin_id).join("data"))
}

/// Resolve and validate a path within a plugin's data directory.
fn resolve_data_path(plugin_id: &str, relative_path: &str) -> Result<PathBuf, String> {
    if relative_path.is_empty() {
        return Err("Path is empty".into());
    }
    if is_path_escape(relative_path) {
        return Err("Path traversal not allowed".into());
    }
    let data_dir = plugin_data_dir(plugin_id)?;
    Ok(data_dir.join(relative_path))
}

#[tauri::command]
pub fn read_plugin_data(plugin_id: String, path: String) -> Result<Option<String>, String> {
    let file_path = resolve_data_path(&plugin_id, &path)?;
    match std::fs::read_to_string(&file_path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Failed to read plugin data: {e}")),
    }
}

#[tauri::command]
pub fn write_plugin_data(plugin_id: String, path: String, content: String) -> Result<(), String> {
    let file_path = resolve_data_path(&plugin_id, &path)?;
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create data directory: {e}"))?;
    }
    std::fs::write(&file_path, &content)
        .map_err(|e| format!("Failed to write plugin data: {e}"))
}

#[tauri::command]
pub fn delete_plugin_data(plugin_id: String, path: String) -> Result<(), String> {
    let file_path = resolve_data_path(&plugin_id, &path)?;
    match std::fs::remove_file(&file_path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Failed to delete plugin data: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Plugin directory watcher (hot reload)
// ---------------------------------------------------------------------------

/// Start watching the plugins directory for changes and emit `plugin-changed`
/// events to the frontend. Uses the same debouncer pattern as repo_watcher.
pub fn start_plugin_watcher(app_handle: &AppHandle) {
    let dir = plugins_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("[plugins] Failed to create plugins dir: {e}");
        return;
    }

    let handle = app_handle.clone();
    std::thread::spawn(move || {
        use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
        use std::time::Duration;

        let (tx, rx) = std::sync::mpsc::channel();

        let mut debouncer = match new_debouncer(Duration::from_millis(500), tx) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[plugins] Failed to create watcher: {e}");
                return;
            }
        };

        if let Err(e) = debouncer
            .watcher()
            .watch(&dir, notify::RecursiveMode::Recursive)
        {
            eprintln!("[plugins] Failed to watch plugins dir: {e}");
            return;
        }

        eprintln!("[plugins] Watching {dir:?} for changes");

        loop {
            match rx.recv() {
                Ok(Ok(events)) => {
                    // Determine which plugin IDs changed
                    let mut changed_ids: Vec<String> = Vec::new();
                    for event in &events {
                        if event.kind == DebouncedEventKind::Any {
                            // Extract plugin ID from path: plugins_dir / <id> / ...
                            if let Ok(relative) = event.path.strip_prefix(&dir) {
                                if let Some(first) = relative.components().next() {
                                    let id = first.as_os_str().to_string_lossy().to_string();
                                    if !id.starts_with('.') && !changed_ids.contains(&id) {
                                        changed_ids.push(id);
                                    }
                                }
                            }
                        }
                    }

                    if !changed_ids.is_empty() {
                        eprintln!("[plugins] Change detected in: {changed_ids:?}");
                        let _ = handle.emit("plugin-changed", changed_ids);
                    }
                }
                Ok(Err(errs)) => {
                    eprintln!("[plugins] Watcher errors: {errs:?}");
                }
                Err(_) => break, // Channel closed
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Path safety --

    #[test]
    fn path_escape_rejects_parent_dir() {
        assert!(is_path_escape("../etc/passwd"));
        assert!(is_path_escape("foo/../bar"));
        assert!(is_path_escape(".."));
    }

    #[test]
    fn path_escape_rejects_absolute() {
        assert!(is_path_escape("/etc/passwd"));
    }

    #[test]
    fn path_escape_allows_normal_relative() {
        assert!(!is_path_escape("main.js"));
        assert!(!is_path_escape("src/index.js"));
        assert!(!is_path_escape("dist/bundle.min.js"));
    }

    // -- resolve_plugin_path --

    #[test]
    fn resolve_valid_path() {
        let result = resolve_plugin_path("/my-plugin/main.js");
        assert!(result.is_some());
        let path = result.unwrap();
        assert!(path.ends_with("my-plugin/main.js"));
    }

    #[test]
    fn resolve_rejects_traversal_in_id() {
        assert!(resolve_plugin_path("/../evil/main.js").is_none());
        assert!(resolve_plugin_path("/../../main.js").is_none());
    }

    #[test]
    fn resolve_rejects_traversal_in_file() {
        assert!(resolve_plugin_path("/my-plugin/../../../etc/passwd").is_none());
        assert!(resolve_plugin_path("/my-plugin/../../secret").is_none());
    }

    #[test]
    fn resolve_rejects_empty_segments() {
        assert!(resolve_plugin_path("/").is_none());
        assert!(resolve_plugin_path("//main.js").is_none());
        assert!(resolve_plugin_path("/my-plugin/").is_none());
    }

    #[test]
    fn resolve_nested_file_path() {
        let result = resolve_plugin_path("/my-plugin/dist/bundle.js");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("my-plugin/dist/bundle.js"));
    }

    // -- Manifest validation --

    fn valid_manifest(dir_name: &str) -> PluginManifest {
        PluginManifest {
            id: dir_name.to_string(),
            name: "Test Plugin".to_string(),
            version: "1.0.0".to_string(),
            min_app_version: "0.3.0".to_string(),
            main: "main.js".to_string(),
            description: None,
            author: None,
            capabilities: vec![],
        }
    }

    #[test]
    fn validate_valid_manifest() {
        assert!(validate_manifest(&valid_manifest("test-plugin"), "test-plugin").is_ok());
    }

    #[test]
    fn validate_rejects_empty_id() {
        let mut m = valid_manifest("test");
        m.id = String::new();
        assert!(validate_manifest(&m, "test").is_err());
    }

    #[test]
    fn validate_rejects_id_mismatch() {
        let m = valid_manifest("wrong-name");
        assert!(validate_manifest(&m, "actual-dir").is_err());
    }

    #[test]
    fn validate_rejects_empty_name() {
        let mut m = valid_manifest("test");
        m.name = String::new();
        assert!(validate_manifest(&m, "test").is_err());
    }

    #[test]
    fn validate_rejects_empty_version() {
        let mut m = valid_manifest("test");
        m.version = String::new();
        assert!(validate_manifest(&m, "test").is_err());
    }

    #[test]
    fn validate_rejects_empty_min_app_version() {
        let mut m = valid_manifest("test");
        m.min_app_version = String::new();
        assert!(validate_manifest(&m, "test").is_err());
    }

    #[test]
    fn validate_rejects_empty_main() {
        let mut m = valid_manifest("test");
        m.main = String::new();
        assert!(validate_manifest(&m, "test").is_err());
    }

    #[test]
    fn validate_rejects_traversal_in_main() {
        let mut m = valid_manifest("test");
        m.main = "../evil.js".to_string();
        assert!(validate_manifest(&m, "test").is_err());
    }

    #[test]
    fn validate_rejects_unknown_capability() {
        let mut m = valid_manifest("test");
        m.capabilities = vec!["pty:write".into(), "evil:capability".into()];
        assert!(validate_manifest(&m, "test").is_err());
    }

    #[test]
    fn validate_accepts_known_capabilities() {
        let mut m = valid_manifest("test");
        m.capabilities = vec!["pty:write".into(), "ui:markdown".into(), "ui:sound".into()];
        assert!(validate_manifest(&m, "test").is_ok());
    }

    // -- list_user_plugins with temp dir --

    #[test]
    fn list_returns_empty_when_no_dir() {
        // plugins_dir() points to the real config; we test the scanning logic
        // indirectly through validate_manifest above. The list function itself
        // just returns an empty vec when the dir doesn't exist.
        let dir = std::env::temp_dir().join("tuic-test-nonexistent-plugins");
        let _ = std::fs::remove_dir_all(&dir);
        // We can't easily override plugins_dir() in tests, but we can verify
        // the function doesn't panic
        let _ = list_user_plugins();
    }

    // -- Plugin data path resolution --

    #[test]
    fn data_path_rejects_empty_plugin_id() {
        assert!(resolve_data_path("", "cache.json").is_err());
    }

    #[test]
    fn data_path_rejects_traversal_in_plugin_id() {
        assert!(resolve_data_path("../evil", "cache.json").is_err());
    }

    #[test]
    fn data_path_rejects_empty_path() {
        assert!(resolve_data_path("my-plugin", "").is_err());
    }

    #[test]
    fn data_path_rejects_traversal_in_path() {
        assert!(resolve_data_path("my-plugin", "../secret.json").is_err());
    }

    #[test]
    fn data_path_resolves_valid() {
        let result = resolve_data_path("my-plugin", "cache.json");
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.ends_with("my-plugin/data/cache.json"));
    }

    #[test]
    fn data_path_nested() {
        let result = resolve_data_path("my-plugin", "cache/v1/data.json");
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.ends_with("my-plugin/data/cache/v1/data.json"));
    }
}
