# Architecture Overview

## Tech Stack

| Layer | Technology | Purpose |
|-------|------------|---------|
| Frontend | SolidJS + TypeScript | Reactive UI with fine-grained updates |
| Build | Vite + LightningCSS | Fast dev server, optimized CSS |
| Backend | Tauri (Rust) | Native APIs, PTY, git, system integration |
| Terminal | alacritty_terminal + canvas | Native VT engine with GPU-accelerated rendering |
| State | SolidJS reactive stores | Frontend state management |
| Persistence | JSON files via Rust | Platform-specific config directory |
| Testing | Vitest + SolidJS Testing Library (frontend), `cargo nextest` (backend) | ~5,700 frontend tests (`vitest list`) plus a comparable Rust suite (~4,800 `#[test]`/`#[tokio::test]` attributes under `src-tauri/src`) |

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
│   ├── Terminal/         # Native terminal renderer (CanvasTerminal) with PTY integration
│   ├── Sidebar/          # Repository tree, branch list, CI rings
│   ├── TabBar/           # Terminal tabs with drag-to-reorder
│   ├── Toolbar/          # Window drag region, repo/branch display
│   ├── StatusBar/        # Status messages, zoom, dictation
│   ├── SettingsPanel/    # Tabbed settings (General, Agents, Services, etc.)
│   ├── GitPanel/         # Git panel (Changes, Log, Stashes)
│   ├── MarkdownPanel/    # Markdown file browser and renderer
│   ├── HelpPanel/        # Keyboard shortcuts documentation
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
├── git.rs                # Git cache and Tauri command adapters
├── github.rs             # GitHub API and state adapters
├── config.rs             # Configuration management
├── state.rs              # Global state (sessions, buffers, metrics)
├── agent.rs              # Agent binary detection and spawning
├── worktree.rs           # Worktree config, events, and Tauri command adapters
├── prompt.rs             # Prompt template processing
├── menu.rs               # Native menu bar
├── mcp_http/             # HTTP/WebSocket + MCP server (routes split per area)
└── dictation/            # Voice application adapters
    ├── mod.rs            # DictationState and domain re-exports
    ├── commands.rs       # Tauri commands and event emission
    ├── browser.rs        # Browser audio transport
    ├── adapters.rs       # PTY delivery ports
    ├── model_download.rs # Whisper HTTP download
    └── asset_download.rs # Speech asset HTTP download

src-tauri/crates/tuic-dictation/src/  # Audio, hands-free and speech domain
```

`src-tauri/crates/` is a sibling of `src-tauri/src/`. `tuic-core` owns shared configuration and path utilities; `tuic-terminal` owns terminal parsing and buffers; `tuic-git` owns blocking Git reads, subprocesses, branch and worktree operations, artifact warming, and pure GitHub models and parsing. Root Git and GitHub adapters own Tokio scheduling, API clients, state, event emission, and Tauri commands.

## Application Startup Flow

1. **Rust** (`main.rs`): Calls `tui_commander_lib::run()`
2. **Library** (`lib.rs`): Creates `AppState`, loads config, spawns HTTP server if enabled, builds Tauri app with plugins, registers the Tauri command surface, and sets up the native menu
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
└── useSplitPanes    → split terminal panes
```

### Relay and Web Push cryptography

Relay AES-256-GCM and HKDF-SHA256 use `ring`. Web Push uses `ring` for
P-256 ephemeral ECDH, HKDF-SHA256, AES-128-GCM and ES256 VAPID signing.
The stored VAPID private key remains a base64url-encoded 32-byte scalar;
`p256` derives its public point because `ring` requires both components when
loading a scalar. Push records retain the existing single-record framing,
exact record size, padding delimiter and HTTP headers. Fixed migration guards
pin the relay bytes, RFC8291 intermediate values and ciphertext, real push
request bodies, and stored-key VAPID verification with both implementations.
