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
githubStore ──provides data to──> Sidebar (CI rings, PR badges), StatusBar (PR/CI badges)
settingsStore ──configures──> Terminal (font, theme, shell)
uiStore ──controls──> panel visibility, sidebar state
promptLibraryStore ──used by──> PromptDrawer, PromptOverlay
dictationStore ──manages──> dictation state, model downloads
notificationsStore ──plays──> sound alerts on terminal events
errorHandlingStore ──retries──> failed operations with backoff
statusBarTicker ──feeds──> StatusBar (rotating priority-based messages)
notesStore ──provides──> NotesPanel (ideas/notes), StatusBar (badge count)
userActivityStore ──tracks──> StatusBar (merged PR grace period)
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
    ├──> repositoriesStore.hydrate()    → load_repositories
    ├──> uiStore.hydrate()              → load_ui_prefs
    ├──> settingsStore.hydrate()        → load_app_config
    ├──> notificationsStore.hydrate()   → load_notification_config
    ├──> repoSettingsStore.hydrate()    → load_repo_settings
    ├──> repoDefaultsStore.hydrate()    → load_repo_defaults
    ├──> promptLibraryStore.hydrate()   → load_prompt_library
    ├──> notesStore.hydrate()           → load_notes
    ├──> keybindingsStore.hydrate()     → load_keybindings
    ├──> agentConfigsStore.hydrate()    → load_agent_configs
    └──> agentDetection.detectAll()     → detect installed AI agents
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
    every 30s (120s when tab hidden, 300s on rate limit, exponential backoff on errors)
    │
    ▼
invoke("get_repo_pr_statuses", {path})  +  invoke("get_github_status", {path})
    │                                          │
    ▼                                          ▼
Rust: GraphQL batch query to GitHub API   Rust: local git ahead/behind
    │                                          │
    ▼                                          ▼
githubStore.updateRepoData(path, statuses)    setState(remoteStatus)
    │
    ├──> detectTransitions() — emit PR notifications on state changes
    │    (merged, closed, blocked, ci_failed, changes_requested, ready)
    │
    ▼
Reactive updates to Sidebar CI rings, PR badges
```

#### PR State Filtering in StatusBar

The StatusBar applies lifecycle rules before displaying PR data:

```
githubStore.getBranchPrData(repoPath, branch)
    │
    ├── state = CLOSED → never show (filtered out)
    ├── state = MERGED → show for 5 min of accumulated user activity, then hide
    │                     (userActivityStore tracks click/keydown events)
    └── state = OPEN   → show as-is (PR badge + CI badge)
```

#### Per-Repo Immediate Polls

On `repo-changed` events (git index/refs/HEAD changes), `githubStore.pollRepo(path)` triggers an immediate re-poll for that repo, debounced to 2 seconds to coalesce rapid git events.

### Claude Usage Polling

Claude Usage is a native feature (not a plugin) managed by `src/features/claudeUsage.ts`. It polls the Anthropic OAuth usage API and posts results to the status bar ticker.

```
initClaudeUsage()  (called from plugins/index.ts if not disabled)
    │
    every 5 min (API_POLL_MS)
    │
    ▼
invoke("get_claude_usage_api")
    │
    ▼
Rust: read ~/.claude/.credentials OAuth token
    → HTTP GET to Anthropic usage endpoint
    → parse UsageApiResponse (five_hour, seven_day, per-model buckets)
    │
    ▼
statusBarTicker.addMessage({
    id: "claude-usage:rate",
    pluginId: "claude-usage",
    text: "Claude: 5h: 42% · 7d: 18%",
    priority: 10–90 (based on utilization),
    onClick: openDashboard
})
    │
    ▼
StatusBar renders ticker message
    ├── Standalone ticker (when active agent is not claude)
    └── Absorbed into agent badge (when active agent is claude)
```

### Status Bar Ticker

The `statusBarTicker` store provides a priority-based rotating message system used by the Claude Usage feature and available to plugins via the `ui:ticker` capability.

```
statusBarTicker
    │
    ├── Messages sorted by priority (descending)
    ├── Equal-priority messages rotate every 5s
    ├── Expired messages scavenged every 1s (TTL-based)
    │
    └── StatusBar rendering:
        ├── Agent badge absorbs claude-usage messages when active agent is claude
        └── Standalone ticker for all other messages
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
