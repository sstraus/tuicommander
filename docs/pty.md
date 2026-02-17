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

## Available MCP Tools (20)

### Session Management

#### `list_sessions`
List all active terminal sessions with their IDs, working directories, and worktree info.

#### `create_session`
Create a new terminal session (PTY). Returns `session_id` for subsequent operations.

**Parameters:**
- `rows` (optional) - Terminal rows (default: 24)
- `cols` (optional) - Terminal columns (default: 80)
- `shell` (optional) - Shell path (default: $SHELL or /bin/zsh)
- `cwd` (optional) - Working directory

#### `send_input`
Send text or a special key to a terminal session.

**Parameters:**
- `session_id` (required) - Terminal session ID
- `input` (optional) - Text to type
- `special_key` (optional) - Special key name

**Supported special keys:**
`enter`, `tab`, `escape`, `backspace`, `delete`, `up`, `down`, `left`, `right`, `home`, `end`, `ctrl+c`, `ctrl+d`, `ctrl+z`, `ctrl+l`, `ctrl+a`, `ctrl+e`, `ctrl+k`, `ctrl+u`, `ctrl+w`, `ctrl+r`, `ctrl+p`, `ctrl+n`

#### `get_output`
Read recent terminal output from a session's ring buffer (default 8KB, max 64KB).

**Parameters:**
- `session_id` (required) - Terminal session ID
- `limit` (optional) - Max bytes to read (default: 8192, max: 65536)

#### `resize_terminal`
Resize a terminal session.

**Parameters:** `session_id` (required), `rows` (required), `cols` (required)

#### `close_session`
Close a terminal session. Sends Ctrl+C and waits briefly for graceful shutdown.

**Parameters:** `session_id` (required)

#### `pause_session` / `resume_session`
Flow control for the terminal session's output reader.

**Parameters:** `session_id` (required)

### Orchestrator

#### `get_stats`
Get orchestrator stats: active sessions, max sessions, available slots.

#### `get_metrics`
Get session metrics: total spawned, failed spawns, bytes emitted, pauses triggered.

### Git / GitHub

All git tools require a `path` parameter (repository path).

#### `get_repo_info`
Get git repository info: branch name, status (clean/dirty/conflict), repo name.

#### `get_git_diff`
Get unified diff for unstaged changes.

#### `get_changed_files`
Get list of changed files with status (M/A/D/R) and per-file addition/deletion counts.

#### `get_github_status`
Get GitHub status: remote info, current branch, PR status, CI status, ahead/behind counts.

#### `get_pr_statuses`
Get all PR statuses for a repository with CI check rollup, review decision, labels, and draft indicators.

#### `get_branches`
Get list of git branches (local and remote) with current branch indicator.

### Config

#### `get_config`
Get TUI Commander application configuration.

#### `save_config`
Save TUI Commander application configuration.

**Parameters:** `config` (required) - Configuration object

### Agents

#### `detect_agents`
Detect installed AI agent binaries (claude, codex, aider, goose, lazygit).

#### `spawn_agent`
Spawn an AI agent in a new terminal session. Returns `session_id` to interact with the agent.

**Parameters:**
- `prompt` (required) - Prompt/task for the agent
- `cwd` (optional) - Working directory
- `model` (optional) - Model to use
- `print_mode` (optional) - Use --print mode (non-interactive)
- `output_format` (optional) - Output format (e.g., 'json')
- `agent_type` (optional) - Agent binary name (default: claude)
- `binary_path` (optional) - Explicit path to agent binary
- `args` (optional) - Custom args array (overrides default arg building)
- `rows` (optional) - Terminal rows (default: 24)
- `cols` (optional) - Terminal columns (default: 80)

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
