# Components

All components are SolidJS functional components in `src/components/`.

## Component Tree

```
App.tsx (central orchestrator)
├── Toolbar/                  # Window drag region, repo/branch display
├── Sidebar/                  # Repository tree with branches
│   ├── GroupSection          # Collapsible repo group with color + drag-reorder
│   ├── RepoSection           # Single repo entry with branches
│   ├── ParkedReposPopover    # Popover to recall parked (hidden) repos
│   ├── CiRing               # CI status ring per branch
│   ├── StatusBadge           # Git status badge (clean/dirty/conflict)
│   └── PrDetailPopover/      # PR details popup (CI, reviews, labels)
├── main
│   ├── TabBar/               # Terminal tabs with drag-to-reorder
│   ├── Terminal/             # xterm.js wrapper (never unmounted)
│   ├── TerminalArea/         # Terminal + split pane layout (up to 6 panes)
│   ├── SuggestOverlay/       # [[suggest: ...]] follow-up action chips
│   ├── GitPanel/             # Git panel (6 tabs)
│   │   ├── ChangesTab        # Staged/unstaged file list with stage/unstage/discard
│   │   ├── LogTab            # Commit log with expandable diffs
│   │   ├── StashesTab        # Stash list with apply/pop/drop/show
│   │   ├── BranchesTab       # Branch CRUD, prefix folding, search, checkout
│   │   ├── BlameTab          # Line-by-line git blame viewer
│   │   ├── HistoryTab        # Per-file commit history
│   │   ├── CommitGraph       # Visual commit graph with lane assignments
│   │   └── SyncRow           # Push/pull/fetch action bar
│   ├── DiffTab/              # Individual file diff tab (with Cmd+F search)
│   │   └── BranchDiffScrollView  # All-files scroll view (scroll mode)
│   ├── PrDiffTab/            # PR diff viewer tab
│   ├── CodeEditorPanel/      # CodeMirror 6 code editor tab
│   ├── MarkdownPanel/        # Markdown file browser
│   │   └── MarkdownRenderer  # Markdown to HTML (DOMPurify)
│   ├── MarkdownTab/          # Individual markdown file tab
│   ├── NotesPanel/           # Ideas/notes panel with edit, send, delete
│   ├── FileBrowserPanel/     # File tree browser with content search
│   │   └── TreeNode          # Recursive tree node (lazy-loaded)
│   ├── PlanPanel/            # Plan file browser (repo-scoped, from activity store)
│   ├── PluginPanel/          # Plugin HTML panel (sandboxed iframe)
│   ├── ClaudeUsageDashboard/ # Claude API usage dashboard (SolidJS)
│   ├── ErrorLogPanel/        # Application error log viewer
│   └── StatusBar/            # Status messages, agent badge, toggles
│       └── ZoomIndicator     # Font size display
├── SettingsPanel/            # Tabbed settings overlay
│   ├── tabs/GeneralTab       # Font, shell, IDE, theme
│   ├── tabs/AgentsTab        # Agent detection, run configs, Claude Usage toggle
│   ├── tabs/ServicesTab      # MCP, remote access, dictation
│   ├── tabs/GitHubTab        # GitHub OAuth login, token management
│   ├── tabs/PluginsTab       # Plugin management, logs
│   ├── tabs/KeyboardShortcutsTab # Rebindable keyboard shortcuts
│   ├── tabs/AppearanceTab    # Visual customization
│   ├── tabs/NotificationsTab # Sound and notification prefs
│   ├── tabs/RepoScriptsTab   # Per-repo scripts
│   └── tabs/RepoWorktreeTab  # Per-repo worktree options
├── HelpPanel/                # Keyboard shortcuts documentation
├── TaskQueuePanel/           # Agent task queue
├── PromptOverlay/            # Agent prompt interception
├── PromptDrawer/             # Prompt library management
├── CommandPalette/           # Cmd+P command palette
├── ActivityDashboard/        # Activity center (bell dropdown)
├── BranchSwitcher/           # Quick branch switcher (held-key overlay)
├── BranchPopover/            # Branch selection popover
├── TipOfTheDay/              # Startup tip notification
├── DictationToast/           # Dictation recording/transcribing indicator
├── ConfirmDialog/            # Reusable in-app confirmation dialog
├── RenameBranchDialog/       # Branch rename dialog
├── CreateWorktreeDialog/     # Worktree creation dialog
├── PostMergeCleanupDialog/   # Post-merge cleanup (switch base, pull, delete)
├── PromptDialog/             # Text input prompt dialog
├── RunCommandDialog/         # Configure terminal commands
├── WorktreeManager/          # Overlay panel for worktree management
├── MergePostActionDialog/    # Dialog for post-merge actions (keep/delete branch)
├── ContextMenu/              # Right-click menu for terminals
└── IdeLauncher/              # Open repository in IDE
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
- **Agents** — Agent detection, run configurations, Claude Usage toggle
- **Services** — MCP server, remote access, dictation settings
- **GitHub** — GitHub OAuth login (Device Flow), token management, diagnostics
- **Plugins** — Plugin management, enable/disable, log viewer
- **Keyboard Shortcuts** — Rebindable shortcuts (auto-populated from `actionRegistry.ts`)
- **Appearance** — Visual customization
- **Notifications** — Sound and notification preferences
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

### StatusBar (`StatusBar/`)

Status messages, agent badge, CWD display, ticker, and panel toggles.

**Layout (left to right):**
1. **ZoomIndicator** — font size display
2. **Status info** — notification text with pendulum ticker for overflow
3. **CWD** — current working directory (click to copy, shortened with `~/`)
4. **Agent badge** — unified agent + usage display (see below)
5. **Ticker** — rotating plugin messages (hidden when absorbed by agent badge)
6. **GitHub badges** — PR badge + CI badge with popover (center area)
7. **Toggle buttons** — Notes (with badge count), File Browser, Markdown, Diff, Dictation mic

**Agent Badge — display priority:**

The agent badge appears when the active terminal has a recognized agent type. It shows a single integrated element with the agent icon and the most relevant info, following this priority cascade:

| Priority | Condition | Display | Example |
|----------|-----------|---------|---------|
| 1 (highest) | PTY rate limit detected | Icon + warning + countdown | `⚠ 3m 20s` |
| 2 | Usage API available (Claude only) | Icon + usage percentages | `5h: 6% · 7d: 69%` |
| 3 | PTY usage limit parsed | Icon + percentage + limit type | `82% daily` |
| 4 (lowest) | No usage data | Icon + agent name | `claude` |

**Data sources:**
- **Rate limit (priority 1):** Detected by Rust output parser via regex on PTY output (e.g. "429", "rate limit", "too many requests"). Stored in `rateLimitStore`. Applies to all agents.
- **Usage API (priority 2):** Polled every 5 min from Claude's API by `claudeUsage.ts`. Posted to `statusBarTicker` with pluginId `"claude-usage"`. Claude Code only. When active, the separate ticker message is suppressed to avoid duplication.
- **PTY usage limit (priority 3):** Parsed from terminal output by the output parser (e.g. Claude's `[C1 S30 K26]` status line). Stored on the terminal entry as `usageLimit`. Applies to all agents that emit usage info.
- **Agent name (priority 4):** Fallback — just shows the agent type name.

**Ticker integration:** When the active agent is `claude` and the Claude Usage ticker is active, the ticker message is absorbed into the agent badge (priority 2) and hidden from the separate ticker area. Other ticker messages (from plugins, etc.) display normally.

**Pendulum ticker:** When the status info text overflows its container, a CSS pendulum animation scrolls the text back and forth at ~50px/s. Clicking the text dismisses the notification until the message changes.

**Notes badge:** The Ideas toggle button shows a count badge (accent-colored) with the number of notes visible for the current repo filter. Uses `notesStore.filteredCount()`.

**PR lifecycle in StatusBar:** CLOSED PRs are never shown. MERGED PRs are shown with a 5-minute activity-based grace period (accumulated user activity tracked by `userActivityStore`). OPEN PRs are shown as-is.

### NotesPanel (`NotesPanel/`)

Ideas/notes panel with per-repo filtering and terminal integration.

**Features:**
- Add, edit, delete notes
- Send note text to active terminal (marks note as "used")
- Notes filtered by active repo (global notes always visible)
- Reassign notes to different projects via dropdown
- Count badge in panel header and in the StatusBar toggle button
- Used notes shown with a checkmark and dimmed styling

### ConfirmDialog (`ConfirmDialog/`)

Reusable in-app confirmation dialog that replaces native Tauri `ask()` dialogs (which render as light-mode macOS system sheets). Uses shared `dialog.module.css` for consistent dark-theme styling.

**Props:** `visible`, `title`, `message`, `confirmLabel`, `cancelLabel`, `kind` (warning/info/error), `onClose`, `onConfirm`.

**Keyboard:** Enter confirms, Escape cancels.

### ClaudeUsageDashboard (`ClaudeUsageDashboard/`)

Native SolidJS component (not a plugin) showing Claude API usage data. Displayed as a tab in the markdown/editor area. Features rate bucket gauges, per-model token breakdown, daily usage chart, and project stats. Opened by clicking the Claude Usage ticker in the status bar.

## UI Primitives (`components/ui/`)

| Component | Description |
|-----------|-------------|
| `AgentIcon` | Agent type icon with consistent sizing and coloring |
| `CiRing` | SVG circular CI status indicator with proportional segments |
| `DiffViewer` | Syntax-highlighted unified diff renderer |
| `Dropdown` | Reusable dropdown select component |
| `MarkdownRenderer` | Safe markdown-to-HTML rendering with DOMPurify sanitization |
| `PanelResizeHandle` | Draggable resize handle for panel boundaries |
| `PromptOption` | Agent prompt multiple-choice option |
| `StatusBadge` | Git status badges (clean/dirty/conflict) |
| `ZoomIndicator` | Terminal font size indicator |

## Shared Components (`components/shared/`)

| Component | Description |
|-----------|-------------|
| `ColorPickerDialog` | Color selection dialog (used by repo groups) |
| `ColorSwatchPicker` | Preset color swatch grid |
| `KeyComboCapture` | Keyboard shortcut capture input (for keybinding editor) |
| `SearchBar` | Reusable search bar with regex/case-sensitive toggles |

## Panel Toggle States

| Panel | Toggle Shortcut | Store |
|-------|-----------------|-------|
| Sidebar | `Cmd+B` | `uiStore.toggleSidebar()` |
| Git Panel | `Cmd+Shift+D` | `uiStore.toggleGitPanel()` |
| Markdown Panel | `Cmd+M` | `uiStore.toggleMarkdownPanel()` |
| Notes/Ideas Panel | `Cmd+N` | `uiStore.toggleNotesPanel()` |
| File Browser | `Cmd+E` | `uiStore.toggleFileBrowserPanel()` |
| Plan Panel | `Cmd+Shift+P` | `uiStore.togglePlanPanel()` |
| Settings | `Cmd+,` | Local state in App.tsx |
| Help | `Cmd+?` | Local state in App.tsx |
| Prompt Library | `Cmd+K` | `promptLibraryStore.toggleDrawer()` |
| Task Queue | — | Local state in App.tsx |
| Command Palette | `Cmd+P` | `commandPaletteStore.toggle()` |
| Activity Dashboard | — | `activityDashboardStore.toggle()` |
| Worktree Manager | `Cmd+Shift+W` | `worktreeManagerStore.toggle()` |
