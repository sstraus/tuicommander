# HTTP API Reference

REST API served by the Axum HTTP server when MCP server is enabled. All Tauri commands are accessible as HTTP endpoints.

## Base URL

`http://localhost:{port}` — port written to `<config_dir>/mcp-port` file on startup.

## Authentication

- **MCP mode (localhost):** No authentication
- **Remote access mode:** HTTP Basic Auth with configured username/password

## Session Endpoints

### List Sessions

```
GET /sessions
```

Returns array of active session info (ID, cwd, worktree path, branch).

### Create Session

```
POST /sessions
Content-Type: application/json

{
  "rows": 24,
  "cols": 80,
  "shell": "/bin/zsh",    // optional
  "cwd": "/path/to/dir"   // optional
}
```

Returns `{ "session_id": "..." }`.

### Create Session with Worktree

```
POST /sessions/worktree
Content-Type: application/json

{ "pty_config": { ... }, "worktree_config": { ... } }
```

Creates a git worktree and a PTY session in one call.

### Spawn Agent Session

```
POST /sessions/agent
Content-Type: application/json

{ "pty_config": { ... }, "agent_config": { ... } }
```

Spawns an AI agent (Claude, etc.) in a PTY session.

### Write to Session

```
POST /sessions/:id/write
Content-Type: application/json

{ "data": "ls -la\n" }
```

### Resize Session

```
POST /sessions/:id/resize
Content-Type: application/json

{ "rows": 30, "cols": 120 }
```

### Read Output

```
GET /sessions/:id/output?limit=4096
```

Returns recent output from the ring buffer (up to 64KB).

### Foreground Process

```
GET /sessions/:id/foreground
```

Returns the foreground process info for a session.

### Pause/Resume

```
POST /sessions/:id/pause
POST /sessions/:id/resume
```

### Close Session

```
DELETE /sessions/:id?cleanup_worktree=false
```

## Streaming Endpoints

### WebSocket PTY Stream

```
WS /sessions/:id/stream
```

Receives real-time PTY output as text frames. One WebSocket per session.

### MCP Streamable HTTP

```
POST /mcp
Content-Type: application/json

{ JSON-RPC message }
```

Single endpoint for all MCP JSON-RPC requests (initialize, tools/list, tools/call). Returns JSON-RPC responses directly in the HTTP response body. Session ID returned via `Mcp-Session-Id` header on initialize.

```
GET /mcp          → 405 Method Not Allowed
DELETE /mcp       → Ends MCP session (pass Mcp-Session-Id header)
```

## Git Endpoints

### Repository Info

```
GET /repo/info?path=/path/to/repo
```

Returns `RepoInfo` (name, branch, status, initials).

### Git Diff

```
GET /repo/diff?path=/path/to/repo
```

Returns unified diff string.

### Diff Stats

```
GET /repo/diff-stats?path=/path/to/repo
```

Returns `{ "additions": N, "deletions": N }`.

### Changed Files

```
GET /repo/files?path=/path/to/repo
```

Returns array of `ChangedFile` (path, status, additions, deletions).

### Single File Diff

```
GET /repo/file-diff?path=/path/to/repo&file=src/main.rs
```

Returns diff for a single file.

### Read File

```
GET /repo/file?path=/path/to/repo&file=src/main.rs
```

Returns file contents as text.

### Branches

```
GET /repo/branches?path=/path/to/repo
```

Returns sorted branch list.

### Local Branches

```
GET /repo/local-branches?path=/path/to/repo
```

Returns local branch list.

### Rename Branch

```
POST /repo/branch/rename
Content-Type: application/json

{ "path": "/path/to/repo", "old_name": "old", "new_name": "new" }
```

### Check Main Branch

```
GET /repo/is-main-branch?branch=main
```

Returns `true` if the branch is main/master/develop.

### Initials

```
GET /repo/initials?name=my-repo
```

Returns 2-char repo initials.

### Markdown Files

```
GET /repo/markdown-files?path=/path/to/repo
```

Returns list of `.md` files in a directory.

### Recent Commits

```
GET /repo/recent-commits?path=/path/to/repo
```

Returns recent git commits.

### GitHub Status

```
GET /repo/github?path=/path/to/repo
```

Returns PR status, CI status, ahead/behind for current branch.

### PR Statuses (Batch)

```
GET /repo/prs?path=/path/to/repo
```

Returns `BranchPrStatus[]` for all branches with open PRs.

### CI Checks

```
GET /repo/ci?path=/path/to/repo
```

Returns detailed CI check list.

## Configuration Endpoints

### App Config

```
GET /config
PUT /config
```

Load/save `AppConfig`.

### Hash Password

```
POST /config/hash-password
Content-Type: application/json

{ "password": "..." }
```

Returns bcrypt hash string.

### Notification Config

```
GET /config/notifications
PUT /config/notifications
```

Load/save `NotificationConfig`.

### UI Preferences

```
GET /config/ui-prefs
PUT /config/ui-prefs
```

Load/save `UIPrefsConfig`.

### Repository Settings

```
GET /config/repo-settings
PUT /config/repo-settings
```

Load/save per-repository settings.

### Check Custom Settings

```
GET /config/repo-settings/has-custom?path=/path/to/repo
```

Returns `true` if the repo has non-default settings.

### Repositories

```
GET /config/repositories
PUT /config/repositories
```

Load/save the repositories list.

### Prompt Library

```
GET /config/prompt-library
PUT /config/prompt-library
```

Load/save prompt entries.

### Notes

```
GET /config/notes
PUT /config/notes
```

Load/save notes (opaque JSON, shape defined by frontend).

### MCP Status

```
GET /mcp/status
```

Returns MCP server status (enabled, port, connected clients).

## Filesystem Endpoints

```
GET  /fs/list?path=/path/to/dir
GET  /fs/read?path=/path/to/file
POST /fs/write         { "path": "...", "content": "..." }
POST /fs/mkdir         { "path": "..." }
POST /fs/delete        { "path": "..." }
POST /fs/rename        { "src": "...", "dest": "..." }
POST /fs/copy          { "src": "...", "dest": "..." }
POST /fs/gitignore     { "path": "...", "pattern": "..." }
```

Sandboxed filesystem operations for the file manager panel.

## Monitoring Endpoints

### Health Check

```
GET /health
```

Returns `{ "status": "ok" }`.

### Orchestrator Stats

```
GET /stats
```

Returns `{ "active_sessions": N, "max_sessions": 50, "available_slots": N }`.

### Session Metrics

```
GET /metrics
```

Returns `{ "total_spawned": N, "failed_spawns": N, "bytes_emitted": N, "pauses_triggered": N }`.

### Local IPs

```
GET /system/local-ips
```

Returns list of local network interfaces and addresses.

## Agent Endpoints

### Detect All Agents

```
GET /agents
```

Returns detected agent binaries and installed IDEs.

### Detect Specific Agent

```
GET /agents/detect?binary=claude
```

Returns detection result for a specific agent binary.

### Detect Installed IDEs

```
GET /agents/ides
```

Returns list of installed IDEs.

## Prompt Endpoints

### Process Prompt

```
POST /prompt/process
Content-Type: application/json

{ "content": "...", "variables": { ... } }
```

Substitutes `{{var}}` placeholders in prompt text.

### Extract Variables

```
POST /prompt/extract-variables
Content-Type: application/json

{ "content": "..." }
```

Returns list of `{{var}}` placeholder names found in content.

## Plugin Endpoints

### List Plugins

```
GET /plugins/list
```

Returns array of valid plugin manifests.

### Plugin Development Guide

```
GET /plugins/docs
```

Returns the complete plugin development reference as `{"content": "..."}`. AI-optimized documentation covering manifest format, PluginHost API, structured event types, and example plugins.

### Plugin Data

```
GET /api/plugins/:plugin_id/data/*path
```

Reads a plugin's stored data file. Returns `application/json` if content starts with `{` or `[`, otherwise `text/plain`. Returns 404 if the file doesn't exist. Goes through the same auth middleware as all other routes.

**Note:** Write and delete operations are only available via Tauri commands (`write_plugin_data`, `delete_plugin_data`), not as HTTP endpoints. Data is sandboxed to `~/.config/tuicommander/plugins/{plugin_id}/data/`.

## Worktree Endpoints

### List Worktrees

```
GET /worktrees
```

Returns list of managed worktrees.

### Create Worktree

```
POST /worktrees
Content-Type: application/json

{ "base_repo": "/path", "branch_name": "feature-x" }
```

### Worktrees Base Directory

```
GET /worktrees/dir
```

Returns the base directory where worktrees are created.

### Get Worktree Paths

```
GET /worktrees/paths?path=/path/to/repo
```

Returns `{ "branch-name": "/worktree/path", ... }`.

### Generate Worktree Name

```
POST /worktrees/generate-name
Content-Type: application/json

{ "existing_names": ["name1", "name2"] }
```

Returns a unique worktree name.

### Remove Worktree

```
DELETE /worktrees/:branch
Content-Type: application/json

{ "repo_path": "/path" }
```

## Tauri-Only Commands (No HTTP Route)

The following commands are accessible only via the Tauri `invoke()` bridge in the desktop app. They have no HTTP endpoint.

| Command | Module | Description |
|---------|--------|-------------|
| `get_claude_usage_api` | `claude_usage.rs` | Fetch rate-limit usage from Anthropic OAuth API |
| `get_claude_usage_timeline` | `claude_usage.rs` | Get hourly token usage timeline from session transcripts |
| `get_claude_session_stats` | `claude_usage.rs` | Scan session transcripts for aggregated token/session stats |
| `get_claude_project_list` | `claude_usage.rs` | List Claude project slugs with session counts |
| `plugin_read_file` | `plugin_fs.rs` | Read file as UTF-8 (within $HOME, 10 MB limit) |
| `plugin_read_file_tail` | `plugin_fs.rs` | Read last N bytes of file, skip partial first line |
| `plugin_list_directory` | `plugin_fs.rs` | List filenames in directory (optional glob filter) |
| `plugin_watch_path` | `plugin_fs.rs` | Start watching path for changes |
| `plugin_unwatch` | `plugin_fs.rs` | Stop watching a path |
| `plugin_http_fetch` | `plugin_http.rs` | Make HTTP request (validated against allowed_urls) |
| `plugin_read_credential` | `plugin_credentials.rs` | Read credential from system store |
| `fetch_plugin_registry` | `registry.rs` | Fetch remote plugin registry index |
| `install_plugin_from_zip` | `plugins.rs` | Install plugin from local ZIP file |
| `install_plugin_from_url` | `plugins.rs` | Install plugin from HTTPS URL |
| `uninstall_plugin` | `plugins.rs` | Remove a plugin and all its files |
| `get_agent_mcp_status` | `agent_mcp.rs` | Check MCP config status for an agent |
| `install_agent_mcp` | `agent_mcp.rs` | Install TUICommander MCP entry in agent config |
| `remove_agent_mcp` | `agent_mcp.rs` | Remove TUICommander MCP entry from agent config |
