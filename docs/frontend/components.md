# Components

All components are SolidJS functional components in `src/components/`.

## Component Tree

```
App.tsx (829 lines - central orchestrator)
├── Toolbar/                  # Window drag region, repo/branch display
├── Sidebar/                  # Repository tree with branches
│   ├── CiRing               # CI status ring per branch
│   ├── StatusBadge           # Git status badge (clean/dirty/conflict)
│   ├── BranchPopover         # Branch details popup
│   └── PrDetailPopover/      # PR details popup (CI, reviews, labels)
├── main
│   ├── TabBar/               # Terminal tabs with drag-to-reorder
│   ├── Terminal/             # xterm.js wrapper (never unmounted)
│   ├── DiffPanel/            # Git diff viewer
│   │   └── DiffViewer        # Syntax-highlighted diff renderer
│   ├── DiffTab/              # Individual file diff tab
│   ├── MarkdownPanel/        # Markdown file browser
│   │   └── MarkdownRenderer  # Markdown to HTML (DOMPurify)
│   ├── MarkdownTab/          # Individual markdown file tab
│   └── StatusBar/            # Status messages, zoom, toggles
│       └── ZoomIndicator     # Font size display
├── SettingsPanel/            # Tabbed settings overlay
│   ├── tabs/GeneralTab       # Font, shell, IDE, theme
│   ├── tabs/AgentsTab        # Agent fallback chains
│   ├── tabs/ServicesTab      # MCP, remote access, dictation
│   ├── tabs/RepoScriptsTab   # Per-repo scripts
│   └── tabs/RepoWorktreeTab  # Per-repo worktree options
├── HelpPanel/                # Keyboard shortcuts documentation
├── GitOperationsPanel/       # Quick git operations
├── TaskQueuePanel/           # Agent task queue
├── PromptOverlay/            # Agent prompt interception
├── PromptDrawer/             # Prompt library management
├── RenameBranchDialog/       # Branch rename dialog
├── RunCommandDialog/         # Configure terminal commands
├── ContextMenu/              # Right-click menu for terminals
├── IdeLauncher/              # Open repository in IDE
└── LazygitFloating           # Lazygit floating window
```

## Core Components

### Terminal (`Terminal/`)

xterm.js wrapper with full PTY integration.

**Responsibilities:**
- Creates and manages xterm.js Terminal instance
- Attaches WebGL renderer for GPU-accelerated rendering
- Subscribes to PTY output events
- Handles terminal resize (with debouncing)
- Applies font, theme, and zoom settings
- Link detection for clickable URLs
- Selection management for copy operations

**Key behavior:** Terminals are **never unmounted** — they stay in the DOM when switching tabs. Only visibility is toggled. This preserves terminal state (scroll position, content, active processes).

### Sidebar (`Sidebar/`)

Repository tree with branch management.

**Features:**
- Expandable/collapsible repository entries
- Icon-only collapsed mode
- Branch list with active branch highlight
- CI ring indicator per branch (from githubStore)
- PR status badge
- Diff stats (additions/deletions) per branch
- Context menu (right-click) for repo/branch operations
- Resizable width via drag handle (200-500px)
- Keyboard redirect to active terminal

### TabBar (`TabBar/`)

Terminal tab management.

**Features:**
- Tabs filtered to active branch only
- Drag-to-reorder tabs
- Tab rename (double-click)
- Close button per tab
- Activity indicator (dot) for background terminals
- Awaiting input indicator (question/error icons)
- Context menu: Close, Close Others, Close to Right

### SettingsPanel (`SettingsPanel/`)

Tabbed settings overlay.

**Tabs:**
- **General** — Font family, font size, shell, IDE, theme, confirmations
- **Agents** — Primary agent, fallback chain, auto-recovery
- **Services** — MCP server, remote access, dictation settings
- **Repo Scripts** — Setup script, run command per repository
- **Repo Worktree** — Base branch, copy ignored/untracked files

### PrDetailPopover (`PrDetailPopover/`)

Rich PR detail popup shown on hover/click in sidebar.

**Displays:**
- PR title, number, author
- State (open, merged, closed, draft)
- Merge readiness (ready, conflicts, behind, blocked)
- Review decision (approved, changes requested, review required)
- CI check summary (passed/failed/pending ring)
- Individual CI check details
- Labels with computed colors
- Line change counts (+additions/-deletions)
- Timestamps (created, updated)

## UI Primitives (`components/ui/`)

| Component | Description |
|-----------|-------------|
| `CiRing` | SVG circular CI status indicator with proportional segments |
| `DiffViewer` | Syntax-highlighted unified diff renderer |
| `MarkdownRenderer` | Safe markdown-to-HTML rendering with DOMPurify sanitization |
| `StatusBadge` | Git status badges (clean/dirty/conflict) |
| `ZoomIndicator` | Terminal font size indicator |
| `Dropdown` | Reusable dropdown select component |
| `PromptOption` | Agent prompt multiple-choice option |

## Panel Toggle States

| Panel | Toggle Shortcut | Store |
|-------|-----------------|-------|
| Sidebar | `Cmd+B` | `uiStore.toggleSidebar()` |
| Diff Panel | `Cmd+Shift+D` | `uiStore.toggleDiffPanel()` |
| Markdown Panel | `Cmd+M` | `uiStore.toggleMarkdownPanel()` |
| Settings | `Cmd+,` | Local state in App.tsx |
| Help | `Cmd+?` | Local state in App.tsx |
| Prompt Library | `Cmd+K` | `promptLibraryStore.toggleDrawer()` |
| Git Operations | `Cmd+G` | Local state in App.tsx |
| Task Queue | — | Local state in App.tsx |
