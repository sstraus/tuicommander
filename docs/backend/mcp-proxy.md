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

The entry point for all MCP traffic is `POST /mcp` (legacy Streamable HTTP
transport, revision 2025-11-25). When a `tools/call` request arrives with a name
containing `__`, the transport layer routes it to the upstream registry instead
of the native tool handler.

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

All proxied tools are exposed with the prefix `{upstream_name}__{tool_name}`. The double underscore (`__`) is the routing discriminator — native TUIC tools never contain it. The separator splits only on the first occurrence, so tool names with internal underscores work correctly (e.g. `upstream__tool__with__underscores` routes to upstream `upstream`, tool `tool__with__underscores`). The namespace identifies the origin; the upstream description is preserved byte-for-byte so TUIC instructions are not duplicated across every discovered tool.

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
| `NeedsAuth` | Upstream returned 401/challenge (or its OAuth token was rejected and could not be refreshed) — awaiting the user to click "Authorize". Tool calls are rejected with `-32001` until a user-initiated OAuth flow succeeds |
| `Authenticating` | OAuth flow in progress (user clicked "Authorize") — tool calls rejected with `-32001` |

### Boot-Time Auto-Connect & `tools/list` Readiness

On startup, `auto_connect_saved_upstreams()` (`mcp_upstream_config.rs`) registers every saved upstream. It is **spawned, not awaited**, on both boot paths (desktop and `run_remote`, both in `lib.rs`): `mcp_http::start_server` parks on the shutdown signal and never returns, so any auto-connect placed *after* it would be dead code — leaving every upstream unconnected until the user touches the UI. Registration is fast (the per-upstream async `initialize` is itself spawned), so it never delays IPC socket binding.

To avoid serving a stale tool list, the registry exposes a one-shot settle gate:

- `mark_initial_connect_complete()` — set by `auto_connect_saved_upstreams` once every upstream is registered (at both exits, including the empty-config early return). Async `initialize` may still be in flight.
- `await_initial_settle(timeout)` — the first `tools/list` calls this before `merged_tool_definitions()`. It blocks (≤ `timeout`, default 3s) until auto-connect is complete **and** no entry is still `Connecting`, then serves. A global `initial_settle_done` latch makes every later call a no-op, so steady-state `tools/list` never blocks. On timeout it logs a warning and serves a possibly-partial list rather than hanging.

This also protects clients and older client versions that fetch `tools/list` during
their handshake but do not apply a later `notifications/tools/list_changed`.
Compatible current clients can refresh live; clients that ignore the notification
still receive the complete settled list at connection time.

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

Implements the legacy MCP Streamable HTTP transport (revision 2025-11-25):

1. **`initialize()`** — Reads Bearer token from OS keyring (if any), sends `initialize` request, caches `mcp-session-id` header, sends `notifications/initialized` (fire-and-forget), fetches `tools/list`.
2. **`call_tool(name, args)`** — Sends `tools/call` with the cached session ID and auth token.
3. **`call_tool_with_reconnect(name, args)`** — Calls `call_tool`, and on HTTP 400 (session expired) or connection error, re-initializes once and retries.
4. **`health_check()`** — Pings via `tools/list`. Used by the background health checker.
5. **`shutdown()`** — Sends `DELETE /mcp` with the session ID to cleanly terminate the upstream session.

Custom HTTP headers are configured as `headers: [{name, credential_ref}]` on each server. The reference is a UUID; each value is stored through the existing upstream credential vault under an upstream-scoped key (`<name>/header/<uuid>`), separate from the Bearer/OAuth entry. No secret read API is exposed to the editor. POST requests and session DELETE resolve values in the backend at request time, so rotation takes effect without caching values in the client. Legacy configs omit `headers` and require no migration.

Header names use HTTP token syntax and are case-insensitively unique. Authorization cannot coexist with Bearer/OAuth; routing, framing and MCP transport headers are reserved. Values are validated without including them in errors and marked sensitive in reqwest. Same-origin redirects retain credentials, while the redirect policy stops cross-origin hops before a request reaches the new origin. Upstream response bodies and authentication challenge parameters are excluded from diagnostics because a server can echo credentials.

The User-Agent header is set to `tuicommander-mcp-proxy/{version}`. Every POST
also carries `MCP-Protocol-Version: 2025-11-25`; the stdio client offers the
same revision during initialization.

### OAuth Refresh Failure → Re-Authorization

When a request needs a fresh token, `refresh_token_if_needed()` runs the refresh and routes any failure through `classify_refresh_error()`:

- **Fatal** (`invalid_grant`, no refresh token available, HTTP 400/401) → `UpstreamError::AuthFailed`. The refresh token is expired/revoked and cannot recover automatically.
- **Transient** (network, 5xx) → `UpstreamError::Other` (retryable) so the circuit breaker + health checks keep trying.

In `initialize_entry_with_oauth`, an `AuthFailed` on an OAuth2-configured upstream deletes the dead keyring token and re-enters `NeedsAuth` (UI shows "Authorize") instead of parking the upstream in a silent red `Failed`/`CircuitOpen` the user can't act on. Non-OAuth upstreams with a bad static token still go red (the arm is guarded by `matches!(auth, Some(OAuth2 { .. }))`). The health checker skips `NeedsAuth` entries, so there is no delete/init loop.

## Stdio Client (`stdio_client.rs`)

Spawns a local process and communicates via newline-delimited JSON-RPC on stdin/stdout.

### Process Lifecycle

1. **`spawn_and_initialize()`** — Rate-limited (minimum 5s between spawns), clears any existing process, spawns a new child with a sanitized environment, runs the MCP handshake.
2. **`call_tool(name, args)`** — Sends `tools/call` JSON-RPC via stdin, reads response from stdout.
3. **`is_alive()`** — Non-blocking `try_wait()` check on the child process.
4. **`shutdown()`** — Closes stdin (signals EOF), waits up to 2s for voluntary exit, then kills.

### Reading stdout

A dedicated thread pumps the child's stdout into a **bounded** queue so `read_line`
can wait with a deadline — a blocking read straight off the pipe is uninterruptible,
and the caller is a blocking-pool thread holding this client's mutex. Nothing drains
that queue between calls, so the bound is what keeps an upstream that chatters while
idle from growing TUIC's memory. Three limits, because a line count alone bounds
nothing: 256 lines, 8 MiB queued in total, and 16 MiB for any single line (a longer
one is discarded whole rather than published as a truncated line nobody sent).

The queue is **lossy — it drops the oldest line, it never blocks the reader.**
Blocking the reader parks the child on its stdout write, and a child that is not
reading its stdin can then park TUIC on the write side: neither side moves again.
What gets dropped is what a call discards anyway — messages that arrived while
nothing was waiting for them. A reply lost to a genuine flood surfaces as the
timeout the call already has, and the timeout log carries the drop count.

### Writing stdin

A second thread owns the child's stdin, for the same reason the reader has one:
`write_all` on a pipe is uninterruptible. A request larger than the pipe buffer
parks there until the child reads it, and a child that stopped reading never
does — so the write parks a blocking-pool thread holding this client's mutex,
and that upstream's call and health-check paths are wedged for the life of the
process.

`write_line` hands the line to that thread and waits on the **same deadline as
the reply**, so an unresponsive child costs one `timeout_secs`, not forever.
A write that runs out of time tears the client down: the bytes may still be
half-inside the pipe, and a child holding a partial JSON-RPC frame cannot be
spoken to again.

One request waits on **one deadline for the whole exchange** (`timeout_secs`),
armed **before** the write rather than after it, and covering write and reads
alike. It is not a per-message count. Server notifications and log messages arriving in
between are skipped, however many there are: a chatty upstream must not lose a
reply it answered correctly. Being out of time outranks having a line ready to
parse, or an upstream that refills the queue faster than TUIC drains it would keep
a call alive forever. On timeout the client is torn down, because a request we gave
up on may still be answered later and that stale reply must not be handed to the
next call as its own.

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

Valid status values: `connecting`, `ready`, `circuit_open`, `disabled`, `failed`, `needs_auth`, `authenticating`.

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

The `build_mcp_instructions()` function supplies TUIC protocol and orchestration context once in the initialize response. Proxied upstream descriptions remain upstream-owned and do not repeat that preamble.

Successful proxied `tools/call` responses that are valid MCP `CallToolResult` objects
(an object with a `content` array) pass through as the downstream JSON-RPC result.
TUIC does not re-wrap or mutate their `content`, `isError`, `structuredContent`, or
extension fields. The same passthrough is used when `collapse_tools` routes an upstream
call through the `call_tool` meta-tool. If an upstream returns a malformed result, TUIC
falls back to a compact JSON text content envelope; native TUIC tools always use that
compact envelope.
