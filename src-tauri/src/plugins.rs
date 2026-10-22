//! User plugin discovery, validation, and serving.
//!
//! Plugins are ES modules installed in `{config_dir}/plugins/{id}/`.
//! Each plugin directory contains a `manifest.json` and a JS entry point.
//!
//! This module provides:
//! - `seed_externalized_builtin_plugins()` — one-time Plan/Stories package migration
//! - `register_plugin_protocol()` — custom `plugin://` URI scheme handler
//! - `list_user_plugins` — Tauri command returning valid manifests
//! - `start_plugin_watcher()` — file watcher for hot-reload events
//! - `read_plugin_data` / `write_plugin_data` / `delete_plugin_data` — sandboxed per-plugin storage

use crate::config;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
#[cfg(feature = "desktop")]
use tauri::http::{Response, StatusCode};
#[cfg(feature = "desktop")]
use tauri::{AppHandle, Emitter};

// ---------------------------------------------------------------------------
// Capability enforcement
// ---------------------------------------------------------------------------
//
// WHAT THIS BOUNDARY IS, AND WHAT IT IS NOT.
//
// Capabilities gate what a plugin DECLARES — a plugin that never asked for
// `exec:cli` cannot reach the exec commands under its own id, and a manifest is
// validated against KNOWN_CAPABILITIES before anything is granted. That is the
// whole of the guarantee.
//
// Capabilities do NOT gate what a plugin can IMPERSONATE. `plugin_id` arrives
// from the caller and is the only key every check below consults, and plugins
// are loaded by a bare dynamic `import()` into the SAME JavaScript realm as the
// host (`src/plugins/pluginLoader.ts`). Any plugin can therefore import `invoke`
// itself and pass a different plugin's id to inherit that plugin's exec,
// credential and allowed-URL grants. Plugins are isolated from the HOST's
// declared surface, not from EACH OTHER.
//
// This is stated plainly rather than papered over: a host-generated per-plugin
// token would not fix it. Same-realm JavaScript can read the token out of the
// module that holds it, proxy the function that uses it, or monkey-patch the
// transport — so the token would add ceremony and a false sense of a boundary
// without moving the actual trust line. TUICommander's threat model is that the
// user vets what they install (see AGENTS.md "Security Scope"); treat every
// installed plugin as trusted with the union of all installed capabilities.
//
// DEFERRED (2026-07-28) — real per-plugin isolation. It requires running each
// plugin off-realm (a Worker or a sandboxed iframe) and giving it a host-created
// MessagePort as its ONLY channel to the backend, so the plugin's identity is
// the port it was handed rather than a string it supplies. That is a rewrite of
// the plugin loading and IPC surface, not an incremental patch, which is why a
// token alone was rejected (Boss decision, 2026-07-28). Re-evaluate if plugins
// ever become installable from an untrusted registry, which is the point where
// mutually-untrusted plugins stop being hypothetical.

/// Check that a plugin has a required capability, using the provided map.
/// This is the testable core — does not depend on AppState.
///
/// Answers "did this plugin declare this capability", NOT "is this caller really
/// this plugin" — see the module-level note above.
/// Built-in plugins and their allowed capabilities. Plan and Stories moved to
/// ordinary external packages; keep the table for future native distributions.
const BUILTIN_PLUGIN_CAPABILITIES: &[(&str, &[&str])] = &[];

fn check_plugin_capability_inner(
    loaded_plugins: &dashmap::DashMap<String, Vec<String>>,
    plugin_id: &str,
    capability: &str,
) -> Result<(), String> {
    // Check dynamically registered (user) plugins first
    if let Some(caps) = loaded_plugins.get(plugin_id) {
        return if caps.iter().any(|c| c == capability) {
            Ok(())
        } else {
            Err(format!(
                "Plugin \"{plugin_id}\" requires capability \"{capability}\" but did not declare it"
            ))
        };
    }

    // Check hardcoded built-in plugins
    for &(id, caps) in BUILTIN_PLUGIN_CAPABILITIES {
        if id == plugin_id {
            return if caps.contains(&capability) {
                Ok(())
            } else {
                Err(format!(
                    "Built-in plugin \"{plugin_id}\" does not have capability \"{capability}\""
                ))
            };
        }
    }

    Err(format!(
        "Plugin \"{plugin_id}\" is not registered — cannot verify capabilities"
    ))
}

/// Check that a loaded plugin has the required capability.
/// Reads the loaded_plugins map from AppState.
pub(crate) fn check_plugin_capability(
    state: &crate::AppState,
    plugin_id: &str,
    capability: &str,
) -> Result<(), String> {
    check_plugin_capability_inner(&state.loaded_plugins, plugin_id, capability)
}

/// Root directory for user plugins: `{config_dir}/plugins/`
fn plugins_dir() -> PathBuf {
    config::config_dir().join("plugins")
}

// ---------------------------------------------------------------------------
// One-time migration of former built-ins to external packages
// ---------------------------------------------------------------------------

const EXTERNALIZED_BUILTINS_MARKER: &str = ".externalized-plan-stories-v1";

struct SeededPlugin {
    id: &'static str,
    files: &'static [(&'static str, &'static str)],
}

const SEEDED_PLUGINS: &[SeededPlugin] = &[
    SeededPlugin {
        id: "plan",
        files: &[
            (
                "manifest.json",
                include_str!("../../plugins/plan/manifest.json"),
            ),
            ("main.js", include_str!("../../plugins/plan/main.js")),
            ("README.md", include_str!("../../plugins/plan/README.md")),
        ],
    },
    SeededPlugin {
        id: "stories-ticker",
        files: &[
            (
                "manifest.json",
                include_str!("../../plugins/stories-ticker/manifest.json"),
            ),
            (
                "main.js",
                include_str!("../../plugins/stories-ticker/main.js"),
            ),
            (
                "README.md",
                include_str!("../../plugins/stories-ticker/README.md"),
            ),
        ],
    },
];

/// Install the former built-ins as ordinary external plugins exactly once.
/// Existing directories win, so a manually installed/newer package is never
/// overwritten. The marker is written only after both packages are present or
/// deliberately preserved; uninstalling later therefore remains permanent.
pub(crate) fn seed_externalized_builtin_plugins(config_dir: &Path) -> Result<(), String> {
    let marker = config_dir.join(EXTERNALIZED_BUILTINS_MARKER);
    if marker.exists() {
        return Ok(());
    }

    let target_root = config_dir.join("plugins");
    std::fs::create_dir_all(&target_root)
        .map_err(|e| format!("Failed to create plugins directory: {e}"))?;

    for plugin in SEEDED_PLUGINS {
        let target = target_root.join(plugin.id);
        if target.exists() {
            continue;
        }

        let staging = target_root.join(format!(".seed-{}-{}", plugin.id, uuid::Uuid::new_v4()));
        let install = (|| -> Result<(), String> {
            std::fs::create_dir(&staging)
                .map_err(|e| format!("Failed to stage plugin {}: {e}", plugin.id))?;
            for (name, contents) in plugin.files {
                std::fs::write(staging.join(name), contents)
                    .map_err(|e| format!("Failed to seed plugin {}/{}: {e}", plugin.id, name))?;
            }
            std::fs::rename(&staging, &target)
                .map_err(|e| format!("Failed to install seeded plugin {}: {e}", plugin.id))
        })();
        if let Err(error) = install {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error);
        }
    }

    crate::config::persist_atomic(&marker, b"1\n")
}

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "settings/plugin_contract_tests.rs"]
mod settings_contract_tests;

/// Plugin manifest as declared in `manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Computed from installed files; manifest input cannot claim an identity.
    #[serde(
        default,
        skip_deserializing,
        skip_serializing_if = "Option::is_none",
        rename = "contentSha256"
    )]
    pub content_digest: Option<String>,
    #[serde(rename = "minAppVersion")]
    pub min_app_version: String,
    pub main: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// URL patterns allowed for net:http requests (e.g. "https://api.example.com/*")
    #[serde(default, rename = "allowedUrls")]
    pub allowed_urls: Vec<String>,
    /// Agent types this plugin is specific to (e.g. ["claude", "gemini"]).
    /// Empty means the plugin is universal and active for all terminals.
    #[serde(default, rename = "agentTypes")]
    pub agent_types: Vec<String>,
    /// CLI binaries this plugin is allowed to execute via exec:cli.
    /// Each entry is a plain binary name (e.g. "rtk", "mdkb").
    #[serde(default)]
    pub binaries: Vec<String>,
    /// Account providers whose accounts `httpFetch({account})` may use: a core
    /// provider id (`"github"`) or a token-only descriptor registered on load.
    #[serde(default)]
    pub providers: Vec<ManifestProvider>,
    /// Complete scoped settings schema. Existing configurable plugins use host migrations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<crate::settings::plugin_schema::Definition>,
}

/// One manifest `providers` entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum ManifestProvider {
    Core(String),
    Declared(crate::accounts::ProviderDescriptor),
}

/// Account providers that core registers itself; a manifest may only reference them.
const CORE_ACCOUNT_PROVIDERS: &[&str] = &["github"];

impl ManifestProvider {
    fn id(&self) -> &str {
        match self {
            Self::Core(id) => id,
            Self::Declared(descriptor) => &descriptor.id,
        }
    }
}

impl PluginManifest {
    /// Manifest consent: may this plugin authenticate with accounts of `provider`?
    pub(crate) fn declares_provider(&self, provider: &str) -> bool {
        self.providers.iter().any(|entry| entry.id() == provider)
    }
}

/// Binary name prefixes that are never permitted in plugin manifests.
/// Uses prefix matching to catch versioned variants (e.g. python3.11, nodejs).
/// Blocks shell interpreters, scripting runtimes, and destructive system tools that would
/// grant a plugin arbitrary code execution via the `args` array in `exec:cli`.
const BLOCKED_BINARY_PREFIXES: &[&str] = &[
    // Shell interpreters
    "bash", "sh", "zsh", "fish", "ksh", "dash", "csh", "tcsh",
    // Scripting runtimes (prefix catches python3.11, nodejs, ruby3, etc.)
    "python", "ruby", "perl", "node", "deno", "bun", "pypy", "jruby", // Network tools
    "curl", "wget", "nc", "netcat", "socat", // Destructive filesystem tools
    "rm", "mv", "cp", "chmod", "chown", "dd", // Privilege escalation
    "sudo", "su", "doas", // Build / package tools (can run arbitrary scripts)
    "make", "cargo", "npm", "yarn", "pnpm", "pip", "go",
];

/// Known capability strings. Anything outside this set is rejected.
const KNOWN_CAPABILITIES: &[&str] = &[
    "pty:write",
    "pty:read",
    "ui:markdown",
    "ui:sound",
    "invoke:read_file",
    "invoke:list_markdown_files",
    "fs:read",
    "fs:list",
    "fs:watch",
    "fs:write",
    "fs:rename",
    "fs:scan",
    "fs:delete",
    "net:http",
    "credentials:read",
    "ui:panel",
    "ui:ticker",
    "exec:cli",
    "git:read",
    "ui:context-menu",
    "ui:sidebar",
    "ui:file-icons",
    "ui:file-preview",
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
        return Err(format!(
            "main \"{}\" attempts path traversal",
            manifest.main
        ));
    }
    // Validate capabilities
    for cap in &manifest.capabilities {
        if !KNOWN_CAPABILITIES.contains(&cap.as_str()) {
            return Err(format!("unknown capability: \"{cap}\""));
        }
    }
    if manifest.capabilities.iter().any(|cap| cap == "net:http") && manifest.allowed_urls.is_empty()
    {
        return Err("net:http requires a non-empty allowedUrls list".into());
    }
    // Validate declared binaries
    for binary in &manifest.binaries {
        if binary.contains('/') || binary.contains('\\') || binary.contains("..") {
            return Err(format!(
                "binary name \"{binary}\" must not contain path separators"
            ));
        }
        if binary.is_empty() {
            return Err("binary name must not be empty".into());
        }
        if BLOCKED_BINARY_PREFIXES
            .iter()
            .any(|prefix| binary.starts_with(prefix))
        {
            return Err(format!(
                "binary \"{binary}\" is blocked — shell interpreters, scripting runtimes, \
                 and destructive system tools are not permitted"
            ));
        }
    }
    // binaries requires exec:cli capability
    if !manifest.binaries.is_empty() && !manifest.capabilities.contains(&"exec:cli".to_string()) {
        return Err("manifest declares binaries but lacks exec:cli capability".into());
    }
    validate_providers(manifest)
}

/// Accounts are used only through `httpFetch`, so `providers` requires `net:http`.
fn validate_providers(manifest: &PluginManifest) -> Result<(), String> {
    if !manifest.providers.is_empty() && !manifest.capabilities.iter().any(|c| c == "net:http") {
        return Err("manifest declares providers but lacks net:http capability".into());
    }
    let mut seen = std::collections::HashSet::new();
    for entry in &manifest.providers {
        let id = entry.id();
        let core = CORE_ACCOUNT_PROVIDERS.contains(&id);
        if matches!(entry, ManifestProvider::Core(_)) != core {
            return Err(format!(
                "provider \"{id}\" must be a core id or a new declared descriptor"
            ));
        }
        if !seen.insert(id) {
            return Err(format!("provider \"{id}\" is declared twice"));
        }
    }
    Ok(())
}

/// Register the manifest's declared descriptors, owned by the plugin id. All or
/// nothing: a rejected descriptor removes the ones registered before it.
fn register_manifest_providers(manifest: &PluginManifest) -> Result<(), String> {
    for entry in &manifest.providers {
        if let ManifestProvider::Declared(descriptor) = entry
            && let Err(e) = crate::accounts::register_provider(&manifest.id, descriptor.clone())
        {
            crate::accounts::unregister_providers(&manifest.id);
            return Err(format!(
                "Plugin \"{}\" provider \"{}\": {e}",
                manifest.id, descriptor.id
            ));
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

/// Drops a leading `~v<digits>/` load-version segment from a plugin file path.
/// Any other leading segment, including `~v` alone or `~vx`, is a real directory.
fn strip_load_version(file: &str) -> &str {
    let Some(rest) = file.strip_prefix("~v") else {
        return file;
    };
    match rest.split_once('/') {
        Some((digits, tail))
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) =>
        {
            tail
        }
        None if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) => "",
        _ => file,
    }
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

    // The frontend loads a plugin under `/{id}/~v{n}/` so that every relative import
    // of one load shares a URL the WebView has not cached. The segment is not on disk.
    let file = strip_load_version(file);
    if file.is_empty() {
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

#[cfg(feature = "desktop")]
/// Register the `plugin://` custom URI scheme protocol on the Tauri builder.
///
/// Serves JS files from `{config_dir}/plugins/{id}/{file}` with the
/// `application/javascript` MIME type. Rejects path traversal attempts.
///
/// A leading `~v<digits>/` path segment (the frontend's per-load cache-busting
/// version for hot reload, see `strip_load_version`) is accepted and ignored.
pub fn register_plugin_protocol(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.register_uri_scheme_protocol("plugin", |_ctx, request| {
        let uri = request.uri();
        // On macOS/Linux: plugin://my-plugin/main.js → host="my-plugin", path="/main.js"
        // On Windows: http://plugin.localhost/my-plugin/main.js → host="plugin.localhost", path="/my-plugin/main.js"
        let host = uri.host().unwrap_or("");
        let path = uri.path();
        let combined = if host.is_empty() || host.ends_with(".localhost") {
            path.to_string()
        } else {
            format!("/{host}{path}")
        };

        let Some(file_path) = resolve_plugin_path(&combined) else {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "text/plain")
                .header("Access-Control-Allow-Origin", "*")
                .body(b"Invalid plugin path".to_vec())
                .unwrap();
        };

        match std::fs::read(&file_path) {
            Ok(data) => {
                // Determine MIME type from extension
                let mime = plugin_asset_mime(&file_path);

                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", mime)
                    .header("Access-Control-Allow-Origin", "*")
                    .body(data)
                    .unwrap()
            }
            Err(_) => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("Content-Type", "text/plain")
                .header("Access-Control-Allow-Origin", "*")
                .body(b"Plugin file not found".to_vec())
                .unwrap(),
        }
    })
}

fn plugin_asset_mime(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("js" | "mjs") => "application/javascript",
        Some("json") => "application/json",
        Some("css") => "text/css",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Scan the user plugins directory and return all valid manifests.
/// Invalid manifests are logged and skipped — never cause an error.
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn list_user_plugins(plugin_ids: Option<Vec<String>>) -> Vec<PluginManifest> {
    if let Some(plugin_ids) = plugin_ids {
        let mut seen = std::collections::HashSet::new();
        return plugin_ids
            .into_iter()
            .filter(|id| !id.starts_with('.') && seen.insert(id.clone()))
            .filter_map(|id| {
                let mut manifest = read_single_manifest(&id).ok()?;
                manifest.content_digest =
                    crate::plugin_content::digest(&plugins_dir().join(&id)).ok();
                Some(manifest)
            })
            .collect();
    }
    let dir = plugins_dir();
    if !dir.exists() {
        return Vec::new();
    }

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(err) => {
            tracing::warn!(source = "plugins", "Failed to read plugins dir: {err}");
            return Vec::new();
        }
    };

    let mut manifests = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            // Detect broken symlinks and warn (is_dir follows symlinks — false for broken)
            if path.is_symlink() {
                let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                tracing::warn!(
                    source = "plugins", plugin = %dir_name,
                    "Plugin symlink target does not exist — skipping"
                );
            }
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
                tracing::warn!(source = "plugins", plugin = %dir_name, "Failed to read manifest.json: {err}");
                continue;
            }
        };

        let manifest: PluginManifest = match serde_json::from_str(&manifest_data) {
            Ok(m) => m,
            Err(err) => {
                tracing::warn!(source = "plugins", plugin = %dir_name, "Invalid manifest.json: {err}");
                continue;
            }
        };

        if let Err(err) = validate_manifest(&manifest, &dir_name) {
            tracing::warn!(source = "plugins", plugin = %dir_name, "Manifest validation failed: {err}");
            continue;
        }

        let mut manifest = manifest;
        manifest.content_digest = crate::plugin_content::digest(&path).ok();
        manifests.push(manifest);
    }

    manifests
}

// ---------------------------------------------------------------------------
// Plugin capability registration (called by frontend on plugin load)
// ---------------------------------------------------------------------------

/// Register a plugin's capabilities server-side so Rust commands can enforce them.
/// Called by the frontend when a plugin is loaded. Overwrites any previous entry.
///
/// Security: validates the claimed capabilities against the on-disk manifest.
/// The frontend cannot self-register arbitrary capabilities.
#[cfg(feature = "desktop")]
#[tauri::command]
pub fn register_loaded_plugin(
    plugin_id: String,
    capabilities: Vec<String>,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<(), String> {
    register_loaded_plugin_impl(&state, plugin_id, capabilities)
}

pub(crate) fn register_loaded_plugin_impl(
    state: &crate::AppState,
    plugin_id: String,
    capabilities: Vec<String>,
) -> Result<(), String> {
    // Ground truth is the on-disk manifest, not the frontend-supplied capabilities.
    // Read only the specific plugin's manifest instead of scanning the entire dir.
    let manifest = read_single_manifest(&plugin_id)?;

    let settings = crate::plugin_settings::prepare_settings(&manifest, Some(state))?;

    // Only allow capabilities that appear in the manifest
    for cap in &capabilities {
        if !manifest.capabilities.contains(cap) {
            return Err(format!(
                "Plugin \"{plugin_id}\" claims capability \"{cap}\" not declared in manifest"
            ));
        }
    }

    if let Some(settings) = &settings {
        settings
            .migrate()
            .map_err(|_| "Plugin settings migration failed".to_owned())?;
    }

    // A WebView reload re-registers an already-loaded plugin without ever
    // calling uninstall. Dispose its previous runtime state (fs watchers,
    // rate-limiter history) before granting the fresh capability set, or
    // each reload leaks a watcher set and MAX_WATCHERS_PER_PLUGIN caps out
    // after a handful of reloads (#649-56fb). Idempotent: a no-op on first load.
    dispose_plugin_runtime_state(state, &plugin_id);
    register_manifest_providers(&manifest)?;

    if let Some(settings) = settings {
        state.plugin_settings.register(settings);
    }

    state.loaded_plugins.insert(plugin_id, capabilities);
    Ok(())
}

/// Read a single plugin's manifest by ID (avoids scanning the entire plugins dir).
pub(crate) fn read_single_manifest(plugin_id: &str) -> Result<PluginManifest, String> {
    if plugin_id.is_empty() || is_path_escape(plugin_id) {
        return Err(format!("Invalid plugin ID: \"{plugin_id}\""));
    }
    let manifest_path = plugins_dir().join(plugin_id).join("manifest.json");
    let data = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Plugin \"{plugin_id}\" manifest not found: {e}"))?;
    let manifest: PluginManifest = serde_json::from_str(&data)
        .map_err(|e| format!("Plugin \"{plugin_id}\" invalid manifest: {e}"))?;
    validate_manifest(&manifest, plugin_id)?;
    Ok(manifest)
}

/// Unregister a plugin's capabilities when it is unloaded.
#[cfg(feature = "desktop")]
#[tauri::command]
pub fn unregister_loaded_plugin(
    plugin_id: String,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) {
    unregister_loaded_plugin_impl(&state, &plugin_id);
}

pub(crate) fn unregister_loaded_plugin_impl(state: &crate::AppState, plugin_id: &str) {
    state.plugin_http_requests.cancel_plugin(plugin_id);
    state.plugin_settings.unregister(plugin_id);
    state.loaded_plugins.remove(plugin_id);
    crate::accounts::unregister_providers(plugin_id);
}

// ---------------------------------------------------------------------------
// Output watchers
// ---------------------------------------------------------------------------

/// Replace the OutputWatcher set of one frontend. The frontend pushes its full
/// set whenever a watcher is added or removed, tagged with the client id it
/// generated at load and a monotonic sequence number.
///
/// `seq` orders the mutations, not the replies: two syncs can be in flight at
/// once and the older one must not install its set on top of the newer one. A
/// stale sync answers `applied: false` and changes nothing.
///
/// The reply lists the watchers whose pattern the Rust `regex` crate cannot
/// express (lookaround, backreferences, a negated class escape inside a class).
/// Those are NOT an error: the frontend keeps matching them itself, and Rust
/// keeps shipping every assembled line for as long as one is registered.
#[cfg(feature = "desktop")]
#[tauri::command]
pub fn set_plugin_output_watchers(
    client_id: String,
    seq: u64,
    watchers: Vec<crate::output_watchers::WatcherSpec>,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
    webview: tauri::Webview,
) -> crate::output_watchers::SyncOutcome {
    let outcome = state.plugin_output_watchers.write().sync_for_webview(
        webview.label(),
        &client_id,
        seq,
        &watchers,
    );
    log_rejected_output_watchers(&outcome);
    outcome
}

pub(crate) fn set_plugin_output_watchers_impl(
    state: &crate::AppState,
    client_id: &str,
    seq: u64,
    watchers: &[crate::output_watchers::WatcherSpec],
) -> crate::output_watchers::SyncOutcome {
    let outcome = state
        .plugin_output_watchers
        .write()
        .sync(client_id, seq, watchers);
    log_rejected_output_watchers(&outcome);
    outcome
}

fn log_rejected_output_watchers(outcome: &crate::output_watchers::SyncOutcome) {
    if !outcome.rejected.is_empty() {
        tracing::info!(
            source = "plugin",
            "OutputWatcher patterns kept on the frontend (not expressible in the regex crate): {:?}",
            outcome.rejected
        );
    }
}

// ---------------------------------------------------------------------------
// Plugin README
// ---------------------------------------------------------------------------

/// Return the absolute path to a plugin's README.md if it exists.
/// Returns `None` if the file doesn't exist (not an error).
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn get_plugin_readme_path(id: String) -> Option<String> {
    if id.is_empty() || is_path_escape(&id) {
        return None;
    }
    let readme = plugins_dir().join(&id).join("README.md");
    if readme.is_file() {
        readme.to_str().map(String::from)
    } else {
        None
    }
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

#[cfg_attr(feature = "desktop", tauri::command)]
pub fn read_plugin_data(plugin_id: String, path: String) -> Result<Option<String>, String> {
    let file_path = resolve_data_path(&plugin_id, &path)?;
    if file_path == plugin_data_dir(&plugin_id)?.join("config.json") {
        return crate::plugin_settings::read_config(&plugin_id);
    }
    match std::fs::read_to_string(&file_path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Failed to read plugin data: {e}")),
    }
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub fn write_plugin_data(plugin_id: String, path: String, content: String) -> Result<(), String> {
    let file_path = resolve_data_path(&plugin_id, &path)?;
    if file_path == plugin_data_dir(&plugin_id)?.join("config.json") {
        return crate::plugin_settings::write_config(&plugin_id, &content);
    }
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create data directory: {e}"))?;
    }
    std::fs::write(&file_path, &content).map_err(|e| format!("Failed to write plugin data: {e}"))
}

// DESKTOP-ONLY (HTTP parity): no frontend caller
#[cfg_attr(feature = "desktop", tauri::command)]
pub fn delete_plugin_data(plugin_id: String, path: String) -> Result<(), String> {
    let file_path = resolve_data_path(&plugin_id, &path)?;
    if file_path == plugin_data_dir(&plugin_id)?.join("config.json") {
        return Err("Use typed settings reset for plugin configuration".into());
    }
    match std::fs::remove_file(&file_path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Failed to delete plugin data: {e}")),
    }
}

// ---------------------------------------------------------------------------
// ZIP plugin installation
// ---------------------------------------------------------------------------

/// Install a plugin from a ZIP archive.
///
/// Extracts the archive into `{plugins_dir}/{manifest.id}/`, validates the
/// manifest, and emits a `plugin-changed` event. If the plugin already exists,
/// the `data/` subdirectory is preserved across the overwrite.
///
/// Returns the validated manifest on success.
// DESKTOP-ONLY (HTTP parity): takes AppHandle; local-FS install/emit
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn install_plugin_from_zip(
    path: String,
    app_handle: AppHandle,
) -> Result<PluginManifest, String> {
    let zip_path = PathBuf::from(&path);
    if !zip_path.exists() {
        return Err(format!("File not found: {path}"));
    }

    install_zip_inner(&zip_path, &app_handle)
}

/// Install a plugin from an HTTPS URL: download to a temp file, then extract.
// DESKTOP-ONLY (HTTP parity): takes AppHandle; local-FS install/emit
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn install_plugin_from_url(
    url: String,
    app_handle: AppHandle,
) -> Result<PluginManifest, String> {
    // Security: only HTTPS URLs
    if !url.starts_with("https://") {
        return Err("Only HTTPS URLs are allowed for plugin installation".into());
    }

    // Download to temp file
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Download failed: HTTP {}", response.status()));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read download: {e}"))?;

    // A `NamedTempFile` is created 0600 with an unguessable name and removed on
    // drop, so a panic or early return cannot leak the archive into shared /tmp.
    let mut temp_file = tempfile::Builder::new()
        .prefix("tuic-plugin-")
        .suffix(".zip")
        .tempfile()
        .map_err(|e| format!("Failed to create temp file: {e}"))?;
    std::io::Write::write_all(&mut temp_file, &bytes)
        .and_then(|()| std::io::Write::flush(&mut temp_file))
        .map_err(|e| format!("Failed to write temp file: {e}"))?;

    install_zip_inner(temp_file.path(), &app_handle)
}

// ---------------------------------------------------------------------------
// Shared install helpers
// ---------------------------------------------------------------------------

/// Holds the prepared target directory and optional backup of existing data/.
///
/// The backup lives next to the plugin dirs (inside `plugins_dir()`), never in
/// the OS temp dir: `rename` fails with EXDEV across filesystems, which is the
/// normal case on Linux (tmpfs `/tmp` vs. a config dir under `$HOME`).
///
/// If the install is abandoned (any `?` between `prepare_plugin_target` and
/// `finalize_plugin_install`, or a panic), `Drop` moves the backup back so the
/// user's `data/` is never lost.
struct PreparedTarget {
    path: PathBuf,
    data_backup: Option<PathBuf>,
}

impl PreparedTarget {
    /// Move the backed-up `data/` back into the target. Idempotent: a no-op
    /// when there is no backup. On failure the backup is kept for `Drop` to retry.
    fn restore_data(&mut self) -> Result<(), String> {
        let Some(backup) = self.data_backup.as_ref() else {
            return Ok(());
        };
        std::fs::create_dir_all(&self.path)
            .map_err(|e| format!("Failed to recreate plugin dir: {e}"))?;
        let data = self.path.join("data");
        if data.exists() {
            // A partial install must not shadow the user's data.
            std::fs::remove_dir_all(&data)
                .map_err(|e| format!("Failed to clear data/ before restore: {e}"))?;
        }
        std::fs::rename(backup, &data).map_err(|e| format!("Failed to restore data/: {e}"))?;
        self.data_backup = None;
        Ok(())
    }
}

impl Drop for PreparedTarget {
    fn drop(&mut self) {
        if let Err(e) = self.restore_data() {
            tracing::warn!(
                source = "plugins",
                "Could not restore plugin data/ from {:?}: {e}",
                self.data_backup
            );
        }
    }
}

/// Prepare the target plugin directory: backup data/, clean, create fresh dir.
fn prepare_plugin_target(manifest: &PluginManifest) -> Result<PreparedTarget, String> {
    let root = plugins_dir();
    let target_dir = root.join(&manifest.id);

    // Preserve data/ directory if plugin already exists. The backup is a hidden
    // sibling inside plugins_dir() (same filesystem as the source, so rename
    // cannot hit EXDEV); discovery skips dot-directories.
    let data_backup = if target_dir.join("data").exists() {
        let backup = root.join(format!(
            ".tuic-data-backup-{}-{}",
            manifest.id,
            uuid::Uuid::new_v4()
        ));
        std::fs::rename(target_dir.join("data"), &backup)
            .map_err(|e| format!("Failed to backup data/: {e}"))?;
        Some(backup)
    } else {
        None
    };

    // From here on the guard restores data/ if anything below fails.
    let prepared = PreparedTarget {
        path: target_dir.clone(),
        data_backup,
    };

    // Clean target
    if target_dir.exists() {
        std::fs::remove_dir_all(&target_dir)
            .map_err(|e| format!("Failed to clean existing plugin dir: {e}"))?;
    }
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Failed to create plugin dir: {e}"))?;

    Ok(prepared)
}

#[cfg(feature = "desktop")]
/// Restore data/ backup, emit plugin-changed event, log success.
fn finalize_plugin_install(
    mut target: PreparedTarget,
    manifest: &PluginManifest,
    app_handle: &AppHandle,
) -> Result<PluginManifest, String> {
    target.restore_data()?;

    let _ = app_handle.emit("plugin-changed", vec![manifest.id.clone()]);
    crate::app_logger::log_via_handle(
        app_handle,
        "info",
        "plugin",
        &format!("Installed plugin \"{}\" v{}", manifest.id, manifest.version),
    );
    let mut installed = manifest.clone();
    installed.content_digest = crate::plugin_content::digest(&target.path).ok();
    Ok(installed)
}

/// Recursively copy a directory, skipping the `data/` subdirectory (preserved separately).
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(src).map_err(|e| format!("Failed to read dir: {e}"))? {
        let entry = entry.map_err(|e| format!("Dir entry error: {e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("File type error: {e}"))?;
        let name = entry.file_name();

        // Skip data/ — it's handled by the backup/restore flow
        if name == "data" && file_type.is_dir() {
            continue;
        }

        let src_path = entry.path();
        let dst_path = dst.join(&name);

        if file_type.is_dir() {
            std::fs::create_dir_all(&dst_path)
                .map_err(|e| format!("Failed to create dir {}: {e}", dst_path.display()))?;
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)
                .map_err(|e| format!("Failed to copy {}: {e}", name.to_string_lossy()))?;
        }
    }
    Ok(())
}

/// Install a plugin from a local folder. Validates manifest, copies files to
/// the plugins directory, preserves existing data/, and emits a reload event.
// DESKTOP-ONLY (HTTP parity): takes AppHandle; local-FS install/emit
#[cfg(feature = "desktop")]
#[tauri::command]
pub async fn install_plugin_from_folder(
    path: String,
    app_handle: AppHandle,
) -> Result<PluginManifest, String> {
    let src_dir = PathBuf::from(&path);
    if !src_dir.is_dir() {
        return Err(format!("Not a directory: {path}"));
    }

    let manifest_path = src_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err("No manifest.json found in the selected folder".to_string());
    }

    let manifest_data = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Failed to read manifest.json: {e}"))?;
    let manifest: PluginManifest =
        serde_json::from_str(&manifest_data).map_err(|e| format!("Invalid manifest.json: {e}"))?;

    let dir_name = src_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&manifest.id);
    validate_manifest(&manifest, dir_name)?;

    let target = prepare_plugin_target(&manifest)?;
    copy_dir_recursive(&src_dir, &target.path)?;

    finalize_plugin_install(target, &manifest, &app_handle)
}

#[cfg(feature = "desktop")]
/// Core ZIP extraction logic shared by install_plugin_from_zip and install_plugin_from_url.
fn install_zip_inner(
    zip_path: &std::path::Path,
    app_handle: &AppHandle,
) -> Result<PluginManifest, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("Failed to open ZIP: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Invalid ZIP archive: {e}"))?;

    // Find manifest.json — may be at root or inside a single top-level dir
    let manifest_path = find_manifest_in_zip(&archive)?;
    let prefix = manifest_path
        .strip_suffix("manifest.json")
        .unwrap_or("")
        .to_string();

    // Read and validate manifest
    let manifest_data = {
        let mut entry = archive
            .by_name(&manifest_path)
            .map_err(|e| format!("Failed to read manifest.json: {e}"))?;
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut entry, &mut buf)
            .map_err(|e| format!("Failed to read manifest.json: {e}"))?;
        buf
    };

    let manifest: PluginManifest =
        serde_json::from_str(&manifest_data).map_err(|e| format!("Invalid manifest.json: {e}"))?;

    validate_manifest(&manifest, &manifest.id)?;

    let target_dir = prepare_plugin_target(&manifest)?;

    extract_zip_entries(&mut archive, &prefix, &target_dir.path)?;

    finalize_plugin_install(target_dir, &manifest, app_handle)
}

/// Extract validated plugin entries into an already prepared target directory.
/// This seam deliberately excludes Tauri state so archive safety and prefix
/// handling can be characterized without installing or activating a plugin.
fn extract_zip_entries(
    archive: &mut zip::ZipArchive<std::fs::File>,
    prefix: &str,
    target_dir: &std::path::Path,
) -> Result<(), String> {
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("ZIP entry error: {e}"))?;

        let entry_path = entry
            .enclosed_name()
            .ok_or_else(|| "ZIP entry has unsafe path".to_string())?
            .to_path_buf();

        // `enclosed_name` hands back a host path, and Windows renders it with
        // `\`. The prefix comes from the archive's own entry names, which ZIP
        // always spells with `/`, so on Windows the strip below never matched:
        // every entry was skipped and a plugin installed into an empty
        // directory without an error. Windows accepts `/` in the paths it is
        // given, so normalising here is enough.
        let entry_str = entry_path.to_string_lossy().replace('\\', "/");

        // Strip prefix (if manifest was inside a subdirectory)
        let relative = if !prefix.is_empty() {
            match entry_str.strip_prefix(prefix) {
                Some(r) if !r.is_empty() => r.to_string(),
                _ => continue,
            }
        } else {
            entry_str.to_string()
        };

        // Zip-slip protection
        if is_path_escape(&relative) {
            return Err(format!("ZIP entry attempts path traversal: {relative}"));
        }

        let target = target_dir.join(&relative);

        if entry.is_dir() {
            std::fs::create_dir_all(&target)
                .map_err(|e| format!("Failed to create dir {relative}: {e}"))?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create parent dir: {e}"))?;
            }
            let mut outfile = std::fs::File::create(&target)
                .map_err(|e| format!("Failed to create {relative}: {e}"))?;
            std::io::copy(&mut entry, &mut outfile)
                .map_err(|e| format!("Failed to write {relative}: {e}"))?;
        }
    }
    Ok(())
}

/// Find the path to manifest.json inside a ZIP archive.
/// It may be at the root or inside a single top-level directory.
fn find_manifest_in_zip(archive: &zip::ZipArchive<std::fs::File>) -> Result<String, String> {
    // Try root-level first
    for i in 0..archive.len() {
        let name = archive
            .name_for_index(i)
            .and_then(Result::ok)
            .unwrap_or_default();
        if name == "manifest.json" {
            return Ok("manifest.json".to_string());
        }
    }

    // Try single top-level dir: <dirname>/manifest.json
    let mut top_dirs = std::collections::HashSet::new();
    for i in 0..archive.len() {
        let name = archive
            .name_for_index(i)
            .and_then(Result::ok)
            .unwrap_or_default();
        if let Some(slash) = name.find('/') {
            top_dirs.insert(name[..=slash].to_string());
        }
    }

    if top_dirs.len() == 1 {
        let dir = top_dirs.into_iter().next().unwrap();
        let candidate = format!("{dir}manifest.json");
        for i in 0..archive.len() {
            if archive
                .name_for_index(i)
                .and_then(Result::ok)
                .is_some_and(|name| name == candidate)
            {
                return Ok(candidate);
            }
        }
    }

    Err("manifest.json not found in ZIP archive".into())
}

// ---------------------------------------------------------------------------
// Plugin uninstall
// ---------------------------------------------------------------------------

/// Dispose of a plugin's live runtime state on uninstall. Without this the
/// process leaks, for its whole lifetime: an OS thread + fs watcher per
/// registered watch (the `debounce_loop`), the capability grant, and the
/// exec:cli rate-limiter history.
pub(crate) fn dispose_plugin_runtime_state(state: &crate::AppState, id: &str) {
    state.plugin_settings.unregister(id);
    // Drop every watcher this plugin registered. Dropping the `RecommendedWatcher`
    // closes its notify channel, so the matching `debounce_loop` thread exits.
    state.plugin_watchers.retain(|_, (pid, _)| pid != id);
    // Revoke the capability grant so a re-install starts clean.
    state.loaded_plugins.remove(id);
    // Clear the exec:cli sliding-window rate-limiter entry for this id.
    crate::plugin_exec::clear_rate_limit(id);
    // Descriptors leave with the plugin; accounts and their credentials stay.
    crate::accounts::unregister_providers(id);
}

/// Remove a plugin and all its files (including data/).
// DESKTOP-ONLY (HTTP parity): takes AppHandle; local-FS install/emit
#[cfg(feature = "desktop")]
#[tauri::command]
pub fn uninstall_plugin(
    id: String,
    app_handle: AppHandle,
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<(), String> {
    uninstall_plugin_inner(&state, &id)?;

    let _ = app_handle.emit("plugin-changed", vec![id.clone()]);
    crate::app_logger::log_via_handle(
        &app_handle,
        "info",
        "plugin",
        &format!("Uninstalled plugin \"{id}\""),
    );
    Ok(())
}

/// Core uninstall, separated from the Tauri command wrapper for testability.
///
/// Runtime disposal happens FIRST and unconditionally. It used to run after the
/// `?` on `remove_dir_all`, so a delete that failed (a file held open, a
/// permissions problem) left the plugin's watchers and capability grant alive for
/// a plugin the user had been told was gone — and every retry hit the same early
/// return, so it never recovered. Disposal is idempotent and independent of the
/// directory, so nothing is lost by doing it before the step that can fail.
fn uninstall_plugin_inner(state: &crate::AppState, id: &str) -> Result<(), String> {
    if id.is_empty() || is_path_escape(id) {
        return Err("Invalid plugin ID".into());
    }

    dispose_plugin_runtime_state(state, id);

    let dir = plugins_dir().join(id);
    if !dir.exists() {
        return Ok(()); // Already gone
    }

    std::fs::remove_dir_all(&dir).map_err(|e| format!("Failed to remove plugin directory: {e}"))
}

// ---------------------------------------------------------------------------
// Plugin directory watcher (hot reload)
// ---------------------------------------------------------------------------

#[cfg(feature = "desktop")]
/// Check if a changed path within a plugin dir is source code (not runtime data).
/// Plugin data is stored under `<plugin_id>/data/` — changes there must not
/// trigger hot-reload (the plugin itself writes stats/config at runtime).
fn is_plugin_code_change(relative: &std::path::Path) -> bool {
    let components: Vec<_> = relative.components().collect();
    // Need at least plugin_id + filename
    if components.len() < 2 {
        return false;
    }
    // Skip anything under <plugin_id>/data/
    if components.len() > 2 && components[1].as_os_str() == "data" {
        return false;
    }
    // Only reload for code files
    matches!(
        relative.extension().and_then(|e| e.to_str()),
        Some("js" | "mjs" | "json")
    )
}

#[cfg(feature = "desktop")]
/// Decide whether a watcher-event path should trigger (re)discovery, returning
/// the top-level plugin id if so.
///
/// Two cases produce an id:
/// 1. A code file (`.js`/`.mjs`/`.json`, not under `data/`) inside a plugin dir
///    — the existing hot-reload path for edits to already-loaded plugins.
/// 2. A brand-new top-level plugin **directory or symlink** created after boot.
///    `notify`'s recursive watch does not descend into symlink targets, so a
///    plugin added via `ln -s` never emits events for its inner `manifest.json`;
///    the only signal is the single-component create of the link/dir itself.
///    We surface that as the id so the frontend re-scans and loads the plugin.
///
/// Hidden entries (dotfiles) never match.
fn plugin_id_for_change(path: &std::path::Path, dir: &std::path::Path) -> Option<String> {
    let relative = path.strip_prefix(dir).ok()?;
    let first = relative.components().next()?;
    let id = first.as_os_str().to_string_lossy().to_string();
    if id.starts_with('.') {
        return None;
    }

    // Nested path (plugin_id/file/...) — only code changes trigger.
    if relative.components().count() >= 2 {
        return is_plugin_code_change(relative).then_some(id);
    }

    // Single component = a top-level entry in the plugins dir. Only treat new
    // directories/symlinks as plugins; ignore stray top-level files.
    (path.is_dir() || path.is_symlink()).then_some(id)
}

#[cfg(feature = "desktop")]
pub fn start_plugin_watcher(app_handle: &AppHandle) {
    let dir = plugins_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        crate::app_logger::log_via_handle(
            app_handle,
            "error",
            "plugin",
            &format!("[plugins] Failed to create plugins dir: {e}"),
        );
        return;
    }

    let handle = app_handle.clone();
    std::thread::spawn(move || {
        use notify::{RecursiveMode, Watcher};
        use std::time::Duration;

        let (tx, rx) = std::sync::mpsc::channel();

        let mut watcher = match notify::recommended_watcher(
            move |result: Result<notify::Event, notify::Error>| {
                let _ = tx.send(result);
            },
        ) {
            Ok(w) => w,
            Err(e) => {
                crate::app_logger::log_via_handle(
                    &handle,
                    "error",
                    "plugin",
                    &format!("[plugins] Failed to create watcher: {e}"),
                );
                return;
            }
        };

        if let Err(e) = watcher.watch(&dir, RecursiveMode::Recursive) {
            crate::app_logger::log_via_handle(
                &handle,
                "error",
                "plugin",
                &format!("[plugins] Failed to watch plugins dir: {e}"),
            );
            return;
        }

        crate::app_logger::log_via_handle(
            &handle,
            "info",
            "plugin",
            &format!("[plugins] Watching {dir:?} for changes"),
        );

        let debounce = Duration::from_millis(500);
        let mut changed_ids: Vec<String> = Vec::new();

        let collect_changes =
            |event: &notify::Event, dir: &std::path::Path, ids: &mut Vec<String>| {
                if !matches!(
                    event.kind,
                    notify::EventKind::Create(_)
                        | notify::EventKind::Modify(notify::event::ModifyKind::Data(_))
                        | notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
                        | notify::EventKind::Remove(_)
                ) {
                    return;
                }
                for path in &event.paths {
                    if let Some(id) = plugin_id_for_change(path, dir)
                        && !ids.contains(&id)
                    {
                        ids.push(id);
                    }
                }
            };

        loop {
            let event = match rx.recv() {
                Ok(Ok(e)) => e,
                Ok(Err(err)) => {
                    crate::app_logger::log_via_handle(
                        &handle,
                        "warn",
                        "plugin",
                        &format!("[plugins] Watcher error: {err}"),
                    );
                    continue;
                }
                Err(_) => break,
            };

            collect_changes(&event, &dir, &mut changed_ids);

            while let Ok(result) = rx.recv_timeout(debounce) {
                if let Ok(event) = result {
                    collect_changes(&event, &dir, &mut changed_ids);
                }
            }

            if !changed_ids.is_empty() {
                crate::app_logger::log_via_handle(
                    &handle,
                    "info",
                    "plugin",
                    &format!("[plugins] Change detected in: {changed_ids:?}"),
                );
                let _ = handle.emit("plugin-changed", changed_ids.clone());
                changed_ids.clear();
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

    // -- Output watcher registration (#599-6e94) --

    use crate::output_watchers::WatcherSpec;

    fn spec(id: &str, pattern: &str) -> WatcherSpec {
        WatcherSpec {
            id: id.to_string(),
            pattern: pattern.to_string(),
            flags: String::new(),
        }
    }

    /// With every pattern compiled in Rust, nothing in the WebView has to scan
    /// lines any more: only the matched ones cross the boundary.
    #[test]
    fn compiling_every_pattern_stops_shipping_every_line() {
        let state = crate::state::tests_support::make_test_app_state();
        assert!(
            state.plugin_output_watchers.read().is_idle(),
            "a fresh state has no watcher to match"
        );

        let outcome =
            set_plugin_output_watchers_impl(&state, "c1", 1, &[spec("a", "done"), spec("b", "x+")]);

        assert!(outcome.applied);
        assert!(outcome.rejected.is_empty());
        let watchers = state.plugin_output_watchers.read();
        assert!(!watchers.needs_all_lines());
        assert!(!watchers.is_idle());
    }

    /// One pattern the crate cannot express keeps every line flowing — that
    /// watcher is still matched in the WebView and must not go blind.
    #[test]
    fn a_rejected_pattern_keeps_every_line_flowing() {
        let state = crate::state::tests_support::make_test_app_state();

        let outcome = set_plugin_output_watchers_impl(
            &state,
            "c1",
            1,
            &[spec("ok", "done"), spec("look", "a(?=b)")],
        );

        assert_eq!(outcome.rejected, vec!["look".to_string()]);
        let watchers = state.plugin_output_watchers.read();
        assert!(watchers.needs_all_lines());
        // The compilable one is still matched in Rust.
        assert_eq!(watchers.matching_ids("done"), vec!["c1/ok".to_string()]);
    }

    /// The frontend pushes the whole set every time, so a sync replaces rather
    /// than accumulates — otherwise a disposed watcher would keep firing.
    #[test]
    fn a_later_sync_replaces_the_previous_set() {
        let state = crate::state::tests_support::make_test_app_state();
        set_plugin_output_watchers_impl(&state, "c1", 1, &[spec("old", "gone")]);

        set_plugin_output_watchers_impl(&state, "c1", 2, &[spec("new", "kept")]);

        let watchers = state.plugin_output_watchers.read();
        assert!(watchers.matching_ids("gone").is_empty());
        assert_eq!(watchers.matching_ids("kept"), vec!["c1/new".to_string()]);
    }

    /// A sync that lost the race must not install its older set: the frontend
    /// would keep a watcher the backend no longer knows about, and nothing
    /// would match it.
    #[test]
    fn a_stale_sync_changes_nothing() {
        let state = crate::state::tests_support::make_test_app_state();
        set_plugin_output_watchers_impl(&state, "c1", 2, &[spec("a", "kept"), spec("b", "also")]);

        let outcome = set_plugin_output_watchers_impl(&state, "c1", 1, &[spec("a", "kept")]);

        assert!(!outcome.applied);
        assert_eq!(
            state.plugin_output_watchers.read().matching_ids("also"),
            vec!["c1/b".to_string()]
        );
    }

    /// Unregistering the last watcher must also drop the set, or the reader
    /// keeps paying for matching nobody asked for.
    #[test]
    fn an_empty_sync_clears_the_set() {
        let state = crate::state::tests_support::make_test_app_state();
        set_plugin_output_watchers_impl(&state, "c1", 1, &[spec("a", "done")]);

        let outcome = set_plugin_output_watchers_impl(&state, "c1", 2, &[]);

        assert!(outcome.applied);
        assert!(outcome.rejected.is_empty());
        assert!(state.plugin_output_watchers.read().is_idle());
    }

    /// Two frontends (desktop window and browser tab) keep independent sets:
    /// one registering must not blind the other.
    #[test]
    fn a_second_client_does_not_replace_the_first() {
        let state = crate::state::tests_support::make_test_app_state();
        set_plugin_output_watchers_impl(&state, "desktop", 1, &[spec("w1", "alpha")]);

        set_plugin_output_watchers_impl(&state, "browser", 1, &[spec("w1", "beta")]);

        let watchers = state.plugin_output_watchers.read();
        assert_eq!(
            watchers.matching_ids("alpha"),
            vec!["desktop/w1".to_string()]
        );
        assert_eq!(
            watchers.matching_ids("beta"),
            vec!["browser/w1".to_string()]
        );
    }

    // -- Uninstall disposal (#485-1bd0) --

    /// A failed directory removal used to skip `dispose_plugin_runtime_state`
    /// entirely, leaving watchers and the capability grant alive for a plugin the
    /// user was told was gone — and every retry hit the same early return.
    #[cfg(unix)]
    #[test]
    fn a_failed_uninstall_still_disposes_the_runtime_state() {
        use std::os::unix::fs::PermissionsExt;

        let home = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(home.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();
        state
            .loaded_plugins
            .insert("doomed".to_string(), vec!["fs:read".to_string()]);

        let dir = plugins_dir().join("doomed");
        std::fs::create_dir_all(dir.join("nested")).expect("create plugin dir");
        // A read-only parent makes remove_dir_all fail without any exotic setup.
        let parent = plugins_dir();
        let original = std::fs::metadata(&parent).expect("stat").permissions();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o555))
            .expect("make parent read-only");

        let result = uninstall_plugin_inner(&state, "doomed");

        // Restore before asserting so a failure cannot leave the tempdir undeletable.
        std::fs::set_permissions(&parent, original).expect("restore permissions");

        assert!(result.is_err(), "the removal was supposed to fail");
        assert!(
            !state.loaded_plugins.contains_key("doomed"),
            "the capability grant must be revoked even when the delete fails"
        );
    }

    #[test]
    fn uninstalling_an_absent_plugin_still_sweeps_its_runtime_state() {
        let home = tempfile::tempdir().expect("tempdir");
        let _guard = crate::config::set_config_dir_override(home.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();
        state
            .loaded_plugins
            .insert("ghost".to_string(), vec!["fs:read".to_string()]);

        uninstall_plugin_inner(&state, "ghost").expect("absent plugin is not an error");

        assert!(!state.loaded_plugins.contains_key("ghost"));
    }

    #[test]
    fn uninstall_rejects_a_path_escaping_id_before_touching_anything() {
        let state = crate::state::tests_support::make_test_app_state();
        assert!(uninstall_plugin_inner(&state, "../etc").is_err());
        assert!(uninstall_plugin_inner(&state, "").is_err());
    }

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

    #[test]
    fn resolve_strips_the_load_version_segment() {
        // The frontend loads a whole plugin under `/{id}/~v{n}/`, so relative imports
        // inherit the version. The file on disk has no such directory.
        let versioned = resolve_plugin_path("/my-plugin/~v1791828000000/provider.js").unwrap();
        assert_eq!(
            versioned,
            resolve_plugin_path("/my-plugin/provider.js").unwrap()
        );
        let nested = resolve_plugin_path("/my-plugin/~v7/lib/model.js").unwrap();
        assert!(nested.ends_with("my-plugin/lib/model.js"));
    }

    #[test]
    fn resolve_versioned_path_still_rejects_traversal_and_empty_files() {
        assert!(resolve_plugin_path("/my-plugin/~v1/../../secret").is_none());
        assert!(resolve_plugin_path("/my-plugin/~v1/").is_none());
        assert!(resolve_plugin_path("/my-plugin/~v1").is_none());
    }

    #[test]
    fn resolve_keeps_a_directory_that_only_looks_like_a_version() {
        // Only `~v<digits>` is a load version; any other name is a real directory.
        assert!(
            resolve_plugin_path("/my-plugin/~vx/a.js")
                .unwrap()
                .ends_with("my-plugin/~vx/a.js")
        );
        assert!(
            resolve_plugin_path("/my-plugin/~v/a.js")
                .unwrap()
                .ends_with("my-plugin/~v/a.js")
        );
    }

    #[test]
    fn plugin_protocol_serves_webassembly_with_its_standard_mime_type() {
        assert_eq!(
            plugin_asset_mime(Path::new("sqlite-viewer/vendor/sql-wasm.wasm")),
            "application/wasm"
        );
        assert_eq!(
            plugin_asset_mime(Path::new("sqlite-viewer/vendor/sql-wasm.js")),
            "application/javascript"
        );
    }

    // -- Manifest validation --

    fn valid_manifest(dir_name: &str) -> PluginManifest {
        PluginManifest {
            id: dir_name.to_string(),
            name: "Test Plugin".to_string(),
            version: "1.0.0".to_string(),
            content_digest: None,
            min_app_version: "0.3.0".to_string(),
            main: "main.js".to_string(),
            description: None,
            author: None,
            capabilities: vec![],
            allowed_urls: vec![],
            agent_types: vec![],
            binaries: vec![],
            providers: vec![],
            settings: None,
        }
    }

    #[test]
    fn loading_http_manifest_without_allowed_urls_fails() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let plugin_dir = dir.path().join("plugins/http-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        let mut manifest = valid_manifest("http-plugin");
        manifest.capabilities = vec!["net:http".into()];
        let write_manifest = |manifest: &PluginManifest| {
            std::fs::write(
                plugin_dir.join("manifest.json"),
                serde_json::to_vec(manifest).unwrap(),
            )
            .unwrap();
        };
        write_manifest(&manifest);
        let error = read_single_manifest("http-plugin").unwrap_err();
        assert!(error.contains("allowedUrls"), "unexpected error: {error}");
        manifest.allowed_urls = vec!["https://api.example.com/*".into()];
        write_manifest(&manifest);
        assert!(read_single_manifest("http-plugin").is_ok());
        manifest.capabilities.clear();
        manifest.allowed_urls.clear();
        write_manifest(&manifest);
        assert!(read_single_manifest("http-plugin").is_ok());
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

    #[test]
    fn validate_accepts_build_cleaner_capabilities() {
        // fs:scan (read-only) and fs:delete (destructive) are the build-cleaner
        // plugin's capabilities — must be in KNOWN_CAPABILITIES so an external
        // manifest declaring them loads instead of being rejected as unknown.
        let mut m = valid_manifest("build-cleaner");
        m.capabilities = vec![
            "fs:scan".into(),
            "fs:delete".into(),
            "ui:panel".into(),
            "ui:ticker".into(),
        ];
        assert!(validate_manifest(&m, "build-cleaner").is_ok());
    }

    // -- Binary validation --

    #[test]
    fn validate_rejects_blocked_binary_bash() {
        let mut m = valid_manifest("test");
        m.capabilities = vec!["exec:cli".into()];
        m.binaries = vec!["bash".into()];
        let err = validate_manifest(&m, "test").unwrap_err();
        assert!(err.contains("blocked"), "should reject bash: {err}");
    }

    #[test]
    fn validate_rejects_blocked_binary_python3() {
        let mut m = valid_manifest("test");
        m.capabilities = vec!["exec:cli".into()];
        m.binaries = vec!["python3".into()];
        assert!(validate_manifest(&m, "test").is_err());
    }

    #[test]
    fn validate_rejects_blocked_binary_curl() {
        let mut m = valid_manifest("test");
        m.capabilities = vec!["exec:cli".into()];
        m.binaries = vec!["curl".into()];
        assert!(validate_manifest(&m, "test").is_err());
    }

    #[test]
    fn validate_rejects_binary_with_path_separator() {
        let mut m = valid_manifest("test");
        m.capabilities = vec!["exec:cli".into()];
        m.binaries = vec!["../../bin/bash".into()];
        let err = validate_manifest(&m, "test").unwrap_err();
        assert!(
            err.contains("path separator"),
            "should reject path traversal: {err}"
        );
    }

    #[test]
    fn validate_rejects_empty_binary_name() {
        let mut m = valid_manifest("test");
        m.capabilities = vec!["exec:cli".into()];
        m.binaries = vec![String::new()];
        assert!(validate_manifest(&m, "test").is_err());
    }

    #[test]
    fn validate_accepts_legitimate_binary() {
        let mut m = valid_manifest("test");
        m.capabilities = vec!["exec:cli".into()];
        m.binaries = vec!["mdkb".into(), "rtk".into()];
        assert!(validate_manifest(&m, "test").is_ok());
    }

    #[test]
    fn validate_rejects_binaries_without_exec_cli_capability() {
        let mut m = valid_manifest("test");
        m.binaries = vec!["mdkb".into()];
        let err = validate_manifest(&m, "test").unwrap_err();
        assert!(err.contains("exec:cli"), "should require exec:cli: {err}");
    }

    // -- list_user_plugins with temp dir --

    #[test]
    fn list_returns_empty_when_no_dir() {
        // plugins_dir() points to the real config; we test the scanning logic
        // indirectly through validate_manifest above. The list function itself
        // just returns an empty vec when the dir doesn't exist.
        // We can't easily override plugins_dir() in tests, but we can verify
        // the function doesn't panic
        let _ = list_user_plugins(None);
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

    // -- ZIP installation helpers --

    /// Create a ZIP archive in memory with given files and return the path.
    fn create_test_zip(dir: &std::path::Path, files: &[(&str, &str)]) -> PathBuf {
        use std::io::Write;
        let zip_path = dir.join("test.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, content) in files {
            zip_writer.start_file(*name, options).unwrap();
            zip_writer.write_all(content.as_bytes()).unwrap();
        }
        zip_writer.finish().unwrap();
        zip_path
    }

    fn valid_manifest_json(id: &str) -> String {
        format!(
            r#"{{"id":"{}","name":"Test","version":"1.0.0","minAppVersion":"0.1.0","main":"main.js","capabilities":[]}}"#,
            id
        )
    }

    #[test]
    fn find_manifest_root_level() {
        let dir = tempfile::TempDir::new().unwrap();
        let zip_path = create_test_zip(
            dir.path(),
            &[
                ("manifest.json", &valid_manifest_json("test")),
                ("main.js", "export default {}"),
            ],
        );
        let file = std::fs::File::open(&zip_path).unwrap();
        let archive = zip::ZipArchive::new(file).unwrap();
        let result = find_manifest_in_zip(&archive);
        assert_eq!(result.unwrap(), "manifest.json");
    }

    #[test]
    fn find_manifest_in_subdirectory() {
        let dir = tempfile::TempDir::new().unwrap();
        let zip_path = create_test_zip(
            dir.path(),
            &[
                ("my-plugin/manifest.json", &valid_manifest_json("my-plugin")),
                ("my-plugin/main.js", "export default {}"),
            ],
        );
        let file = std::fs::File::open(&zip_path).unwrap();
        let archive = zip::ZipArchive::new(file).unwrap();
        let result = find_manifest_in_zip(&archive);
        assert_eq!(result.unwrap(), "my-plugin/manifest.json");
    }

    #[test]
    fn find_manifest_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let zip_path = create_test_zip(dir.path(), &[("main.js", "export default {}")]);
        let file = std::fs::File::open(&zip_path).unwrap();
        let archive = zip::ZipArchive::new(file).unwrap();
        let result = find_manifest_in_zip(&archive);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn extract_zip_entries_strips_single_directory_prefix() {
        let dir = tempfile::TempDir::new().unwrap();
        let zip_path = create_test_zip(
            dir.path(),
            &[
                ("my-plugin/manifest.json", &valid_manifest_json("my-plugin")),
                ("my-plugin/main.js", "export default {}"),
                ("my-plugin/assets/icon.txt", "icon"),
                ("unrelated.txt", "ignored"),
            ],
        );
        let file = std::fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let target = dir.path().join("out");
        std::fs::create_dir(&target).unwrap();

        extract_zip_entries(&mut archive, "my-plugin/", &target).unwrap();

        assert!(target.join("manifest.json").is_file());
        assert!(target.join("main.js").is_file());
        assert_eq!(
            std::fs::read_to_string(target.join("assets/icon.txt")).unwrap(),
            "icon"
        );
        assert!(!target.join("unrelated.txt").exists());
    }

    #[test]
    fn extract_zip_entries_rejects_parent_traversal() {
        let dir = tempfile::TempDir::new().unwrap();
        let zip_path = create_test_zip(dir.path(), &[("../escape.txt", "escape")]);
        let file = std::fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let target = dir.path().join("out");
        std::fs::create_dir(&target).unwrap();

        let error = extract_zip_entries(&mut archive, "", &target)
            .expect_err("parent traversal must be rejected");

        assert!(error.contains("unsafe path") || error.contains("path traversal"));
        assert!(!dir.path().join("escape.txt").exists());
    }

    // -- Capability enforcement --

    #[test]
    fn check_capability_passes_with_valid_capability() {
        let plugins = dashmap::DashMap::new();
        plugins.insert(
            "test-plugin".to_string(),
            vec!["fs:read".to_string(), "net:http".to_string()],
        );
        assert!(check_plugin_capability_inner(&plugins, "test-plugin", "fs:read").is_ok());
        assert!(check_plugin_capability_inner(&plugins, "test-plugin", "net:http").is_ok());
    }

    #[test]
    fn check_capability_rejects_missing_capability() {
        let plugins = dashmap::DashMap::new();
        plugins.insert("test-plugin".to_string(), vec!["fs:read".to_string()]);
        let result = check_plugin_capability_inner(&plugins, "test-plugin", "exec:cli");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("test-plugin"),
            "Error should mention plugin ID"
        );
        assert!(
            err.contains("exec:cli"),
            "Error should mention missing capability"
        );
    }

    #[test]
    fn check_capability_rejects_unknown_plugin() {
        let plugins = dashmap::DashMap::new();
        let result = check_plugin_capability_inner(&plugins, "nonexistent", "fs:read");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("nonexistent"),
            "Error should mention unknown plugin ID"
        );
    }

    #[test]
    fn removed_builtin_plugin_has_no_implicit_capabilities() {
        let plugins = dashmap::DashMap::new();
        let result = check_plugin_capability_inner(&plugins, "plan", "fs:read");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not registered"));
    }

    #[test]
    fn seeds_externalized_builtins_once_and_keeps_uninstall_permanent() {
        let dir = tempfile::TempDir::new().unwrap();
        seed_externalized_builtin_plugins(dir.path()).unwrap();

        for id in ["plan", "stories-ticker"] {
            let plugin = dir.path().join("plugins").join(id);
            assert!(plugin.join("manifest.json").is_file());
            assert!(plugin.join("main.js").is_file());
            assert!(plugin.join("README.md").is_file());
        }
        assert!(dir.path().join(EXTERNALIZED_BUILTINS_MARKER).is_file());

        std::fs::remove_dir_all(dir.path().join("plugins/plan")).unwrap();
        seed_externalized_builtin_plugins(dir.path()).unwrap();
        assert!(!dir.path().join("plugins/plan").exists());
    }

    #[test]
    fn seeding_preserves_an_existing_external_plugin() {
        let dir = tempfile::TempDir::new().unwrap();
        let plan = dir.path().join("plugins/plan");
        std::fs::create_dir_all(&plan).unwrap();
        std::fs::write(plan.join("main.js"), "user version").unwrap();

        seed_externalized_builtin_plugins(dir.path()).unwrap();

        assert_eq!(
            std::fs::read_to_string(plan.join("main.js")).unwrap(),
            "user version"
        );
        assert!(dir.path().join("plugins/stories-ticker/main.js").is_file());
    }

    // -- is_plugin_code_change --

    #[cfg(feature = "desktop")]
    mod plugin_code_change {
        use super::super::{is_plugin_code_change, plugin_id_for_change};
        use std::path::Path;

        #[test]
        fn main_js_is_code() {
            assert!(is_plugin_code_change(Path::new("my-plugin/main.js")));
        }

        #[test]
        fn mjs_is_code() {
            assert!(is_plugin_code_change(Path::new("my-plugin/index.mjs")));
        }

        #[test]
        fn json_is_code() {
            assert!(is_plugin_code_change(Path::new("my-plugin/manifest.json")));
        }

        #[test]
        fn data_stats_json_is_not_code() {
            // Under <plugin_id>/data/ — runtime data, must NOT trigger hot-reload
            assert!(!is_plugin_code_change(Path::new(
                "my-plugin/data/stats.json"
            )));
        }

        #[test]
        fn data_subdir_any_file_is_not_code() {
            assert!(!is_plugin_code_change(Path::new(
                "my-plugin/data/sub/counts.json"
            )));
        }

        #[test]
        fn image_png_is_not_code() {
            assert!(!is_plugin_code_change(Path::new("my-plugin/image.png")));
        }

        #[test]
        fn css_file_is_not_code() {
            // CSS is not in the allowed set — no hot-reload for stylesheets
            assert!(!is_plugin_code_change(Path::new("my-plugin/styles.css")));
        }

        #[test]
        fn bare_plugin_id_no_filename_is_not_code() {
            // Only one component — fails the len < 2 guard
            assert!(!is_plugin_code_change(Path::new("my-plugin")));
        }

        #[test]
        fn empty_path_is_not_code() {
            assert!(!is_plugin_code_change(Path::new("")));
        }

        #[test]
        fn nested_js_not_in_data_is_code() {
            // dist/ subfolder is code, not data
            assert!(is_plugin_code_change(Path::new("my-plugin/dist/bundle.js")));
        }

        // -- plugin_id_for_change: the top-level dir/symlink discovery path --

        #[test]
        fn change_id_for_code_file_inside_plugin() {
            let dir = tempfile::TempDir::new().unwrap();
            let path = dir.path().join("my-plugin").join("main.js");
            assert_eq!(
                plugin_id_for_change(&path, dir.path()),
                Some("my-plugin".to_string())
            );
        }

        #[test]
        fn change_id_ignores_runtime_data_file() {
            let dir = tempfile::TempDir::new().unwrap();
            let path = dir.path().join("my-plugin").join("data").join("stats.json");
            assert_eq!(plugin_id_for_change(&path, dir.path()), None);
        }

        #[test]
        fn change_id_for_new_top_level_dir() {
            // A brand-new plugin directory created after boot must surface its id
            // so the frontend re-scans — even before any inner file event.
            let dir = tempfile::TempDir::new().unwrap();
            let plugin_dir = dir.path().join("fresh-plugin");
            std::fs::create_dir(&plugin_dir).unwrap();
            assert_eq!(
                plugin_id_for_change(&plugin_dir, dir.path()),
                Some("fresh-plugin".to_string())
            );
        }

        #[cfg(unix)]
        #[test]
        fn change_id_for_new_top_level_symlink() {
            // The reported bug: a plugin added via `ln -s` after boot. notify does
            // not descend into the symlink target, so the single-component create
            // of the link itself is the only signal we get.
            let dir = tempfile::TempDir::new().unwrap();
            let target = dir.path().join("target");
            std::fs::create_dir(&target).unwrap();
            let link = dir.path().join("linked-plugin");
            std::os::unix::fs::symlink(&target, &link).unwrap();
            assert_eq!(
                plugin_id_for_change(&link, dir.path()),
                Some("linked-plugin".to_string())
            );
        }

        #[test]
        fn change_id_ignores_stray_top_level_file() {
            // A loose file at the plugins root (e.g. .DS_Store sibling) is not a plugin.
            let dir = tempfile::TempDir::new().unwrap();
            let path = dir.path().join("notes.txt");
            std::fs::write(&path, "x").unwrap();
            assert_eq!(plugin_id_for_change(&path, dir.path()), None);
        }

        #[test]
        fn change_id_ignores_hidden_top_level_dir() {
            let dir = tempfile::TempDir::new().unwrap();
            let hidden = dir.path().join(".git");
            std::fs::create_dir(&hidden).unwrap();
            assert_eq!(plugin_id_for_change(&hidden, dir.path()), None);
        }
    }

    // -- uninstall runtime-state disposal --

    #[cfg(feature = "desktop")]
    #[test]
    fn dispose_sweeps_watchers_and_capabilities_for_id_only() {
        use notify::{Config, RecommendedWatcher, Watcher};

        let state = crate::state::tests_support::make_test_app_state();

        let make_watcher = || {
            let (tx, _rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
            RecommendedWatcher::new(tx, Config::default()).unwrap()
        };

        // Two watch handles for the plugin being uninstalled, one for another.
        state
            .plugin_watchers
            .insert("w1".into(), ("victim".into(), make_watcher()));
        state
            .plugin_watchers
            .insert("w2".into(), ("victim".into(), make_watcher()));
        state
            .plugin_watchers
            .insert("w3".into(), ("survivor".into(), make_watcher()));

        state
            .loaded_plugins
            .insert("victim".into(), vec!["exec:cli".into()]);
        state
            .loaded_plugins
            .insert("survivor".into(), vec!["ui:panel".into()]);

        dispose_plugin_runtime_state(&state, "victim");

        // The victim's watchers are dropped (their debounce threads exit); the
        // unrelated plugin's watcher survives.
        assert!(
            !state
                .plugin_watchers
                .iter()
                .any(|e| e.value().0 == "victim")
        );
        assert!(state.plugin_watchers.contains_key("w3"));
        assert_eq!(state.plugin_watchers.len(), 1);

        // The capability grant is revoked for the victim only.
        assert!(!state.loaded_plugins.contains_key("victim"));
        assert!(state.loaded_plugins.contains_key("survivor"));
    }

    // -- register_loaded_plugin manifest-capability gate --

    /// register_loaded_plugin trusts the on-disk manifest, not the caller's
    /// capability list: it accepts declared capabilities and rejects any that
    /// the manifest does not list (so the frontend cannot self-grant).
    #[test]
    fn register_gates_capabilities_on_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let plugin_dir = dir.path().join("plugins").join("gate-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("manifest.json"),
            r#"{
                "id": "gate-plugin",
                "name": "Gate Plugin",
                "version": "1.0.0",
                "minAppVersion": "0.0.0",
                "main": "main.js",
                "capabilities": ["pty:read"]
            }"#,
        )
        .unwrap();

        let state = crate::state::tests_support::make_test_app_state();

        // Declared capability → registered.
        register_loaded_plugin_impl(&state, "gate-plugin".into(), vec!["pty:read".into()]).unwrap();
        assert_eq!(
            state.loaded_plugins.get("gate-plugin").map(|c| c.clone()),
            Some(vec!["pty:read".to_string()])
        );

        // Undeclared capability → rejected, and NOT recorded.
        state.loaded_plugins.remove("gate-plugin");
        let err =
            register_loaded_plugin_impl(&state, "gate-plugin".into(), vec!["exec:cli".into()])
                .unwrap_err();
        assert!(err.contains("not declared in manifest"), "got: {err}");
        assert!(!state.loaded_plugins.contains_key("gate-plugin"));
    }

    /// A WebView reload calls register_loaded_plugin again for an already-loaded
    /// plugin — uninstall is never invoked. Without disposing the previous
    /// runtime state first, each reload leaks the old fs-watcher set and
    /// MAX_WATCHERS_PER_PLUGIN caps out after a handful of reloads (#649-56fb).
    #[cfg(feature = "desktop")]
    #[test]
    fn register_disposes_previous_runtime_state_on_reregister() {
        use notify::{Config, RecommendedWatcher, Watcher};

        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let plugin_dir = dir.path().join("plugins").join("reload-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("manifest.json"),
            r#"{
                "id": "reload-plugin",
                "name": "Reload Plugin",
                "version": "1.0.0",
                "minAppVersion": "0.0.0",
                "main": "main.js",
                "capabilities": ["fs:watch"]
            }"#,
        )
        .unwrap();

        let state = crate::state::tests_support::make_test_app_state();

        register_loaded_plugin_impl(&state, "reload-plugin".into(), vec!["fs:watch".into()])
            .unwrap();

        // The plugin sets up a watcher after loading.
        let (tx, _rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
        let watcher = RecommendedWatcher::new(tx, Config::default()).unwrap();
        state
            .plugin_watchers
            .insert("w1".into(), ("reload-plugin".into(), watcher));
        assert_eq!(state.plugin_watchers.len(), 1);

        // WebView reload re-registers the same plugin without ever unloading it.
        register_loaded_plugin_impl(&state, "reload-plugin".into(), vec!["fs:watch".into()])
            .unwrap();

        // The stale watcher set from before the reload must be gone, not
        // accumulated alongside whatever the reloaded plugin sets up next.
        assert!(
            state.plugin_watchers.is_empty(),
            "reload must dispose the previous watcher set"
        );
        // The capability grant itself survives the reload.
        assert_eq!(
            state.loaded_plugins.get("reload-plugin").map(|c| c.clone()),
            Some(vec!["fs:watch".to_string()])
        );
    }

    // -- install data/ preservation (AppHandle-free seam) --

    /// A reinstall wipes the plugin dir but must NOT lose the user's data/.
    /// prepare_plugin_target backs data/ out before the clean and hands back the
    /// backup for finalize_plugin_install to restore. (The full install fns take
    /// an AppHandle and can't run in a unit test without a Tauri mock runtime, so
    /// we exercise the preservation seam directly, including the restore rename.)
    #[test]
    fn prepare_plugin_target_preserves_existing_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let target = dir.path().join("plugins").join("dp-plugin");
        std::fs::create_dir_all(target.join("data")).unwrap();
        std::fs::write(target.join("data").join("state.json"), "keepme").unwrap();
        std::fs::write(target.join("old.js"), "stale code").unwrap();

        let mut prepared = prepare_plugin_target(&valid_manifest("dp-plugin")).unwrap();

        // data/ was moved out to the backup (contents intact), the stale file is
        // gone, and a fresh empty target dir exists.
        let backup = prepared
            .data_backup
            .clone()
            .expect("data/ must be backed up");
        // The backup must live under plugins_dir() (same filesystem as the
        // plugin dirs) so rename can never fail with EXDEV, and be hidden so
        // discovery skips it.
        assert_eq!(backup.parent().unwrap(), plugins_dir());
        assert!(
            backup
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".tuic-data-backup-")
        );
        assert_eq!(
            std::fs::read_to_string(backup.join("state.json")).unwrap(),
            "keepme"
        );
        assert!(
            !prepared.path.join("old.js").exists(),
            "stale file must be wiped"
        );
        assert!(
            !prepared.path.join("data").exists(),
            "data/ moved to backup"
        );
        assert!(prepared.path.is_dir(), "fresh target dir must exist");

        // The restore (as finalize_plugin_install performs) round-trips it and
        // leaves no backup dir behind.
        prepared.restore_data().unwrap();
        assert_eq!(
            std::fs::read_to_string(prepared.path.join("data").join("state.json")).unwrap(),
            "keepme"
        );
        assert!(!backup.exists(), "backup must be gone after restore");
    }

    /// An abandoned install (error between prepare and finalize) must put the
    /// user's data/ back instead of orphaning it in the backup dir.
    #[test]
    fn prepare_plugin_target_drop_restores_data_on_abandoned_install() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let target = dir.path().join("plugins").join("dp-plugin");
        std::fs::create_dir_all(target.join("data")).unwrap();
        std::fs::write(target.join("data").join("state.json"), "keepme").unwrap();

        let backup = {
            let prepared = prepare_plugin_target(&valid_manifest("dp-plugin")).unwrap();
            // Simulate a half-extracted install that then errors out.
            std::fs::write(prepared.path.join("partial.js"), "x").unwrap();
            prepared.data_backup.clone().unwrap()
        }; // dropped without finalize

        assert_eq!(
            std::fs::read_to_string(target.join("data").join("state.json")).unwrap(),
            "keepme"
        );
        assert!(!backup.exists(), "backup must be cleaned up on drop");
    }

    /// A leftover hidden backup dir (e.g. after a crash) must never be listed
    /// as a plugin, even if it contains a manifest.
    #[test]
    fn list_user_plugins_ignores_leftover_data_backup_dir() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let leftover = dir.path().join("plugins").join(".tuic-data-backup-x-1234");
        std::fs::create_dir_all(&leftover).unwrap();
        std::fs::write(leftover.join("manifest.json"), valid_manifest_json("x")).unwrap();

        assert!(list_user_plugins(None).is_empty());
    }

    /// A first install (no prior dir) has nothing to back up.
    #[test]
    fn prepare_plugin_target_first_install_has_no_backup() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());

        let prepared = prepare_plugin_target(&valid_manifest("fresh-plugin")).unwrap();
        assert!(prepared.data_backup.is_none());
        assert!(prepared.path.is_dir());
    }

    // -- uninstall runtime-state disposal is idempotent --

    /// uninstall_plugin's early-return path (dir already gone) and its cleanup
    /// both call dispose_plugin_runtime_state; it must be a safe no-op when
    /// there is nothing to sweep, so a repeated uninstall cannot panic.
    #[cfg(feature = "desktop")]
    #[test]
    fn dispose_runtime_state_is_idempotent() {
        let state = crate::state::tests_support::make_test_app_state();
        // Disposing an id with no registered state is a no-op…
        dispose_plugin_runtime_state(&state, "never-loaded");
        // …and doing it again is still safe.
        dispose_plugin_runtime_state(&state, "never-loaded");
        assert!(state.loaded_plugins.is_empty());
        assert!(state.plugin_watchers.is_empty());
    }
}
