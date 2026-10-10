# Transport Layer

The transport layer provides a unified IPC abstraction so the same frontend code works in both Tauri (native desktop) and browser (HTTP) modes.

Repository-owned calls resolve their machine from path arguments, including
`project` on the Project Progress commands. The remote backend owns the
journal; the local transport routes reads and writes to it.

OS drops are a cross-machine operation: desktop-only `fs_transfer_remote_paths` runs through local Tauri IPC, reads Finder source paths and streams bounded archives to `/fs/upload-copy` on the connection id supplied by the drop target. It is `INTENTIONALLY_UNMAPPED`: HTTP token holders must not trigger exfiltration of sources that cannot be gated to registered roots. It uses `remote_runtime` for the live endpoint and existing token. It must not route local source paths to the receiving daemon.

## Files

| File | Purpose |
|------|---------|
| `src/invoke.ts` | Smart `invoke()` wrapper — zero overhead in Tauri |
| `src/transport.ts` | Public transport entry point (re-exports) |
| `src/transport/http.ts` | RPC routing, coalescing, cached IPC invocation and HTTP requests |
| `src/transport/ipc.ts` | Event and PTY subscriptions, including their browser WS/SSE branches |
| `src/transport/commandTable.ts` | Command-to-endpoint mappings and native/WS exclusions |
| `src/transport/types.ts` | Shared transport and upstream MCP types |
| `src/transport/environment.ts` | Shared Tauri environment detection |

## invoke.ts

```typescript
export function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>
export function listen<T>(event: string, handler: (event: Event<T>) => void): Promise<Unsubscribe>
```

**Resolution:** At module import time, detects if running in Tauri webview:
- **Tauri mode:** Delegates directly to `@tauri-apps/api/core.invoke()` (zero overhead)
- **Browser mode:** Maps command to HTTP endpoint via `transport.ts`

### In-flight dedup (Tauri mode)

Concurrent identical calls for read-only commands in `DEDUP_COMMANDS` share a single IPC round-trip — the second caller gets the same `Promise` as the first, cleared on settle. Prevents the `repo-changed` fan-out storm where ~20 mounted components each spawned parallel git processes for the same repo. Mutations (stage/commit/push) are never deduped. Browser mode has the equivalent via `isIdempotentRpc` in `transport.ts`.

```typescript
export function isTauri(): boolean
// Checks window.__TAURI__ existence
```

## transport.ts

### Command Mapping

Maps every Tauri command name to an HTTP method + path via a declarative `COMMAND_TABLE`. Each entry is a `CommandTableEntry` — an object `{ map }` whose `map(args, p)` returns `{ method, path, body?, transform? }`.

```typescript
// Table-driven: each command maps to an HTTP request
const COMMAND_TABLE: Record<string, CommandTableEntry> = {
  create_pty: { map: (args) => ({ method: "POST", path: "/sessions", body: args.config }) },
  get_repo_info: { map: (args) => ({ method: "GET", path: `/repo/info?path=${enc(args.path)}` }) },
  write_pty: { map: (args) => ({ method: "POST", path: `/sessions/${args.sessionId}/write`, body: { data: args.data } }) },
  // ... ~80 commands
};
```

This replaces the previous 370-line switch statement with a flat lookup table for easier maintenance and review.

### HTTP Response Semantics

Successful JSON responses preserve the backend's decoded value exactly. In
particular, a literal `null` body is a valid result for commands whose Tauri
contract returns `Option<T>`; browser and PWA callers receive the same `null`
that desktop IPC returns for `None`.

A successful non-JSON response to a JSON RPC is rejected with the request URL
and content type; connection tokens are excluded from that URL. This prevents an
HTML page from becoming a remote picker's home path. JSON bodies without a
content type remain supported. `read_plugin_data` also accepts `text/plain`
(or an omitted content type) and preserves those bytes as a string. HTML is
rejected for that route too.

A zero-length text or JSON body is not the same value and is rejected with the
command name in the error. Responses declared as JSON are also rejected when
their non-empty body is malformed, while non-success HTTP statuses retain their
status and command context. Binary `application/octet-stream` routes are the
deliberate exception: an empty buffer can be a valid terminal chunk.

### PTY Subscription

```typescript
export function subscribePty(
  sessionId: string,
  onData: (data: string) => void,
  onExit: () => void,
  onParsedOrOptions?: ((event: WsParsedEvent) => void) | SubscribePtyOptions
): PtySubscription
```

- **Tauri mode:** Uses `listen("pty-activity-{id}")` and `listen("pty-exit-{id}")` Tauri events
- **Browser mode:** Opens WebSocket to `/sessions/{id}/stream`

`PtySubscription` is still callable to dispose the subscription — every existing
caller works unchanged — and carries `pause()` / `resume()` for a client that is
not on screen (`src/mobile/utils/pageVisibility.ts` wires them to
`visibilitychange`).

`pause()` is deliberately NOT `unsubscribe()` followed by a fresh
`subscribePty()`. Two things would break:

- The close handler reads codes 1000/1001 as a real session exit, so the view
  would print "session exited" and clear the screen on every tab switch.
- The consumed-line cursor is closure-private, so a fresh subscription replays
  from the **mount** offset and duplicates the whole scrollback.

Instead the paused socket is closed with the cursor kept alive, `onExit` stays
silent, pending reconnect backoff is cancelled, and `resume()` reopens from that
cursor. A paused subscription delivers nothing, and because the cursor only
advances on delivery, nothing is skipped either. On desktop there is no socket
to drop, so `pause()` suppresses delivery instead — the same observable
contract.

`onData` receives PTY output in **browser/PWA mode only**. Desktop sends no output
over IPC: the canvas renders from grid frames and plugin watcher lines are
assembled in Rust, so no desktop consumer needs the bytes.

For "is this session producing output", use `options.onActivity` — a payload-free
pulse the backend throttles to ~1/s and delivers on both transports from one
signal (`pty-activity-{id}` on desktop, the `{"type":"activity"}` WS frame in the
browser). It is the only activity signal that works for a background tab: the
canvas stops acking grid frames while a terminal is hidden, so frame traffic
cannot stand in for it.

### URL Building

```typescript
export function buildHttpUrl(path: string): string
// Reads MCP port from config, builds http://localhost:{port}{path}
```

## Design

The transport abstraction enables:

1. **Development:** Run frontend with `pnpm dev` against the Rust HTTP server
2. **Browser mode:** Access TUICommander from a browser on another device
3. **Testing:** Frontend tests can mock at the invoke level
4. **MCP integration:** External tools use the same HTTP API

The abstraction is resolved once at module load — no per-call overhead in production Tauri mode.

The `enqueue_agent_command` mapper forwards optional `idempotencyKey` unchanged
to `POST /sessions/:id/queue`. Both transports return `{ accepted, typed, queued }`.
A recognized retry has `accepted: true` even after the queue drained; `typed`
reports no new typing on that retry.

Telegram setup maps `telegram_settings`/`telegram_setup` to authenticated GET/PUT `/config/telegram`, with identical action payloads across IPC and HTTP. Mobile Settings mounts the same Telegram component.
