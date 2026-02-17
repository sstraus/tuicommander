# Architecture Overview

## Tech Stack

| Layer | Technology | Purpose |
|-------|------------|---------|
| Frontend | SolidJS + TypeScript | Reactive UI with fine-grained updates |
| Build | Vite + LightningCSS | Fast dev server, optimized CSS |
| Backend | Tauri (Rust) | Native APIs, PTY, git, system integration |
| Terminal | xterm.js + WebGL | GPU-accelerated terminal rendering |
| State | SolidJS reactive stores | Frontend state management |
| Persistence | JSON files via Rust | Platform-specific config directory |
| Testing | Vitest + SolidJS Testing Library | Unit/integration tests (~830 tests) |

## Hexagonal Architecture

The project follows hexagonal architecture with clear separation between layers:

```
┌──────────────────────────────────────────────┐
│                  UI Layer                     │
│  SolidJS Components (render + user input)     │
│  ┌──────┐ ┌───────┐ ┌─────────┐ ┌────────┐  │
│  │Sidebar│ │TabBar │ │Terminal │ │Settings│  │
│  └──┬───┘ └──┬────┘ └──┬──────┘ └──┬─────┘  │
├─────┼────────┼─────────┼───────────┼─────────┤
│     │    Application Layer (Hooks)  │         │
│  ┌──┴────────┴─────────┴───────────┴──┐      │
│  │ useGitOps · usePty · useTerminals  │      │
│  │ useGitHub · useDictation · etc.    │      │
│  └──┬────────┬─────────┬─────────────┘      │
├─────┼────────┼─────────┼─────────────────────┤
│     │   State Layer (Stores)   │              │
│  ┌──┴────────┴─────────┴──────┐              │
│  │ terminals · repositories   │              │
│  │ settings · github · ui     │              │
│  └──┬─────────────────────────┘              │
├─────┼────────────────────────────────────────┤
│     │     IPC / Transport Layer              │
│  ┌──┴─────────────────────────────────┐      │
│  │ invoke.ts / transport.ts           │      │
│  │ Tauri IPC (native) | HTTP (browser)│      │
│  └──┬─────────────────────────────────┘      │
├─────┼────────────────────────────────────────┤
│     │     Backend (Rust/Tauri)               │
│  ┌──┴─────────────────────────────────┐      │
│  │ pty · git · github · config        │      │
│  │ agent · worktree · dictation       │      │
│  │ output_parser · error_classification│      │
│  └────────────────────────────────────┘      │
└──────────────────────────────────────────────┘
```

## Design Principles

- **Logic in Rust**: All business logic, data transformation, and parsing implemented in the Rust backend. The frontend handles rendering and user interaction only.
- **Cross-Platform**: Targets macOS, Windows, and Linux. Uses Tauri cross-platform primitives.
- **KISS/YAGNI**: Minimal complexity, no premature abstractions.
- **Dual Transport**: Same app works as native Tauri desktop app or browser app via HTTP/WebSocket.

## Directory Structure

```
src/
├── components/           # SolidJS UI components
│   ├── Terminal/         # xterm.js wrapper with PTY integration
│   ├── Sidebar/          # Repository tree, branch list, CI rings
│   ├── TabBar/           # Terminal tabs with drag-to-reorder
│   ├── Toolbar/          # Window drag region, repo/branch display
│   ├── StatusBar/        # Status messages, zoom, dictation
│   ├── SettingsPanel/    # Tabbed settings (General, Agents, Services, etc.)
│   ├── DiffPanel/        # Git diff viewer with file tree
│   ├── MarkdownPanel/    # Markdown file browser and renderer
│   ├── HelpPanel/        # Keyboard shortcuts documentation
│   ├── GitOperationsPanel/ # Quick git operations (commit, push, pull)
│   ├── TaskQueuePanel/   # Agent task queue visualization
│   ├── PromptOverlay/    # Agent prompt interception UI
│   ├── PromptDrawer/     # Prompt library management
│   └── ui/               # Reusable UI primitives (CiRing, DiffViewer, etc.)
├── stores/               # Reactive state management
├── hooks/                # Business logic and side effects
├── utils/                # Pure utility functions
├── types/                # TypeScript type definitions
├── transport.ts          # IPC abstraction (Tauri vs HTTP)
└── invoke.ts             # Smart invoke wrapper

src-tauri/src/
├── lib.rs                # App setup, plugin init, command registration
├── main.rs               # Entry point
├── pty.rs                # PTY session lifecycle
├── git.rs                # Git operations
├── github.rs             # GitHub API integration
├── config.rs             # Configuration management
├── state.rs              # Global state (sessions, buffers, metrics)
├── agent.rs              # Agent binary detection and spawning
├── worktree.rs           # Git worktree management
├── output_parser.rs      # Terminal output parsing
├── prompt.rs             # Prompt template processing
├── error_classification.rs # Error classification and backoff
├── menu.rs               # Native menu bar
├── mcp_http.rs           # HTTP/WebSocket server
└── dictation/            # Voice dictation (Whisper)
    ├── mod.rs            # State management
    ├── audio.rs          # Audio capture (CPAL)
    ├── commands.rs       # Tauri commands
    ├── model.rs          # Whisper model management
    ├── transcribe.rs     # Whisper transcription
    └── corrections.rs    # Post-processing corrections
```

## Application Startup Flow

1. **Rust** (`main.rs`): Calls `tui_commander_lib::run()`
2. **Library** (`lib.rs`): Creates `AppState`, loads config, spawns HTTP server if enabled, builds Tauri app with plugins, registers 73+ commands, sets up native menu
3. **Frontend** (`index.tsx`): Mounts `<App />` component
4. **App** (`App.tsx`): Initializes all hooks, calls `initApp()` which hydrates stores from backend, detects binaries, sets up keyboard shortcuts, starts GitHub polling
5. **Render**: Full UI hierarchy with terminals, panels, overlays, and dialogs

## Module Dependencies

```
App.tsx
├── useAppInit       → hydrates all stores from Rust config
├── usePty           → PTY session management via invoke()
├── useGitOperations → branch switching, worktree creation
├── useTerminalLifecycle → tab management, zoom, copy/paste
├── useKeyboardShortcuts → global keyboard handler
├── useGitHub        → GitHub polling (uses githubStore)
├── useDictation     → push-to-talk (uses dictationStore)
├── useQuickSwitcher → branch quick-switch UI
├── useSplitPanes    → split terminal panes
└── useAppLazygit    → lazygit integration
```
