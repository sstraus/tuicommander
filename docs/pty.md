# MCP Bridge

TUICommander exposes terminal sessions, git operations, config, and agent spawning to external tools (Claude Code, Cursor, Windsurf, VS Code, Zed, Amp, Gemini) via the Model Context Protocol (MCP).

## Architecture

```
AI Agent  <--stdio/JSON-RPC-->  tuic-bridge  <--HTTP over Unix socket-->  TUICommander (axum)
```

The system has two components:

1. **HTTP API** — Embedded axum server inside TUICommander, always listening on a Unix domain socket at `<config_dir>/mcp.sock`
2. **MCP Bridge** — Sidecar binary (`tuic-bridge`) shipped alongside the app, translating MCP stdio transport to HTTP calls over the Unix socket

## How It Works

The bridge binary (`tuic-bridge`) is a Tauri sidecar — it ships with the app and requires no manual build. It:

- Reads JSON-RPC messages from stdin (MCP stdio transport)
- Forwards them as `POST /mcp` requests over the Unix socket to TUICommander
- Returns responses on stdout
- Handles `initialize` locally so it works even when TUICommander is not running
- Reconnects automatically every 3 seconds if TUICommander is stopped or restarted
- Maintains a persistent SSE connection (`GET /mcp`) to receive server-initiated `notifications/tools/list_changed` events, which it forwards to the AI agent
- When disconnected, returns empty tool lists and graceful error messages instead of crashing

## Auto-Install

On first launch, TUICommander auto-installs the MCP bridge config into all supported agents:

| Agent | Config File | Key Path |
|-------|------------|----------|
| Claude Code | `~/.claude.json` | `mcpServers.tuicommander` |
| Cursor | `~/.cursor/mcp.json` | `mcpServers.tuicommander` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | `mcpServers.tuicommander` |
| VS Code | `<vscode_user_dir>/mcp.json` | `servers.tuicommander` |
| Zed | `~/.config/zed/settings.json` | `context_servers.tuicommander` |
| Amp | `~/.config/amp/settings.json` | `amp.mcpServers.tuicommander` |
| Gemini | `~/.gemini/settings.json` | `mcpServers.tuicommander` |

The installed config entry looks like:

```json
{
  "tuicommander": {
    "command": "/path/to/tuic-bridge"
  }
}
```

The bridge binary path is resolved from the sidecar location (same directory as the main executable). You can also manage MCP installation per-agent from **Settings > Agents** in the UI.

## Manual Configuration

If auto-install didn't run or you need to configure manually, add the entry to your agent's MCP config file. For example, in Claude Code (`~/.claude.json`):

```json
{
  "mcpServers": {
    "tuicommander": {
      "command": "/Applications/TUICommander.app/Contents/MacOS/tuic-bridge"
    }
  }
}
```

The exact path depends on your platform and installation location.

## HTTP API Reference

The HTTP API is served over the Unix socket at `<config_dir>/mcp.sock`. When remote access is enabled, a TCP listener also starts on the configured port.

### Health

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check, returns `{"ok": true}` |

### Session Lifecycle

| Method | Path | Description |
|--------|------|-------------|
| GET | `/sessions` | List all active sessions |
| POST | `/sessions` | Create a new terminal session |
| POST | `/sessions/{id}/write` | Write data to session |
| POST | `/sessions/{id}/resize` | Resize session |
| GET | `/sessions/{id}/output` | Read session output |
| POST | `/sessions/{id}/pause` | Pause session reader |
| POST | `/sessions/{id}/resume` | Resume session reader |
| DELETE | `/sessions/{id}` | Close session |

### Agent Sessions

| Method | Path | Description |
|--------|------|-------------|
| POST | `/sessions/agent` | Spawn an AI agent in a PTY |

### Orchestrator

| Method | Path | Description |
|--------|------|-------------|
| GET | `/stats` | Orchestration stats (active/max/available) |
| GET | `/metrics` | Session metrics (spawned, failed, bytes, pauses) |

### Git / GitHub

All git endpoints take a `?path=` query parameter specifying the repository path.

| Method | Path | Description |
|--------|------|-------------|
| GET | `/repo/info?path=` | Repository info (name, branch, status) |
| GET | `/repo/diff?path=` | Unified diff (unstaged changes) |
| GET | `/repo/diff-stats?path=` | Diff stats (+/- counts) |
| GET | `/repo/files?path=` | Changed files with per-file stats |
| GET | `/repo/github?path=` | GitHub status (remote, PR, CI, ahead/behind) |
| GET | `/repo/prs?path=` | All PR statuses with CI rollup |
| GET | `/repo/branches?path=` | Branch list (local + remote) |
| GET | `/repo/ci?path=` | CI check details |

### Config

| Method | Path | Description |
|--------|------|-------------|
| GET | `/config` | Load application config |
| PUT | `/config` | Save application config |

### Agents

| Method | Path | Description |
|--------|------|-------------|
| GET | `/agents` | Detect installed agent binaries |

### Plugins

| Method | Path | Description |
|--------|------|-------------|
| GET | `/plugins/docs` | Plugin development guide (AI-optimized reference) |

## Available MCP Tools (7 meta-commands)

All tools except `plugin_dev_guide` require an `action` parameter to select the operation.

### `session` — PTY terminal session management

| Action | Description | Required params |
|--------|-------------|-----------------|
| `list` | Returns `[{session_id, cwd, worktree_path, worktree_branch}]` for all active sessions | — |
| `create` | Creates a new PTY session. Returns `{session_id}` | — |
| `input` | Sends text and/or a special key to a session | `session_id`, plus `input` and/or `special_key` |
| `output` | Returns `{data, total_written}` from session ring buffer | `session_id` |
| `resize` | Resizes PTY dimensions | `session_id`, `rows`, `cols` |
| `close` | Terminates a session | `session_id` |
| `pause` | Pauses output buffering | `session_id` |
| `resume` | Resumes output buffering | `session_id` |

**Optional params for `create`:** `rows`, `cols`, `shell`, `cwd`
**Optional params for `output`:** `limit` (default 8192, max 65536)

**Special keys for `input`:** `enter`, `tab`, `escape`, `backspace`, `delete`, `up`, `down`, `left`, `right`, `home`, `end`, `ctrl+c`, `ctrl+d`, `ctrl+z`, `ctrl+l`, `ctrl+a`, `ctrl+e`, `ctrl+k`, `ctrl+u`, `ctrl+w`, `ctrl+r`, `ctrl+p`, `ctrl+n`

### `git` — Repository state and GitHub integration

All actions require `path` (absolute path to git repository).

| Action | Description |
|--------|-------------|
| `info` | Returns `{name, branch, status, remote_url, is_dirty, ahead, behind}` |
| `diff` | Returns `{diff}` with unified diff of unstaged changes |
| `files` | Returns `[{path, status, insertions, deletions}]` for changed files |
| `branches` | Returns `[{name, is_current, is_remote}]` branch list |
| `github` | Returns GitHub integration data (remote, PR, CI, ahead/behind) |
| `prs` | Returns all open PR statuses with CI rollup |

### `agent` — AI agent detection and management

| Action | Description | Required params |
|--------|-------------|-----------------|
| `detect` | Returns `[{name, path, version}]` for known agents (claude, codex, aider, goose) | — |
| `spawn` | Launches an agent in a new PTY session. Returns `{session_id}` | `prompt` |
| `stats` | Returns `{active_sessions, max_sessions, available_slots}` | — |
| `metrics` | Returns cumulative metrics `{total_spawned, total_failed, active_sessions, bytes_emitted, pauses_triggered}` | — |

**Optional params for `spawn`:** `cwd`, `model`, `print_mode`, `output_format`, `agent_type`, `binary_path`, `args`, `rows`, `cols`

### `config` — Application configuration

| Action | Description | Required params |
|--------|-------------|-----------------|
| `get` | Returns app config (password hash stripped) | — |
| `save` | Persists configuration. Partial updates OK | `config` (object) |

### `workspace` — Workspace repositories and groups

| Action | Description | Required params |
|--------|-------------|-----------------|
| `list` | Returns all open repos with group membership, branch, dirty status, and worktrees | — |
| `active` | Returns the currently focused repo path, branch, and group | — |

### `notify` — User notifications

| Action | Description | Required params |
|--------|-------------|-----------------|
| `toast` | Shows a temporary notification to the TUIC user | `title` |
| `confirm` | Shows a blocking confirmation dialog. Returns `{confirmed: bool}`. Localhost only | `title` |

**Optional params for `toast`:** `message`, `level` (info, warn, error; default: info)
**Optional params for `confirm`:** `message`

### `plugin_dev_guide` — Plugin authoring reference

Returns the complete plugin development reference (manifest format, PluginHost API, structured event types, examples). No `action` parameter needed.

## Security

- The Unix socket is only accessible to the current user (filesystem permissions)
- No authentication on the Unix socket (same-user, same-machine assumption)
- TCP listener only starts when remote access is explicitly enabled, with Basic Auth required
- MCP bridge config is auto-installed on first launch, manageable from Settings > Agents

## Troubleshooting

| Problem | Fix |
|---------|-----|
| Bridge reports "TUIC not running" | Start TUICommander. The bridge reconnects automatically every 3 seconds. |
| Agent shows no tools | Ensure TUICommander is running. The bridge returns empty tool lists when disconnected. |
| Config not auto-installed | Check if `~/.claude.json` (or equivalent) exists with a `tuicommander` entry. Reinstall from Settings > Agents. |
| Socket permission denied | Verify `<config_dir>/mcp.sock` is owned by your user. Delete the stale socket and restart TUICommander. |
| Bridge binary not found | The bridge should be in the same directory as the main TUICommander executable. Check your installation. |

## Limitations

- **Headless sessions** — Sessions created via HTTP don't emit Tauri events (no frontend rendering). Use `get_output` to poll for output.
- **Raw output** — `get_output` returns raw terminal output including ANSI escape codes. No VT100 screen parsing.
- **Polling only** — No WebSocket streaming. Use `get_output` to poll for new output.
- **64KB buffer** — The ring buffer holds the last 64KB of output per session. Older output is discarded.
