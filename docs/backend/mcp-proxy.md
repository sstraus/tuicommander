# MCP Proxy Hub

**Module:** `src-tauri/src/mcp_proxy/`

The MCP Proxy Hub turns TUICommander into a universal MCP aggregator. TUIC acts simultaneously as an MCP server (serving downstream clients such as Claude Code or Cursor) and as an MCP client (connecting to upstream MCP servers). Tools from all connected upstreams are merged into the single `/mcp` endpoint that TUIC already exposes, with each upstream's tools namespaced as `{upstream_name}__{tool_name}`.

## Architecture

```
Claude Code ──┐
Cursor ───────┼──▶  POST /mcp  ──┬──▶ GitHub MCP   (HTTP)
VS Code ──────┘   (TUIC server)  ├──▶ Filesystem MCP (stdio)
                                  ├──▶ Database MCP  (HTTP)
                                  └──▶ Custom MCP     (HTTP/stdio)
```

The entry point for all MCP traffic is `POST /mcp` (Streamable HTTP transport, spec 2025-03-26). When a `tools/call` request arrives with a name containing `__`, the transport layer routes it to the upstream registry instead of the native tool handler.

## Module Layout

| File | Purpose |
|------|---------|
| `mcp_proxy/mod.rs` | Module declaration |
| `mcp_proxy/registry.rs` | Central registry — connection lifecycle, tool aggregation, routing, circuit breaker |
| `mcp_proxy/http_client.rs` | MCP client over Streamable HTTP |
| `mcp_proxy/stdio_client.rs` | MCP client over stdio (spawned process) |
| `mcp_upstream_config.rs` | Config schema, validation, persistence (`mcp-upstreams.json`) |
| `mcp_upstream_credentials.rs` | OS keyring credential management |
| `mcp_http/mcp_transport.rs` | Routing logic inside the `/mcp` handler |

## Tool Namespace

All proxied tools are exposed with the prefix `{upstream_name}__{tool_name}`. The double underscore (`__`) is the routing discriminator — native TUIC tools never contain it. The separator splits only on the first occurrence, so tool names with internal underscores work correctly (e.g. `upstream__tool__with__underscores` routes to upstream `upstream`, tool `tool__with__underscores`).

The tool description is also annotated with `[via {upstream_name}]` so the downstream AI client knows the origin.

## UpstreamRegistry

`UpstreamRegistry` (`registry.rs`) is the central hub stored in `AppState` as `Arc<UpstreamRegistry>`. It is thread-safe — all internal maps use `DashMap` (lock-free concurrent HashMap) and per-entry state is protected by `parking_lot` RwLocks and Mutexes.

### Entry Lifecycle

```
connect_upstream(config)
    │
    ├── Validate: no duplicate name, no circular URL
    ├── Build client (Http or Stdio)
    ├── Insert UpstreamEntry into DashMap
    │
    ├── If disabled → status = Disabled (done)
    │
    └── Spawn async task:
            initialize_entry()
                ├── Run MCP handshake
                ├── Fetch tools/list
                ├── On success → status = Ready, cache tools
                └── On failure → circuit breaker records failure
                                 → status = CircuitOpen or Failed
```

### Statuses

| Status | Meaning |
|--------|---------|
| `Connecting` | Handshake in progress (initial state for enabled entries) |
| `Ready` | Handshake complete, tools available |
| `CircuitOpen` | Too many failures, backoff timer active |
| `Disabled` | Disabled by user in config (`enabled: false`) |
| `Failed` | Permanently failed after max retries exceeded |

### Tool Aggregation

`aggregated_tools()` collects tools from all `Ready` upstreams, applies per-upstream tool filters, prefixes names, and annotates descriptions. Non-Ready upstreams are silently omitted. The merged list is returned as the `tools` array in `tools/list` responses alongside native TUIC tools.

### Tool Routing

`proxy_tool_call(prefixed_name, args)` parses the `__` separator, looks up the upstream by name, checks the circuit breaker, dispatches the call to the correct client, and records metrics and circuit breaker outcomes.

## Circuit Breaker

Each upstream has an independent circuit breaker with the following thresholds:

| Parameter | Value |
|-----------|-------|
| Failures before circuit opens | 3 |
| Initial backoff on open | 1 second |
| Maximum backoff cap | 60 seconds |
| Backoff growth | Exponential (`1000ms × 2^excess`) |
| Maximum retries before permanent failure | 10 |

State transitions:
- **Closed → CircuitOpen:** 3 consecutive failures trigger the circuit. Backoff starts at 1s and doubles with each additional failure, capped at 60s.
- **CircuitOpen → Ready:** A successful tool call or health check resets the failure count and closes the circuit.
- **CircuitOpen → Failed:** After 10 total circuit re-opens without recovery, the entry is marked Failed and requires manual reconnect (`reconnect_mcp_upstream`).

## Health Checks

A background task (`spawn_health_checker`) runs every 60 seconds and probes all `Ready` upstreams via `tools/list` (HTTP) or `is_alive()` process check (stdio). `CircuitOpen` upstreams whose backoff has expired are also probed for recovery.

## HTTP Client (`http_client.rs`)

Implements the MCP Streamable HTTP transport (spec 2025-03-26):

1. **`initialize()`** — Reads Bearer token from OS keyring (if any), sends `initialize` request, caches `mcp-session-id` header, sends `notifications/initialized` (fire-and-forget), fetches `tools/list`.
2. **`call_tool(name, args)`** — Sends `tools/call` with the cached session ID and auth token.
3. **`call_tool_with_reconnect(name, args)`** — Calls `call_tool`, and on HTTP 400 (session expired) or connection error, re-initializes once and retries.
4. **`health_check()`** — Pings via `tools/list`. Used by the background health checker.
5. **`shutdown()`** — Sends `DELETE /mcp` with the session ID to cleanly terminate the upstream session.

The User-Agent header is set to `tuicommander-mcp-proxy/{version}`.

## Stdio Client (`stdio_client.rs`)

Spawns a local process and communicates via newline-delimited JSON-RPC on stdin/stdout.

### Process Lifecycle

1. **`spawn_and_initialize()`** — Rate-limited (minimum 5s between spawns), clears any existing process, spawns a new child with a sanitized environment, runs the MCP handshake.
2. **`call_tool(name, args)`** — Sends `tools/call` JSON-RPC via stdin, reads response from stdout.
3. **`is_alive()`** — Non-blocking `try_wait()` check on the child process.
4. **`shutdown()`** — Closes stdin (signals EOF), waits up to 2s for voluntary exit, then kills.

### Environment Sanitization

The parent environment is cleared before spawning to prevent credential leakage (`ANTHROPIC_API_KEY`, `AWS_SECRET_ACCESS_KEY`, etc.) to potentially untrusted MCP server processes. A safe allowlist is re-applied:

```
PATH, HOME, USER, LANG, LC_ALL, TMPDIR, TEMP, TMP, SHELL, TERM
```

User-configured `env` overrides from the upstream config are then applied on top of the safe set.

### Respawn Rate Limit

To prevent tight loops when an MCP server crashes, the client enforces a minimum 5-second interval between spawn attempts. A premature respawn call returns an error immediately without spawning.

## Config Schema

Configuration is persisted to `mcp-upstreams.json` in the platform config directory, separate from the main `AppConfig`.

### UpstreamMcpConfig (top-level)

```json
{
  "servers": [ /* array of UpstreamMcpServer */ ]
}
```

### UpstreamMcpServer

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `id` | `String` | required | Unique UUID for config diff tracking |
| `name` | `String` | required | Human-readable name, also the namespace prefix. Must match `[a-z0-9_-]+` |
| `transport` | `UpstreamTransport` | required | Connection type (http or stdio) |
| `enabled` | `bool` | `true` | If false, the entry is registered but never connected |
| `timeout_secs` | `u32` | `30` | Per-request timeout (0 = no timeout, HTTP only) |
| `tool_filter` | `ToolFilter?` | `null` | Optional allow/deny filter |

### UpstreamTransport

**HTTP variant:**
```json
{
  "type": "http",
  "url": "https://example.com/mcp"
}
```

**Stdio variant:**
```json
{
  "type": "stdio",
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-filesystem"],
  "env": { "ALLOWED_PATHS": "/home/user/projects" }
}
```

### ToolFilter

| Field | Type | Description |
|-------|------|-------------|
| `mode` | `"allow"` or `"deny"` | Allow only matching tools, or deny matching tools |
| `patterns` | `Vec<String>` | Exact names or glob patterns (trailing `*` = prefix match) |

Filter examples:
- Allow only read tools: `{ "mode": "allow", "patterns": ["read_*", "list_*"] }`
- Block dangerous tools: `{ "mode": "deny", "patterns": ["delete_*", "rm", "exec_*"] }`

## Validation

`validate_upstream_config()` runs before every `save_mcp_upstreams` call and collects all errors (not just the first):

| Error | Cause |
|-------|-------|
| `EmptyName` | Server `name` field is empty |
| `InvalidName` | Name contains characters outside `[a-z0-9_-]` |
| `DuplicateName` | Two servers share the same `name` |
| `EmptyUrl` | HTTP transport has an empty URL |
| `InvalidUrlScheme` | HTTP URL does not start with `http://` or `https://` |
| `SelfReferentialUrl` | HTTP URL points to TUIC's own MCP port (circular proxy guard) |
| `EmptyCommand` | Stdio transport has an empty `command` |

The self-referential check compares the URL's host (localhost, 127.0.0.1, ::1, 0.0.0.0) and port against TUIC's own running port.

## Credential Management

Credentials (Bearer tokens for HTTP upstreams) are stored in the platform OS keyring, never in config files:

| Platform | Backend |
|----------|---------|
| macOS | Keychain |
| Windows | Credential Manager |
| Linux | keyutils / Secret Service |

The keyring service name is `tuicommander-mcp`. The account name is the upstream `name`. Credential names follow the same `[a-z0-9_-]+` validation as upstream names.

`read_upstream_credential(name)` returns `None` (not an error) when no credential exists.

## Hot-Reload

`apply_config_diff(old, new)` compares two configs using server `id` as the stable identifier:

- **Removed** servers → `disconnect_upstream(name)` (stdio: graceful shutdown, HTTP: no-op)
- **Added** servers → `connect_upstream(config)`
- **Changed** servers (same `id`, any field changed) → disconnect + reconnect
- **Unchanged** servers → left running, no interruption

This is called automatically by `save_mcp_upstreams` after writing the config file, so adding or reconfiguring upstreams takes effect immediately without restarting TUIC.

## SSE Events

Status changes emit `UpstreamStatusChanged` events via the app event bus, which surfaces as Server-Sent Events on `GET /events`. The event payload is:

```json
{
  "type": "upstream_status_changed",
  "name": "github-mcp",
  "status": "ready"
}
```

Valid status values: `connecting`, `ready`, `circuit_open`, `disabled`, `failed`.

## Metrics

Each upstream tracks lock-free atomic counters:

| Metric | Type | Description |
|--------|------|-------------|
| `call_count` | `AtomicU32` | Total tool calls routed |
| `error_count` | `AtomicU32` | Total failed tool calls |
| `last_latency_ms` | `AtomicU32` | Last observed round-trip time |

Available via `status_snapshot()` which returns a JSON snapshot of all upstreams including status, transport info, tool count, and metrics.

## Integration with `/mcp` Transport

In `mcp_transport.rs`, the `tools/call` handler checks the tool name for `__`:

```rust
if tool_name.contains("__") {
    // Route to upstream registry (async)
    state.mcp_upstream_registry.proxy_tool_call(&tool_name, args).await
} else {
    // Handle natively (sync, via spawn_blocking)
    handle_mcp_tool_call(&state, addr, &tool_name, &args)
}
```

The `tools/list` response merges native tools with upstream tools via `merged_tool_definitions()`.

The `build_mcp_instructions()` function that generates the system prompt for connecting agents also documents the proxied upstream tools and their statuses, giving the AI client situational awareness about which upstream servers are available.
