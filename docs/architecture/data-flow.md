# Data Flow

## IPC Communication

TUICommander supports two IPC modes through a unified transport abstraction:

### Tauri Mode (Native Desktop)

```
Frontend (SolidJS) ──invoke()──> Tauri IPC ──> Rust Command
Frontend (SolidJS) <──listen()── Tauri Events <── Rust emit()
```

- Zero-overhead RPC via `@tauri-apps/api/core.invoke()`
- Event subscription via `@tauri-apps/api/event.listen()`
- Used when running as native Tauri application

### Browser Mode (HTTP/WebSocket)

```
Frontend (SolidJS) ──fetch()──> HTTP REST API ──> Rust Handler
Frontend (SolidJS) <──WebSocket── PTY Output Stream
```

- HTTP REST for all commands (mapped from Tauri command names)
- WebSocket for real-time PTY output streaming
- SSE for MCP JSON-RPC transport
- Used when running in browser via `npm run dev`

### Transport Abstraction

`src/invoke.ts` provides `invoke<T>(cmd, args)` that resolves to the correct transport at module initialization:

```typescript
// Zero overhead in Tauri - resolved once at import
const invoke = isTauri() ? tauriInvoke : httpInvoke;
```

`src/transport.ts` maps Tauri command names to HTTP endpoints:

```typescript
mapCommandToHttp("get_repo_info", { path }) → GET /repo/info?path=...
mapCommandToHttp("create_pty", config)       → POST /sessions
```

## PTY Output Pipeline

Terminal output flows through multiple processing stages:

```
PTY Process (shell)
    │
    ▼
Raw Bytes ──> Utf8ReadBuffer
    │         (handles split multi-byte UTF-8 characters)
    ▼
UTF-8 String ──> EscapeAwareBuffer
    │             (prevents splitting ANSI escape sequences)
    ▼
Safe String
    ├──> Ring Buffer (64KB, for MCP access)
    ├──> WebSocket broadcast (for browser clients)
    └──> Tauri Event ("pty-output", {session_id, data})
              │
              ▼
         Frontend: xterm.js terminal.write(data)
              │
              ▼
         OutputParser detects special events
         (rate limits, PR URLs, progress, prompts)
```

Each PTY session has a dedicated reader thread (spawned in `pty.rs`) that reads from the PTY master fd in a loop.

## State Management

### Frontend Stores

SolidJS reactive stores hold all frontend state. Each store follows the pattern:

```typescript
const [state, setState] = createStore<StoreType>(initialState);

// Public API exposed as object with methods
export const myStore = {
  state,        // Read-only reactive state
  hydrate(),    // Load from Rust backend
  action(),     // Modify state + persist to Rust
};
```

### Store Dependency Graph

```
repositoriesStore ──references──> terminalsStore (terminal IDs per branch)
githubStore ──provides data to──> Sidebar (CI rings, PR badges)
settingsStore ──configures──> Terminal (font, theme, shell)
uiStore ──controls──> panel visibility, sidebar state
promptLibraryStore ──used by──> PromptDrawer, PromptOverlay
dictationStore ──manages──> dictation state, model downloads
notificationsStore ──plays──> sound alerts on terminal events
errorHandlingStore ──retries──> failed operations with backoff
```

### Persistence Flow

```
User Action
    │
    ▼
Store.action()
    ├──> setState() (immediate reactive update)
    └──> invoke("save_xxx_config", data) (async persist to Rust)
              │
              ▼
         Rust: save_json_config(filename, data)
              │
              ▼
         JSON file in platform config directory
```

### Hydration Flow (App Startup)

```
useAppInit.initApp()
    │
    ├──> settingsStore.hydrate()      → load_app_config
    ├──> uiStore.hydrate()            → load_ui_prefs
    ├──> repositoriesStore.hydrate()  → load_repositories
    ├──> repoSettingsStore.hydrate()  → load_repo_settings
    ├──> notificationsStore.hydrate() → load_notification_config
    ├──> promptLibraryStore.hydrate() → load_prompt_library
    └──> errorHandlingStore.hydrate() → load_ui_prefs (error section)
```

## Event System

### Tauri Events (Backend → Frontend)

| Event | Payload | Source |
|-------|---------|--------|
| `pty-output` | `{session_id, data}` | PTY reader thread |
| `pty-exit` | `{session_id, exit_code}` | PTY child exit |
| `dictation-progress` | `{percent}` | Model download |
| `menu-event` | `{id}` | Native menu click |

### Frontend Event Handling

Menu events are handled in `App.tsx`:

```typescript
listen("menu-event", (event) => {
  switch (event.payload.id) {
    case "new-tab": handleNewTab(); break;
    case "close-tab": closeTerminal(); break;
    case "toggle-sidebar": uiStore.toggleSidebar(); break;
    // ... 30+ menu actions
  }
});
```

### GitHub Polling

```
githubStore.startPolling()
    │
    every 60s (30s base × visibility multiplier)
    │
    ▼
invoke("get_repo_pr_statuses", {path})
    │
    ▼
Rust: GraphQL batch query to GitHub API
    │
    ▼
githubStore.updateRepoData(path, statuses)
    │
    ▼
Reactive updates to Sidebar CI rings, PR badges
```

## Keyboard Shortcut Flow

```
KeyDown Event
    │
    ▼
useKeyboardShortcuts (global listener)
    │
    ├── Platform modifier detection (Cmd on macOS, Ctrl on Win/Linux)
    ├── Shortcut matching against registered handlers
    │
    ▼
Handler execution (e.g., handleNewTab, toggleSidebar)
    │
    ▼
Store updates → Reactive UI updates
```

Quick Switcher (held-key UI):

```
Cmd+Ctrl pressed (macOS) / Ctrl+Alt pressed (Win/Linux)
    │
    ▼
Show branch overlay with numbered shortcuts
    │
    ▼
Press 1-9 while holding modifier
    │
    ▼
switchToBranchByIndex(index) → handleBranchSelect()
    │
    ▼
Release modifier → hide overlay
```
