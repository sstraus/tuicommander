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
| `can_spawn_session` | — | `bool` | Check session limit |
| `get_orchestrator_stats` | — | `OrchestratorStats` | Active/max/available |
| `get_session_metrics` | — | `JSON` | Spawn/fail/byte counts |
| `list_active_sessions` | — | `Vec<ActiveSessionInfo>` | List all sessions |
| `list_worktrees` | — | `Vec<JSON>` | List managed worktrees |

## Git Operations (`git.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `get_repo_info` | `path` | `RepoInfo` | Repo name, branch, status |
| `get_git_diff` | `path` | `String` | Full git diff |
| `get_diff_stats` | `path` | `DiffStats` | Addition/deletion counts |
| `get_changed_files` | `path` | `Vec<ChangedFile>` | Changed files with stats |
| `get_file_diff` | `path, file` | `String` | Single file diff |
| `get_git_branches` | `path` | `Vec<JSON>` | All branches (sorted) |
| `rename_branch` | `path, old_name, new_name` | `()` | Rename branch |
| `check_is_main_branch` | `branch` | `bool` | Is main/master/develop |
| `get_initials` | `name` | `String` | 2-char repo initials |

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
| `remove_worktree` | `repo_path, branch_name` | `()` | Remove worktree |
| `get_worktree_paths` | `repo_path` | `HashMap<String,String>` | Worktree paths for repo |
| `get_worktrees_dir` | — | `String` | Worktrees base directory |
| `generate_worktree_name_cmd` | `existing_names` | `String` | Generate unique name |

## Configuration (`config.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `load_app_config` | — | `AppConfig` | Load app settings |
| `save_app_config` | `config` | `()` | Save app settings |
| `load_agent_config` | — | `AgentConfig` | Load agent config |
| `save_agent_config` | `config` | `()` | Save agent config |
| `load_notification_config` | — | `NotificationConfig` | Load notifications |
| `save_notification_config` | `config` | `()` | Save notifications |
| `load_ui_prefs` | — | `UIPrefsConfig` | Load UI preferences |
| `save_ui_prefs` | `config` | `()` | Save UI preferences |
| `load_repo_settings` | — | `RepoSettingsMap` | Load per-repo settings |
| `save_repo_settings` | `config` | `()` | Save per-repo settings |
| `check_has_custom_settings` | `path` | `bool` | Has non-default settings |
| `load_repositories` | — | `JSON` | Load saved repositories |
| `save_repositories` | `config` | `()` | Save repositories |
| `load_prompt_library` | — | `PromptLibraryConfig` | Load prompts |
| `save_prompt_library` | `config` | `()` | Save prompts |

## Agent Detection (`agent.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `detect_agent_binary` | `binary` | `AgentBinaryDetection` | Check binary in PATH |
| `detect_claude_binary` | — | `String` | Detect Claude binary |
| `detect_lazygit_binary` | — | `AgentBinaryDetection` | Detect lazygit |
| `detect_installed_ides` | — | `Vec<String>` | Detect installed IDEs |
| `open_in_app` | `path, app` | `()` | Open path in application |
| `spawn_agent` | `pty_config, agent_config` | `String` (session ID) | Spawn agent in PTY |

## Prompt Processing (`prompt.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `extract_prompt_variables` | `content` | `Vec<String>` | Parse `{{var}}` placeholders |
| `process_prompt_content` | `content, variables` | `String` | Substitute variables |

## Error Classification (`error_classification.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `classify_error_message` | `message` | `String` | Classify error type |
| `calculate_backoff_delay_cmd` | `retry_count, base, max, multiplier` | `f64` | Exponential backoff delay |

## Voice Dictation (`dictation/`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `start_dictation` | — | `()` | Start recording |
| `stop_dictation_and_transcribe` | — | `String` | Stop + transcribe |
| `inject_text` | `text` | `String` | Apply corrections |
| `get_dictation_status` | — | `DictationStatus` | Model/recording status |
| `get_model_info` | — | `Vec<ModelInfo>` | Available models |
| `download_whisper_model` | `model_name` | `String` | Download model |
| `delete_whisper_model` | `model_name` | `String` | Delete model |
| `get_correction_map` | — | `HashMap<String,String>` | Load corrections |
| `set_correction_map` | `map` | `()` | Save corrections |
| `list_audio_devices` | — | `Vec<AudioDevice>` | List input devices |
| `get_dictation_config` | — | `DictationConfig` | Load config |
| `set_dictation_config` | `config` | `()` | Save config |

## Utility Commands (`lib.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `load_config` | — | `AppConfig` | Alias for load_app_config |
| `save_config` | `config` | `()` | Alias for save_app_config |
| `hash_password` | `password` | `String` | Bcrypt hash |
| `list_markdown_files` | `path` | `Vec<String>` | List .md files in dir |
| `read_file` | `path, file` | `String` | Read file contents |
| `get_mcp_status` | — | `JSON` | MCP server status |
