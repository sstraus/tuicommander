# MCP PTY Bridge

TUI Commander exposes terminal sessions, git operations, config, and agent spawning to external tools (Claude Code, Cursor, etc.) via the Model Context Protocol (MCP).

## Architecture

```
Claude Code  <--stdio/JSON-RPC-->  tui-mcp-bridge  <--HTTP-->  TUI Commander (axum on 127.0.0.1)
```

The system has two components:

1. **HTTP API** - Embedded axum server inside TUI Commander, listening on `127.0.0.1` (localhost only)
2. **MCP Bridge** - Standalone binary (`tui-mcp-bridge`) that translates MCP protocol to HTTP calls

## Enabling the MCP Server

The MCP HTTP server is **disabled by default**. Enable it in TUI Commander's config:

Edit `~/.tui-commander/config.json`:
```json
{
  "mcp_server_enabled": true
}
```

Restart TUI Commander after changing this setting. When enabled, the app starts an HTTP server on a random localhost port and writes the port number to `~/.tui-commander/mcp-port`.

## Building the Bridge Binary

```bash
cd src-tauri
cargo build --bin tui-mcp-bridge --release
```

The binary will be at `src-tauri/target/release/tui-mcp-bridge`.

## Configuring Claude Code

Add to your Claude Code MCP config (`~/.claude.json` or project `.claude/settings.json`):

```json
{
  "mcpServers": {
    "tui-commander": {
      "command": "/path/to/tui-mcp-bridge"
    }
  }
}
```

## HTTP API Reference

Read the port from `~/.tui-commander/mcp-port`:
```bash
PORT=$(cat ~/.tui-commander/mcp-port)
BASE=http://127.0.0.1:$PORT
```

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

## Available MCP Tools (5 meta-commands)

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
| `detect` | Returns `[{name, path, version}]` for known agents (claude, codex, aider, goose, lazygit) | — |
| `spawn` | Launches an agent in a new PTY session. Returns `{session_id}` | `prompt` |
| `stats` | Returns `{active_sessions, max_sessions, available_slots}` | — |
| `metrics` | Returns cumulative metrics `{total_spawned, total_failed, active_sessions, bytes_emitted, pauses_triggered}` | — |

**Optional params for `spawn`:** `cwd`, `model`, `print_mode`, `output_format`, `agent_type`, `binary_path`, `args`, `rows`, `cols`

### `config` — Application configuration

| Action | Description | Required params |
|--------|-------------|-----------------|
| `get` | Returns app config (password hash stripped) | — |
| `save` | Persists configuration. Partial updates OK | `config` (object) |

### `plugin_dev_guide` — Plugin authoring reference

Returns the complete plugin development reference (manifest format, PluginHost API, structured event types, examples). No `action` parameter needed.

## Example Usage

```bash
PORT=$(cat ~/.tui-commander/mcp-port)

# Create a session and run a command
SESSION=$(curl -s -X POST http://127.0.0.1:$PORT/sessions \
  -H "Content-Type: application/json" \
  -d '{"cwd": "/path/to/repo"}' | jq -r .session_id)

curl -X POST http://127.0.0.1:$PORT/sessions/$SESSION/write \
  -H "Content-Type: application/json" \
  -d '{"data": "git status\r"}'

sleep 1
curl "http://127.0.0.1:$PORT/sessions/$SESSION/output?limit=4096"

# Get repo info
curl "http://127.0.0.1:$PORT/repo/info?path=/path/to/repo"

# Get GitHub PR statuses
curl "http://127.0.0.1:$PORT/repo/prs?path=/path/to/repo"

# Spawn a Claude agent
AGENT=$(curl -s -X POST http://127.0.0.1:$PORT/sessions/agent \
  -H "Content-Type: application/json" \
  -d '{"prompt": "Explain this codebase", "cwd": "/path/to/repo", "print_mode": true}' \
  | jq -r .session_id)

# Read agent output
curl "http://127.0.0.1:$PORT/sessions/$AGENT/output?limit=65536"

# Check orchestrator stats
curl http://127.0.0.1:$PORT/stats

# Detect installed agents
curl http://127.0.0.1:$PORT/agents
```

## Security

- The HTTP server binds to `127.0.0.1` only (not `0.0.0.0`) - no network exposure
- No authentication (same-user, same-machine assumption)
- The MCP bridge reads the port from a file only accessible to the current user
- The feature is opt-in via `mcp_server_enabled` config flag

## Limitations

- **Headless sessions** - Sessions created via HTTP don't emit Tauri events (no frontend rendering). Use `get_output` to poll for output.
- **Raw output** - `get_output` returns raw terminal output including ANSI escape codes. No VT100 screen parsing.
- **Polling only** - No WebSocket streaming. Use `get_output` to poll for new output.
- **64KB buffer** - The ring buffer holds the last 64KB of output per session. Older output is discarded.
