# TUICommander — Complete Feature Reference

> Canonical feature inventory. Update this file when adding, changing, or removing features.
> See [AGENTS.md](../AGENTS.md) for the maintenance requirement.

**Version:** 0.5.2 | **Last verified:** 2026-02-23

---

## 1. Terminal Management

### 1.1 PTY Sessions
- Up to 50 concurrent PTY sessions (configurable in Rust `MAX_SESSIONS`)
- Each tab runs an independent pseudo-terminal with the user's shell
- Terminals are never unmounted — hidden tabs stay alive with full scroll history
- Session persistence across app restarts (lazy restore on branch click)
- Foreground process detection (macOS: `libproc`, Windows: `CreateToolhelp32Snapshot`)
- PTY environment: `TERM=xterm-256color`, `COLORTERM=truecolor`, `LANG=en_US.UTF-8`
- Pause/resume PTY output (`pause_pty` / `resume_pty` Tauri commands) — suspends reader thread without killing the session

### 1.2 Tab Bar
- Create: `Cmd+T`, `+` button (click = new tab, right-click = split options)
- Close: `Cmd+W`, middle-click, context menu
- Reopen last closed: `Cmd+Shift+T` (remembers last 10 closed tabs)
- Switch: `Cmd+1` through `Cmd+9`, `Cmd+Shift+[` / `Cmd+Shift+]`
- Rename: double-click tab name (inline editing)
- Reorder: drag-and-drop with visual drop indicators
- Tab status dot (left of name): grey=running, green=idle, blue-pulse=background activity, orange-pulse=awaiting input, red-pulse=error
- Progress bar (OSC 9;4)
- Context menu (right-click): Close Tab, Close Other Tabs, Close Tabs to the Right, Detach to Window
- Detach to Window: right-click a tab to open it in a floating OS window
  - PTY session stays alive in Rust — floating window reconnects to the same session
  - Closing the floating window automatically returns the tab to the main window
  - Requires an active PTY session (disabled for tabs without a session)
- Tab pinning: pinned tabs are visible across all branches (not scoped to branch key)

### 1.3 Split Panes
- Vertical split: `Cmd+\` (side by side)
- Horizontal split: `Cmd+Alt+\` (stacked)
- Navigate: `Alt+←/→` (vertical), `Alt+↑/↓` (horizontal)
- Close active pane: `Cmd+W`
- Drag-resize divider between panes
- Maximum 2 panes at a time
- Split layout persists per branch

### 1.4 Zoom (Per-Terminal)
- Zoom in: `Cmd+=` (+2px)
- Zoom out: `Cmd+-` (-2px)
- Reset: `Cmd+0`
- Range: 8px to 32px
- Current zoom shown in status bar

### 1.5 Copy & Paste
- Copy selection: `Cmd+C`
- Paste to terminal: `Cmd+V`

### 1.6 Clear Terminal
- `Cmd+L` — clears display, running processes unaffected

### 1.7 Clickable File Paths
- File paths in terminal output are auto-detected and become clickable links
- Paths validated against filesystem before activation (Rust `resolve_terminal_path`)
- `.md`/`.mdx` → opens in Markdown panel; all other code files → opens in configured IDE
- Supports `:line` and `:line:col` suffixes for precise navigation
- Recognized extensions: rs, ts, tsx, js, jsx, py, go, java, kt, swift, c, cpp, cs, rb, php, lua, zig, css, scss, html, vue, svelte, json, yaml, toml, sql, graphql, tf, sh, dockerfile, and more

### 1.8 Find in Terminal
- `Cmd+F` opens search overlay in the active terminal pane
- Incremental search with highlight decorations (yellow matches, orange active match)
- Navigate matches: `Enter` / `Cmd+G` (next), `Shift+Enter` / `Cmd+Shift+G` (previous)
- Toggle options: case sensitive, whole word, regex
- Match counter shows "N of M" results
- `Escape` closes search and refocuses terminal
- Uses `@xterm/addon-search` for native xterm integration

### 1.9 International Keyboard Support
- Terminal handles international keyboard input correctly
- Rate-limit false positives reduced for non-ASCII input

### 1.10 Kitty Keyboard Protocol
- Supports Kitty keyboard protocol flag 1 (disambiguate escape codes)
- Per-session flag tracking via `get_kitty_flags` Tauri command
- Enables correct handling of `Shift+Enter` (multi-line input), `Ctrl+Backspace`, and modifier key combinations in agents that request the protocol (e.g. Claude Code)

---

## 2. Sidebar

### 2.1 Repository List
- Add repository via `+` button or folder dialog
- Click repo header to expand/collapse branch list
- Click again to toggle icon-only mode (shows initials)
- `⋯` button: Repo Settings, Show All Branches / Show Active Only, Move to Group, Park Repository, Remove

### 2.2 Repository Groups
- Named, colored groups for organizing repositories
- Create: repo `⋯` → Move to Group → New Group...
- Move repo: drag onto group header, or repo `⋯` → Move to Group → select group
- Remove from group: repo `⋯` → Move to Group → Ungrouped
- Group context menu (right-click header): Rename, Change Color, Delete
- Collapse/expand: click group header
- Reorder groups: drag-and-drop
- Color inheritance: repo color > group color > none

### 2.2.1 Show All Branches
- `⋯` → Show All Branches: lists every local git branch in the sidebar (not just worktrees)
- Toggle label changes to "Show Active Only" when active — click to revert to worktree-only view
- Per-repo state: each repository remembers its own setting independently
- Global default (Settings → General → Git Integration): configures the initial state for newly added repositories

### 2.3 Branch Items
- Click: switch to branch (shows its terminals, creates worktree if needed)
- Double-click branch name: rename branch
- Right-click context menu: Copy Path, Add Terminal, Delete Worktree, Open in IDE, Rename Branch
- CI ring: proportional arc segments (green=passed, red=failed, yellow=pending)
- PR badge: colored by state (green=open, purple=merged, red=closed, gray=draft) — click for detail popover
- Diff stats: `+N / -N` additions/deletions
- Question indicator: `?` icon (orange, pulsing) when agent asks a question
- Quick switcher badge: numbered index shown when `Cmd+Ctrl` held
- Branch sorting: main/master/develop always first, then alphabetical; merged PR branches sorted last

### 2.4 Git Quick Actions
- Bottom of sidebar when a repo is active
- Pull, Push, Fetch, Stash buttons — execute in active terminal

### 2.5 Sidebar Resize
- Drag right edge to resize (200-500px range)
- Toggle visibility: `Cmd+[`
- Width persists across sessions

### 2.6 Quick Branch Switcher
- Hold `Cmd+Ctrl` (macOS) or `Ctrl+Alt` (Win/Linux): show numbered overlay
- `Cmd+Ctrl+1-9`: switch to branch by index

### 2.7 Park Repos
- Right-click any repo in the sidebar to park or unpark it
- Parked repos are hidden from the main repository list
- Sidebar footer button opens a popover showing all parked repos
- Unpark a repo from the popover to restore it to the main list

---

## 3. Panels

### 3.1 Panel System
- File Browser, Markdown, and Diff panels are **mutually exclusive** — opening one closes the others
- Ideas panel is independent (can be open alongside any of the above)
- All panels have drag-resize handles on their left edge (200-800px)
- Min-width constraints prevent panels from collapsing (Markdown: 300px, File Browser: 200px)
- Toggle buttons in status bar with hotkey hints visible during quick switcher

### 3.2 Diff Panel (`Cmd+Shift+D`)
- Scope selector dropdown: Working tree (default) or any of the last 5 commits
- File list with change indicators
- Click a file to open a dedicated inline diff tab in the main tab area
- Auto-refreshes via repo watcher (`.git/` file change detection)

### 3.3 Markdown Panel (`Cmd+M`)
- Renders `.md` and `.mdx` files with syntax-highlighted code blocks
- File list from repository's markdown files
- Clickable file paths in terminal open `.md` files here
- Header bar shows file path (or title for virtual tabs) with Edit button (pencil icon) to open in CodeEditor

### 3.4 File Browser Panel (`Cmd+E`)
- Directory tree of active repository
- Navigation: `↑/↓` (navigate), `Enter` (open/enter dir), `Backspace` (parent dir)
- Breadcrumb bar: click any segment to jump to that directory
- Search filter: text input with `*` and `**` glob wildcard support
- Git status indicators: orange (modified), green (staged), blue (untracked)
- Context menu (right-click): Copy (`Cmd+C`), Cut (`Cmd+X`), Paste (`Cmd+V`), Rename, Delete, Add to .gitignore
- Keyboard shortcuts work when panel is focused (copy/cut/paste)
- Click file to open in code editor tab

### 3.5 Code Editor (CodeMirror 6)
- Opens in main tab area when clicking a file in file browser
- Syntax highlighting auto-detected from extension (disabled for files > 500 KB)
- Line numbers, bracket matching, active line highlight, Tab-to-indent
- Save: `Cmd+S` (when editor tab is focused)
- Read-only toggle: padlock icon in editor header
- Unsaved changes: dot indicator in tab bar and header
- Disk conflict detection: banner with "Reload" (discard local) or "Keep mine" options
- Auto-reloads silently when file changes on disk and editor is clean

### 3.6 Ideas Panel (`Cmd+N`)
- Quick notes / idea capture with send-to-terminal
- `Enter` submits idea, `Shift+Enter` inserts newline
- Per-idea actions: Edit (copies back to input), Send to Terminal (sends + return), Delete
- Mark as used: notes sent to terminal are timestamped (`usedAt`) for tracking
- Badge count: status bar toggle shows count of notes visible for the active repo
- Per-repo filtering: notes can be tagged to a repository; untagged notes visible everywhere
- Data persisted to Rust config backend

### 3.7 Help Panel (`Cmd+?`)
- Shows app info and links (About, GitHub, docs)
- Keyboard shortcuts are now in Settings > Keyboard Shortcuts tab (auto-generated from `actionRegistry.ts`)

### 3.8 Git Operations Panel (`Cmd+Shift+G`)
- Quick actions: Pull, Push, Fetch
- Branch operations: Merge, Checkout (with branch selector dropdown)
- Stash operations: Stash, Pop
- Merge conflict resolution: Abort Merge, Continue Merge, Accept Ours, Accept Theirs
- Auto-refreshes branch list via repo watcher

### 3.9 Task Queue Panel (`Cmd+J`)
- Task management with status tracking (pending, running, completed, failed, cancelled)
- Drag-and-drop task reordering

### 3.10 Command Palette (`Cmd+Shift+P`)
- Fuzzy-search across all app actions by name
- Recency-weighted ranking: recently used actions surface first
- Each row shows action label, category badge, and keybinding hint
- Keyboard-navigable: `↑/↓` to move, `Enter` to execute, `Esc` to close
- Powered by `actionRegistry.ts` (`ACTION_META` map)

### 3.11 Activity Dashboard (`Cmd+Shift+A`)
- Real-time view of all active terminal sessions in a compact list
- Each row shows: terminal name, agent type, status, last activity time
- Status color codes: green=working, yellow=waiting, red=rate-limited, gray=idle
- Rate limit indicators with countdown timers
- Click any row to switch to that terminal and close the dashboard
- Relative timestamps auto-refresh ("2s ago", "1m ago")

### 3.12 Error Log Panel (`Cmd+Shift+E`)
- Centralized log of all errors, warnings, and info messages across the app
- Sources: App, Plugin, Git, Network, Terminal, GitHub, Dictation, Store, Config
- Level filter tabs: All, Error, Warn, Info, Debug
- Source filter dropdown to narrow by subsystem
- Text search across all log messages
- Each entry shows timestamp, level badge (color-coded), source tag, and message
- Copy individual entries or all visible entries to clipboard
- Clear button to flush the log
- Status bar badge shows unseen error/warning count (red, resets when panel opens)
- Global error capture: uncaught exceptions and unhandled promise rejections are automatically logged
- Ring buffer of 1000 entries (oldest dropped when full)
- Also accessible via Command Palette: "Error log"

---

## 4. Toolbar

### 4.1 Sidebar Toggle
- `◧` button (left side) — same as `Cmd+[`
- Hotkey hint visible during quick switcher

### 4.2 Branch Display
- Center: shows `repo / branch` name
- Click to open branch rename dialog

### 4.3 Plan File Button
- Appears when an AI agent emits a plan file path (e.g., `PLAN.md`)
- Click: `.md`/`.mdx` files open in Markdown panel; others open in IDE
- Dismiss (×) button to hide without opening

### 4.4 PR Notification Bell
- Bell icon with count badge when PRs have updates
- Click: opens popover listing all active notifications
- Notification types: Merged, Closed, Conflicts, CI Failed, Changes Requested, Ready
- Click notification item: opens full PR detail popover for that branch
- Individual dismiss (×) per notification, or "Dismiss All"

### 4.5 IDE Launcher
- Button with current IDE icon — click to open repo/file in IDE
- Dropdown: shows all detected installed IDEs, grouped by category
- Categories: Code Editors, Terminals, Git Tools, System
- File-capable editors open the focused file (from editor or MD tab); others open the repo
- Run command button: `Cmd+R` (run), `Cmd+Shift+R` (edit & run)

---

## 5. Status Bar

### 5.1 Left Section
- Zoom indicator: current font size (shown when != default)
- Status info text (with pendulum ticker for overflow)
- CWD path: shortened with `~/`, click to copy to clipboard (shows "Copied!" feedback)
- Unified agent badge with priority cascade:
  1. Rate limit warning (highest): count + countdown timer when sessions are rate-limited
  2. Claude Usage API ticker: live utilization from Anthropic API (click opens dashboard)
  3. PTY usage limit: weekly/session percentage from terminal output detection
  4. Agent name (lowest): icon + name of detected agent
  - Color coding: blue < 70%, yellow 70-89%, red pulsing >= 90%
  - Claude usage ticker absorbed into badge when active agent is Claude (avoids duplicate display)
- Shared ticker area: multi-source rotating messages from plugins with source labels, counter badge (1/3 ▸), click-to-cycle, right-click popover, and priority tiers (low/normal/urgent)
- Update badge: "Update vX.Y.Z" (click to download & install), progress percentage during download

### 5.2 GitHub Section (center)
- Branch badge: name + ahead/behind counts — click for branch popover
- PR badge: number + state color — click for PR detail popover
  - PR lifecycle filtering: CLOSED PRs hidden immediately; MERGED PRs hidden after 5 minutes of accumulated user activity
- CI badge: ring indicator — click for PR detail popover

### 5.3 Right Section — Panel Toggles
- Ideas (lightbulb icon) — `Cmd+N`
- File Browser (folder icon) — `Cmd+E`
- Markdown (MD icon) — `Cmd+M`
- Diff (diff icon) — `Cmd+Shift+D`
- Mic button (when dictation enabled): hold to record, release to transcribe

---

## 6. AI Agent Support

### 6.1 Supported Agents
| Agent | Binary | Resume Command |
|-------|--------|----------------|
| Claude Code | `claude` | `claude --continue` |
| Gemini CLI | `gemini` | `gemini --resume` |
| OpenCode | `opencode` | `opencode -c` |
| Aider | `aider` | `aider --restore-chat-history` |
| Codex CLI | `codex` | `codex resume --last` |
| Amp | `amp` | — |
| Jules | `jules` | — |
| Cursor Agent | `cursor-agent` | — |
| Warp Oz | `oz` | — |
| ONA (Gitpod) | `gitpod` | — |
| Droid (Factory) | `droid` | — |

### 6.2 Agent Detection
- Auto-detection from terminal output patterns
- Brand SVG logos for each agent (fallback to capital letter)
- Agent badge in status bar showing active agent
- Binary detection: Rust probes well-known directories via `resolve_cli()` for reliable PATH resolution in desktop-launched apps

### 6.3 Rate Limit Detection
- Provider-specific regex patterns detect rate limit messages
- Status bar warning with countdown timer
- Per-session tracking with cleanup of expired limits

### 6.4 Question Detection
- Recognizes interactive prompts (yes/no, multiple choice, numbered options)
- Tab dot turns orange (pulsing) when awaiting input; sidebar branch icon shows `?` in orange
- Prompt overlay: keyboard navigation (↑/↓, Enter, number keys 1-9, Escape)
- Silence-based detection for unrecognized agents

### 6.5 Usage Limit Detection
- Claude Code weekly and session usage percentage (from PTY output patterns)
- Color-coded badge in status bar (blue < 70%, yellow 70-89%, red pulsing >= 90%)
- Integrated into unified agent badge (see section 5.1)

### 6.6 Claude Usage Dashboard
- Native SolidJS component (not a plugin panel — renders as a first-class tab)
- Opens via status bar agent badge click or `Cmd+Shift+A` action
- **Rate Limits section:** Live utilization bars from Anthropic OAuth usage API
  - 5-Hour, 7-Day, 7-Day Opus, 7-Day Sonnet, 7-Day Cowork buckets
  - Color-coded bars: green < 70%, yellow 70-89%, red >= 90%
  - Reset countdown per bucket
- **Usage Over Time chart:** SVG line chart of token usage over 7 days
  - Input tokens (blue) and output tokens (red) stacked area
  - Interactive hover crosshair with tooltip
- **Insights:** Session count, message totals, input/output tokens, cache stats
- **Activity heatmap:** 52-week GitHub-style contribution grid
  - Tooltip shows date, message count, and top 3 projects
- **Model Usage table:** Per-model breakdown (messages, input, output, cache)
- **Projects breakdown:** Per-project token usage with click to filter
- **Scope selector:** Filter all analytics by project slug
- **Auto-refresh:** API data polled every 5 minutes
- **Rust data layer:** Incremental JSONL parsing of `~/.claude/projects/*/` transcripts
  - File-size-based cache (only new bytes parsed on each scan)
  - Cache persisted to disk as JSON for fast restarts

### 6.7 Agent Configuration (Settings > Agents)
- **Agent list:** All supported agents with availability status and version detection
- **Run configurations:** Named command templates per agent (binary, args, env vars)
- **Default config:** One run config per agent marked as default for quick launching
- **MCP bridge install:** One-click install/remove of `tui-mcp-bridge` into agent's native MCP config file
- **Supported MCP agents:** Claude, Cursor, Windsurf, VS Code, Zed, Amp, Gemini
- **Edit agent config:** Opens agent's own configuration file in the user's preferred IDE
- **Context menu integration:** Right-click terminal > Agents submenu with per-agent run configurations
- **Busy detection:** Agents submenu disabled when a process is already running in the active terminal

---

## 7. Git Integration

### 7.1 Repository Info
- Branch name, remote URL, ahead/behind counts
- Read directly from `.git/` files (no subprocess for basic info)
- Repo watcher: monitors `.git/index`, `.git/refs/`, `.git/HEAD`, `.git/MERGE_HEAD` for changes

### 7.2 Worktrees
- Auto-creation on branch select (non-main branches)
- Storage: `{config_dir}/worktrees/{repo}--{branch}`
- Sci-fi themed auto-generated names
- Per-repo settings: base branch, copy ignored/untracked files
- Setup script: runs once after creation (e.g., `npm install`)
- Remove via sidebar `×` button or context menu (with confirmation)

### 7.3 HEAD File Watcher
- Watches `.git/HEAD` for branch changes via file system events
- Triggers UI refresh without polling
- When a terminal runs `git checkout -b new-branch` in the main working directory (not a worktree), the sidebar renames the existing branch entry in-place (preserving all terminal state) instead of creating a duplicate

### 7.4 Lazygit Integration
- In terminal: `Cmd+G`
- Split pane: `Cmd+Shift+L`
- Dedicated tab naming to avoid OSC title pollution
- Binary detection via `resolve_cli()`

### 7.5 Diff
- Working tree diff and per-commit diff (last 5 commits)
- Per-file diff in dedicated tab
- Diff stats: additions/deletions per branch

---

## 8. GitHub Integration

### 8.1 PR Monitoring
- GraphQL API (replaces `gh` CLI for data fetching)
- PR badge colors: green (open), purple (merged), red (closed), gray (draft)
- Merge state: Ready to merge, Checks failing, Has conflicts, Behind base, Blocked, Draft
- Review state: Approved, Changes requested, Review required
- PR lifecycle rules: CLOSED PRs hidden from sidebar and status bar; MERGED PRs shown for 5 minutes of accumulated user activity then hidden
- Auto-show PR popover filters out CLOSED and MERGED PRs (configurable in Settings > General)

### 8.2 CI Checks
- Ring indicator with proportional segments
- Individual check names and status in PR detail popover
- Labels with GitHub-matching colors

### 8.3 PR Detail Popover
- Title, number, link to GitHub
- Author, timestamps, state, merge readiness, review decision
- CI check details, labels, line changes, commit count
- Triggered from: sidebar PR badge, status bar PR badge, status bar CI badge, toolbar notification bell

### 8.4 PR Notifications
- Types: Merged, Closed, Conflicts, CI Failed, Changes Requested, Ready
- Toolbar bell with count badge
- Individual dismiss or dismiss all
- Click to open PR detail popover

### 8.5 Polling
- Active window: every 30 seconds
- Hidden window: every 2 minutes
- API budget: ~2 calls/min/repo

### 8.6 Token Resolution
- `gh_token` crate with empty-string bug workaround
- Fallback to `gh auth token` CLI

---

## 9. Voice Dictation

### 9.1 Whisper Inference
- Local processing via `whisper-rs` (no cloud)
- macOS: GPU-accelerated via Metal
- Linux/Windows: CPU-only

### 9.2 Models
| Model | Size | Quality |
|-------|------|---------|
| tiny | ~75 MB | Low |
| base | ~140 MB | Fair |
| small | ~460 MB | Good |
| medium | ~1.5 GB | Very good |
| large-v3-turbo | ~1.6 GB | Best (recommended) |

### 9.3 Push-to-Talk
- Default hotkey: `F5` (configurable, registered globally)
- Mic button in status bar: hold to record, release to transcribe
- Transcribed text injected into active terminal via PTY

### 9.4 Configuration
- Enable/disable, hotkey, language (auto-detect or explicit), model download
- Audio device selection
- Text correction dictionary (e.g., "new line" → `\n`)

---

## 10. Prompt Library

### 10.1 Access
- `Cmd+K` to open drawer
- Toolbar button

### 10.2 Prompts
- Create, edit, delete saved prompts
- Variable substitution: `{{variable_name}}`
- Built-in variables: `{{diff}}`, `{{changed_files}}`, `{{repo_name}}`, `{{branch}}`, `{{cwd}}`
- Custom variables prompt user for input
- Categories: Custom, Recent, Favorites
- Pin prompts to top
- Search by name or content

### 10.3 Keyboard Navigation
- `↑/↓`: navigate, `Enter`: insert, `Ctrl+N`: new, `Ctrl+E`: edit, `Ctrl+F`: toggle favorite, `Esc`: close

### 10.4 Run Commands
- `Cmd+R`: run saved command for active branch
- `Cmd+Shift+R`: edit command before running
- Configure per-repo in Settings → Repository → Scripts

---

## 11. Settings

### 11.1 General
- Language, Default IDE, Shell
- Confirmations: quit, close tab
- Power management: prevent sleep when busy
- Updates: auto-check, check now
- Git integration: auto-show PR popover
- Repository defaults: base branch, file handling, setup/run scripts

### 11.2 Appearance
- Terminal theme: multiple themes, color swatches
- Terminal font: 11 bundled monospace fonts (JetBrains Mono default)
- Default font size: 8-32px slider
- Split tab mode: separate / unified
- Max tab name length: 10-60 slider
- Repository groups: create, rename, delete, color-coded
- Reset panel sizes: restore sidebar and panel widths to defaults

### 11.3 Services
- MCP HTTP server: enable/disable, port, session count
- Remote access: port, username, password (bcrypt hash), URL display
- Voice dictation: full setup (see section 9)

### 11.4 Repository Settings (per-repo)
- Display name
- Worktree tab: base branch, copy ignored/untracked files
- Scripts tab: setup script (post-worktree), run script (`Cmd+R`)

### 11.5 Notifications
- Master toggle, volume (0-100%)
- Per-event: question, error, completed, warning
- Test buttons per sound
- Reset to defaults

### 11.6 Keyboard Shortcuts
- Settings > Keyboard Shortcuts tab (`Cmd+,` to open Settings)
- All app actions listed with their current keybinding
- Any action can be rebound by clicking it and pressing the new key combination
- Custom bindings stored in `keybindings.json` in the platform config directory
- Auto-populated from `actionRegistry.ts` (`ACTION_META` map) — new actions appear automatically

### 11.7 Agents
- See **6.7 Agent Configuration** for full details
- Claude Usage Dashboard enable/disable toggle (under Claude agent section)

---

## 12. Persistence

### 12.1 Rust Config Backend
All data persisted to platform config directory via Rust:
- `app_config.json` — general settings
- `notification_config.json` — sound settings
- `ui_prefs.json` — sidebar visibility/width
- `repo_settings.json` — per-repo worktree/script settings
- `repositories.json` — repository list, groups, branches
- `agents.json` — per-agent run configurations
- `prompt_library.json` — saved prompts
- `notes.json` — ideas panel data
- `dictation_config.json` — dictation settings
- `claude-usage-cache.json` — incremental session transcript parse cache

### 12.2 Hydration Safety
- `save()` blocks before `hydrate()` completes to prevent data loss

---

## 13. Cross-Platform

### 13.1 Supported Platforms
- macOS (primary), Windows, Linux

### 13.2 Platform Adaptations
- `Cmd` ↔ `Ctrl` key abstraction
- `resolve_cli()`: probes well-known directories when PATH unavailable (release builds)
- Windows: `cmd.exe` shell escaping, `CreateToolhelp32Snapshot` for process detection
- Windows: `if exist` syntax for lazygit config detection
- IDE detection: `.app` bundles (macOS), registry entries (Windows), PATH probing (Linux)

---

## 14. System Features

### 14.1 Auto-Update
- Check for updates on startup via `tauri-plugin-updater`
- Status bar badge with version
- Download progress percentage
- One-click install and relaunch
- Menu: Check for Updates (app menu and Help menu)

### 14.2 Sleep Prevention
- `keepawake` integration prevents system sleep while agents are working
- Configurable in Settings

### 14.3 Splash Screen
- Branded loading screen on app start

### 14.4 Confirmation Dialogs
- In-app `ConfirmDialog` component replaces native Tauri `ask()` dialogs
- Dark-themed to match the app (native macOS sheets render in light mode)
- `useConfirmDialog` hook provides a `confirm()` → `Promise<boolean>` API
- Pre-built helpers: `confirmRemoveWorktree()`, `confirmCloseTerminal()`, `confirmRemoveRepo()`
- Keyboard support: `Enter` to confirm, `Escape` to cancel

### 14.5 Error Handling
- ErrorBoundary crash screen with recovery UI
- WebGL canvas fallback (graceful degradation)
- Error classification with backoff calculation

### 14.6 MCP & HTTP Server
- REST API on localhost for external tool integration
- Exposes terminal sessions, git operations, agent spawning
- WebSocket streaming, Streamable HTTP transport
- Used by Claude Code, Cursor, and other tools via MCP protocol
- One-click MCP registration with Claude CLI (`claude mcp add` via `register_mcp_with_claude` command)

### 14.7 macOS Dock Badge
- Badge count for attention-requiring notifications (questions, errors)

---

## 15. Keyboard Shortcut Reference

### Terminal
| Shortcut | Action |
|----------|--------|
| `Cmd+T` | New terminal tab |
| `Cmd+W` | Close tab / close active split pane |
| `Cmd+Shift+T` | Reopen last closed tab |
| `Cmd+1`–`Cmd+9` | Switch to tab by number |
| `Cmd+Shift+[` / `]` | Previous / next tab |
| `Cmd+L` | Clear terminal |
| `Cmd+C` | Copy selection |
| `Cmd+V` | Paste to terminal |
| `Cmd+R` | Run saved command |
| `Cmd+Shift+R` | Edit and run command |

### Zoom
| Shortcut | Action |
|----------|--------|
| `Cmd+=` | Zoom in (+2px) |
| `Cmd+-` | Zoom out (-2px) |
| `Cmd+0` | Reset zoom |

### Split Panes
| Shortcut | Action |
|----------|--------|
| `Cmd+\` | Split vertically |
| `Cmd+Alt+\` | Split horizontally |
| `Alt+←/→` | Navigate vertical panes |
| `Alt+↑/↓` | Navigate horizontal panes |

### Panels
| Shortcut | Action |
|----------|--------|
| `Cmd+[` | Toggle sidebar |
| `Cmd+Shift+D` | Toggle diff panel |
| `Cmd+M` | Toggle markdown panel |
| `Cmd+N` | Toggle Ideas panel |
| `Cmd+E` | Toggle file browser |
| `Cmd+,` | Open settings |
| `Cmd+?` | Toggle help panel |
| `Cmd+K` | Prompt library |
| `Cmd+J` | Task queue |
| `Cmd+Shift+E` | Error log |

### Git & Lazygit
| Shortcut | Action |
|----------|--------|
| `Cmd+G` | Open lazygit in terminal |
| `Cmd+Shift+G` | Git operations panel |
| `Cmd+Shift+L` | Lazygit in split pane |

### File Browser (when focused)
| Shortcut | Action |
|----------|--------|
| `↑/↓` | Navigate files |
| `Enter` | Open file / enter directory |
| `Backspace` | Go to parent directory |
| `Cmd+C` | Copy file |
| `Cmd+X` | Cut file |
| `Cmd+V` | Paste file |

### Code Editor (when focused)
| Shortcut | Action |
|----------|--------|
| `Cmd+S` | Save file |

### Ideas Panel (when textarea focused)
| Shortcut | Action |
|----------|--------|
| `Enter` | Submit idea |
| `Shift+Enter` | Insert newline |

### Quick Switcher
| Shortcut | Action |
|----------|--------|
| Hold `Cmd+Ctrl` | Show quick switcher overlay |
| `Cmd+Ctrl+1-9` | Switch to branch by index |

### Voice Dictation
| Shortcut | Action |
|----------|--------|
| Hold `F5` | Push-to-talk (configurable) |

### Mouse Actions
| Action | Where | Effect |
|--------|-------|--------|
| Click | Sidebar branch | Switch to branch |
| Double-click | Sidebar branch name | Rename branch |
| Double-click | Tab name | Rename tab |
| Right-click | Tab | Tab context menu |
| Right-click | Sidebar branch | Branch context menu |
| Right-click | Sidebar repo `⋯` | Repo context menu |
| Right-click | Sidebar group header | Group context menu |
| Right-click | File browser entry | File context menu |
| Middle-click | Tab | Close tab |
| Drag | Tab | Reorder tabs |
| Drag | Sidebar right edge | Resize sidebar |
| Drag | Panel left edge | Resize panel |
| Drag | Split pane divider | Resize panes |
| Drag | Repo onto group | Move repo to group |
| Click | Status bar CWD path | Copy to clipboard |
| Click | PR badge (sidebar/status) | Open PR detail popover |
| Click | CI ring | Open PR detail popover |
| Click | Toolbar bell | Open notifications popover |
| Click | Status bar panel buttons | Toggle panels |
| Hold | Mic button (status bar) | Record dictation |

---

## 16. Build & Release

### 16.1 Makefile Targets
| Target | Description |
|--------|-------------|
| `dev` | Start development server |
| `build` | Build production app |
| `build-dmg` | Build macOS DMG |
| `sign` | Code sign the app |
| `notarize` | Notarize with Apple |
| `release` | Build + sign + notarize |
| `build-github-release` | Build for GitHub release (CI) |
| `publish-github-release` | Publish GitHub release |
| `github-release` | One-command release |
| `clean` | Clean build artifacts |

### 16.2 CI/CD
- GitHub Actions for cross-platform builds
- macOS code signing and notarization
- Linux: `libasound2-dev` dependency, `-fPIC` flags
- Updater signing with dedicated keys

## 17. Plugin System

### 17.1 Architecture
- Obsidian-style plugin API with 4 capability tiers
- Built-in plugins (TypeScript, compiled with app) and external plugins (JS, loaded at runtime)
- Hot-reload: file changes in plugin directories trigger automatic re-import
- Per-plugin error logging with ring buffer (500 entries)
- Capability-gated access: `pty:write`, `ui:markdown`, `ui:sound`, `ui:panel`, `ui:ticker`, `net:http`, `credentials:read`, `invoke:read_file`, `invoke:list_markdown_files`, `fs:read`, `fs:list`, `fs:watch`, `exec:cli`
- CLI execution API: sandboxed execution of whitelisted CLI binaries (`mdkb`) with timeout and size limits
- Filesystem API: sandboxed read, list, tail-read, and watch operations restricted to `$HOME`
- HTTP API: outbound requests scoped to manifest-declared URL patterns (SSRF prevention)
- Credential API: cross-platform credential reading (macOS Keychain, Linux/Windows JSON file) with user consent
- Panel API: rich HTML panels in sandboxed iframes (`sandbox="allow-scripts"`)
- Shared ticker system: `setTicker`/`clearTicker` API with source labels, priority tiers (low <10, normal 10-99, urgent >=100), counter badge, click-to-cycle, right-click popover
- Agent-scoped plugins: `agentTypes` manifest field restricts output watchers and structured events to terminals running specific agents (e.g. `["claude"]`)

### 17.2 Plugin Management (Settings > Plugins)
- **Installed tab:** List all plugins with enable/disable toggle, logs viewer, uninstall button
- **Browse tab:** Discover plugins from the community registry with one-click install/update
- **Enable/Disable:** Persisted in `AppConfig.disabled_plugin_ids`
- **ZIP Installation:** Install from local `.zip` file or HTTPS URL
- **Uninstall:** Removes plugin directory (confirmation required)

### 17.3 Plugin Registry
- Remote JSON registry hosted on GitHub (`tuicommander-plugins` repo)
- Fetched on demand with 1-hour TTL cache
- Version comparison for "Update available" detection
- Install/update via download URL

### 17.4 Deep Links (`tuic://`)
- `tuic://install-plugin?url=https://...` — Download and install plugin (HTTPS only, confirmation dialog)
- `tuic://open-repo?path=/path` — Switch to repo (must be in sidebar)
- `tuic://settings?tab=plugins` — Open Settings to specific tab

### 17.5 Built-in Plugins
- **Plan Tracker** — Detects Claude Code plan files from structured events

> **Note:** Claude Usage Dashboard was promoted from a plugin to a native SolidJS feature (see section 6.6). It is managed via Settings > Agents > Claude > Usage Dashboard toggle.

### 17.6 Example External Plugins
See `examples/plugins/` for reference implementations:
- `hello-world` — Minimal output watcher example
- `auto-confirm` — Auto-respond to Y/N prompts
- `ci-notifier` — Sound notifications and markdown panels
- `repo-dashboard` — Read-only state and dynamic markdown
- `report-watcher` — Generic report file watcher with markdown viewer
- `claude-status` — Agent-scoped plugin (`agentTypes: ["claude"]`) tracking usage and rate limits
