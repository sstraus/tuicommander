# MCP & HTTP Server

**Module:** `src-tauri/src/mcp_http.rs`

Optional HTTP/WebSocket server that exposes all Tauri commands as REST endpoints. Enables browser-mode operation and MCP (Model Context Protocol) integration for external AI tools.

## Activation

Opt-in via Settings > Services > MCP Server toggle, or `config.json`:

```json
{
  "mcp_server_enabled": true,
  "remote_access_enabled": false,
  "remote_access_port": 3100
}
```

When enabled, the server:
1. Binds to a random port (or configured port for remote access)
2. Writes port number to `<config_dir>/mcp-port` file
3. Starts Axum HTTP server on a background tokio thread
4. Enables CORS for browser mode

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

### Streamable HTTP (`POST /mcp`)

MCP Streamable HTTP transport (spec 2025-03-26):

```
Client ──POST──> /mcp   (JSON-RPC request, response in body)
Client ──GET───> /mcp   (405 Method Not Allowed)
Client ──DELETE─> /mcp  (end session, pass Mcp-Session-Id header)
```

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
- **No TLS:** Intended for local network use; use SSH tunnel for remote

## Browser Mode Integration

The frontend's `transport.ts` maps all Tauri commands to HTTP endpoints:

```typescript
// In browser mode:
invoke("create_pty", { config }) → POST /sessions { config }
invoke("get_repo_info", { path }) → GET /repo/info?path=...
```

PTY output in browser mode uses WebSocket instead of Tauri events.
