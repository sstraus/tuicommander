# MCP & HTTP Server

**Module:** `src-tauri/src/mcp_http.rs`

Optional HTTP/WebSocket server that exposes all Tauri commands as REST endpoints. Enables browser-mode operation and MCP (Model Context Protocol) integration for external AI tools.

## Activation

The server has two independent listeners:

- **IPC listener** (always started): On macOS/Linux, listens at `<config_dir>/mcp.sock` (Unix domain socket). On Windows, listens on `\\.\pipe\tuicommander-mcp` (named pipe). No authentication — used by the local `tuic-bridge` sidecar.
- **TCP listener** (opt-in): Only starts when remote access is enabled. Binds to `0.0.0.0:<remote_access_port>` with Basic Auth.

The `mcp_server_enabled` config flag controls whether the `/mcp` protocol route is active (MCP tool discovery and invocation), not whether the server itself starts. The HTTP API endpoints (sessions, git, config, etc.) are always available on the IPC listener.

Configuration via Settings > Services, or `config.json`:

```json
{
  "mcp_server_enabled": true,
  "remote_access_enabled": false,
  "remote_access_port": 9876
}
```

On startup, the server:
1. Binds the IPC listener: Unix socket at `<config_dir>/mcp.sock` (macOS/Linux) or named pipe `\\.\pipe\tuicommander-mcp` (Windows)
2. If remote access is enabled, binds a TCP listener on the configured port
3. Starts Axum HTTP server on a background tokio thread
4. Enables CORS for browser mode
5. Spawns MCP session reaper (evicts stale sessions after 1h TTL)
6. Spawns upstream health checker for proxied MCP servers

## Unix Socket Lifecycle (macOS/Linux)

The socket at `<config_dir>/mcp.sock` is managed with three safety layers to survive crashes and rapid restarts:

| Layer | Mechanism | Purpose |
|-------|-----------|---------|
| **RAII guard** | `SocketGuard(PathBuf)` struct with `impl Drop { remove_file }` | Removes socket when the server task is dropped, even on panic or kill |
| **Retry bind** | 3 attempts × 100 ms, each removes stale file before trying | A crashed previous run leaves a dead socket file that blocks `bind(2)` — retrying clears it |
| **Real liveness check** | `UnixStream::connect()` in `get_mcp_status` | `file.exists()` returns `true` for stale sockets; only a real connect reveals whether the server is alive |

**Why this matters for AI tool integrations:** The `tuic-bridge` sidecar connects via the Unix socket to expose TUICommander tools to Claude Code. If the socket is stale (app crashed, Tauri force-quit), the bridge cannot connect and returns `tools: []`, silently disabling all MCP tools in the agent session. The retry bind ensures the socket is always valid on restart; the real liveness check ensures the UI accurately reports the server state.

## REST API Endpoints

### Session Management

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/sessions` | List active PTY sessions |
| `POST` | `/sessions` | Create new PTY session |
| `POST` | `/sessions/:id/write` | Write data to session |
| `POST` | `/sessions/:id/resize` | Resize session terminal |
| `GET` | `/sessions/:id/output` | Read session output (ring buffer) |
| `POST` | `/sessions/:id/pause` | Pause session output |
| `POST` | `/sessions/:id/resume` | Resume session output |
| `DELETE` | `/sessions/:id` | Close session |

### Monitoring

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check |
| `GET` | `/stats` | Orchestrator stats (active/max/available) |
| `GET` | `/metrics` | Session metrics (spawned, failed, bytes) |

### Git Operations

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/repo/info?path=` | Get repository info |
| `GET` | `/repo/diff?path=` | Get git diff |
| `GET` | `/repo/diff-stats?path=` | Get diff stats |
| `GET` | `/repo/changed-files?path=` | List changed files |
| `GET` | `/repo/branches?path=` | List git branches |
| `GET` | `/repo/github-status?path=` | Get GitHub status |
| `GET` | `/repo/pr-statuses?path=` | Get batch PR statuses |
| `GET` | `/repo/ci-checks?path=` | Get CI check details |

### Configuration

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/config` | Get app config |
| `PUT` | `/config` | Save app config |
| `POST` | `/auth/hash-password` | Hash password for remote access |

### Agents

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/agents/detect` | Detect installed agents and IDEs |

### Plugins

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/plugins/docs` | Plugin development guide (AI-optimized reference) |
| `GET` | `/api/plugins/:plugin_id/data/*path` | Read plugin data file (JSON or plain text) |

### Worktrees

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/worktrees` | Create worktree |
| `DELETE` | `/worktrees` | Remove worktree |
| `GET` | `/worktrees/paths?path=` | Get worktree paths for repo |

## Streaming

### WebSocket (`/sessions/:id/stream`)

Real-time PTY output streaming per session. Connects to the session's broadcast channel.

```
Client ──WebSocket──> /sessions/{session_id}/stream
                      Server pushes PTY output as text frames
```

### Session Lifecycle Events

When sessions are created or closed (via HTTP, MCP, or PTY exit), the server broadcasts events through the SSE event bus:

- **`session-created`** — Emitted when a new PTY session is created (both local and MCP-spawned). Carries `session_id` and `cwd`. Frontend uses this to auto-add terminal tabs for remotely spawned agents.
- **`session-closed`** — Emitted when a session exits. Carries `session_id`. Frontend uses this for cleanup.

These events are available on the SSE `/events` stream used by the mobile PWA and any connected WebSocket clients.

### Streamable HTTP (`POST /mcp`)

MCP Streamable HTTP transport (spec 2025-03-26):

```
Client ──POST──> /mcp   (JSON-RPC request, response in body)
Client ──GET───> /mcp   (SSE stream for server notifications, requires Mcp-Session-Id header)
Client ──DELETE─> /mcp  (end session, pass Mcp-Session-Id header)
```

The `GET /mcp` SSE stream emits `notifications/tools/list_changed` whenever the available tool set changes (e.g., native tools are enabled/disabled via config, or upstream MCP servers connect/disconnect). The bridge sidecar subscribes to this stream and forwards the notification to the AI agent.

### Lazy Tool Discovery (`collapse_tools`)

When `collapse_tools: true` in `config.json` (or via Settings > Services > TUIC Tools > "Collapse tools"), the server replaces the full tool list in `tools/list` with exactly three meta-tools (the Speakeasy pattern):

| Meta-tool | Purpose |
|-----------|---------|
| `search_tools` | BM25 search over the full native + upstream tool corpus; returns name/description pairs |
| `get_tool_schema` | Returns the full `{name, description, inputSchema}` for a specific tool |
| `call_tool` | Dispatches to the named tool — routes to the native handler or `proxy_tool_call` for `{upstream}__{tool}` names |

Rationale: a cold tool list of 100+ tools costs ~35k tokens in every agent turn; the 3 meta-tools cost ~500 fixed tokens and the agent fetches schemas on demand. Toggling `collapse_tools` fires `notifications/tools/list_changed` so connected clients refresh their tool cache.

**Filter enforcement.** Both `search_tools` and `call_tool` re-apply the safety filters that the full listing would apply: `disabled_native_tools` is checked up-front in `handle_call_tool`, and upstream allow/deny filters are enforced at both enumeration time (`aggregated_tools`) and dispatch time (`proxy_tool_call`). This is critical under collapse mode: discovery no longer gates dispatch, so an agent that knows a filtered tool name cannot bypass the filter by calling `call_tool` directly. `search_tools` and `get_tool_schema` also reject meta-tool names, and `call_tool` refuses to recurse into itself.

The BM25 index lives in `AppState::tool_search_index` (`parking_lot::RwLock<ToolSearchIndex>`, backed by `src-tauri/src/tool_search.rs`). A background task subscribes to the `mcp_tools_changed` broadcast and rebuilds the index whenever the tool set changes (upstream connect/disconnect, `disabled_native_tools` edit, `collapse_tools` toggle).

The MCP instructions string returned by `initialize` (`build_mcp_instructions`) swaps to a "lazy discovery" guide when `collapse_tools: true` so agents know to call `search_tools` first rather than looking for a flat tool table.

### MCP Native Tools

Seven native tools, organized by domain. Two (`config`, `debug`) are hidden by default via `disabled_native_tools` — discoverable through `search_tools`/`get_tool_schema`/`call_tool` when `collapse_tools` is enabled.

| Tool | Actions | Default |
|------|---------|---------|
| `session` | list, create, input, output, resize, close, kill, pause, resume | Enabled |
| `agent` | spawn, detect, stats, metrics, register, list_peers, send, inbox | Enabled |
| `repo` | list, active, prs, status, worktree_list, worktree_create, worktree_remove | Enabled |
| `ui` | tab, toast, confirm | Enabled |
| `plugin_dev_guide` | *(no actions — returns guide text)* | Enabled |
| `config` | get, save | Disabled |
| `debug` | agent_detection, logs, sessions, invoke_js | Disabled |

The `disabled_native_tools` config key accepts an array of tool names to hide from `tools/list`. Default: `["config", "debug"]`.

### MCP Tool: `session` Output

The `session` tool's `action=output` strips ANSI escape codes by default, returning clean text suitable for AI consumption. Pass `format="raw"` to preserve escape sequences (e.g. for terminal rendering). The `action=list` response includes process details per session: `child_pid`, `foreground_pgid`, and `foreground_process`.

| Param | Default | Description |
|-------|---------|-------------|
| `limit` | `8192` | Max bytes to read |
| `format` | (text) | `"raw"` preserves ANSI escape codes |

### MCP Tool: `repo` — Worktree Create (Claude Code Agent Hint)

When the MCP client identifies as Claude Code (detected via `clientInfo.name` at initialize time), the `repo action=worktree_create` response includes an additional `cc_agent_hint` field:

```json
{
  "worktree_path": "/path/to/repo__wt/feature-branch",
  "branch": "feature-branch",
  "cc_agent_hint": {
    "worktree_path": "/path/to/repo__wt/feature-branch",
    "suggested_prompt": "Work in the worktree at `/path/...`. Use absolute paths for ALL file operations..."
  }
}
```

This works around Claude Code's inability to change its working directory mid-session. The hint tells CC to spawn a subagent that uses absolute paths for all file operations (Read, Edit, Glob, Grep) and `cd <path> && ...` for shell commands.

Non-Claude Code MCP clients do not receive this field.

## Inter-Agent Messaging

The `agent` tool's messaging actions (`register`, `list_peers`, `send`, `inbox`) enable coordination between multiple AI agents connected to TUICommander.

### Protocol

1. **Register**: Agent reads `$TUIC_SESSION` env var and calls `agent action=register tuic_session=<uuid>`. This links the MCP session to the stable tab identity.
2. **Discover**: `agent action=list_peers` returns all registered peers (filterable by project).
3. **Send**: `agent action=send to=<tuic_session> message="..."` routes the message to the recipient's inbox.
4. **Receive**: Messages arrive via MCP channel notification (real-time, if SSE connected) and/or `agent action=inbox` (polling).

### Channel Push Delivery

When a recipient has an active SSE stream (`GET /mcp`), messages are pushed as `notifications/claude/channel` JSON-RPC notifications:

```json
{
    "jsonrpc": "2.0",
    "method": "notifications/claude/channel",
    "params": {
        "content": "Message from worker-1: done with auth module",
        "meta": { "from_tuic_session": "abc-123", "from_name": "worker-1", "message_id": "msg-uuid" }
    }
}
```

This requires the client to be launched with `--dangerously-load-development-channels server:tuicommander`. The server declares `experimental.claude/channel` in its capabilities. Spawned Claude Code agents get this flag automatically.

### Limits

- Max message size: 64 KB
- Inbox capacity: 100 messages per agent (FIFO eviction)
- Peer registrations cleaned up on MCP session delete and TTL reap

## Authentication

When remote access is enabled:
- Basic Auth with username/password
- Password stored as bcrypt hash in config
- Applied to all endpoints

When MCP-only (localhost):
- No authentication required
- Localhost binding only

## Security Model

- **Default:** Localhost-only, no authentication, opt-in
- **Remote access:** Configurable port, Basic Auth required
- **CORS:** Enabled for all origins (browser mode support)
- **Compression:** Gzip and Brotli via `CompressionLayer` (responses >860 bytes, auto-negotiated). SSE and WebSocket excluded by `DefaultPredicate`
- **No TLS:** Intended for local network use; use SSH tunnel for remote

## Browser Mode Integration

The frontend's `transport.ts` maps all Tauri commands to HTTP endpoints:

```typescript
// In browser mode:
invoke("create_pty", { config }) → POST /sessions { config }
invoke("get_repo_info", { path }) → GET /repo/info?path=...
```

PTY output in browser mode uses WebSocket instead of Tauri events.

## Mobile Transport

The mobile companion UI (`/mobile`) uses the same HTTP/WebSocket infrastructure as the desktop browser mode:

- **Session polling**: `GET /sessions` every 3s, enriched with `SessionState` (question, rate-limit, busy, agent type)
- **Real-time events**: SSE via `GET /events` for session create/close notifications
- **Live output**: WebSocket to `/sessions/{id}/stream` with JSON framing (`output`, `parsed`, `exit`)
- **Input**: `POST /sessions/{id}/write` sends text to PTY (used by quick-reply chips and command input)
- **History**: `GET /sessions/{id}/output?format=text` fetches initial ANSI-stripped output buffer

The mobile entry point shares `transport.ts` and `invoke.ts` with the desktop — no mobile-specific transport code.
