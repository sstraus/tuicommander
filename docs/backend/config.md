# Configuration

**Module:** `src-tauri/src/config.rs`

Manages all application configuration as JSON files in the platform config directory.

## Config Directory

| Platform | Path |
|----------|------|
| macOS | `~/Library/Application Support/tuicommander/` |
| Linux | `~/.config/tuicommander/` |
| Windows | `%APPDATA%/tuicommander/` |

Legacy path `~/.tuicommander/` is auto-migrated on first launch.

## Core Functions

```rust
pub fn config_dir() -> PathBuf
pub fn load_json_config<T: DeserializeOwned + Default>(filename: &str) -> T
pub fn save_json_config<T: Serialize>(filename: &str, config: &T) -> Result<(), String>
```

## Config Files and Commands

### Application Config (`config.json`)

**Type:** `AppConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `shell` | `Option<String>` | `None` | Shell override (platform default if None) |
| `font_family` | `String` | `"JetBrains Mono"` | Terminal font family |
| `font_size` | `u16` | `12` | Terminal font size |
| `theme` | `String` | `"dark"` | Terminal theme |
| `ide` | `String` | `"cursor"` | IDE for "Open in..." |
| `default_font_size` | `u16` | `12` | Default font size for reset |
| `mcp_server_enabled` | `bool` | `false` | Enable MCP HTTP server |
| `remote_access_enabled` | `bool` | `false` | Enable remote access |
| `remote_access_port` | `u16` | `3100` | Remote access port |
| `remote_access_username` | `String` | `""` | Basic auth username |
| `remote_access_password_hash` | `String` | `""` | Bcrypt password hash |
| `confirm_before_quit` | `bool` | `true` | Show quit confirmation |
| `confirm_before_closing_tab` | `bool` | `true` | Show tab close confirmation |
| `max_tab_name_length` | `u32` | `20` | Max tab name display length |

**Commands:** `load_app_config()`, `save_app_config(config)`

### Notification Config (`notification-config.json`)

**Type:** `NotificationConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `true` | Global enable |
| `volume` | `f64` | `0.5` | Volume (0.0-1.0) |
| `sounds.question` | `bool` | `true` | Play on agent question |
| `sounds.error` | `bool` | `true` | Play on error |
| `sounds.completion` | `bool` | `true` | Play on completion |
| `sounds.warning` | `bool` | `true` | Play on warning |

**Commands:** `load_notification_config()`, `save_notification_config(config)`

### UI Preferences (`ui-prefs.json`)

**Type:** `UIPrefsConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `sidebar_visible` | `bool` | `true` | Sidebar visibility |
| `sidebar_width` | `u32` | `280` | Sidebar width in pixels |
| `error_handling.strategy` | `String` | `"retry"` | Error strategy |
| `error_handling.max_retries` | `u32` | `3` | Max retry count |

**Commands:** `load_ui_prefs()`, `save_ui_prefs(config)`

### Repository Settings (`repo-settings.json`)

**Type:** `RepoSettingsMap` (HashMap of `RepoSettingsEntry`)

Per-repository fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | `String` | — | Repository path |
| `display_name` | `String` | — | Display name |
| `base_branch` | `String` | `"main"` | Base branch for worktrees |
| `copy_ignored_files` | `bool` | `false` | Copy .gitignored files to worktree |
| `copy_untracked_files` | `bool` | `false` | Copy untracked files to worktree |
| `setup_script` | `String` | `""` | Script to run after worktree creation |
| `run_script` | `String` | `""` | Default run command |

**Commands:** `load_repo_settings()`, `save_repo_settings(config)`, `check_has_custom_settings(path)`

### Repositories (`repositories.json`)

**Type:** `serde_json::Value` (flexible JSON, shape defined by frontend)

**Commands:** `load_repositories()`, `save_repositories(config)`

### Prompt Library (`prompt-library.json`)

**Type:** `PromptLibraryConfig`

```rust
struct PromptEntry {
    id: String,
    label: String,
    text: String,
    pinned: bool,
}
```

**Commands:** `load_prompt_library()`, `save_prompt_library(config)`

### Dictation Config (`dictation-config.json`)

**Type:** `DictationConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `false` | Dictation enabled |
| `hotkey` | `String` | `"CommandOrControl+Shift+D"` | Push-to-talk hotkey |
| `language` | `String` | `"en"` | Transcription language |
| `model` | `String` | `"large-v3-turbo"` | Whisper model name |

**Commands:** `get_dictation_config()`, `set_dictation_config(config)`

## Additional Commands

| Command | Description |
|---------|-------------|
| `hash_password(password)` | Bcrypt hash for remote access authentication |
| `list_markdown_files(path)` | List .md files in a directory |
| `read_file(path, file)` | Read a file's contents |
| `get_mcp_status()` | Get MCP server status (enabled, port, connected clients) |
