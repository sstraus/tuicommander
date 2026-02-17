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

### SSE (MCP Transport)

```
GET /mcp/sse
```

Server-Sent Events stream for MCP JSON-RPC messages.

```
POST /mcp/message?session_id=...
Content-Type: application/json

{ JSON-RPC message }
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
GET /repo/changed-files?path=/path/to/repo
```

Returns array of `ChangedFile` (path, status, additions, deletions).

### Branches

```
GET /repo/branches?path=/path/to/repo
```

Returns sorted branch list.

### GitHub Status

```
GET /repo/github-status?path=/path/to/repo
```

Returns PR status, CI status, ahead/behind for current branch.

### PR Statuses (Batch)

```
GET /repo/pr-statuses?path=/path/to/repo
```

Returns `BranchPrStatus[]` for all branches with open PRs.

### CI Checks

```
GET /repo/ci-checks?path=/path/to/repo
```

Returns detailed CI check list.

## Configuration Endpoints

### Get Config

```
GET /config
```

Returns `AppConfig`.

### Save Config

```
PUT /config
Content-Type: application/json

{ AppConfig fields }
```

### Hash Password

```
POST /auth/hash-password
Content-Type: application/json

{ "password": "..." }
```

Returns bcrypt hash string.

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

## Agent Endpoints

### Detect Agents

```
GET /agents/detect
```

Returns detected agent binaries and installed IDEs.

## Worktree Endpoints

### Create Worktree

```
POST /worktrees
Content-Type: application/json

{ "base_repo": "/path", "branch_name": "feature-x" }
```

### Remove Worktree

```
DELETE /worktrees
Content-Type: application/json

{ "repo_path": "/path", "branch_name": "feature-x" }
```

### Get Worktree Paths

```
GET /worktrees/paths?path=/path/to/repo
```

Returns `{ "branch-name": "/worktree/path", ... }`.
