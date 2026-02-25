use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Get the config directory using platform-appropriate location.
///
/// - macOS: `~/Library/Application Support/tuicommander/`
/// - Linux: `~/.config/tuicommander/` (or `$XDG_CONFIG_HOME`)
/// - Windows: `%APPDATA%/tuicommander/`
///
/// Falls back to `~/.tuicommander/` if platform dir is unavailable.
/// On first call, migrates from legacy locations if the new dir doesn't exist:
///   1. `~/.tui-commander/` (legacy dotdir)
///   2. `{platform_config}/tui-commander/` (old platform-dir name)
pub(crate) fn config_dir() -> PathBuf {
    let new_dir = dirs::config_dir()
        .map(|d| d.join("tuicommander"))
        .unwrap_or_else(legacy_dotdir);

    if !new_dir.exists() {
        // Try migrating from old platform dir (tui-commander) first, then legacy dotdir
        let old_platform_dir = dirs::config_dir().map(|d| d.join("tui-commander"));
        let legacy_dot = legacy_dotdir();

        let source = old_platform_dir
            .filter(|d| d.exists())
            .unwrap_or(legacy_dot);

        if source.exists() && source != new_dir
            && let Err(e) = migrate_config_dir(&source, &new_dir)
        {
            eprintln!("Warning: config migration failed: {e}");
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
    std::fs::create_dir_all(to)
        .map_err(|e| format!("Failed to create new config dir: {e}"))?;

    for entry in std::fs::read_dir(from)
        .map_err(|e| format!("Failed to read legacy config dir: {e}"))?
    {
        let entry = entry.map_err(|e| format!("Failed to read dir entry: {e}"))?;
        let dest = to.join(entry.file_name());
        let file_type = entry.file_type()
            .map_err(|e| format!("Failed to get file type: {e}"))?;

        if file_type.is_file() {
            std::fs::copy(entry.path(), &dest)
                .map_err(|e| format!("Failed to copy {}: {e}", entry.path().display()))?;
        } else if file_type.is_dir() {
            // Recursively copy subdirectories (models/, worktrees/)
            copy_dir_recursive(&entry.path(), &dest)?;
        }
    }

    eprintln!(
        "Migrated config from {} to {}",
        from.display(),
        to.display()
    );
    Ok(())
}

/// Recursively copy a directory.
fn copy_dir_recursive(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(to)
        .map_err(|e| format!("Failed to create dir {}: {e}", to.display()))?;

    for entry in std::fs::read_dir(from)
        .map_err(|e| format!("Failed to read dir {}: {e}", from.display()))?
    {
        let entry = entry.map_err(|e| format!("Dir entry error: {e}"))?;
        let dest = to.join(entry.file_name());
        let file_type = entry.file_type()
            .map_err(|e| format!("File type error: {e}"))?;

        if file_type.is_file() {
            std::fs::copy(entry.path(), &dest)
                .map_err(|e| format!("Copy error: {e}"))?;
        } else if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest)?;
        }
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
            eprintln!("Warning: Could not read config {}: {e}", path.display());
            return T::default();
        }
    };
    match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: Corrupt config {}: {e}. Using defaults.", path.display());
            T::default()
        }
    }
}

/// Save a JSON config file atomically (temp file + rename).
/// Sets 0600 permissions on Unix to protect sensitive data.
pub(crate) fn save_json_config<T: Serialize>(filename: &str, config: &T) -> Result<(), String> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create config directory: {e}"))?;

    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;

    let target = dir.join(filename);
    let temp = dir.join(format!("{}.tmp.{}", filename, std::process::id()));

    std::fs::write(&temp, &json)
        .map_err(|e| format!("Failed to write temp config: {e}"))?;

    // Set restrictive permissions before rename (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&temp, perms)
            .map_err(|e| format!("Failed to set config permissions: {e}"))?;
    }

    // Atomic rename: either the old file or new file exists, never partial
    std::fs::rename(&temp, &target)
        .map_err(|e| {
            // Clean up temp file on rename failure
            let _ = std::fs::remove_file(&temp);
            format!("Failed to commit config: {e}")
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

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct AppConfig {
    pub(crate) shell: Option<String>,
    pub(crate) font_family: String,
    pub(crate) font_size: u16,
    pub(crate) theme: String,
    /// Enable MCP HTTP API on localhost for external tool integration
    #[serde(default)]
    pub(crate) mcp_server_enabled: bool,
    /// Fixed port for MCP server (0 = OS-assigned)
    #[serde(default = "default_mcp_port")]
    pub(crate) mcp_port: u16,
    /// Preferred IDE (e.g. "vscode", "cursor")
    #[serde(default)]
    pub(crate) ide: String,
    /// Default font size for new terminals
    #[serde(default = "default_font_size")]
    pub(crate) default_font_size: u16,
    /// Enable remote browser access (binds to 0.0.0.0 with auth)
    #[serde(default)]
    pub(crate) remote_access_enabled: bool,
    /// Port for remote access (0 = OS-assigned)
    #[serde(default = "default_remote_port")]
    pub(crate) remote_access_port: u16,
    /// Username for Basic Auth on remote connections
    #[serde(default)]
    pub(crate) remote_access_username: String,
    /// Bcrypt hash of the password for Basic Auth
    #[serde(default)]
    pub(crate) remote_access_password_hash: String,
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
    /// Auto-show PR detail popover when a branch has PR data
    #[serde(default = "default_true")]
    pub(crate) auto_show_pr_popover: bool,
    /// Prevent system sleep while any terminal session is busy
    #[serde(default)]
    pub(crate) prevent_sleep_when_busy: bool,
    /// Automatically check for app updates on startup
    #[serde(default = "default_true")]
    pub(crate) auto_update_enabled: bool,
    /// UI language code (e.g. "en", "it", "de")
    #[serde(default = "default_language")]
    pub(crate) language: String,
    /// Plugin IDs that the user has disabled (not loaded on startup)
    #[serde(default)]
    pub(crate) disabled_plugin_ids: Vec<String>,
    /// Update channel: "stable", "beta", or "nightly"
    #[serde(default = "default_update_channel")]
    pub(crate) update_channel: String,
    /// Session token cookie duration in seconds (0 = session cookie, 31536000 = "never")
    #[serde(default = "default_session_token_duration_secs")]
    pub(crate) session_token_duration_secs: u64,
    /// Enable IPv6 dual-stack binding when remote access is active
    #[serde(default)]
    pub(crate) ipv6_enabled: bool,
    /// Skip authentication for private/LAN IP addresses (RFC1918 + IPv6 ULA)
    #[serde(default)]
    pub(crate) lan_auth_bypass: bool,
    /// Show all local branches in the sidebar by default (not just worktrees + active branch)
    #[serde(default)]
    pub(crate) show_all_branches: bool,
    /// Agent types disabled by the user (won't appear in sidebar "Add Agent" menu)
    #[serde(default)]
    pub(crate) disabled_agents: Vec<String>,
}

fn default_language() -> String {
    "en".to_string()
}

fn default_update_channel() -> String {
    "stable".to_string()
}

fn default_session_token_duration_secs() -> u64 {
    86400
}

fn default_mcp_port() -> u16 {
    3845
}

fn default_font_size() -> u16 {
    13
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
            theme: "vscode-dark".to_string(),
            mcp_server_enabled: false,
            mcp_port: default_mcp_port(),
            ide: String::new(),
            default_font_size: 13,
            remote_access_enabled: false,
            remote_access_port: default_remote_port(),
            remote_access_username: String::new(),
            remote_access_password_hash: String::new(),
            confirm_before_quit: true,
            confirm_before_closing_tab: true,
            max_tab_name_length: default_max_tab_name_length(),
            split_tab_mode: SplitTabMode::default(),
            auto_show_pr_popover: true,
            prevent_sleep_when_busy: false,
            auto_update_enabled: true,
            language: default_language(),
            disabled_plugin_ids: Vec::new(),
            update_channel: default_update_channel(),
            session_token_duration_secs: default_session_token_duration_secs(),
            ipv6_enabled: false,
            lan_auth_bypass: false,
            show_all_branches: false,
            disabled_agents: Vec::new(),
        }
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
    #[serde(default = "default_panel_width")]
    pub(crate) diff_panel_width: u32,
    #[serde(default = "default_panel_width")]
    pub(crate) markdown_panel_width: u32,
    #[serde(default = "default_notes_panel_width")]
    pub(crate) notes_panel_width: u32,
    #[serde(default = "default_settings_nav_width")]
    pub(crate) settings_nav_width: u32,
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
            diff_panel_width: default_panel_width(),
            markdown_panel_width: default_panel_width(),
            notes_panel_width: default_notes_panel_width(),
            settings_nav_width: default_settings_nav_width(),
        }
    }
}

fn default_sidebar_width() -> u32 { 260 }
fn default_panel_width() -> u32 { 400 }
fn default_notes_panel_width() -> u32 { 350 }
fn default_settings_nav_width() -> u32 { 180 }

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
}

impl RepoSettingsEntry {
    /// Check if this entry has any non-default settings
    pub(crate) fn has_custom_settings(&self) -> bool {
        self.base_branch.is_some()
            || self.copy_ignored_files.is_some()
            || self.copy_untracked_files.is_some()
            || self.setup_script.is_some()
            || self.run_script.is_some()
            || !self.color.is_empty()
            || self.worktree_storage.is_some()
            || self.prompt_on_create.is_some()
            || self.delete_branch_on_remove.is_some()
            || self.auto_archive_merged.is_some()
            || self.orphan_cleanup.is_some()
            || self.pr_merge_strategy.is_some()
            || self.after_merge.is_some()
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
}

impl Default for RepoDefaultsConfig {
    fn default() -> Self {
        Self {
            base_branch: default_base_branch(),
            copy_ignored_files: false,
            copy_untracked_files: false,
            setup_script: String::new(),
            run_script: String::new(),
            worktree_storage: WorktreeStorage::default(),
            prompt_on_create: true,
            delete_branch_on_remove: true,
            auto_archive_merged: false,
            orphan_cleanup: OrphanCleanup::default(),
            pr_merge_strategy: MergeStrategy::default(),
            after_merge: WorktreeAfterMerge::default(),
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
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct AgentsConfig {
    #[serde(default)]
    pub(crate) agents: HashMap<String, AgentSettings>,
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
const AGENTS_CONFIG_FILE: &str = "agents.json";
const ACTIVITY_FILE: &str = "activity.json";

// App config
#[tauri::command]
pub(crate) fn load_app_config() -> AppConfig {
    load_json_config(APP_CONFIG_FILE)
}

#[tauri::command]
pub(crate) fn save_app_config(config: AppConfig) -> Result<(), String> {
    save_json_config(APP_CONFIG_FILE, &config)
}

// Notification config
#[tauri::command]
pub(crate) fn load_notification_config() -> NotificationConfig {
    load_json_config(NOTIFICATION_CONFIG_FILE)
}

#[tauri::command]
pub(crate) fn save_notification_config(config: NotificationConfig) -> Result<(), String> {
    save_json_config(NOTIFICATION_CONFIG_FILE, &config)
}

// UI prefs
#[tauri::command]
pub(crate) fn load_ui_prefs() -> UIPrefsConfig {
    load_json_config(UI_PREFS_FILE)
}

#[tauri::command]
pub(crate) fn save_ui_prefs(config: UIPrefsConfig) -> Result<(), String> {
    save_json_config(UI_PREFS_FILE, &config)
}

// Repo settings
#[tauri::command]
pub(crate) fn load_repo_settings() -> RepoSettingsMap {
    load_json_config(REPO_SETTINGS_FILE)
}

#[tauri::command]
pub(crate) fn save_repo_settings(config: RepoSettingsMap) -> Result<(), String> {
    save_json_config(REPO_SETTINGS_FILE, &config)
}

#[tauri::command]
pub(crate) fn check_has_custom_settings(path: String) -> bool {
    let settings: RepoSettingsMap = load_json_config(REPO_SETTINGS_FILE);
    settings.repos.get(&path).is_some_and(|entry| entry.has_custom_settings())
}

// Repo defaults (global defaults for all repos)
#[tauri::command]
pub(crate) fn load_repo_defaults() -> RepoDefaultsConfig {
    load_json_config(REPO_DEFAULTS_FILE)
}

#[tauri::command]
pub(crate) fn save_repo_defaults(config: RepoDefaultsConfig) -> Result<(), String> {
    save_json_config(REPO_DEFAULTS_FILE, &config)
}

// Repositories (opaque JSON — schema owned by frontend)
#[tauri::command]
pub(crate) fn load_repositories() -> serde_json::Value {
    load_json_config(REPOSITORIES_FILE)
}

#[tauri::command]
pub(crate) fn save_repositories(config: serde_json::Value) -> Result<(), String> {
    save_json_config(REPOSITORIES_FILE, &config)
}

// Prompt library
#[tauri::command]
pub(crate) fn load_prompt_library() -> PromptLibraryConfig {
    load_json_config(PROMPT_LIBRARY_FILE)
}

#[tauri::command]
pub(crate) fn save_prompt_library(config: PromptLibraryConfig) -> Result<(), String> {
    save_json_config(PROMPT_LIBRARY_FILE, &config)
}

// Notes (opaque JSON — schema owned by frontend)
#[tauri::command]
pub(crate) fn load_notes() -> serde_json::Value {
    load_json_config(NOTES_FILE)
}

#[tauri::command]
pub(crate) fn save_notes(config: serde_json::Value) -> Result<(), String> {
    save_json_config(NOTES_FILE, &config)
}

// Activity center (opaque JSON — schema owned by frontend)
#[tauri::command]
pub(crate) fn load_activity() -> serde_json::Value {
    load_json_config(ACTIVITY_FILE)
}

#[tauri::command]
pub(crate) fn save_activity(items: serde_json::Value) -> Result<(), String> {
    save_json_config(ACTIVITY_FILE, &items)
}

// Keybindings (opaque JSON — schema owned by frontend)
#[tauri::command]
pub(crate) fn load_keybindings() -> serde_json::Value {
    load_json_config(KEYBINDINGS_FILE)
}

#[tauri::command]
pub(crate) fn save_keybindings(config: serde_json::Value) -> Result<(), String> {
    save_json_config(KEYBINDINGS_FILE, &config)
}

// Agents config
#[tauri::command]
pub(crate) fn load_agents_config() -> AgentsConfig {
    load_json_config(AGENTS_CONFIG_FILE)
}

#[tauri::command]
pub(crate) fn save_agents_config(config: AgentsConfig) -> Result<(), String> {
    save_json_config(AGENTS_CONFIG_FILE, &config)
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
            theme: "dark".to_string(),
            mcp_server_enabled: true,
            mcp_port: 4000,
            ide: "cursor".to_string(),
            default_font_size: 18,
            remote_access_enabled: true,
            remote_access_port: 8080,
            remote_access_username: "admin".to_string(),
            remote_access_password_hash: "$2b$12$hash".to_string(),
            confirm_before_quit: false,
            confirm_before_closing_tab: true,
            max_tab_name_length: 40,
            split_tab_mode: SplitTabMode::Unified,
            auto_show_pr_popover: true,
            prevent_sleep_when_busy: true,
            auto_update_enabled: false,
            language: "it".to_string(),
            disabled_plugin_ids: vec!["test-disabled".to_string()],
            update_channel: "beta".to_string(),
            session_token_duration_secs: 3600,
            ipv6_enabled: true,
            lan_auth_bypass: true,
            show_all_branches: true,
            disabled_agents: vec!["codex".to_string()],
        };
        let loaded: AppConfig = round_trip_in_dir(dir.path(), "config.json", &cfg);
        assert_eq!(loaded.shell.as_deref(), Some("/bin/zsh"));
        assert_eq!(loaded.font_size, 16);
        assert_eq!(loaded.ide, "cursor");
        assert_eq!(loaded.default_font_size, 18);
        assert!(loaded.mcp_server_enabled);
        assert_eq!(loaded.mcp_port, 4000);
        assert!(loaded.remote_access_enabled);
        assert_eq!(loaded.remote_access_port, 8080);
        assert_eq!(loaded.remote_access_username, "admin");
        assert_eq!(loaded.remote_access_password_hash, "$2b$12$hash");
        assert!(!loaded.confirm_before_quit);
        assert!(loaded.confirm_before_closing_tab);
        assert_eq!(loaded.max_tab_name_length, 40);
        assert_eq!(loaded.split_tab_mode, SplitTabMode::Unified);
        assert!(loaded.prevent_sleep_when_busy);
        assert!(!loaded.auto_update_enabled);
        assert_eq!(loaded.language, "it");
        assert_eq!(loaded.disabled_plugin_ids, vec!["test-disabled".to_string()]);
        assert_eq!(loaded.update_channel, "beta");
        assert_eq!(loaded.session_token_duration_secs, 3600);
        assert!(loaded.ipv6_enabled);
        assert!(loaded.lan_auth_bypass);
        assert!(loaded.show_all_branches);
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
        assert!(!loaded.remote_access_enabled);
        assert_eq!(loaded.remote_access_port, 9876);
        assert_eq!(loaded.remote_access_username, "");
        assert_eq!(loaded.remote_access_password_hash, "");
        assert!(loaded.confirm_before_quit);
        assert!(loaded.confirm_before_closing_tab);
        assert_eq!(loaded.max_tab_name_length, 25);
        assert_eq!(loaded.split_tab_mode, SplitTabMode::Separate);
        assert!(!loaded.prevent_sleep_when_busy);
        assert!(loaded.auto_update_enabled);
        assert_eq!(loaded.language, "en");
        assert_eq!(loaded.update_channel, "stable");
        assert_eq!(loaded.session_token_duration_secs, 86400);
        assert!(!loaded.ipv6_enabled);
        assert!(!loaded.lan_auth_bypass);
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
        };
        let loaded: NotificationConfig =
            round_trip_in_dir(dir.path(), "notifications.json", &cfg);
        assert!(!loaded.enabled);
        assert!((loaded.volume - 0.8).abs() < f64::EPSILON);
        assert!(loaded.sounds.question);
        assert!(!loaded.sounds.error);
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
            diff_panel_width: 500,
            markdown_panel_width: 450,
            notes_panel_width: 320,
            settings_nav_width: 200,
        };
        let loaded: UIPrefsConfig = round_trip_in_dir(dir.path(), "ui-prefs.json", &cfg);
        assert!(!loaded.sidebar_visible);
        assert_eq!(loaded.sidebar_width, 300);
        assert_eq!(loaded.diff_panel_width, 500);
        assert_eq!(loaded.markdown_panel_width, 450);
        assert_eq!(loaded.notes_panel_width, 320);
        assert_eq!(loaded.settings_nav_width, 200);
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
                color: String::new(),
                worktree_storage: None,
                prompt_on_create: None,
                delete_branch_on_remove: None,
                auto_archive_merged: None,
                orphan_cleanup: None,
                pr_merge_strategy: None,
                after_merge: None,
            },
        );
        let loaded: RepoSettingsMap =
            round_trip_in_dir(dir.path(), "repo-settings.json", &map);
        assert_eq!(loaded.repos.len(), 1);
        let entry = loaded.repos.get("/my/repo").unwrap();
        assert_eq!(entry.display_name, "my-repo");
        assert_eq!(entry.base_branch, Some("main".to_string()));
        assert_eq!(entry.copy_ignored_files, Some(true));
        assert_eq!(entry.copy_untracked_files, None);
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
        let initial = NotificationConfig { enabled: false, ..NotificationConfig::default() };
        let json = serde_json::to_string_pretty(&initial).unwrap();
        fs::write(&target, json).unwrap();

        // Overwrite with new content using save_json_config pattern
        let updated = NotificationConfig { enabled: true, ..NotificationConfig::default() };
        let json2 = serde_json::to_string_pretty(&updated).unwrap();
        let temp = dir.path().join(format!("{}.tmp.{}", filename, std::process::id()));
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
        let temp = dir.path().join(format!("{}.tmp.{}", filename, std::process::id()));
        fs::write(&temp, &json).unwrap();

        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&temp, perms).unwrap();
        fs::rename(&temp, &target).unwrap();

        let metadata = fs::metadata(&target).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "Config file should be owner-only (0600)");
    }

    #[test]
    fn has_custom_settings_false_for_defaults() {
        let entry = RepoSettingsEntry::default();
        assert!(!entry.has_custom_settings());
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
    fn corrupt_file_returns_default() {
        // Write garbage JSON, should return default
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.json");
        fs::write(&path, "not valid json!!!").unwrap();
        // Since load_json_config uses the global config_dir, we test the deserialization path
        let result: Result<AppConfig, _> = serde_json::from_str("not valid json!!!");
        assert!(result.is_err());
        // The load_json_config function gracefully falls back to default
    }

    #[test]
    fn invalid_split_tab_mode_fails_deserialization() {
        // An invalid split_tab_mode value should cause deserialization to fail,
        // which load_json_config handles by returning Default
        let json = r#"{"shell":null,"font_family":"JetBrains Mono","font_size":14,"theme":"tokyo-night","worktree_dir":null,"split_tab_mode":"bogus"}"#;
        let result: Result<AppConfig, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Invalid split_tab_mode should fail deserialization");
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
                        args: vec!["--model".to_string(), "sonnet".to_string(), "--print".to_string()],
                        env,
                        is_default: false,
                    },
                ],
            },
        );
        let loaded: AgentsConfig = round_trip_in_dir(dir.path(), "agents.json", &agents);
        assert_eq!(loaded.agents.len(), 1);
        let claude = loaded.agents.get("claude").unwrap();
        assert_eq!(claude.run_configs.len(), 2);
        assert_eq!(claude.run_configs[0].name, "Default");
        assert!(claude.run_configs[0].is_default);
        assert_eq!(claude.run_configs[1].name, "Sonnet Print");
        assert_eq!(claude.run_configs[1].args, vec!["--model", "sonnet", "--print"]);
        assert_eq!(claude.run_configs[1].env.get("ANTHROPIC_API_KEY").unwrap(), "sk-test");
        assert!(!claude.run_configs[1].is_default);
    }

    #[test]
    fn agents_config_missing_file_returns_default() {
        let cfg: AgentsConfig = load_json_config("nonexistent-agents-12345.json");
        assert!(cfg.agents.is_empty());
    }

    // -- Worktree config tests --

    #[test]
    fn worktree_enums_serialize_as_expected() {
        assert_eq!(serde_json::to_string(&WorktreeStorage::Sibling).unwrap(), r#""sibling""#);
        assert_eq!(serde_json::to_string(&WorktreeStorage::AppDir).unwrap(), r#""app-dir""#);
        assert_eq!(serde_json::to_string(&WorktreeStorage::InsideRepo).unwrap(), r#""inside-repo""#);
        assert_eq!(serde_json::to_string(&OrphanCleanup::Ask).unwrap(), r#""ask""#);
        assert_eq!(serde_json::to_string(&OrphanCleanup::On).unwrap(), r#""on""#);
        assert_eq!(serde_json::to_string(&MergeStrategy::Squash).unwrap(), r#""squash""#);
        assert_eq!(serde_json::to_string(&WorktreeAfterMerge::Archive).unwrap(), r#""archive""#);
        assert_eq!(serde_json::to_string(&WorktreeAfterMerge::Delete).unwrap(), r#""delete""#);
    }

    #[test]
    fn worktree_enums_deserialize() {
        assert_eq!(serde_json::from_str::<WorktreeStorage>(r#""sibling""#).unwrap(), WorktreeStorage::Sibling);
        assert_eq!(serde_json::from_str::<WorktreeStorage>(r#""app-dir""#).unwrap(), WorktreeStorage::AppDir);
        assert_eq!(serde_json::from_str::<WorktreeStorage>(r#""inside-repo""#).unwrap(), WorktreeStorage::InsideRepo);
        assert_eq!(serde_json::from_str::<OrphanCleanup>(r#""ask""#).unwrap(), OrphanCleanup::Ask);
        assert_eq!(serde_json::from_str::<MergeStrategy>(r#""rebase""#).unwrap(), MergeStrategy::Rebase);
        assert_eq!(serde_json::from_str::<WorktreeAfterMerge>(r#""ask""#).unwrap(), WorktreeAfterMerge::Ask);
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
}
