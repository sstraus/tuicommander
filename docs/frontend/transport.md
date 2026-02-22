# Transport Layer

The transport layer provides a unified IPC abstraction so the same frontend code works in both Tauri (native desktop) and browser (HTTP) modes.

## Files

| File | Purpose |
|------|---------|
| `src/invoke.ts` | Smart `invoke()` wrapper — zero overhead in Tauri |
| `src/transport.ts` | HTTP transport implementation and command-to-endpoint mapping |

## invoke.ts

```typescript
export function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>
export function listen<T>(event: string, handler: (event: Event<T>) => void): Promise<Unsubscribe>
```

**Resolution:** At module import time, detects if running in Tauri webview:
- **Tauri mode:** Delegates directly to `@tauri-apps/api/core.invoke()` (zero overhead)
- **Browser mode:** Maps command to HTTP endpoint via `transport.ts`

```typescript
export function isTauri(): boolean
// Checks window.__TAURI__ existence
```

## transport.ts

### Command Mapping

Maps every Tauri command name to an HTTP method + path:

```typescript
mapCommandToHttp("create_pty", config) → { method: "POST", path: "/sessions", body: config }
mapCommandToHttp("get_repo_info", { path }) → { method: "GET", path: "/repo/info?path=..." }
mapCommandToHttp("write_pty", { session_id, data }) → { method: "POST", path: "/sessions/{id}/write" }
```

### PTY Subscription

```typescript
export function subscribePty(
  sessionId: string,
  onData: PtyDataHandler,
  onExit: PtyExitHandler
): Unsubscribe
```

- **Tauri mode:** Uses `listen("pty-output")` and `listen("pty-exit")` Tauri events
- **Browser mode:** Opens WebSocket to `/sessions/{id}/stream`

### URL Building

```typescript
export function buildHttpUrl(path: string): string
// Reads MCP port from config, builds http://localhost:{port}{path}
```

## Design

The transport abstraction enables:

1. **Development:** Run frontend with `npm run dev` against the Rust HTTP server
2. **Browser mode:** Access TUICommander from a browser on another device
3. **Testing:** Frontend tests can mock at the invoke level
4. **MCP integration:** External tools use the same HTTP API

The abstraction is resolved once at module load — no per-call overhead in production Tauri mode.
