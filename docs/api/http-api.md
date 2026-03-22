# HTTP API Reference

REST API served by the Axum HTTP server when MCP server is enabled. All Tauri commands are accessible as HTTP endpoints.

## Base URL

- **Local (Unix socket):** `<config_dir>/mcp.sock` — always started on macOS/Linux. No auth, MCP always enabled. Used by the local MCP bridge binary.
- **Remote (TCP):** `http://<host>:{remote_access_port}` — only started when remote access is enabled in settings. HTTP Basic Auth required.

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
GET /sessions/:id/output?limit=4096&format=text
```

Returns recent output. Format controls what is returned:

| `format` | Response shape | Description |
|----------|----------------|-------------|
| (omit) | `{ "data": "<bytes>" }` | Raw PTY bytes, base64-encoded |
| `text` | `{ "data": "<string>" }` | ANSI-stripped plain text from ring buffer |
| `log` | `{ "lines": [...], "total_lines": N }` | VT100-extracted clean lines (no ANSI, no TUI garbage) |

| Param | Default | Description |
|-------|---------|-------------|
| `limit` | (all) | `raw`/`text`: max bytes; `log`: max lines to return (newest N) |
| `format` | (raw) | See table above |

`format=log` reads from `VtLogBuffer` — a VT100-aware buffer that extracts only scrolled-off lines, suppressing alternate-screen TUI apps (vim, htop, claude). Ideal for mobile clients.

### Kitty Protocol Flags

```
GET /sessions/:id/kitty-flags
```

Returns the current Kitty keyboard protocol flags (integer) for a session.

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

### Rename Session

```
PUT /sessions/:id/name
Content-Type: application/json

{ "name": "my-session" }
```

Sets a custom display name for a session.

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

### WebSocket JSON Framing (Mobile/Browser)

WebSocket connections to `/sessions/:id/stream` receive JSON-framed messages:

```json
{"type": "output", "data": "raw terminal output text"}
{"type": "parsed", "event": {"type": "question", "text": "Allow?"}}
{"type": "exit"}
{"type": "closed"}
```

Frame types:
- `output` — Raw PTY output (ANSI-stripped when `?format=text`)
- `log` — VT100-extracted clean lines batch (when `?format=log`): `{"type":"log","lines":[...],"offset":N}`
- `parsed` — Structured events (questions, rate limits, errors) from the output parser
- `exit` — Session process exited
- `closed` — Session was closed

#### WebSocket format=log

```
WS /sessions/:id/stream?format=log
```

When `?format=log` is specified, the connection streams VT100-extracted log lines instead of raw PTY chunks:
- On connect: sends all accumulated lines as a single catch-up frame
- While running: polls every 200ms and sends new lines batched by offset
- PTY input passthrough is still available (write text/binary frames to send to PTY)

### Server-Sent Events (SSE)

```
GET /events?types=repo-changed,pty-parsed
```

Broadcasts server-side events to all browser/mobile clients. Supports optional `?types=` query parameter for comma-separated event name filtering. Uses monotonic event IDs and 15-second keep-alive pings.

| Event | Payload | Description |
|-------|---------|-------------|
| `session-created` | `{session_id, cwd}` | New session started |
| `session-closed` | `{session_id}` | Session ended |
| `repo-changed` | `{repo_path}` | Git repository state changed |
| `head-changed` | `{repo_path, branch}` | Git HEAD changed (branch switch) |
| `pty-parsed` | `{session_id, parsed}` | Structured output event from PTY parser |
| `pty-exit` | `{session_id}` | PTY process exited |
| `plugin-changed` | `{plugin_ids}` | Plugin(s) installed/removed/updated |
| `upstream-status-changed` | `{name, status}` | MCP upstream server status change |
| `mcp-toast` | `{title, message, level}` | Toast notification from MCP layer |
| `lagged` | `{missed}` | Client fell behind; N events were dropped |

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

### Repo Summary

```
GET /repo/summary?path=/path/to/repo
```

Aggregate snapshot: worktree paths, merged branches, and per-path diff stats in one round-trip. Replaces 3+ separate IPC calls.

### Repo Structure (Progressive Phase 1)

```
GET /repo/structure?path=/path/to/repo
```

Returns `{ "worktree_paths": { "branch": "/path", ... }, "merged_branches": ["branch", ...] }`. Fast path — no diff stats computation.

### Repo Diff Stats (Progressive Phase 2)

```
GET /repo/diff-stats/batch?path=/path/to/repo
```

Returns `{ "diff_stats": { "/path": { "additions": N, "deletions": N }, ... }, "last_commit_ts": { "branch": N, ... } }`. Slow path — computes per-worktree diff stats and last commit timestamps.

### Local Branches

```
GET /repo/local-branches?path=/path/to/repo
```

Returns local branch list.

### Checkout Remote Branch

```
POST /repo/checkout-remote
Content-Type: application/json

{ "repoPath": "/path/to/repo", "branchName": "feat-remote" }
```

Creates a local tracking branch from `origin/<branchName>`.

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

### PR Statuses (Multi-Repo Batch)

```
POST /repo/prs/batch
Content-Type: application/json

{ "paths": ["/repo1", "/repo2"], "include_merged": false }
```

Returns aggregated PR statuses across multiple repositories.

### Merged Branches

```
GET /repo/branches/merged?path=/path/to/repo
```

Returns list of branch names merged into the default branch.

### Orphan Worktrees

```
GET /repo/orphan-worktrees?repoPath=/path/to/repo
```

Returns list of worktree directory paths that are in detached HEAD state (their branch was deleted).

### Remove Orphan Worktree

```
POST /repo/remove-orphan
Content-Type: application/json

{ "repoPath": "/path/to/repo", "worktreePath": "/path/to/worktree" }
```

Removes an orphan worktree by filesystem path. The worktree path is validated against the repo's actual worktree list.

### Merge PR via GitHub

```
POST /repo/merge-pr
Content-Type: application/json

{ "repoPath": "/path/to/repo", "prNumber": 42, "mergeMethod": "squash" }
```

Merges a PR via the GitHub API. `mergeMethod` must be `"merge"`, `"squash"`, or `"rebase"`. Returns `{"sha": "..."}` on success.

### Approve PR

```
POST /repo/approve-pr
Content-Type: application/json

{ "repoPath": "/path/to/repo", "prNumber": 42 }
```

Submits an approving review on a PR via the GitHub API.

### CI Checks

```
GET /repo/ci?path=/path/to/repo
```

Returns detailed CI check list.

### PR Diff

```
GET /repo/pr-diff?path=/path/to/repo
```

Returns diff for the current branch's open PR.

### Remote URL

```
GET /repo/remote-url?path=/path/to/repo
```

Returns the remote origin URL.

## Git Panel Endpoints

### Working Tree Status

```
GET /repo/working-tree-status?path=/path/to/repo
```

Returns porcelain v2 working tree status.

### Panel Context

```
GET /repo/panel-context?path=/path/to/repo
```

Returns aggregated context for the Git Panel (status, branch, merge state).

### Stage Files

```
POST /repo/stage
Content-Type: application/json

{ "repoPath": "/path/to/repo", "files": ["src/main.rs"] }
```

### Unstage Files

```
POST /repo/unstage
Content-Type: application/json

{ "repoPath": "/path/to/repo", "files": ["src/main.rs"] }
```

### Discard Files

```
POST /repo/discard
Content-Type: application/json

{ "repoPath": "/path/to/repo", "files": ["src/main.rs"] }
```

### Commit

```
POST /repo/commit
Content-Type: application/json

{ "repoPath": "/path/to/repo", "message": "feat: add feature" }
```

### Run Git Command

```
POST /repo/run-git
Content-Type: application/json

{ "repoPath": "/path/to/repo", "args": ["log", "--oneline", "-5"] }
```

Runs an arbitrary git command in the repo directory.

### Commit Log

```
GET /repo/commit-log?path=/path/to/repo
```

Returns commit log entries.

### File History

```
GET /repo/file-history?path=/path/to/repo&file=src/main.rs
```

Returns git log for a specific file.

### File Blame

```
GET /repo/file-blame?path=/path/to/repo&file=src/main.rs
```

Returns line-by-line blame annotations.

## Stash Endpoints

### List Stashes

```
GET /repo/stash?path=/path/to/repo
```

Returns stash list.

### Apply Stash

```
POST /repo/stash/apply
Content-Type: application/json

{ "repoPath": "/path/to/repo", "index": 0 }
```

### Pop Stash

```
POST /repo/stash/pop
Content-Type: application/json

{ "repoPath": "/path/to/repo", "index": 0 }
```

### Drop Stash

```
POST /repo/stash/drop
Content-Type: application/json

{ "repoPath": "/path/to/repo", "index": 0 }
```

### Show Stash

```
GET /repo/stash/show?path=/path/to/repo&index=0
```

Returns diff of a stash entry.

## Log Endpoints

### Get Logs

```
GET /logs?limit=50&level=error&source=terminal
```

Retrieve log entries from the ring buffer (1000 entries max). All query params optional:
- `limit` — max entries to return (0 = all, default: 0)
- `level` — filter by level: `debug`, `info`, `warn`, `error`
- `source` — filter by source: `app`, `plugin`, `git`, `network`, `terminal`, `github`, `dictation`, `store`, `config`

### Push Log

```
POST /logs
{ "level": "warn", "source": "git", "message": "...", "data_json": "{...}" }
```

### Clear Logs

```
DELETE /logs
```

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

### Repository Defaults

```
GET /config/repo-defaults
PUT /config/repo-defaults
```

Load/save default settings applied to new repositories.

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

### MCP Upstream Status

```
GET /mcp/upstream-status
```

Returns status and metrics for all upstream MCP servers (connecting, ready, circuit_open, disabled, failed).

### MCP Instructions

```
GET /mcp/instructions
```

Returns dynamic server instructions for the MCP bridge binary as `{"instructions": "..."}`.

## Filesystem Endpoints

```
GET  /fs/list?repoPath=/path/to/repo&subdir=src
GET  /fs/search?repoPath=/path/to/repo&query=main&limit=50
GET  /fs/search-content?repoPath=/path/to/repo&query=foo&caseSensitive=false&useRegex=false&wholeWord=false&limit=200
GET  /fs/read?repoPath=/path/to/repo&file=src/main.rs
GET  /fs/read-external?path=/absolute/path/to/file
POST /fs/write         { "repoPath": "...", "file": "...", "content": "..." }
POST /fs/mkdir         { "repoPath": "...", "dir": "..." }
POST /fs/delete        { "repoPath": "...", "path": "..." }
POST /fs/rename        { "repoPath": "...", "from": "...", "to": "..." }
POST /fs/copy          { "repoPath": "...", "from": "...", "to": "..." }
POST /fs/gitignore     { "repoPath": "...", "pattern": "..." }
```

Sandboxed filesystem operations for the file manager panel. `/fs/read-external` reads an arbitrary absolute path (not sandboxed to a repo).

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

### Local IP (Primary)

```
GET /system/local-ip
```

Returns the preferred local IP address (single value).

## Watcher Endpoints

### Head Watcher

```
POST   /watchers/head?path=/path/to/repo
DELETE /watchers/head?path=/path/to/repo
```

Start/stop watching `.git/HEAD` for branch changes. Browser-only mode.

### Repo Watcher

```
POST   /watchers/repo?path=/path/to/repo
DELETE /watchers/repo?path=/path/to/repo
```

Start/stop watching `.git/` for repository state changes. Browser-only mode.

### Directory Watcher

```
POST   /watchers/dir?path=/path/to/directory
DELETE /watchers/dir?path=/path/to/directory
```

Start/stop watching a directory (non-recursive) for file changes (create/delete/rename). Emits `dir-changed` SSE event. Used by File Browser panel for auto-refresh.

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

### Finalize Merged Worktree

```
POST /worktrees/finalize
Content-Type: application/json

{ "repoPath": "/path/to/repo", "branchName": "feature-x", "action": "archive" }
```

Finalizes a merged worktree branch. `action` must be `"archive"` (moves to archive directory) or `"delete"` (removes worktree and branch).

### Remove Worktree

```
DELETE /worktrees/:branch?repoPath=/path&deleteBranch=true
```

Query parameters:
- `repoPath` (required) -- base repository path
- `deleteBranch` (optional, default `true`) -- when `true`, also deletes the local git branch

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
