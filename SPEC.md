# TUICommander Specification

**Version:** 0.5.1
**Last Updated:** 2026-02-23

## Overview

TUICommander is a multi-agent terminal orchestrator designed to manage multiple AI coding agents (Claude Code, Gemini CLI, OpenCode, Aider, Codex) in parallel. It provides per-pane zoom, git worktree isolation, and GitHub integration.

## Goals

1. **Parallel Agent Orchestration** - Run 50+ coding agents simultaneously
2. **Git Worktree Isolation** - Each task gets its own isolated workspace
3. **Per-Pane Font Control** - Independent zoom levels for each terminal
4. **Rate Limit Resilience** - Automatic fallback when agents hit rate limits
5. **Productivity Features** - Prompt library, keyboard shortcuts, IDE integration

## Architecture

### Technology Stack

| Layer | Technology | Purpose |
|-------|------------|---------|
| Frontend | SolidJS | Reactive UI with fine-grained reactivity |
| Terminal | xterm.js | Terminal emulation with WebGL-accelerated rendering |
| Backend | Rust + Tauri | Native PTY management, file system access |
| Build | Vite | Fast HMR development, optimized production builds |

### Why SolidJS?

- Fine-grained reactivity without virtual DOM
- Direct DOM manipulation for terminal performance
- Compile-time optimizations
- Smaller bundle size than React/Vue
- Familiar JSX syntax

### Component Architecture

```
App
├── Sidebar
│   ├── Repository List
│   └── Terminal List
├── TabBar
│   └── Terminal Tabs
├── Terminal Container
│   ├── Terminal (xterm.js)
│   ├── DiffPanel
│   ├── MarkdownPanel
│   └── IdeasPanel
├── StatusBar
├── PromptOverlay
└── PromptDrawer
```

## State Management

### Stores (SolidJS createStore)

#### terminalsStore
Manages terminal instances and their state.

```typescript
interface TerminalState {
  terminals: Record<string, TerminalData>;
  activeId: string | null;
  nextId: number;
}

interface TerminalData {
  id: string;
  sessionId: string | null;
  fontSize: number;
  name: string;
  awaitingInput: AwaitingInputType;
}

type AwaitingInputType = "question" | "error" | "confirmation" | null;
```

#### repositoriesStore
Manages the list of git repositories.

```typescript
interface Repository {
  path: string;
  displayName: string;
}
```

#### settingsStore
User preferences with localStorage persistence.

```typescript
interface SettingsState {
  ide: IdeType;
  fontFamily: FontType;
  defaultFontSize: number;
}
```

#### promptLibraryStore
Saved prompts with variable substitution.

```typescript
interface SavedPrompt {
  id: string;
  name: string;
  content: string;
  description?: string;
  shortcut?: string;
  category: PromptCategory;
  isFavorite: boolean;
  variables?: PromptVariable[];
  lastUsed?: number;
  useCount: number;
}

interface PromptVariable {
  name: string;
  description?: string;
  defaultValue?: string;
}
```

#### rateLimitStore
Tracks rate limit status per session.

```typescript
interface RateLimitInfo {
  sessionId: string;
  agentType: AgentType;
  detectedAt: number;
  retryAfterMs: number | null;
}
```

## Hooks

### usePty
PTY session management via Tauri events.

```typescript
interface PtyHook {
  spawn(cols: number, rows: number): Promise<string>;
  write(sessionId: string, data: string): Promise<void>;
  resize(sessionId: string, cols: number, rows: number): Promise<void>;
  close(sessionId: string): Promise<void>;
  canSpawn(): Promise<boolean>;
  onData(sessionId: string, callback: (data: string) => void): () => void;
  onExit(sessionId: string, callback: () => void): () => void;
}
```

### useRepository
Git operations.

```typescript
interface RepositoryHook {
  getInfo(path: string): Promise<{ path: string; name: string }>;
  getDiff(path: string): Promise<string>;
  openInApp(path: string, app: IdeType): Promise<void>;
}
```

### useGitHub
GitHub CLI wrapper.

```typescript
interface GitHubHook {
  getPRStatus(path: string): Promise<PRStatus | null>;
  getBranchInfo(path: string): Promise<BranchInfo>;
}
```

### useKeyboardRedirect
Redirects keyboard input from non-terminal areas to active terminal.

## Agent Types

```typescript
type AgentType = "claude" | "gemini" | "opencode" | "aider" | "codex" | "unknown";

const AGENT_CONFIGS: Record<AgentType, AgentConfig> = {
  claude: { name: "Claude Code", command: "claude" },
  gemini: { name: "Gemini CLI", command: "gemini" },
  opencode: { name: "OpenCode", command: "opencode" },
  aider: { name: "Aider", command: "aider" },
  codex: { name: "Codex", command: "codex" },
  unknown: { name: "Unknown", command: "" },
};
```

## Rate Limit Detection

Provider-specific patterns detect rate limits in terminal output:

### Claude Code
- `rate limit`
- `API rate limit`
- `overloaded`
- `try again later`
- Retry-after extraction from error messages

### Gemini CLI
- `429`
- `quota exceeded`
- `rate limit exceeded`
- `resource exhausted`

### Generic Patterns
- `too many requests`
- `rate limited`
- `slow down`
- `retry after`

## Output Parser

JSONL event parser for structured agent output:

```typescript
type OutputEventType = "result" | "assistant" | "error" | "tool" | "system" | "unknown";

interface OutputEvent {
  type: OutputEventType;
  content: string;
  timestamp: number;
  raw?: unknown;
}
```

Features:
- Streaming parser with line buffering
- 100KB buffer limit to prevent memory bloat
- Handles partial lines across chunks

## Keyboard Shortcuts

### Global
| Shortcut | Action |
|----------|--------|
| Cmd+T | New terminal |
| Cmd+W | Close terminal |
| Cmd+K | Open prompt library |
| Cmd+Shift+D | Toggle diff panel |
| Cmd+M | Toggle markdown panel |
| Cmd+N | Toggle Ideas panel |
| Cmd+1-9 | Switch to tab N |
| Cmd++/- | Zoom in/out |
| Cmd+0 | Reset zoom |
| Cmd+F | Find in terminal |
| Cmd+E | Toggle file browser |
| Cmd+[ | Toggle sidebar |
| Cmd+? | Toggle help panel |
| Cmd+G | Open lazygit |
| Cmd+Shift+G | Git operations panel |
| Cmd+Shift+L | Lazygit split pane |
| Cmd+Shift+[ | Previous tab |
| Cmd+Shift+] | Next tab |
| Cmd+Shift+T | Reopen closed tab |
| Cmd+Shift+P | Command palette |
| Cmd+Shift+A | Activity dashboard |
| Cmd+, | Settings |

### Prompt Library
| Shortcut | Action |
|----------|--------|
| ↑/↓ | Navigate prompts |
| Enter | Insert prompt |
| Ctrl+N | New prompt |
| Ctrl+E | Edit selected |
| Ctrl+F | Toggle favorite |
| Esc | Close drawer |

## Persistence

All stores persist to localStorage:

| Key | Store | Content |
|-----|-------|---------|
| `tui-commander-settings` | settingsStore | IDE, font, preferences |
| `tui-commander-prompt-library` | promptLibraryStore | Saved prompts |

## Feature Status

### Completed (P1)
- [x] Multi-agent support (Claude, Gemini, OpenCode, Aider, Codex)
- [x] Git worktree management per task
- [x] Agent spawning integration
- [x] SolidJS migration

### Completed (P2)
- [x] Split pane layout
- [x] Multi-repository sidebar
- [x] Git diff panel
- [x] Interactive agent prompts UI
- [x] IDE launcher dropdown
- [x] GitHub integration
- [x] Parallel agent orchestration
- [x] Font selection setting
- [x] Tab bar with keyboard navigation
- [x] Density modes for readability
- [x] Status bar with branch and PR info
- [x] Rate limit detection
- [x] JSONL output parsing
- [x] Prompt library with variables
- [x] Keyboard redirect to terminal
- [x] Fallback agent chain
- [x] Ideas panel (formerly Notes) with send-to-terminal and delete actions
- [x] Terminal session persistence across app restarts
- [x] GitHub GraphQL API (replaces gh CLI for PR/CI data)
- [x] Auto-update via tauri-plugin-updater with progress badge
- [x] Prevent system sleep while agents are working (keepawake)
- [x] Usage limit detection for Claude Code (weekly/session) with status bar badge
- [x] Repository groups with accordion UI (named, colored, collapsible, drag-and-drop)
- [x] HEAD file watcher for branch change detection
- [x] Git status via .git file reads (no subprocess)
- [x] Lazygit integration (inline, split pane, floating window)
- [x] Lazy terminal restore (sessions materialize on branch click, not app startup)
- [x] Windows compatibility (shell escaping, process detection, resolve_cli, IDE detection)
- [x] Repo watcher for automatic GitOperationsPanel refresh
- [x] Context menu submenus and "New Group..." via PromptDialog
- [x] File Browser panel (`Cmd+E`)
- [x] CodeMirror code editor
- [x] Find in terminal (`Cmd+F`)
- [x] Configurable keybindings system
- [x] Command palette (`Cmd+Shift+P`)
- [x] Activity dashboard (`Cmd+Shift+A`)
- [x] Park repos feature
- [x] Plugin system (see FEATURES.md section 17)
- [x] Remote access / HTTP server
- [x] Copy Path in Markdown panel

### Completed (Voice Dictation)
- [x] Local Whisper inference via whisper-rs (Metal GPU acceleration)
- [x] Audio capture (cpal, 16kHz mono resampling)
- [x] Text correction map (longest-match-first dictionary)
- [x] Model download from HuggingFace (large-v3-turbo)
- [x] Push-to-talk mic button in StatusBar (blue pulsing animation)
- [x] Configurable push-to-talk hotkey (keydown/keyup)
- [x] Transcribed text injection into active terminal via PTY
- [x] Settings > Dictation tab (model, hotkey, language, corrections)
- [x] Shell integration inject_text stub (prepared for external triggers)

### Pending (P2)
- [ ] Task completion detection
- [ ] Error handling strategy config
- [ ] Audio notification when agent awaits input
- [ ] IDE launcher with app icons

### Pending (P3)
- [ ] Markdown rendering in terminal
- [ ] Agent stats display
- [ ] Config file support
- [ ] Task queue UI
- [ ] Advanced keyboard shortcuts
- [ ] TypeScript PTY wrapper

## Future Considerations

### libghostty Integration
When libghostty-vt becomes stable, consider replacing xterm.js for:
- Better VT parsing accuracy
- Native rendering performance

### WebSocket Backend
For web deployment without Tauri:
- Go backend with PTY multiplexing
- WebSocket protocol for PTY I/O
- Session management

## References

- [SolidJS Documentation](https://www.solidjs.com/docs/latest)
- [xterm.js Documentation](https://xtermjs.org/docs/)
- [Tauri Documentation](https://tauri.app/v1/guides/)
- [Feasibility Analysis](docs/FEASIBILITY-ANALYSIS.md)
