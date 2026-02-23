# State Management

## Overview

State is split between the Rust backend (source of truth for persistence) and SolidJS frontend stores (reactive UI state).

## Backend State (`src-tauri/src/state.rs`)

### AppState

The central backend state, shared across all Tauri commands via `State<'_, Arc<AppState>>`:

```rust
pub struct AppState {
    pub sessions: DashMap<String, Mutex<PtySession>>,       // Active PTY sessions
    pub worktrees_dir: PathBuf,                              // Worktree storage path
    pub metrics: SessionMetrics,                             // Atomic counters
    pub output_buffers: DashMap<String, Mutex<OutputRingBuffer>>, // MCP output access
    pub mcp_sse_sessions: DashMap<String, UnboundedSender<String>>, // SSE clients
    pub ws_clients: DashMap<String, Vec<UnboundedSender<String>>>,  // WebSocket clients
}
```

**Concurrency model:**
- `DashMap` for lock-free concurrent read/write of session maps
- `Mutex` for interior mutability of individual PTY writers and buffers
- `Arc<AtomicBool>` for pause/resume signaling per session
- `AtomicUsize` for zero-overhead metrics counters

### PtySession

```rust
pub struct PtySession {
    pub writer: Box<dyn Write + Send>,          // Write to PTY
    pub master: Box<dyn MasterPty + Send>,      // PTY master handle
    pub(crate) _child: Box<dyn Child + Send>,   // Child process
    pub(crate) paused: Arc<AtomicBool>,         // Pause flag
    pub worktree: Option<WorktreeInfo>,         // Associated worktree
    pub cwd: Option<String>,                    // Working directory
}
```

### SessionMetrics

Zero-overhead atomic counters:

```rust
pub(crate) struct SessionMetrics {
    pub(crate) total_spawned: AtomicUsize,
    pub(crate) failed_spawns: AtomicUsize,
    pub(crate) active_sessions: AtomicUsize,
    pub(crate) bytes_emitted: AtomicUsize,
    pub(crate) pauses_triggered: AtomicUsize,
}
```

### Buffer Types

| Buffer | Purpose | Capacity |
|--------|---------|----------|
| `Utf8ReadBuffer` | Accumulates bytes until valid UTF-8 boundary | Variable |
| `EscapeAwareBuffer` | Holds incomplete ANSI escape sequences | Variable |
| `OutputRingBuffer` | Circular buffer for MCP output access | 64 KB |

### Constants

```rust
pub(crate) const MAX_CONCURRENT_SESSIONS: usize = 50;
pub(crate) const OUTPUT_RING_BUFFER_CAPACITY: usize = 64 * 1024;
```

## Frontend Stores

### Store Pattern

All stores follow a consistent pattern:

```typescript
// Internal reactive state
const [state, setState] = createStore<Type>(defaults);

// Exported as a module object
export const myStore = {
  get state() { return state; },  // Read-only access
  hydrate() { ... },              // Load from Rust
  action() { ... },               // Mutate + persist
};
```

### Store Registry

| Store | File | Purpose | Persisted |
|-------|------|---------|-----------|
| `terminalsStore` | `terminals.ts` | Terminal instances, active tab, split layout | Partial (IDs in repos) |
| `repositoriesStore` | `repositories.ts` | Saved repos, branches, terminal associations | `repositories.json` |
| `settingsStore` | `settings.ts` | App settings (font, shell, IDE, theme) | `config.json` |
| `repoSettingsStore` | `repoSettings.ts` | Per-repository settings (scripts, worktree) | `repo-settings.json` |
| `uiStore` | `ui.ts` | Panel visibility, sidebar width | `ui-prefs.json` |
| `githubStore` | `github.ts` | PR/CI data per branch | Not persisted |
| `promptLibraryStore` | `promptLibrary.ts` | Prompt templates | `prompt-library.json` |
| `notificationsStore` | `notifications.ts` | Notification preferences | `notification-config.json` |
| `dictationStore` | `dictation.ts` | Dictation config and state | `dictation-config.json` |
| `errorHandlingStore` | `errorHandling.ts` | Error retry config | `ui-prefs.json` |
| `rateLimitStore` | `ratelimit.ts` | Active rate limits | Not persisted |
| `tasksStore` | `tasks.ts` | Agent task queue | Not persisted |
| `promptStore` | `prompt.ts` | Active prompt overlay state | Not persisted |
| `diffTabsStore` | `diffTabs.ts` | Open diff tabs | Not persisted |
| `mdTabsStore` | `mdTabs.ts` | Open markdown tabs | Not persisted |

### Key Store Relationships

```
repositoriesStore
    │
    ├── BranchState.terminals: string[]  ──references──> terminalsStore IDs
    ├── BranchState.worktreePath         ──managed by──> worktree.rs
    └── BranchState.additions/deletions  ──from──> git.rs (get_diff_stats)

terminalsStore
    │
    ├── TerminalData.sessionId           ──maps to──> AppState.sessions key
    └── TabLayout.panes                  ──indexes into──> TerminalData[]

githubStore
    │
    ├── Per-branch PR status             ──from──> github.rs (get_repo_pr_statuses)
    └── CheckSummary                     ──drives──> CiRing component

settingsStore
    │
    ├── font/theme                       ──configures──> Terminal component
    ├── shell                            ──passed to──> create_pty
    └── ide                              ──used by──> open_in_app
```

## Configuration Files

All config files are JSON, stored in the platform config directory:

| Platform | Path |
|----------|------|
| macOS | `~/Library/Application Support/tuicommander/` |
| Linux | `~/.config/tuicommander/` |
| Windows | `%APPDATA%/tuicommander/` |

Legacy path `~/.tuicommander/` is auto-migrated on first launch.

### Config File Map

| File | Contents | Rust Type |
|------|----------|-----------|
| `config.json` | Shell, font, theme, MCP, remote access | `AppConfig` |
| `notification-config.json` | Sound preferences, volume | `NotificationConfig` |
| `ui-prefs.json` | Sidebar, error handling settings | `UIPrefsConfig` |
| `repo-settings.json` | Per-repo scripts, worktree options | `RepoSettingsMap` |
| `repositories.json` | Saved repos and branches | `serde_json::Value` |
| `prompt-library.json` | Prompt templates | `PromptLibraryConfig` |
| `dictation-config.json` | Dictation on/off, hotkey, language, model | `DictationConfig` |
