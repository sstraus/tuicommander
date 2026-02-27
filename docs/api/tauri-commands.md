# Tauri Commands Reference

All commands are invoked from the frontend via `invoke(command, args)`. In browser mode, these map to HTTP endpoints (see [HTTP API](http-api.md)).

## PTY Session Management (`pty.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `create_pty` | `config: PtyConfig` | `String` (session ID) | Create PTY session |
| `create_pty_with_worktree` | `pty_config, worktree_config` | `WorktreeResult` | Create worktree + PTY |
| `write_pty` | `session_id, data` | `()` | Write to PTY |
| `resize_pty` | `session_id, rows, cols` | `()` | Resize PTY |
| `pause_pty` | `session_id` | `()` | Pause reader thread |
| `resume_pty` | `session_id` | `()` | Resume reader thread |
| `close_pty` | `session_id, cleanup_worktree` | `()` | Close PTY session |
| `can_spawn_session` | -- | `bool` | Check session limit |
| `get_orchestrator_stats` | -- | `OrchestratorStats` | Active/max/available |
| `get_session_metrics` | -- | `JSON` | Spawn/fail/byte counts |
| `list_active_sessions` | -- | `Vec<ActiveSessionInfo>` | List all sessions |
| `list_worktrees` | -- | `Vec<JSON>` | List managed worktrees |
| `get_session_foreground_process` | `session_id` | `JSON` | Get foreground process info |

## Git Operations (`git.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `get_repo_info` | `path` | `RepoInfo` | Repo name, branch, status |
| `get_git_diff` | `path` | `String` | Full git diff |
| `get_diff_stats` | `path` | `DiffStats` | Addition/deletion counts |
| `get_changed_files` | `path` | `Vec<ChangedFile>` | Changed files with stats |
| `get_file_diff` | `path, file` | `String` | Single file diff |
| `get_git_branches` | `path` | `Vec<JSON>` | All branches (sorted) |
| `get_recent_commits` | `path` | `Vec<JSON>` | Recent git commits |
| `rename_branch` | `path, old_name, new_name` | `()` | Rename branch |
| `check_is_main_branch` | `branch` | `bool` | Is main/master/develop |
| `get_initials` | `name` | `String` | 2-char repo initials |
| `run_git_command` | `path, args` | `String` | Run arbitrary git command |

## GitHub Integration (`github.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `get_github_status` | `path` | `GitHubStatus` | PR + CI for current branch |
| `get_ci_checks` | `path` | `Vec<JSON>` | CI check details |
| `get_repo_pr_statuses` | `path` | `Vec<BranchPrStatus>` | Batch PR status (all branches) |

## Worktree Management (`worktree.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `create_worktree` | `base_repo, branch_name` | `JSON` | Create git worktree |
| `remove_worktree` | `repo_path, branch_name, delete_branch?` | `()` | Remove worktree; `delete_branch` (default true) controls whether the local branch is also deleted |
| `delete_local_branch` | `repo_path, branch_name` | `()` | Delete a local branch (and its worktree if linked). Refuses to delete the default branch. Uses safe `git branch -d` |
| `check_worktree_dirty` | `repo_path, branch_name` | `bool` | Check if a branch's worktree has uncommitted changes. Returns false if no worktree exists |
| `get_worktree_paths` | `repo_path` | `HashMap<String,String>` | Worktree paths for repo |
| `get_worktrees_dir` | -- | `String` | Worktrees base directory |
| `generate_worktree_name_cmd` | `existing_names` | `String` | Generate unique name |
| `list_local_branches` | `path` | `Vec<String>` | List local branches |
| `checkout_remote_branch` | `repo_path, branch_name` | `()` | Check out a remote-only branch as a new local tracking branch |

## Configuration (`config.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `load_app_config` | -- | `AppConfig` | Load app settings |
| `save_app_config` | `config` | `()` | Save app settings |
| `load_notification_config` | -- | `NotificationConfig` | Load notifications |
| `save_notification_config` | `config` | `()` | Save notifications |
| `load_ui_prefs` | -- | `UIPrefsConfig` | Load UI preferences |
| `save_ui_prefs` | `config` | `()` | Save UI preferences |
| `load_repo_settings` | -- | `RepoSettingsMap` | Load per-repo settings |
| `save_repo_settings` | `config` | `()` | Save per-repo settings |
| `check_has_custom_settings` | `path` | `bool` | Has non-default settings |
| `load_repo_defaults` | -- | `RepoDefaultsConfig` | Load repo defaults |
| `save_repo_defaults` | `config` | `()` | Save repo defaults |
| `load_repositories` | -- | `JSON` | Load saved repositories |
| `save_repositories` | `config` | `()` | Save repositories |
| `load_prompt_library` | -- | `PromptLibraryConfig` | Load prompts |
| `save_prompt_library` | `config` | `()` | Save prompts |
| `load_notes` | -- | `JSON` | Load notes |
| `save_notes` | `config` | `()` | Save notes |
| `load_keybindings` | -- | `JSON` | Load keybinding overrides |
| `save_keybindings` | `config` | `()` | Save keybinding overrides |
| `load_agents_config` | -- | `AgentsConfig` | Load per-agent run configs |
| `save_agents_config` | `config` | `()` | Save per-agent run configs |

## Agent Detection (`agent.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `detect_agent_binary` | `binary` | `AgentBinaryDetection` | Check binary in PATH |
| `detect_all_agent_binaries` | -- | `Vec<AgentBinaryDetection>` | Detect all known agents |
| `detect_claude_binary` | -- | `String` | Detect Claude binary |
| `detect_lazygit_binary` | -- | `AgentBinaryDetection` | Detect lazygit |
| `detect_installed_ides` | -- | `Vec<String>` | Detect installed IDEs |
| `open_in_app` | `path, app` | `()` | Open path in application |
| `spawn_agent` | `pty_config, agent_config` | `String` (session ID) | Spawn agent in PTY |

## Agent MCP Configuration (`agent_mcp.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `get_agent_mcp_status` | `agent` | `AgentMcpStatus` | Check MCP config for an agent |
| `install_agent_mcp` | `agent` | `String` | Install TUICommander MCP entry |
| `remove_agent_mcp` | `agent` | `String` | Remove TUICommander MCP entry |
| `get_agent_config_path` | `agent` | `String` | Get agent's MCP config file path |

## Prompt Processing (`prompt.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `extract_prompt_variables` | `content` | `Vec<String>` | Parse `{{var}}` placeholders |
| `process_prompt_content` | `content, variables` | `String` | Substitute variables |

## Claude Usage (`claude_usage.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `get_claude_usage_api` | -- | `UsageApiResponse` | Fetch rate-limit usage from Anthropic OAuth API |
| `get_claude_usage_timeline` | `scope, days?` | `Vec<TimelinePoint>` | Hourly token usage from session transcripts |
| `get_claude_session_stats` | `scope` | `SessionStats` | Aggregated token/session stats from JSONL transcripts |
| `get_claude_project_list` | -- | `Vec<ProjectEntry>` | List project slugs with session counts |

`scope` values: `"all"` (all projects) or a specific project slug. `days` defaults to 7.

Uses incremental parsing with a file-size-based cache (`claude-usage-cache.json`) so only newly appended JSONL data is processed on each call. The cache is persisted across app restarts.

## Voice Dictation (`dictation/`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `start_dictation` | -- | `()` | Start recording |
| `stop_dictation_and_transcribe` | -- | `String` | Stop + transcribe |
| `inject_text` | `text` | `String` | Apply corrections |
| `get_dictation_status` | -- | `DictationStatus` | Model/recording status |
| `get_model_info` | -- | `Vec<ModelInfo>` | Available models |
| `download_whisper_model` | `model_name` | `String` | Download model |
| `delete_whisper_model` | `model_name` | `String` | Delete model |
| `get_correction_map` | -- | `HashMap<String,String>` | Load corrections |
| `set_correction_map` | `map` | `()` | Save corrections |
| `list_audio_devices` | -- | `Vec<AudioDevice>` | List input devices |
| `get_dictation_config` | -- | `DictationConfig` | Load config |
| `set_dictation_config` | `config` | `()` | Save config |

## Filesystem (`fs.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `resolve_terminal_path` | `path` | `String` | Resolve terminal path |
| `list_directory` | `path` | `Vec<DirEntry>` | List directory contents |
| `fs_read_file` | `path` | `String` | Read file contents |
| `write_file` | `path, content` | `()` | Write file |
| `create_directory` | `path` | `()` | Create directory |
| `delete_path` | `path` | `()` | Delete file or directory |
| `rename_path` | `src, dest` | `()` | Rename/move path |
| `copy_path` | `src, dest` | `()` | Copy file or directory |
| `add_to_gitignore` | `path, pattern` | `()` | Add pattern to .gitignore |

## Plugin Management (`plugins.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `list_user_plugins` | -- | `Vec<PluginManifest>` | List valid plugin manifests |
| `get_plugin_readme_path` | `id` | `Option<String>` | Get plugin README.md path |
| `read_plugin_data` | `plugin_id, path` | `Option<String>` | Read plugin data file |
| `write_plugin_data` | `plugin_id, path, content` | `()` | Write plugin data file |
| `delete_plugin_data` | `plugin_id, path` | `()` | Delete plugin data file |
| `install_plugin_from_zip` | `path` | `PluginManifest` | Install from local ZIP |
| `install_plugin_from_url` | `url` | `PluginManifest` | Install from HTTPS URL |
| `uninstall_plugin` | `id` | `()` | Remove plugin and all files |

## Plugin Filesystem (`plugin_fs.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `plugin_read_file` | `path, plugin_id` | `String` | Read file as UTF-8 (within $HOME, 10 MB limit) |
| `plugin_read_file_tail` | `path, max_bytes, plugin_id` | `String` | Read last N bytes of file, skip partial first line |
| `plugin_list_directory` | `path, pattern?, plugin_id` | `Vec<String>` | List filenames in directory (optional glob filter) |
| `plugin_watch_path` | `path, plugin_id, recursive?, debounce_ms?` | `String` (watch ID) | Start watching path for changes |
| `plugin_unwatch` | `watch_id, plugin_id` | `()` | Stop watching a path |

## Plugin HTTP (`plugin_http.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `plugin_http_fetch` | `url, method?, headers?, body?, allowed_urls, plugin_id` | `HttpResponse` | Make HTTP request (validated against allowed_urls) |

## Plugin CLI Execution (`plugin_exec.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `plugin_exec_cli` | `binary, args, cwd?, plugin_id` | `String` | Execute whitelisted CLI binary, return stdout. Allowed: `mdkb`. 30s timeout, 5 MB limit. |

## Plugin Credentials (`plugin_credentials.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `plugin_read_credential` | `service_name, plugin_id` | `String?` | Read credential from system store (Keychain/file) |

## Plugin Registry (`registry.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `fetch_plugin_registry` | -- | `Vec<RegistryEntry>` | Fetch remote plugin registry index |

## Watchers

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `start_head_watcher` | `path` | `()` | Watch .git/HEAD for branch changes |
| `stop_head_watcher` | `path` | `()` | Stop watching .git/HEAD |
| `start_repo_watcher` | `path` | `()` | Watch .git/ for repo changes |
| `stop_repo_watcher` | `path` | `()` | Stop watching .git/ |

## System (`lib.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `load_config` | -- | `AppConfig` | Alias for load_app_config |
| `save_config` | `config` | `()` | Alias for save_app_config |
| `hash_password` | `password` | `String` | Bcrypt hash |
| `list_markdown_files` | `path` | `Vec<MarkdownFileEntry>` | List .md files in dir |
| `read_file` | `path, file` | `String` | Read file contents |
| `get_mcp_status` | -- | `JSON` | MCP server status |
| `clear_caches` | -- | `()` | Clear in-memory caches |
| `get_local_ip` | -- | `Option<String>` | Get primary local IP |
| `get_local_ips` | -- | `Vec<LocalIpEntry>` | List local network interfaces |
| `regenerate_session_token` | -- | `String` | Regenerate MCP session token |
| `block_sleep` | -- | `()` | Prevent system sleep |
| `unblock_sleep` | -- | `()` | Allow system sleep |
