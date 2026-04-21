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
| `collapse_tools` | `bool` | `false` | Replace the full MCP tool list with 3 lazy-discovery meta-tools (`search_tools`, `get_tool_schema`, `call_tool`) — see [`mcp-http.md`](mcp-http.md#lazy-tool-discovery-collapse_tools) |
| `remote_access_enabled` | `bool` | `false` | Enable remote access |
| `remote_access_port` | `u16` | `3100` | Remote access port |
| `remote_access_username` | `String` | `""` | Basic auth username |
| `remote_access_password_hash` | `String` | `""` | Bcrypt password hash |
| `confirm_before_quit` | `bool` | `true` | Show quit confirmation |
| `confirm_before_closing_tab` | `bool` | `true` | Show tab close confirmation |
| `copy_on_select` | `bool` | `true` | Auto-copy terminal selection to clipboard |
| `bell_style` | `String` | `"visual"` | Terminal bell: "none", "visual", "sound", "both" |
| `disabled_agents` | `Vec<String>` | `[]` | Agent IDs hidden from the Add menu |
| `global_hotkey` | `Option<String>` | `null` | OS-level window toggle hotkey combo |
| `intent_tab_title` | `bool` | `true` | Show agent intent as tab title |
| `ipv6_enabled` | `bool` | `false` | IPv6 dual-stack binding |
| `language` | `String` | `"en"` | UI language code |
| `max_tab_name_length` | `u32` | `20` | Max tab name display length |
| `prevent_sleep_when_busy` | `bool` | `false` | Prevent macOS sleep when terminal is busy |
| `push_enabled` | `bool` | `false` | Enable push notifications to PWA clients |
| `relay_enabled` | `bool` | `false` | Cloud relay for mobile access |
| `suggest_followups` | `bool` | `true` | Show `suggest:` follow-up actions |
| `issue_filter` | `Option<String>` | `"assigned"` | GitHub Issues filter: "assigned", "created", "mentioned", "all", "disabled" |
| `experimental_features_enabled` | `bool` | `false` | Master toggle for experimental features |
| `ai_chat_enabled` | `bool` | `false` | Sub-flag: enable AI Chat panel and shortcuts (requires `experimental_features_enabled`) |
| `ai_terminal_mcp_enabled` | `bool` | `false` | Expose `ai_terminal_*` tools to external MCP clients. Off by default — see [`mcp-http.md`](mcp-http.md#mcp-tools-ai_terminal_-external-agent-surface) |
| `auto_show_pr_popover` | `bool` | `false` | Auto-show PR popover when switching to a branch with a PR |
| `update_channel` | `String` | `"stable"` | Update channel: "stable" or "nightly" |

**Commands:** `load_app_config()`, `save_app_config(config)`

### Notification Config (`notifications.json`)

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

### AI Chat Config (`ai-chat-config.json`)

**Type:** `AiChatConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `provider` | `String` | `"ollama"` | AI provider: `ollama`, `anthropic`, `openai`, `openrouter`, `custom` |
| `model` | `String` | `""` | Model name |
| `base_url` | `Option<String>` | per-provider | Endpoint base URL |
| `temperature` | `f32` | `0.7` | Sampling temperature |
| `context_lines` | `u32` | `150` | VtLogBuffer rows injected per turn |
| `experimental_ai_block_enrichment` | `bool` | `false` | Enrich OSC 133 blocks with semantic intent |
| `agent_model_overrides` | `Option<HashMap<ToolPhase, String>>` | `None` | Per-phase model routing. Keys: `plan`, `search`, `read`, `write` |

**Commands:** `load_ai_chat_config()`, `save_ai_chat_config(config)`

### Cron Scheduler Config (`ai-cron.json`)

**Type:** `SchedulerConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `jobs` | `Vec<ScheduledJob>` | `[]` | List of scheduled agent jobs |

Each `ScheduledJob`:

| Field | Type | Description |
|-------|------|-------------|
| `id` | `String` | Unique job identifier |
| `cron_expr` | `String` | Cron expression (validated on save) |
| `goal` | `String` | Agent goal to execute |

**Commands:** `load_scheduler_config()`, `save_scheduler_config(config)`

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
| `path` | `String` | -- | Repository path |
| `display_name` | `String` | -- | Display name |
| `base_branch` | `String` | `"main"` | Base branch for worktrees |
| `copy_ignored_files` | `bool` | `false` | Copy .gitignored files to worktree |
| `copy_untracked_files` | `bool` | `false` | Copy untracked files to worktree |
| `setup_script` | `String` | `""` | Script to run after worktree creation |
| `run_script` | `String` | `""` | Default run command |
| `auto_fetch_interval_minutes` | `u32` | `0` | Auto-fetch interval in minutes (0 = disabled) |
| `auto_delete_on_pr_close` | `AutoDeleteOnPrClose` | `"off"` | Auto-delete branch when PR merged/closed (`off`/`ask`/`auto`) |
| `archive_script` | `String` | `""` | Script to run before archive/delete (non-zero exit blocks) |

**Commands:** `load_repo_settings()`, `save_repo_settings(config)`, `check_has_custom_settings(path)`

### Repository Defaults (`repo-defaults.json`)

**Type:** `RepoDefaultsConfig`

Default values applied to new repositories when no per-repo override exists.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `base_branch` | `String` | `"automatic"` | Default base branch |
| `copy_ignored_files` | `bool` | `false` | Copy .gitignored files to worktree |
| `copy_untracked_files` | `bool` | `false` | Copy untracked files to worktree |
| `setup_script` | `String` | `""` | Default setup script |
| `run_script` | `String` | `""` | Default run command |
| `archive_script` | `String` | `""` | Default archive script |

**Commands:** `load_repo_defaults()`, `save_repo_defaults(config)`

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

### Notes (`notes.json`)

**Type:** `serde_json::Value` (flexible JSON, shape defined by frontend)

**Commands:** `load_notes()`, `save_notes(config)`

### Keybindings (`keybindings.json`)

**Type:** `serde_json::Value` (flexible JSON, shape defined by frontend)

Custom keyboard shortcut overrides.

**Commands:** `load_keybindings()`, `save_keybindings(config)`

### Agents Config (`agents.json`)

**Type:** `AgentsConfig`

Per-agent run configurations (custom commands, arguments, environment variables).

```rust
struct AgentRunConfig {
    name: String,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    is_default: bool,
}

struct AgentSettings {
    run_configs: Vec<AgentRunConfig>,
}

struct AgentsConfig {
    agents: HashMap<String, AgentSettings>,
}
```

**Commands:** `load_agents_config()`, `save_agents_config(config)`

### AI Chat Config (`ai-chat-config.json`)

**Type:** `AiChatConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `provider` | `String` | `"ollama"` | Provider: `"ollama"`, `"anthropic"`, `"openai"`, `"openrouter"`, `"custom"` |
| `model` | `String` | provider-specific | Model name (free text; settings tab suggests per provider) |
| `base_url` | `Option<String>` | provider-specific | Pre-filled per provider, editable. Ollama default: `http://localhost:11434/v1/` |
| `temperature` | `f32` | `0.7` | Sampling temperature passed through to provider |
| `context_lines` | `u32` | `150` | Maximum `VtLogBuffer` lines injected into each turn's context |

**Commands:** `load_ai_chat_config()`, `save_ai_chat_config(config)`

API keys are stored in the OS keyring — service `tuicommander-ai-chat`, user `api-key` — via `save_ai_chat_api_key` / `delete_ai_chat_api_key`. Saved conversations live in `<config_dir>/ai-chat-conversations/<id>.json`.

### Dictation Config (`dictation-config.json`)

**Type:** `DictationConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `false` | Dictation enabled |
| `hotkey` | `String` | `"CommandOrControl+Shift+D"` | Push-to-talk hotkey |
| `language` | `String` | `"en"` | Transcription language |
| `model` | `String` | `"large-v3-turbo"` | Whisper model name |
| `auto_send` | `bool` | `false` | Auto-submit after transcription |

**Commands:** `get_dictation_config()`, `set_dictation_config(config)`

## Cache Files

### Claude Usage Cache (`claude-usage-cache.json`)

**Module:** `src-tauri/src/claude_usage.rs`

Persistent cache for incremental JSONL parsing of Claude session transcripts. Stored in the config directory. The cache maps `project_slug -> (filename -> CachedFileStats)` and tracks per-file byte offsets so only newly appended data is parsed on subsequent scans.

This is an internal cache file, not user-editable. It is automatically pruned when projects or session files are deleted.

## Repo-Local Config (`.tuic.json`)

**Module:** `src-tauri/src/config.rs`

A `.tuic.json` file in the repository root provides team-shareable settings. It is read-only from the app — teams edit it directly in their repo and commit it.

**Precedence chain:** `.tuic.json` > per-repo app settings (`repo-settings.json`) > global defaults (`repo-defaults.json`)

**Type:** `RepoLocalConfig` (all fields `Option<T>`, missing fields fall through to lower tiers)

| Field | Type | Description |
|-------|------|-------------|
| `base_branch` | `String` | Base branch for worktrees |
| `copy_ignored_files` | `bool` | Copy .gitignored files to worktree |
| `copy_untracked_files` | `bool` | Copy untracked files to worktree |
| `setup_script` | `String` | Script to run after worktree creation |
| `run_script` | `String` | Default run command |
| `archive_script` | `String` | Script to run before archive/delete |
| `worktree_storage` | `WorktreeStorage` | Storage strategy (sibling/app-dir/inside-repo) |
| `delete_branch_on_remove` | `bool` | Delete branch when removing worktree |
| `auto_archive_merged` | `bool` | Auto-archive merged worktrees |
| `orphan_cleanup` | `OrphanCleanup` | Orphan worktree handling |
| `pr_merge_strategy` | `MergeStrategy` | PR merge method preference |
| `after_merge` | `WorktreeAfterMerge` | Post-merge worktree action |
| `auto_delete_on_pr_close` | `AutoDeleteOnPrClose` | Auto-delete on PR close |

**Command:** `load_repo_local_config(repo_path)` — returns `RepoLocalConfig` or `null` if file is missing or malformed.

## Additional Commands

| Command | Module | Description |
|---------|--------|-------------|
| `hash_password(password)` | `lib.rs` | Bcrypt hash for remote access authentication |
| `list_markdown_files(path)` | `lib.rs` | List .md files in a directory |
| `read_file(path, file)` | `lib.rs` | Read a file's contents |
| `get_mcp_status()` | `lib.rs` | Get MCP server status (enabled, port, connected clients) |
| `clear_caches()` | `lib.rs` | Clear in-memory caches |
| `get_local_ip()` | `lib.rs` | Get primary local IP address |
| `get_local_ips()` | `lib.rs` | List all local network interfaces |
| `get_claude_usage_api()` | `claude_usage.rs` | Fetch rate-limit usage from Anthropic OAuth API |
| `get_claude_usage_timeline(scope, days?)` | `claude_usage.rs` | Get hourly token usage timeline from session transcripts |
| `get_claude_session_stats(scope)` | `claude_usage.rs` | Scan JSONL transcripts for aggregated token/session stats |
| `get_claude_project_list()` | `claude_usage.rs` | List Claude project slugs with session counts |
| `fetch_plugin_registry()` | `registry.rs` | Fetch remote plugin registry index |
