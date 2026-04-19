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

#### `ui` tool — `tab` URL schemes

The `url` param of `action=tab` supports three schemes:

| Scheme | Behaviour |
|--------|-----------|
| `http(s)://` / `file://` | Loaded in a sandboxed iframe |
| `tuic://edit/<path>?line=N` | Opens a native code-editor tab at the given file and line. Absolute paths require a `//` prefix: `tuic://edit//Users/x/file.rs?line=42`. Relative paths resolve against the active repo root. |
| `tuic://open/<path>` | Opens a native markdown/preview tab |

Custom URL schemes (`vscode://`, `x-devonthink://`, etc.) do **not** work inside iframes and must not be used with `action=tab`.

### MCP Tools: `ai_terminal_*` (external agent surface)

Six tools exposed to external MCP clients (e.g. Claude Code, Cursor) that let a
remote AI agent observe and interact with a TUICommander terminal. All input
operations (`send_input`, `send_key`) require user confirmation and are
rejected while an internal agent loop is active on the target session.

**Gated by `ai_terminal_mcp_enabled` config flag (default `false`).** When the flag is off, these tools are hidden from `tools/list` (via `filtered_native_tools`) and calls are rejected at dispatch time. Enable in `config.json` or Settings > Services. Note: no live-reload — a connected client may see a stale tools snapshot until it reconnects or `notifications/tools/list_changed` fires.

| Tool | Params | Description |
|------|--------|-------------|
| `ai_terminal_read_screen` | `session_id`, `lines?` (default 50) | Read visible terminal text. Output passes through secret redaction. |
| `ai_terminal_send_input` | `session_id`, `text` | Send a text command to the session. Always prompts for confirmation. |
| `ai_terminal_send_key` | `session_id`, `key` (enter/tab/ctrl+c/escape/up/down/…) | Send a single special key. Always prompts for confirmation. |
| `ai_terminal_wait_for` | `session_id`, `pattern?`, `timeout_ms?` (10000), `stability_ms?` (500) | Wait for a regex match or for the screen to stabilise. |
| `ai_terminal_get_state` | `session_id` | Return structured `SessionState` (shell_state, cwd, terminal_mode, agent_type, …). |
| `ai_terminal_get_context` | `session_id` | Compact ~500-char context summary (mode, recent CWDs, recent errors, known fixes, TUI apps). |

### MCP Tool: `debug` — `invoke_js` and the Debug Registry

`invoke_js` executes JavaScript in the WebView (localhost-only). Results are logged with `source='eval_js'` and read via `debug(action='logs', source='eval_js', limit=1)`.

**`window.__TUIC__` bridge** — runtime introspection API:

| Method | Description |
|--------|-------------|
| `stores()` | List all registered store snapshot names |
| `store(name)` | Get a store snapshot by name |
| `plugins()` | All plugin states (legacy) |
| `plugin(id)` | Single plugin state with manifest (legacy) |
| `pluginLogs(id, limit?)` | Plugin log entries (legacy) |
| `terminals()` | All terminal states (legacy) |
| `terminal(id)` | Single terminal state (legacy) |
| `agentTypeForSession(sid)` | Agent type lookup (legacy) |
| `activity()` | Activity center sections/items (legacy) |
| `logs(limit?)` | App log entries (legacy) |

**Registered stores** (via debug registry): `github`, `globalWorkspace`, `keybindings`, `notes`, `paneLayout`, `repositories`, `settings`, `tasks`, `ui`. New stores self-register — see `src/stores/debugRegistry.ts`.

**Adding a new store snapshot** — 2 lines at the end of the store file:
```ts
import { registerDebugSnapshot } from "./debugRegistry";
registerDebugSnapshot("storeName", () => ({ /* fields to expose */ }));
```

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

## Upstream MCP Proxy

TUICommander can proxy upstream MCP servers (stdio or HTTP) and aggregate their tools into its own `tools/list` response. Configuration lives in `mcp-servers.json`.

### Stdio transport

`StdioMcpClient` spawns a child process and communicates via newline-delimited JSON-RPC over stdin/stdout. The handshake is: `initialize` → `notifications/initialized` → `tools/list`.

**RPC id-matching.** The `rpc()` method matches responses by JSON-RPC `id`, skipping any server notifications (messages without an `id` field) that arrive between request and response. This prevents silent "0 tools" when a server emits `notifications/tools/list_changed` or log messages during the handshake.

**Tilde expansion.** All user-supplied paths (`command`, `args`, `cwd`) are expanded via `crate::cli::expand_tilde()` before being passed to `std::process::Command`. This applies globally across the codebase — PTY, agent spawn, headless prompts, worktree scripts, plugin exec, and file validation all expand `~` to `$HOME`.

### HTTP transport

`HttpMcpClient` communicates via Streamable HTTP (POST to the server URL, `mcp-session-id` header for session affinity).

**Bearer token caching.** The resolved bearer token (from OS keyring) is cached in memory after the first `resolve_bearer()` call. Subsequent calls (health checks every 60s, tool calls) use the cache. The cache is invalidated on 401 → `force_refresh()` and re-populated after a successful token refresh. This eliminates repeated macOS keychain permission prompts.

### Health checker

A background task runs every 60s (`HEALTH_CHECK_INTERVAL`) and calls `tools/list` on every `Ready` upstream. Failures feed a circuit breaker (3 consecutive failures → backoff starting at 1s, capped at 60s, max 5 retries before permanent `Failed`). Recovery from `CircuitOpen`, `Connecting`, or `Failed` is attempted on each tick.

### Diagnostics

Both transports log `warn!` when `tools/list` returns a response without `result.tools` — making "0 tools" diagnosable instead of silent.

## OAuth 2.1 Upstream Authentication

When an upstream MCP server requires OAuth instead of a static Bearer token, TUICommander runs a full RFC 9728 (Protected Resource Metadata) + RFC 8414 (Authorization Server Discovery) flow with PKCE S256.

### Configuration

`UpstreamMcpServer.auth` is an enum:

```rust
enum UpstreamAuth {
    Bearer  { token: String },
    OAuth2  {
        client_id: String,
        scopes: Vec<String>,
        authorization_endpoint: Option<String>,  // None → discover
        token_endpoint: Option<String>,          // None → discover
    },
}
```

Missing endpoints trigger metadata discovery: the proxy issues an unauthenticated probe, follows the `WWW-Authenticate: Bearer resource_metadata=<url>` challenge to fetch `ProtectedResourceMetadata`, then resolves the authorization server's `.well-known/oauth-authorization-server` (falling back to OIDC `.well-known/openid-configuration` when required).

### Error → flow transition

`src-tauri/src/mcp_proxy/http_client.rs` emits a typed error:

```rust
enum UpstreamError {
    NeedsOAuth { www_authenticate: String },
    AuthFailed,
    Other(String),
}
```

A `NeedsOAuth` on any request transitions the upstream registry to `needs_auth`. The Services tab in Settings surfaces an *Authorize* button that calls `start_mcp_upstream_oauth`. Auto-triggered OAuth is gated behind explicit user consent (the confirm dialog shows the AS origin so the user can refuse an Authorization Server mix-up attempt).

### Flow

1. **Start** — `start_mcp_upstream_oauth(name)` generates a PKCE verifier/challenge (S256), mints an opaque `state`, records the pending flow in a DashMap keyed by state, sets upstream status to `authenticating`, and returns the authorization URL + AS origin.
2. **Consent UI** — The frontend opens the URL via `tauri-plugin-opener` after user approval. The status bar and Services tab show "Awaiting authorization…".
3. **Callback** — The AS redirects to `tuic://oauth-callback?code=…&state=…`. The OS routes the deep link to the desktop app (`src-tauri/src/mcp_oauth/mod.rs` — `DEEP_LINK_SCHEME = "tuic://oauth-callback"`). The deep-link handler calls `mcp_oauth_callback(code, oauth_state)`.
4. **Exchange** — `TokenManager` posts code + PKCE verifier to the token endpoint, receives `{ access_token, refresh_token?, expires_in? }`, serializes into `OAuthTokenSet`, persists to the OS keyring (`mcp_upstream_credentials.rs` — structured JSON format with `"type": "oauth2"`), and transitions upstream to `connecting`.
5. **Refresh** — `TokenManager` is shared across every `HttpMcpClient` refresh path (unified per upstream); a semaphore serializes concurrent refresh attempts to defeat thundering-herd. `expires_at` uses a 60 s margin; `None` means "no known expiry — do not treat as expired".

### Cancel

`cancel_mcp_upstream_oauth(name)` drops the pending flow entry and resets the upstream status to whatever it was before the attempt (`disconnected` / `failed` / `ready`).

### Deep-link scheme

| Scheme | Purpose |
|--------|---------|
| `tuic://oauth-callback?code=…&state=…` | OAuth 2.1 authorization code return path for upstream MCP servers |

Registered at boot via Tauri's single-instance + deep-link plugins. The frontend listener routes callbacks to `mcp_oauth_callback` without exposing the code to the WebView console.

### Threat model

OAuth callbacks arrive exclusively through the OS-level `tuic://` deep link — not over the network. There is no adversary position from which a remote attacker can probe the pending-flow map, so state comparison uses a direct DashMap lookup (no constant-time compare). The localhost dev callback server (used only in development) binds `127.0.0.1` with a random port; it is never exposed in production builds.

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
