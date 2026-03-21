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
| `update_session_cwd` | `session_id, cwd` | `()` | Update session working directory (from OSC 7) |
| `get_session_foreground_process` | `session_id` | `JSON` | Get foreground process info |
| `get_kitty_flags` | `session_id` | `u32` | Get Kitty keyboard protocol flags for session |
| `get_last_prompt` | `session_id` | `Option<String>` | Get last user-typed prompt from input line buffer |

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
| `get_merged_branches` | `repo_path` | `Vec<String>` | Branches merged into default branch |
| `get_repo_summary` | `repo_path` | `RepoSummary` | Aggregate snapshot: worktree paths + merged branches + per-path diff stats in one IPC |
| `get_repo_structure` | `repo_path` | `RepoStructure` | Fast phase: worktree paths + merged branches only (Phase 1 of progressive loading) |
| `get_repo_diff_stats` | `repo_path` | `RepoDiffStats` | Slow phase: per-worktree diff stats + last commit timestamps (Phase 2 of progressive loading) |
| `run_git_command` | `path, args` | `GitCommandResult` | Run arbitrary git command (success, stdout, stderr, exit_code) |
| `get_git_panel_context` | `path` | `GitPanelContext` | Rich context for Git Panel (branch, ahead/behind, staged/changed/stash counts, last commit, rebase/cherry-pick state). Cached 5s TTL. |
| `get_working_tree_status` | `path` | `WorkingTreeStatus` | Full porcelain v2 status: branch, upstream, ahead/behind, stash count, staged/unstaged entries, untracked files |
| `git_stage_files` | `path, files` | `()` | Stage files (`git add`). Path-traversal validated |
| `git_unstage_files` | `path, files` | `()` | Unstage files (`git restore --staged`). Path-traversal validated |
| `git_discard_files` | `path, files` | `()` | Discard working tree changes (`git restore`). Destructive. Path-traversal validated |
| `git_commit` | `path, message, amend?` | `String` (commit hash) | Commit staged changes; optional `--amend`. Returns new HEAD hash |
| `get_commit_log` | `path, count?, after?` | `Vec<CommitLogEntry>` | Paginated commit log (default 50, max 500). `after` is a commit hash for cursor-based pagination |
| `get_stash_list` | `path` | `Vec<StashEntry>` | List stash entries (index, ref_name, message, hash) |
| `get_file_history` | `path, file, count?, after?` | `Vec<CommitLogEntry>` | Per-file commit log following renames (default 50, max 500) |
| `get_file_blame` | `path, file` | `Vec<BlameLine>` | Per-line blame: hash, author, author_time (unix), line_number, content |
| `get_branches_detail` | `path` | `Vec<BranchDetail>` | Rich branch listing: name, ahead/behind, last commit date, tracking upstream, merged status |
| `delete_branch` | `path, name, force` | `()` | Delete a local branch. `force=false` uses safe `-d`; `force=true` uses `-D`. Refuses to delete the current branch or default branch |
| `create_branch` | `path, name, start_point, checkout` | `()` | Create a new branch from `start_point` (defaults to HEAD). `checkout=true` switches to it immediately |
| `get_recent_branches` | `path, limit` | `Vec<String>` | Recently checked-out branches from reflog, ordered by recency |

## Commit Graph (`git_graph.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `get_commit_graph` | `path, count?` | `Vec<GraphNode>` | Lane-assigned commit graph for visual rendering. Default 200, max 1000. Returns hash, column, row, color_index (0–7), parents, refs, and connection metadata (from/to col/row) for Bezier curve drawing |

## GitHub Authentication (`github_auth.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `github_start_login` | — | `DeviceCodeResponse` | Start OAuth Device Flow, returns user/device code |
| `github_poll_login` | `device_code` | `PollResult` | Poll for token; saves to keyring on success |
| `github_logout` | — | `()` | Delete OAuth token from keyring, fall back to env/CLI |
| `github_auth_status` | — | `AuthStatus` | Current auth: login, avatar, source, scopes |

## GitHub Integration (`github.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `get_github_status` | `path` | `GitHubStatus` | PR + CI for current branch |
| `get_ci_checks` | `path` | `Vec<JSON>` | CI check details |
| `get_repo_pr_statuses` | `path, include_merged` | `Vec<BranchPrStatus>` | Batch PR status (all branches) |
| `approve_pr` | `repo_path, pr_number` | `String` | Submit approving review via GitHub API |
| `merge_pr_via_github` | `repo_path, pr_number, merge_method` | `String` | Merge PR via GitHub API |
| `get_all_pr_statuses` | `path` | `Vec<BranchPrStatus>` | Batch PR status for all branches (includes merged) |
| `get_pr_diff` | `repo_path, pr_number` | `String` | Get PR diff content |
| `check_github_circuit` | `path` | `CircuitState` | Check GitHub API circuit breaker state |

## Worktree Management (`worktree.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `create_worktree` | `base_repo, branch_name` | `JSON` | Create git worktree |
| `remove_worktree` | `repo_path, branch_name, delete_branch?` | `()` | Remove worktree; `delete_branch` (default true) controls whether the local branch is also deleted. Archive script resolved from config (not IPC). |
| `delete_local_branch` | `repo_path, branch_name` | `()` | Delete a local branch (and its worktree if linked). Refuses to delete the default branch. Uses safe `git branch -d` |
| `check_worktree_dirty` | `repo_path, branch_name` | `bool` | Check if a branch's worktree has uncommitted changes. Returns false if no worktree exists |
| `get_worktree_paths` | `repo_path` | `HashMap<String,String>` | Worktree paths for repo |
| `get_worktrees_dir` | -- | `String` | Worktrees base directory |
| `generate_worktree_name_cmd` | `existing_names` | `String` | Generate unique name |
| `list_local_branches` | `path` | `Vec<String>` | List local branches |
| `checkout_remote_branch` | `repo_path, branch_name` | `()` | Check out a remote-only branch as a new local tracking branch |
| `detect_orphan_worktrees` | `repo_path` | `Vec<String>` | Detect worktrees in detached HEAD state (branch deleted) |
| `remove_orphan_worktree` | `repo_path, worktree_path` | `()` | Remove an orphan worktree by filesystem path (validated against repo) |
| `switch_branch` | `repo_path, branch_name` | `()` | Switch main worktree to a different branch (with dirty-state and process checks) |
| `merge_and_archive_worktree` | `repo_path, branch_name` | `MergeResult` | Merge worktree branch into base and archive |
| `finalize_merged_worktree` | `repo_path, branch_name` | `()` | Clean up worktree after merge (delete branch + worktree) |
| `list_base_ref_options` | `repo_path` | `Vec<String>` | List valid base refs for worktree creation |
| `run_setup_script` | `repo_path, worktree_path` | `()` | Run post-creation setup script in new worktree |
| `generate_clone_branch_name_cmd` | `base_name, existing_names` | `String` | Generate hybrid branch name for clone worktree |

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
| `save_note_image` | `note_id, data_base64, extension` | `String` (absolute path) | Decode base64 image, validate ≤10 MB, write to `config_dir()/note-images/<note_id>/<timestamp>.<ext>` |
| `delete_note_assets` | `note_id` | `()` | Remove `note-images/<note_id>/` directory recursively (no-op if missing) |
| `get_note_images_dir` | -- | `String` | Return `config_dir()/note-images/` absolute path |
| `load_keybindings` | -- | `JSON` | Load keybinding overrides |
| `save_keybindings` | `config` | `()` | Save keybinding overrides |
| `load_agents_config` | -- | `AgentsConfig` | Load per-agent run configs |
| `save_agents_config` | `config` | `()` | Save per-agent run configs |
| `load_activity` | -- | `ActivityConfig` | Load activity dashboard state |
| `save_activity` | `config` | `()` | Save activity dashboard state |
| `load_repo_local_config` | `repo_path` | `RepoLocalConfig?` | Read `.tuic.json` from repo root; returns null if absent or malformed |

## Agent Detection (`agent.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `detect_agent_binary` | `binary` | `AgentBinaryDetection` | Check binary in PATH |
| `detect_all_agent_binaries` | -- | `Vec<AgentBinaryDetection>` | Detect all known agents |
| `detect_claude_binary` | -- | `String` | Detect Claude binary |
| `detect_installed_ides` | -- | `Vec<String>` | Detect installed IDEs |
| `open_in_app` | `path, app` | `()` | Open path in application |
| `spawn_agent` | `pty_config, agent_config` | `String` (session ID) | Spawn agent in PTY |
| `discover_agent_session` | `session_id, agent_type, cwd` | `Option<String>` | Discover agent session UUID from filesystem for session-aware resume |
| `verify_agent_session` | `agent_type, session_id, cwd` | `bool` | Verify if a specific agent session file exists on disk (for TUIC_SESSION resume) |

## MCP Upstream Proxy (`mcp_upstream_config.rs`, `mcp_upstream_credentials.rs`)

Commands for managing upstream MCP servers proxied through TUICommander's `/mcp` endpoint.

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `load_mcp_upstreams` | -- | `UpstreamMcpConfig` | Load upstream config from `mcp-upstreams.json` |
| `save_mcp_upstreams` | `config: UpstreamMcpConfig` | `()` | Validate, persist, and hot-reload upstream config. Errors if validation fails |
| `reconnect_mcp_upstream` | `name: String` | `()` | Disconnect and reconnect a single upstream by name. Useful after credential changes or transient failures |
| `get_mcp_upstream_status` | -- | `Vec<UpstreamStatus>` | Get live status of all upstream MCP servers |
| `save_mcp_upstream_credential` | `name: String, token: String` | `()` | Store a Bearer token for an upstream in the OS keyring |
| `delete_mcp_upstream_credential` | `name: String` | `()` | Remove a Bearer token from the OS keyring (idempotent) |

### UpstreamMcpConfig schema

```typescript
interface UpstreamMcpConfig {
  servers: UpstreamMcpServer[];
}

interface UpstreamMcpServer {
  id: string;              // Unique UUID, used for config diff tracking
  name: string;            // Namespace prefix — must match [a-z0-9_-]+
  transport: UpstreamTransport;
  enabled: boolean;        // Default: true
  timeout_secs: number;    // Default: 30 (0 = no timeout, HTTP only)
  tool_filter?: ToolFilter; // Optional allow/deny filter
}

type UpstreamTransport =
  | { type: "http"; url: string }
  | { type: "stdio"; command: string; args: string[]; env: Record<string, string> };

interface ToolFilter {
  mode: "allow" | "deny";
  patterns: string[];  // Exact names or trailing-* glob prefix patterns
}
```

### Upstream status values

The live registry exposes status via SSE events (`upstream_status_changed`). Valid status strings:

| Value | Meaning |
|-------|---------|
| `connecting` | Handshake in progress |
| `ready` | Tools available |
| `circuit_open` | Circuit breaker open, backoff active |
| `disabled` | Disabled in config |
| `failed` | Permanently failed, manual reconnect required |

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
| `stop_dictation_and_transcribe` | -- | `TranscribeResponse` | Stop + transcribe. Returns `{text, skip_reason?, duration_s}` |
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
| `check_microphone_permission` | -- | `String` | Check macOS microphone TCC permission status |
| `open_microphone_settings` | -- | `()` | Open macOS System Settings > Privacy > Microphone |

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
| `search_files` | `path, query` | `Vec<SearchResult>` | Search files by name in directory |
| `search_content` | `repoPath, query, caseSensitive?, useRegex?, wholeWord?, limit?` | `()` | Full-text content search; streams results progressively via `content-search-batch` events. Binary files and files >1 MB are skipped. Supports cancellation. |

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
| `install_plugin_from_folder` | `path` | `PluginManifest` | Install from local folder |

## Plugin Filesystem (`plugin_fs.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `plugin_read_file` | `path, plugin_id` | `String` | Read file as UTF-8 (within $HOME, 10 MB limit) |
| `plugin_read_file_tail` | `path, max_bytes, plugin_id` | `String` | Read last N bytes of file, skip partial first line |
| `plugin_list_directory` | `path, pattern?, plugin_id` | `Vec<String>` | List filenames in directory (optional glob filter) |
| `plugin_watch_path` | `path, plugin_id, recursive?, debounce_ms?` | `String` (watch ID) | Start watching path for changes |
| `plugin_unwatch` | `watch_id, plugin_id` | `()` | Stop watching a path |
| `plugin_write_file` | `path, content, plugin_id` | `()` | Write file within $HOME (path-traversal validated) |
| `plugin_rename_path` | `src, dest, plugin_id` | `()` | Rename/move path within $HOME (path-traversal validated) |

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
| `start_dir_watcher` | `path` | `()` | Watch directory for file changes (non-recursive) |
| `stop_dir_watcher` | `path` | `()` | Stop watching directory |

## System (`lib.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `load_config` | -- | `AppConfig` | Alias for load_app_config |
| `save_config` | `config` | `()` | Alias for save_app_config |
| `hash_password` | `password` | `String` | Bcrypt hash |
| `list_markdown_files` | `path` | `Vec<MarkdownFileEntry>` | List .md files in dir |
| `read_file` | `path, file` | `String` | Read file contents |
| `get_mcp_status` | -- | `JSON` | MCP server status (no token — use `get_connect_url` for QR) |
| `get_connect_url` | `ip` | `String` | Build QR connect URL server-side (token stays in backend) |
| `check_update_channel` | `channel` | `UpdateCheckResult` | Check beta/nightly channel for updates (hardcoded URLs, SSRF-safe) |
| `clear_caches` | -- | `()` | Clear in-memory caches |
| `get_local_ip` | -- | `Option<String>` | Get primary local IP |
| `get_local_ips` | -- | `Vec<LocalIpEntry>` | List local network interfaces |
| `regenerate_session_token` | -- | `()` | Regenerate MCP session token (invalidates all remote sessions) |
| `fetch_update_manifest` | `url` | `JSON` | Fetch update manifest via Rust HTTP (bypasses WebView CSP) |
| `read_external_file` | `path` | `String` | Read file outside repo (standalone file open) |
| `get_relay_status` | -- | `JSON` | Cloud relay connection status |

## App Logger (`app_logger.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `push_log` | `level, source, message` | `()` | Push entry to ring buffer (survives webview reloads) |
| `get_logs` | `level?, source?, limit?` | `Vec<LogEntry>` | Query ring buffer with optional filters |
| `clear_logs` | -- | `()` | Flush all log entries |

## Notification Sound (`notification_sound.rs`)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `play_notification_sound` | `sound_type` | `()` | Play notification sound via Rust rodio (types: completion, question, error, info) |
| `block_sleep` | -- | `()` | Prevent system sleep |
| `unblock_sleep` | -- | `()` | Allow system sleep |
