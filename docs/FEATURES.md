# TUICommander — Complete Feature Reference

> Canonical feature inventory. Update this file when adding, changing, or removing features.
> See [AGENTS.md](../AGENTS.md) for the maintenance requirement.

**Version:** 0.9.8 | **Last verified:** 2026-03-31

---

## 1. Terminal Management

### 1.1 PTY Sessions
- Up to 50 concurrent PTY sessions (configurable in Rust `MAX_SESSIONS`)
- Each tab runs an independent pseudo-terminal with the user's shell
- Terminals are never unmounted — hidden tabs stay alive with full scroll history
- Session persistence across app restarts (lazy restore on branch click)
- Agent session restore shows a clickable banner ("Agent session was active — click to resume") instead of auto-injecting the resume command; Space/Enter resumes, other keys dismiss
- Foreground process detection (macOS: `libproc`, Windows: `CreateToolhelp32Snapshot`)
- PTY environment: `TERM=xterm-256color`, `COLORTERM=truecolor`, `LANG=en_US.UTF-8`
- Pause/resume PTY output (`pause_pty` / `resume_pty` Tauri commands) — suspends reader thread without killing the session

### 1.2 Tab Bar
- Create: `Cmd+T`, `+` button (click = new tab, right-click = split options)
- Close: `Cmd+W`, middle-click, context menu
- Reopen last closed: `Cmd+Shift+T` (remembers last 10 closed tabs)
- Switch: `Cmd+1` through `Cmd+9`, `Ctrl+Tab` / `Ctrl+Shift+Tab`
- Rename: double-click tab name (inline editing)
- Reorder: drag-and-drop with visual drop indicators
- Tab status dot (left of name): grey=idle, blue-pulse=busy, green=done, purple=unseen (completed while not viewed), orange-pulse=question (needs input), red-pulse=error
- Tab type colors: red gradient=diff, blue gradient=editor, teal gradient=markdown, purple gradient=panel, amber gradient=remote PTY session
- Remote PTY sessions (created via HTTP/MCP) show "PTY:" prefix and amber styling
- Progress bar (OSC 9;4)
- Context menu (right-click): Close Tab, Close Other Tabs, Close Tabs to the Right, Detach to Window, Copy Path (on diff/editor/markdown file tabs)
- Detach to Window: right-click a tab to open it in a floating OS window
  - PTY session stays alive in Rust — floating window reconnects to the same session
  - Closing the floating window automatically returns the tab to the main window
  - Requires an active PTY session (disabled for tabs without a session)
- Overflow menu on scroll arrows (right-click) shows clipped tabs; the `+` button always stays visible regardless of scroll position
- Tab pinning: pinned tabs are visible across all branches (not scoped to branch key)

### 1.3 Split Panes
- Vertical split: `Cmd+\` (side by side)
- Horizontal split: `Cmd+Alt+\` (stacked)
- Navigate: `Alt+←/→` (vertical), `Alt+↑/↓` (horizontal)
- Close active pane: `Cmd+W`
- Drag-resize divider between panes
- Up to 6 panes in same direction (N-way split)
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
- **Copy on Select** — When enabled (Settings > Appearance), selecting text in the terminal automatically copies it to the clipboard. A brief 'Copied to clipboard' confirmation appears in the status bar.

### 1.6 Clear Terminal
- `Cmd+L` — clears display, running processes unaffected

### 1.7 Clickable File Paths
- File paths in terminal output are auto-detected and become clickable links
- Paths validated against filesystem before activation (Rust `resolve_terminal_path`)
- `.md`/`.mdx` → opens in Markdown panel; `.html`/`.htm` → opens in internal HTML preview tab (with "Open in browser" button); all other code files → opens in the built-in code editor
- `file://` URLs are recognized in addition to plain paths — the prefix is stripped and the path resolved like any other
- Supports `:line` and `:line:col` suffixes for precise navigation
- Recognized extensions: rs, ts, tsx, js, jsx, py, go, java, kt, swift, c, cpp, cs, rb, php, lua, zig, css, scss, html, vue, svelte, json, yaml, toml, sql, graphql, tf, sh, dockerfile, and more

### 1.8 Find in Content
- `Cmd+F` opens search overlay — context-aware: routes to terminal, markdown tab, or diff tab based on active view
- **Terminal:** incremental search via `@xterm/addon-search` with highlight decorations
- **Markdown viewer:** DOM-based search with cross-element matching (finds text spanning inline tags)
- **Diff viewer:** DOM-based search via SearchBar + DomSearchEngine (same engine as markdown viewer)
- Yellow highlight for matches, orange for active match
- Navigate matches: `Enter` / `Cmd+G` (next), `Shift+Enter` / `Cmd+Shift+G` (previous)
- Toggle options: case sensitive, whole word, regex
- Match counter shows "N of M" results
- `Escape` closes search and refocuses content

### 1.9 International Keyboard Support
- Terminal handles international keyboard input correctly
- Rate-limit false positives reduced for non-ASCII input

### 1.10 Move Terminal to Worktree
- Right-click a terminal tab → "Move to Worktree" submenu lists available worktrees (excluding the current one)
- Selecting a worktree sends `cd` to the PTY; OSC 7 auto-reassigns the terminal to the target branch
- Also available via Command Palette: dynamic "Move to worktree: \<branch\>" entries appear when the active terminal belongs to a repo with multiple worktrees
- Only shown when the repo has more than one worktree

### 1.11 OSC 7 CWD Tracking
- Terminals report their current working directory via OSC 7 escape sequences
- Parsed in the frontend via xterm.js `registerOscHandler(7, ...)`, then sent to Rust via `update_session_cwd` IPC and stored per-session as `session_cwd`
- When a terminal's CWD falls inside a known worktree path, the session is automatically reassigned to the correct branch in the sidebar
- Enables accurate branch association even when the user `cd`s into a different worktree from a single terminal

### 1.12 Kitty Keyboard Protocol
- Supports Kitty keyboard protocol flag 1 (disambiguate escape codes)
- Per-session flag tracking via `get_kitty_flags` Tauri command
- Enables correct handling of `Shift+Enter` (multi-line input), `Ctrl+Backspace`, and modifier key combinations in agents that request the protocol (e.g. Claude Code)

### 1.13 File Drag & Drop
- Drag files from Finder/Explorer onto the terminal area or any panel
- Uses Tauri's native `onDragDropEvent` API (not HTML5 File API — Tauri webviews do not expose file paths via HTML5)
- **Active PTY session:** dropped file paths are forwarded directly to the terminal as text (enables Claude Code image drops and similar workflows)
- **No active PTY session:** `.md`/`.mdx` files open in Markdown viewer, all other files open in Code Editor
- Multiple files can be dropped at once
- Visual overlay with dashed border appears during drag hover
- Global `dragover`/`drop` `preventDefault` prevents the Tauri webview from treating drops as browser navigation (which would replace the UI with a white screen)
- macOS file association: `.md`/`.mdx` files registered with TUICommander — double-click in Finder opens them directly

### 1.14 Cross-Terminal Search
- Type `~` in the command palette (`Cmd+P`) to search text across all open terminal buffers
- Results show terminal name, line number, and highlighted match text
- Selecting a result switches to the correct terminal tab/pane and scrolls to the matched line (centered in viewport)
- Minimum 3 characters after prefix
- Also accessible via the explicit "Search Terminals" command in the palette

### 1.15 Terminal Bell
- **Terminal Bell** — Configurable bell behavior when the terminal receives a BEL character (`\x07`). Four modes: `none` (silent), `visual` (screen flash animation), `sound` (plays the Info notification sound), `both` (flash + sound). Configure in Settings > Appearance.

---

## 2. Sidebar

### 2.1 Repository List
- Add repository via `+` button or folder dialog
- Click repo header to expand/collapse branch list
- Click again to toggle icon-only mode (shows initials)
- `⋯` button: Repo Settings, Switch Branch (via context menu on main worktree), Create Worktree, Move to Group, Park Repository, Remove
- **macOS TCC access dialog:** when the OS denies access to a repository directory (e.g. Desktop, Documents), a dialog explains the issue and guides the user to grant Full Disk Access in System Settings

### 2.2 Repository Groups
- Named, colored groups for organizing repositories
- Create: repo `⋯` → Move to Group → New Group...
- Move repo: drag onto group header, or repo `⋯` → Move to Group → select group
- Remove from group: repo `⋯` → Move to Group → Ungrouped
- Group context menu (right-click header): Rename, Change Color, Delete
- Collapse/expand: click group header
- Reorder groups: drag-and-drop
- Color inheritance: repo color > group color > none

### 2.2.1 Switch Branch
Right-click the main worktree row → **Switch Branch** submenu to checkout a different branch. The submenu shows all local branches with a checkmark on the current one. If the working tree is dirty, prompts to stash changes first. Blocks switching when a terminal has a running process.

### 2.3 Branch Items
- Click: switch to branch (shows its terminals, creates worktree if needed)
- Double-click branch name: rename branch
- Right-click context menu: Copy Path, Add Terminal, Create Worktree, Merge & Archive, Delete Worktree, Open in IDE, Rename Branch
- CI ring: proportional arc segments (green=passed, red=failed, yellow=pending)
- PR badge: colored by state (green=open, purple=merged, red=closed, gray=draft) — click for detail popover
- Diff stats: `+N / -N` additions/deletions
- Merged badge: branches merged into main show a "Merged" badge
- Question indicator: `?` icon (orange, pulsing) when agent asks a question
- Idle indicator: branch icons turn grey when the repo has no active terminals
- Quick switcher badge: numbered index shown when `Cmd+Ctrl` held
- Remote-only branches with open PRs: shown in sidebar with PR badge and inline accordion actions (Checkout, Create Worktree). Additional actions when PR popover is open: Merge, View Diff, Approve, Dismiss
- Dismiss/Show Dismissed: remote-only PRs can be dismissed from the sidebar; a "Show Dismissed" toggle reveals them again
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
- File Browser, Markdown, Diff, and Plan panels are **mutually exclusive** — opening one closes the others
- Ideas panel is independent (can be open alongside any of the above)
- Subtle fade transition when closing side panels (opacity + transform animation)
- All panels have drag-resize handles on their left edge (200-800px)
- Min-width constraints prevent panels from collapsing (Markdown: 300px, File Browser: 200px)
- Toggle buttons in status bar with hotkey hints visible during quick switcher

### 3.2 ~~Diff Panel~~ (Removed in 0.9.0)
Replaced by the Git Panel's Changes tab (section 3.8). `Cmd+Shift+D` now opens the Git Panel

### 3.3 Markdown Panel (`Cmd+Shift+M`)
- Renders `.md` and `.mdx` files with syntax-highlighted code blocks
- File list from repository's markdown files
- Clickable file paths in terminal open `.md` files here
- Auto-show: adding any markdown tab automatically opens the Markdown panel if it's closed
- Header bar shows file path (or title for virtual tabs) with Edit button (pencil icon) to open in CodeEditor
- `Cmd+F` search: find text in rendered markdown with highlight navigation (shared SearchBar component)

### 3.4 File Browser Panel (`Cmd+E`)
- Directory tree of active repository
- **Auto-refresh**: directory watcher detects external file changes (create/delete/rename) and refreshes automatically within ~1s, preserving selection
- Navigation: `↑/↓` (navigate), `Enter` (open/enter dir), `Backspace` (parent dir)
- Breadcrumb toolbar: always-visible path bar with click-to-navigate segments + inline sort dropdown (funnel icon)
- Search filter: text input with `*` and `**` glob wildcard support
- Git status indicators: orange (modified), green (staged), blue (untracked)
- Context menu (right-click): Copy (`Cmd+C`), Cut (`Cmd+X`), Paste (`Cmd+V`), Rename, Delete, Add to .gitignore
- Keyboard shortcuts work when panel is focused (copy/cut/paste)
- Sort dropdown: Name (alphabetical, directories first) or Date (newest first, directories first)
- **View modes**: flat list (default) and tree view — toggle via toolbar buttons. Tree view shows a collapsible hierarchy with lazy-loaded subdirectories on expand. Switching to tree resets to repo root. Search always uses flat results
- Click file to open in code editor tab

#### 3.4.1 Content Search (`Cmd+Shift+F`)
- Full-text search across file contents — toggle from filename search via the `C` button in the search bar
- Options: case-sensitive, regex, whole-word
- Results stream progressively and are grouped by file with match count per file
- Each result row shows file path, line number, and highlighted match context
- Click a result to open the file in the code editor at the matched line
- Binary files and files larger than 1 MB are automatically skipped
- Backed by `search_content` Tauri command; results delivered via `content-search-batch` events

### 3.5 Code Editor (CodeMirror 6)
- Opens in main tab area when clicking a file in file browser
- Syntax highlighting auto-detected from extension (disabled for files > 500 KB)
- Line numbers, bracket matching, active line highlight, Tab-to-indent
- Find/Replace: `Cmd+F` (find), `Cmd+G` / `Cmd+Shift+G` (next/prev), `Cmd+H` (replace), selection match highlighting
- Save: `Cmd+S` (when editor tab is focused)
- Read-only toggle: padlock icon in editor header
- Unsaved changes: dot indicator in tab bar and header
- Disk conflict detection: banner with "Reload" (discard local) or "Keep mine" options
- Auto-reloads silently when file changes on disk and editor is clean

### 3.6 Ideas Panel (`Cmd+Alt+N`)
- Quick notes / idea capture with send-to-terminal
- `Enter` submits idea, `Shift+Enter` inserts newline
- Per-idea actions: Edit (copies back to input), Send to Terminal (sends + return), Delete
- Mark as used: notes sent to terminal are timestamped (`usedAt`) for tracking
- Badge count: status bar toggle shows count of notes visible for the active repo
- Per-repo filtering: notes can be tagged to a repository; untagged notes visible everywhere
- **Image paste**: `Ctrl+V` / `Cmd+V` pastes clipboard images as thumbnails attached to the note
  - Images saved to `config_dir()/note-images/<note-id>/` on disk
  - Thumbnails displayed inline below note text and in the input area before submit
  - Image-only notes (no text) are supported
  - Images removed from disk when the note is deleted
  - Send to terminal appends absolute image paths so AI agents can read them
  - Max 10 MB per image; accepted formats: PNG, JPEG, WebP, GIF
- Edit preserves note identity (in-place update, no ID change)
- `Escape` cancels edit mode
- Data persisted to Rust config backend

### 3.7 Help Panel (`Cmd+?`)
- Shows app info and links (About, GitHub, docs)
- Keyboard shortcuts are now in Settings > Keyboard Shortcuts tab (auto-generated from `actionRegistry.ts`)

### 3.8 Git Panel (`Cmd+Shift+D`)
Tabbed side panel with four tabs: Changes, Log, Stashes, Branches. Replaces the former Git Operations Panel floating overlay and the standalone Diff Panel.

**Changes tab:**
- Porcelain v2 working tree status via `get_working_tree_status` (branch, upstream, ahead/behind, stash count, staged/unstaged/untracked files)
- Sync row: Pull, Push, Fetch buttons (background execution via `run_git_command`)
- Stage / unstage individual files or stage all / unstage all
- Discard unstaged changes (with confirmation dialog)
- Inline commit form with message input and Amend toggle
- Click a file row to open its diff in the diff panel
- Status icons per file: Modified, Added, Deleted, Renamed, Untracked
- Per-file diff counts (additions/deletions) shown inline
- Glob filter to narrow the file list
- Path-traversal validation on all stage/unstage/discard operations
- **History sub-panel** (collapsible): per-file commit history via `get_file_history` (follows renames), paginated with virtual scroll
- **Blame sub-panel** (collapsible): per-line blame via `get_file_blame` (porcelain format), age heatmap (green=recent, fading to neutral), commit metadata per line

**Log tab:**
- Paginated commit log via `get_commit_log` (default 50, max 500)
- Virtual scroll via `@tanstack/solid-virtual` for large histories
- Canvas-based commit graph via `get_commit_graph`: lane assignment, Bezier curve connections, 8-color palette, ref badges (branch, tag, HEAD). Graph follows HEAD only
- Click a commit row to expand and see its changed files (via `get_changed_files`)
- Click a file in an expanded commit to open its diff at that commit hash
- Relative timestamps (e.g., "3h ago")

**Stashes tab:**
- List all stash entries via `get_stash_list`
- Per-stash actions: Apply, Pop, Drop (via `run_git_command`)

**Branches tab (`Cmd+G` — opens Git Panel directly on this tab):**
- Local and Remote branches in collapsible sections
- Rich info per branch: ahead/behind counts (↑N ↓M), relative date, merged badge, stale dimming (branches with last commit > 30 days)
- Prefix folding: groups branches by `/` separator (e.g. `feature/`, `bugfix/`), toggle to expand/collapse groups
- Recent Branches section from git reflog
- Inline search/filter to narrow branch list
- Checkout (Enter / double-click): switches to the selected branch, with dirty worktree dialog (stash / force / cancel)
- **n** — Create new branch (inline form, optional checkout)
- **d** — Delete branch (safe + force options; refuses main branch and current branch)
- **R** — Rename branch (inline edit)
- **M** — Merge selected branch into current
- **r** — Rebase current onto selected branch
- **P** — Push branch (auto-detects missing upstream and sets tracking)
- **p** — Pull current branch
- **f** — Fetch all remotes
- Context menu (right-click): Checkout, Create Branch from Here, Delete, Rename, Merge into Current, Rebase Current onto This, Push, Pull, Fetch, Compare (shows `diff --name-status`)
- Backend: `get_branches_detail`, `delete_branch`, `create_branch`, `get_recent_branches`
- Click on sidebar "GIT" vertical label also opens Git Panel on the Branches tab

**Keyboard navigation:**
- `Escape` to close the panel
- `Ctrl/Cmd+1–4` to switch between tabs (1=Changes, 2=Log, 3=Stashes, 4=Branches)
- Auto-refreshes via repo revision subscription

### 3.9 Quick Branch Switch (`Cmd+B`)
- Fuzzy-search dialog to switch branches instantly
- Shows all local and remote branches for the active repo
- Badges: current, remote, main branch indicators
- Keyboard navigation: Arrow keys, Enter to switch, Escape to close
- Remote branches auto-checkout as local tracking branch
- Fetches live branch list via `get_git_branches`

### 3.10 Task Queue Panel (`Cmd+J`)
- Task management with status tracking (pending, running, completed, failed, cancelled)
- Drag-and-drop task reordering

### 3.11 Command Palette (`Cmd+P`)
- Fuzzy-search across all app actions by name
- Recency-weighted ranking: recently used actions surface first
- Each row shows action label, category badge, and keybinding hint
- Keyboard-navigable: `↑/↓` to move, `Enter` to execute, `Esc` to close
- **Search modes**: type `!` to search files by name, `?` to search file contents, `~` to search across all open terminal buffers. File/content results open in editor tab (content matches jump to the matched line). Terminal results navigate to the terminal tab/pane and scroll to the matched line. Leading spaces after prefix are ignored
- **Discoverable search commands**: "Search Terminals", "Search Files", "Search in File Contents" appear as regular palette commands and pre-fill the corresponding prefix
- Powered by `actionRegistry.ts` (`ACTION_META` map)

### 3.12 Activity Dashboard (`Cmd+Shift+A`)
- Real-time view of all active terminal sessions in a compact list
- Each row shows: terminal name, project name badge (last segment of CWD), agent type, status, last activity time
- Sub-rows (up to one shown per terminal, in priority order):
  - `currentTask` (gear icon) — current agent task from status-line parsing (e.g. "Reading files"). Suppressed for Claude Code (spinner verbs are decorative)
  - `agentIntent` (crosshair icon) — LLM-declared intent via `intent:` token
  - `lastPrompt` (speech bubble icon) — last user prompt (>= 10 words). Shown only when no `agentIntent` is present
- Status color codes: green=working, yellow=waiting, red=rate-limited, gray=idle
- Rate limit indicators with countdown timers
- Click any row to switch to that terminal and close the dashboard
- Relative timestamps auto-refresh ("2s ago", "1m ago")

### 3.13 Error Log Panel (`Cmd+Shift+E`)
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
- Ring buffer of 1000 entries (oldest dropped when full), Rust-backed — warn/error entries survive webview reloads via `push_log`/`get_logs` Tauri commands
- Also accessible via Command Palette: "Error log"

### 3.14 Plan Panel (`Cmd+Shift+P`)
- Lists active plan files for the current repository from the activity store
- Plans are detected via structured `plan-file` events from the output parser and via `plans/` directory watcher
- Click a plan to open it as a virtual markdown tab (frontmatter auto-stripped)
- Plan count badge in the header
- Repo-scoped: only shows plans belonging to the active repository
- Auto-open: restores the active plan from `.claude/active-plan.json` on startup; new plans opened as background tabs on first detection (no focus change)
- Directory watcher: monitors `plans/` directory for new plan files created externally
- Mutually exclusive with Markdown, Diff, and File Browser panels
- Panel width and visibility persist across restarts via `UIPrefsConfig`

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

### 4.4 Notification Bell
- Bell icon with count badge when notifications are available
- Click: opens popover listing all active notifications
- Empty state: shows "No notifications" when nothing is pending
- **PR Updates section** — types: Merged, Closed, Conflicts, CI Failed, CI Passed, Changes Requested, Ready
- **Git section** — background git operation results (push, pull, fetch) with success/failure status
- **Worktrees section** — worktree creation events (from MCP/agent)
- **Plugin activity sections** — registered by plugins via activityStore
- Click PR notification: opens full PR detail popover for that branch
- Individual dismiss (×) per notification, section "Dismiss All", auto-dismiss after 5min focused time

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
- Ideas (lightbulb icon) — `Cmd+Alt+N`
- File Browser (folder icon) — `Cmd+E`
- Markdown (MD icon) — `Cmd+Shift+M`
- Git (diff icon) — `Cmd+Shift+D` (opens Git Panel)
- Mic button (when dictation enabled): hold to record, release to transcribe

---

## 6. AI Agent Support

### 6.1 Supported Agents
| Agent | Binary | Resume Command |
|-------|--------|----------------|
| Claude Code | `claude` | `claude --resume <uuid>` (session-aware) / `claude --continue` (fallback) |
| Gemini CLI | `gemini` | `gemini --resume <uuid>` (session-aware) / `gemini --resume` (fallback) |
| OpenCode | `opencode` | `opencode -c` |
| Aider | `aider` | `aider --restore-chat-history` |
| Codex CLI | `codex` | `codex resume <uuid>` (session-aware) / `codex resume --last` (fallback) |
| Amp | `amp` | `amp threads continue` |
| Cursor Agent | `cursor-agent` | `cursor-agent resume` |
| Warp Oz | `oz` | — |
| Droid (Factory) | `droid` | — |
| Git (background) | `git` | — |

### 6.1.1 Session-Aware Resume
When an agent is detected running in a terminal, TUICommander automatically discovers its session ID from the filesystem and stores it per-terminal (`agentSessionId`). On restore, this enables session-specific resume instead of generic fallback commands.

- **Claude Code** — Sessions stored as `~/.claude/projects/<slug>/<uuid>.jsonl`; UUID from filename
- **Gemini CLI** — Sessions stored in `~/.gemini/tmp/<hash>/chats/session-*.json`; `sessionId` field from JSON
- **Codex CLI** — Sessions stored in `~/.codex/sessions/YYYY/MM/DD/rollout-*-<UUID>.jsonl`; UUID from filename

Discovery runs once per terminal on `null→agent` transition. Multiple concurrent agents are handled via a `claimed_ids` deduplication list. On agent exit, the stored session ID is cleared to allow re-discovery on next launch.

### 6.1.2 TUIC_SESSION Environment Variable
Every terminal tab has a stable UUID (`tuicSession`) injected as the `TUIC_SESSION` environment variable in the PTY shell. This UUID persists across app restarts and enables:

- **Manual session binding**: `claude --session-id $TUIC_SESSION` to start a session bound to this tab
- **Automatic resume**: On restore, TUICommander verifies if the session file exists on disk (`verify_agent_session`) before using `--resume $TUIC_SESSION`
- **UI spawn coherence**: When spawning agents via the context menu, `TUIC_SESSION` is used as `--session-id` automatically
- **Custom scripts**: `$TUIC_SESSION` is available as a stable key for any tab-specific state

### 6.2 Agent Detection
- Auto-detection from terminal output patterns
- Multi-agent status line detection via regex patterns anchored to line start: Claude Code (`*`/`✢`/`·` + task text + `...`/`…`), `[Running] Task` format, Aider (Knight Rider scanner `░█` + token reports), Codex CLI (`•`/`◦` bullet spinner with time suffix), Copilot CLI (`∴`/`●`/`○` indicators), Gemini CLI (braille dots `⠋⠙⠹...`)
- Status lines rejected when they appear in diff output, code listings, or block comments
- Brand SVG logos for each agent (fallback to capital letter)
- Agent badge in status bar showing active agent
- Binary detection: Rust probes well-known directories via `resolve_cli()` for reliable PATH resolution in desktop-launched apps
- Foreground process detection: `tcgetpgrp()` on the PTY master fd, then `proc_pidpath()` to get the binary name. Handles versioned binary paths (e.g. Claude Code installs as `~/.local/share/claude/versions/2.1.87`) by scanning parent directory names when the basename is not a known agent

### 6.3 Rate Limit Detection
- Provider-specific regex patterns detect rate limit messages
- Status bar warning with countdown timer
- Per-session tracking: rate-limit events are only accepted for sessions where agent activity has been detected (prevents false warnings in plain shell sessions)
- Auto-expire: rate limits are cleared automatically after `retry_after_ms` (or 120s default) without requiring agent output

### 6.4 Question Detection
- Recognizes interactive prompts (yes/no, multiple choice, numbered options)
- Tab dot turns orange (pulsing) when awaiting input; sidebar branch icon shows `?` in orange
- Prompt overlay: keyboard navigation (↑/↓, Enter, number keys 1-9, Escape)
- Two detection strategies run in priority order:
  1. **Screen-based** (Strategy 1): reads the live terminal screen, finds the last chat line above the prompt box (delimited by separator lines), checks if it ends with `?`. Works with Claude Code, Codex (`›` prompt), and Gemini (`> ` prompt) layouts
  2. **Silence-based** (Strategy 2, fallback): if terminal output stops for 10s after a line ending with `?`, the session is treated as awaiting input
- Stale candidate clearing: candidates that fail screen verification are purged so the same question can re-fire in a future agent cycle
- Echo suppression: user-typed input echoed by PTY is ignored for 500ms to prevent false question detection
- `extract_question_line()` scans all changed rows (not just the last) for question text, applied in both normal and headless reader threads
- Question state auto-clears when a `status-line` event fires (agent is actively working, so it's no longer awaiting input)

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
- **Insights:** Session count, message totals, input/output tokens, cache stats, tokens-per-hour metric (based on real active hours from session timestamps)
- **Activity heatmap:** 52-week GitHub-style contribution grid
  - Tooltip shows date, message count, and top 3 projects
- **Model Usage table:** Per-model breakdown (messages, input, output, cache)
- **Projects breakdown:** Per-project token usage with click to filter
- **Scope selector:** Filter all analytics by project slug
- **Auto-refresh:** API data polled every 5 minutes
- **Rust data layer:** Incremental JSONL parsing of `~/.claude/projects/*/` transcripts
  - File-size-based cache (only new bytes parsed on each scan)
  - Cache persisted to disk as JSON for fast restarts

### 6.7 Intent Event Tracking
- Agents declare work phases via `intent: text (Title)` tokens at column 0, colorized dim yellow in terminal output
- Colorization is agent-gated (only applied in sessions with a detected agent) to prevent false positives
- Structural tokens stripped from log lines served to PWA/REST consumers via `LogLine::strip_structural_tokens()`
- Structured `Intent` events emitted for LLM-declared work phase tracking
- Centralized debounced busy signal with completion notifications for accurate idle/active status

### 6.8 API Error Detection
- Detects API errors (server errors, auth failures) from agent output and provider-level JSON error responses
- Covers Claude Code, Aider, Codex CLI, Gemini CLI, Copilot, and raw API error JSON from providers (OpenAI, Anthropic, Google, OpenRouter, MiniMax)
- Triggers error notification sound and logs to the Error Log Panel

### 6.9 Agent Configuration (Settings > Agents)
- **Agent list:** All supported agents with availability status and version detection
- **Run configurations:** Named command templates per agent (binary, args, env vars)
- **Default config:** One run config per agent marked as default for quick launching
- **MCP bridge install:** One-click install/remove of `tui-mcp-bridge` into agent's native MCP config file
- **Supported MCP agents:** Claude, Cursor, Windsurf, VS Code, Zed, Amp, Gemini
- **Edit agent config:** Opens agent's own configuration file in the user's preferred IDE
- **Context menu integration:** Right-click terminal > Agents submenu with per-agent run configurations
- **Busy detection:** Agents submenu disabled when a process is already running in the active terminal
- **Environment Flags** — Per-agent environment variables injected into every new terminal session. Configure in Settings > Agents > expand an agent > Environment Flags. Useful for setting feature flags like `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` without manual export.

### 6.10 Agent Teams
- **Purpose:** Enables Claude Code's Agent Teams feature to use TUIC tabs instead of tmux panes
- **Approach:** Environment variable `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` injected into PTY sessions, which unlocks Claude Code's TeamCreate/TaskCreate/SendMessage tools. Agent spawning uses direct MCP tool calls (`agent spawn`) instead of the deprecated it2 shim
- **Session lifecycle events:** MCP-spawned sessions emit `session-created` and `session-closed` events so they automatically appear as tabs and clean up on exit
- **Settings toggle:** Settings > Agents > Agent Teams
- **Suggest follow-ups:** Agents can propose follow-up actions via `suggest: A | B | C` tokens, displayed as floating chip bar
- **Deprecated:** The it2 shim approach (iTerm2 CLI emulation) is commented out — superseded by direct MCP tool spawning

### 6.11 Suggest Follow-up Actions
- **Protocol:** Agents emit `suggest: action1 | action2 | action3` at column 0 after completing a task
- **Token concealment:** Suggest tokens are concealed in terminal output via line erasure or space replacement — the raw token never appears on screen. Concealment is agent-gated
- **Desktop:** Floating chip bar (SuggestOverlay) above terminal with larger buttons and keyboard shortcut badges (`1`–`9` to select, `Esc` to dismiss). Auto-dismiss after 30s, on typing, or on Esc
- **Mobile:** Horizontal scrollable pill buttons above CommandInput in SessionDetailScreen
- **Action:** Clicking a chip (or pressing its number key) sends the text to the PTY via `write_pty`
- **Settings:** Configurable via Settings > Agents > Show suggested follow-up actions

### 6.12 Slash Menu Detection
- When the user types `/` in a terminal, `slash_mode` activates and the output parser scans the bottom screen rows for slash command menus
- Detection: 2+ consecutive rows starting with `/command` patterns, with `❯` highlight for the selected item
- Produces `ParsedEvent::SlashMenu { items }` — used by mobile PWA to render a native bottom-sheet overlay
- `slash_mode` cleared on user-input events and status-line events

### 6.13 Inter-Agent Messaging
- New `messaging` MCP tool for agent-to-agent coordination when multiple agents are spawned in parallel
- **Identity**: Each agent uses its `$TUIC_SESSION` env var (stable tab UUID) as its messaging identity
- **Actions**: `register` (announce presence), `list_peers` (discover other agents), `send` (message a peer by tuic_session), `inbox` (poll for messages)
- **Dual delivery**: Real-time push via MCP `notifications/claude/channel` over SSE when the client supports channels; polling fallback via `inbox` always available
- **Channel support**: TUICommander declares `experimental.claude/channel` capability; spawned Claude Code agents automatically get `--dangerously-load-development-channels server:tuicommander`
- **Lifecycle**: Peer registrations cleaned up on MCP session delete and TTL reap; `PeerRegistered`/`PeerUnregistered` events broadcast via event bus for frontend visibility
- **Limits**: 64 KB max message size, 100 messages per inbox (FIFO eviction), optional project filtering for `list_peers`
- TUICommander acts as the messaging hub — no external daemon needed

---

## 7. Git Integration

### 7.1 Repository Info
- Branch name, remote URL, ahead/behind counts
- Read directly from `.git/` files (no subprocess for basic info)
- Repo watcher: monitors `.git/index`, `.git/refs/`, `.git/HEAD`, `.git/MERGE_HEAD` for changes

### 7.2 Worktrees
- Auto-creation on branch select (non-main branches)
- Configurable storage strategies: sibling (`__wt`), app directory, inside-repo (`.worktrees/`), or Claude Code default (`.claude/worktrees/`)
- Sci-fi themed auto-generated names
- Three creation flows: dialog (with base ref dropdown), instant (auto-name), right-click branch (quick-clone with hybrid `{branch}--{random}` name)
- Base ref selection: choose which branch to start from when creating new worktrees
- Per-repo settings: storage strategy, prompt on create, delete branch on remove, auto-archive, orphan cleanup, PR merge strategy, after-merge behavior
- Setup script: runs once after creation (e.g., `npm install`)
- Archive script: runs before a worktree is archived or deleted; non-zero exit blocks the operation
- Merge & Archive: right-click → merge branch into main, then archive or delete based on setting
- External worktree detection: monitors `.git/worktrees/` for changes from CLI or other tools
- Remove via sidebar `×` button or context menu (with confirmation)
- **Worktree Manager panel** (`Cmd+Shift+W` or Command Palette → "Worktree manager"):
  - Dedicated overlay listing all worktrees across all repos with metadata: branch name, repo badge, PR state (open/merged/closed), dirty stats, last commit timestamp
  - Orphan worktree detection with warning badge and Prune action
  - Repo filter pills and text search for branch names
  - Multi-select with checkboxes and select-all for batch operations
  - Batch delete and batch merge & archive
  - Single-row actions: Open Terminal, Merge & Archive, Delete (disabled on main worktrees)

### 7.3 Auto-Fetch
- Per-repo configurable interval (5/15/30/60 minutes, default: disabled)
- Background `git fetch --all` via non-interactive subprocess
- Bumps revision counter to refresh branch stats and ahead/behind counts
- Errors logged to appLogger, never blocking
- Master-tick architecture: single 1-minute timer checks all repos

### 7.4 Unified Repo Watcher
- Single watcher per repository monitoring the entire working tree recursively (replaces separate HEAD/index watchers)
- Uses raw `notify::RecommendedWatcher` with manual per-category trailing debounce
- Event categories: `Git` (HEAD, refs, index, MERGE_HEAD), `WorkTree` (source files), `Config` (app config changes)
- Each category has its own debounce window — git metadata changes propagate faster than file edits
- Respects `.gitignore` rules — ignored paths do not trigger refreshes
- **Gitignore hot-reload:** editing `.gitignore` rebuilds the ignore filter without restarting the watcher
- When a terminal runs `git checkout -b new-branch` in the main working directory (not a worktree), the sidebar renames the existing branch entry in-place (preserving all terminal state) instead of creating a duplicate

### 7.5 Diff
- Working tree diff and per-commit diff via Git Panel Changes tab
- Per-file diff counts (additions/deletions) shown inline in Changes tab
- Click a file row to view its diff
- **Side-by-side (split), unified (inline), and scroll (all files) view modes** — toggle in toolbar, preference persisted
- **Scroll mode (all-files diff)** — shows every changed file (staged + unstaged) in a continuous scrollable view with collapsible file sections, per-file addition/deletion stats, sticky header with totals, and clickable filenames that open in the editor. Reactively reloads on git operations via revision tracking
- **Auto-unified for new/deleted files** — split view is forced to unified when the diff is one-sided
- **Word-level diff highlighting** via `@git-diff-view/solid` with virtualized rendering
- **Hunk-level restore** — hover a hunk header to reveal a revert button (discard for working tree, unstage for staged)
- **Line-level restore** — click individual addition/deletion lines to select them (shift+click for ranges), then restore only the selected lines via partial patch
- Text selection and copy enabled in diff panels (`user-select: text`)
- `Cmd+F` search in diff tabs via SearchBar + DomSearchEngine
- Submodule entries are filtered from working tree status (not shown as regular files)
- Standalone DiffPanel removed in v0.9.0 (see section 3.2)

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
- View Diff button: opens PR diff as a dedicated panel tab with collapsible file sections, dual line numbers, and color-coded additions/deletions
- Merge button: visible when PR is open, approved, CI green — merges via GitHub API. Merge method auto-detected from repo-allowed methods; auto-fallback to squash on HTTP 405 rejection
- Approve button: submit an approving review via GitHub API (remote-only PRs)
- Post-merge cleanup dialog: after merge, offers checkable steps (switch to base, pull, delete local/remote branch)
- Review button: if the branch's active agent has a run config named "review", spawns a terminal running the interpolated command with `{pr_number}`, `{branch}`, `{base_branch}`, `{repo}`, `{pr_url}`. Hidden when no matching config exists
- Triggered from: sidebar PR badge, status bar PR badge, status bar CI badge, toolbar notification bell

### 8.4 CI Auto-Heal
- When CI checks fail on a branch with an active agent terminal, auto-heal can fetch failure logs and inject them into the agent
- Toggle per-branch via checkbox in PR detail popover (visible when CI checks are failing)
- Fetches logs via `gh run view --log-failed`, truncated to ~4000 chars
- Waits for agent to be idle/awaiting input before injecting
- Max 3 attempts per failure cycle, then stops and logs a warning
- Attempt counter visible in PR detail popover
- Status tracked per-branch in `BranchState.ciAutoHeal`

### 8.5 PR Notifications
- Types: Merged, Closed, Conflicts, CI Failed, Changes Requested, Ready
- Toolbar bell with count badge
- Individual dismiss or dismiss all
- Click to open PR detail popover

### 8.5 Merge PR via GitHub API
- Merge PRs directly from TUICommander without switching to GitHub web
- Configurable merge strategy per repo: merge commit, squash, or rebase (Settings > Repository > Worktree tab)
- Merge method auto-detected from repo's allowed methods via GitHub API (`get_repo_merge_methods`); auto-fallback to squash on HTTP 405 rejection
- Triggered from: PR detail popover (local branches), remote-only PR popover, Merge & Archive workflow (sidebar context menu)
- Post-merge cleanup dialog: sequential steps executed via Rust backend (not PTY — terminal may be occupied by AI agent)
  - Switch to base branch (auto-stash if dirty — inline warning shown with "Unstash after switch" checkbox)
  - Pull base branch (ff-only)
  - Close terminals + delete local branch (safe delete, refuses default branch)
  - Delete remote branch (gracefully handles "already deleted")
  - Steps are checkable — user can toggle which to execute
  - Per-step status reporting: pending → running → success/error
- After-merge behavior setting for worktrees: `archive` (auto-archive), `delete` (remove), `ask` (show dialog)
- When `afterMerge=ask`: unified cleanup dialog includes an archive/delete worktree step (with inline selector) alongside branch cleanup steps — replaces the old 3-button MergePostActionDialog

### 8.6 Auto-Delete Branch on PR Close
- Per-repo setting: Off (default) / Ask / Auto
- Triggered when GitHub polling detects PR merged or closed transition
- If branch has a linked worktree, removes worktree first then deletes branch
- Safety: never deletes default/main branch; dirty worktrees always escalate to ask mode
- Uses safe `git branch -d` (refuses unmerged branches)
- Deduplication prevents double-firing on the same PR

### 8.7 Polling
- Active window: every 30 seconds
- Hidden window: every 2 minutes
- API budget: ~2 calls/min/repo

### 8.8 Token Resolution
- Priority: `GH_TOKEN` env → `GITHUB_TOKEN` env → OAuth keyring token → `gh_token` crate → `gh auth token` CLI
- `gh_token` crate with empty-string bug workaround
- Fallback to `gh auth token` CLI

### 8.9 OAuth Device Flow Login
- One-click GitHub authentication from Settings > GitHub tab
- Uses GitHub OAuth App Device Flow (no client secret, works on desktop)
- Token stored in OS keyring (macOS Keychain, Windows Credential Manager, Linux Secret Service)
- Requested scopes: `repo`, `read:org`
- Shows user avatar, login name, and token source after authentication
- Logout removes OAuth token, falls back to env/gh CLI
- On 401: auto-clears invalid OAuth token and prompts re-auth

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
- Transcribed text inserts into the focused input element (textarea, input, contenteditable); falls back to active terminal PTY when no text input has focus. Focus target captured at key-press time.

### 9.4 Streaming Transcription
- Real-time partial results during push-to-talk via adaptive sliding windows
- First partial within ~1.5s, subsequent windows grow to 3s for quality
- VAD energy gate skips silence windows (prevents hallucination)
- Floating toast shows partial text above status bar during recording
- 200ms audio window overlap (`keep_ms`) carries context across windows for continuity
- Final transcription pass on full captured audio at key release

### 9.5 Microphone Permission Detection (macOS)
- On first use, checks microphone permission via macOS TCC (Transparency, Consent, and Control) framework
- Permission states: `NotDetermined` (will prompt), `Authorized`, `Denied`, `Restricted`
- If denied, shows a dialog guiding the user to System Settings > Privacy & Security > Microphone with an "Open Settings" button
- Linux/Windows: always returns `Authorized` (no TCC framework)

### 9.6 Configuration
- Enable/disable, hotkey, language (auto-detect or explicit), model download
- Audio device selection
- Text correction dictionary (e.g., "new line" → `\n`)
- **Auto-send** — Enable in Settings > Services > Dictation to automatically submit (press Enter) after transcription completes.

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
- `↑/↓`: navigate, `Enter`: insert (restores terminal focus), `Ctrl+N`: new, `Ctrl+E`: edit, `Ctrl+F`: toggle favorite, `Esc`: close

### 10.4 Run Commands
- `Cmd+R`: run saved command for active branch
- `Cmd+Shift+R`: edit command before running
- Configure per-repo in Settings → Repository → Scripts

### 10.5 Smart Prompts

AI automation layer with 24 built-in context-aware prompts. Each prompt includes a description explaining what it does. Prompts auto-resolve git context variables and execute via inject (PTY write), headless (one-shot subprocess), or API (direct LLM call) mode.

- **Open**: `Cmd+K` or toolbar lightning bolt button
- Drawer with category filtering (All/Custom/Recent/Favorites), search by name/description, and enable/disable toggles
- Prompt rows show inline badges: execution mode (inject/headless/api), built-in, placement tags
- Prompts are context-aware: 31 variables auto-resolved from git, GitHub, and terminal state
- **Variable Input Dialog**: unresolved variables show a compact form with variable name + description before execution
- **Edit Prompt dialog**: full editor with name, description, content textarea, variable insertion dropdown (grouped by Git/GitHub/Terminal with descriptions), placement checkboxes, execution mode + auto-execute side-by-side, keyboard shortcut capture
- **Auto-execute**: when enabled, inject-mode prompts send Enter immediately via agent-aware `sendCommand`; when disabled, text is pasted without Enter so the user can review before sending
- **API execution mode**: calls LLM providers directly via HTTP API (genai crate) without terminal or agent CLI. Per-prompt system prompt field. Output routed via the same outputTarget options (clipboard, commit-message, toast, panel). Tauri-only (PWA shows "requires desktop app")
- **LLM API config** (Settings > Agents): global provider/model/API key for all API-mode prompts. Supports OpenAI, Anthropic, Gemini, OpenRouter, Ollama, and any OpenAI-compatible endpoint via custom base URL. API key stored in OS keyring. Test button validates connection

### 10.6 Built-in Prompts by Category

| Category | Prompts |
|----------|---------|
| **Git & Commit** | Smart Commit, Commit & Push, Amend Commit, Generate Commit Message |
| **Code Review** | Review Changes, Review Staged, Review PR, Address Review Comments |
| **Pull Requests** | Create PR, Update PR Description, Generate PR Description |
| **Merge & Conflicts** | Resolve Conflicts, Merge Main Into Branch, Rebase on Main |
| **CI & Quality** | Fix CI Failures, Fix Lint Issues, Write Tests, Run & Fix Tests |
| **Investigation** | Investigate Issue, What Changed?, Summarize Branch, Explain Changes |
| **Code Operations** | Suggest Refactoring, Security Audit |

### 10.7 Context Variables

Variables are resolved from the Rust backend (`resolve_context_variables`) and frontend stores:

| Variable | Source | Description |
|----------|--------|-------------|
| `{branch}` | git | Current branch name |
| `{base_branch}` | git | Detected default branch (main/master/develop) |
| `{repo_name}` | git | Repository directory name |
| `{repo_path}` | git | Full filesystem path to the repository root |
| `{repo_owner}` | git | GitHub owner parsed from remote URL |
| `{repo_slug}` | git | Repository name parsed from remote URL |
| `{diff}` | git | Full working tree diff (truncated to 50KB) |
| `{staged_diff}` | git | Staged changes diff (truncated to 50KB) |
| `{changed_files}` | git | Short status output |
| `{dirty_files_count}` | git | Number of modified files (derived from changed_files) |
| `{commit_log}` | git | Last 20 commits (oneline) |
| `{last_commit}` | git | Last commit hash + message |
| `{conflict_files}` | git | Files with merge conflicts |
| `{stash_list}` | git | Stash entries |
| `{branch_status}` | git | Ahead/behind remote tracking branch |
| `{remote_url}` | git | Remote origin URL |
| `{current_user}` | git | Git config user.name |
| `{pr_number}` | GitHub store | PR number for current branch |
| `{pr_title}` | GitHub store | PR title |
| `{pr_url}` | GitHub store | PR URL |
| `{pr_state}` | GitHub store | PR state (OPEN, MERGED, CLOSED) |
| `{pr_author}` | GitHub store | PR author username |
| `{pr_labels}` | GitHub store | PR labels (comma-separated) |
| `{pr_additions}` | GitHub store | Lines added in PR |
| `{pr_deletions}` | GitHub store | Lines deleted in PR |
| `{pr_checks}` | GitHub store | CI check summary (passed/failed/pending) |
| `{merge_status}` | GitHub store | PR mergeable status |
| `{review_decision}` | GitHub store | PR review decision |
| `{agent_type}` | terminal store | Active agent type (claude, gemini, etc.) |
| `{cwd}` | terminal store | Active terminal working directory |
| `{issue_number}` | manual | Prompted from user at execution time |

### 10.8 Execution Modes

- **Inject** (default): writes the resolved prompt text into the active terminal's PTY. Checks agent idle state before sending (configurable via `requiresIdle`). Appends newline for auto-execution
- **Headless**: runs a one-shot subprocess via `execute_headless_prompt` Tauri command. Requires a per-agent headless template configured in Settings → Agents (e.g. `claude -p "{prompt}"`). Output routed to clipboard or toast depending on `outputTarget`. Falls back to inject in PWA mode. 5-minute timeout cap

### 10.9 UI Integration Points

| Location | Prompts shown | Trigger |
|----------|---------------|---------|
| **Toolbar dropdown** | All enabled prompts with `toolbar` placement | `Cmd+Shift+K` or lightning bolt button |
| **Git Panel — Changes tab** | SmartButtonStrip with `git-changes` placement | Inline buttons above changed files |
| **PR Detail Popover** | SmartButtonStrip with `pr-popover` placement | Inline buttons in PR detail view |
| **Command Palette** | All prompts with `Smart:` prefix | `Cmd+P` then type "Smart" |
| **Branch context menu** | Prompts with `git-branches` placement | Right-click branch in Branches tab |

### 10.10 Smart Prompts Management (Cmd+Shift+K Drawer)

- All prompt management consolidated in the Cmd+Shift+K drawer (Settings tab removed)
- Enable/disable individual prompts via toggle button on each row
- Edit prompt: opens modal with name, description, content, variable dropdown, placement, execution mode, auto-execute, keyboard shortcut
- Variable insertion dropdown below content textarea: grouped by Git/GitHub/Terminal, click to insert `{variable}` at cursor
- Create custom smart prompts with `+ New Prompt` button
- Built-in prompts show a "Reset to Default" button when content is overridden

### 10.11 Headless Template Configuration

- Settings → Agents → per-agent "Headless Command Template" field
- Template uses `{prompt}` placeholder for the resolved prompt text
- Example: `claude -p "{prompt}"`, `gemini -p "{prompt}"`
- Required for headless execution mode; without it, headless prompts fall back to inject

---

## 11. Settings

### 11.1 General
- Language, Default IDE, Shell
- Confirmations: quit, close tab (only when a process is running — agents or busy shell; idle shells close immediately)
- Power management: prevent sleep when busy
- Updates: auto-check, check now
- Git integration: auto-show PR popover
- Repository defaults: base branch, file handling, setup/run scripts, worktree defaults (storage strategy, prompt on create, etc.)

### 11.2 Appearance
- Terminal theme: multiple themes, color swatches
- Terminal font: 11 bundled monospace fonts (JetBrains Mono default)
- Default font size: 8-32px slider
- Split tab mode: separate / unified
- Max tab name length: 10-60 slider
- Repository groups: create, rename, delete, color-coded
- Reset panel sizes: restore sidebar and panel widths to defaults

### 11.3 Services
- HTTP API server: always active on IPC listener (Unix domain socket on macOS/Linux, named pipe `\\.\pipe\tuicommander-mcp` on Windows). TCP port only for remote access
- MCP connection info: bridge sidecar auto-installs configs for supported agents (Claude Code, Cursor, etc.)
- TUIC native tool toggles: enable/disable individual MCP tools (`session`, `agent`, `repo`, `ui`, `plugin_dev_guide`, `config`, `knowledge`, `debug`) to restrict what AI agents can access
- MCP Upstreams: add/edit/remove upstream MCP servers (HTTP or stdio with optional `cwd`), per-upstream enable/disable, reconnect, credential storage via OS keyring, live status dots, tool count and metrics. Saved upstreams auto-connect on boot
- MCP Per-Repo Scoping: each repo can define which upstream MCP servers are relevant via an allowlist in repo settings (3-layer: per-repo > `.tuic.json` > defaults). Null/empty allowlist = all servers. Quick toggle via **Cmd+Shift+M** popup
- Remote access: port, username, password (bcrypt hash), URL display, QR code, token duration, IPv6 dual-stack, LAN auth bypass
- Voice dictation: full setup (see section 9)

### 11.4 Repository Settings (per-repo)
- Display name
- Worktree tab: storage strategy, prompt on create, delete branch on remove, auto-archive, orphan cleanup, PR merge strategy, after-merge action (each overridable from global defaults)
- Scripts tab: setup script (post-worktree), run script (`Cmd+R`), archive script (pre-archive/delete hook)
- Repo-local config: `.tuic.json` in repo root provides team-shared settings. Three-tier precedence: `.tuic.json` > per-repo app settings > global defaults. **Scripts (setup, run, archive) are intentionally excluded from `.tuic.json` merging** — arbitrary script execution by a checked-in file poses a security risk; scripts are always sourced from the local per-repo app settings only

### 11.5 Notifications
- Master toggle, volume (0-100%)
- Per-event: question, error, completed, warning, info
- Test buttons per sound
- Reset to defaults

### 11.6 Keyboard Shortcuts
- Settings > Keyboard Shortcuts tab (`Cmd+,` to open Settings), also accessible from Help > Keyboard Shortcuts
- All app actions listed with their current keybinding
- Click the pencil icon to rebind — inline key recorder with pulsing accent border
- Conflict detection: warns when the new combo is already bound to another action, with option to replace
- Overridden shortcuts highlighted with accent color; per-shortcut reset icon to revert to default
- "Reset all to defaults" button at the bottom
- Custom bindings stored in `keybindings.json` in the platform config directory
- Auto-populated from `actionRegistry.ts` (`ACTION_META` map) — new actions appear automatically
- **Global Hotkey:** configurable OS-level shortcut to toggle window visibility from any application. Set in the "Global Hotkey" section at the top of the Keyboard Shortcuts tab. No default — user must configure. Toggle: hidden/minimized → show+focus, visible but unfocused → focus, focused → instant hide (no dock animation). Cmd and Ctrl are distinct modifiers. Uses `tauri-plugin-global-shortcut` (no Accessibility permission required on macOS). Hidden in browser/PWA mode.

### 11.7 Agents
- See **6.9 Agent Configuration** for full details
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
- `.tuic.json` — repo-root team config (read-only from app, highest precedence for overridable fields)
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
- `tuic-bridge` ships as a Tauri sidecar; auto-installs MCP configs on first launch for Claude Code, Cursor, Windsurf, VS Code, Zed, Amp, Gemini
- Local connections use Unix domain socket (`<config_dir>/mcp.sock`) on macOS/Linux or named pipe (`\\.\pipe\tuicommander-mcp`) on Windows; TCP port reserved for remote access only
- Unix socket lifecycle is crash-safe: RAII guard removes the socket file on `Drop`; bind retries 3× (×100 ms) removing any stale file before each attempt; liveness check uses a real `connect()` probe so a dead socket from a crashed run never blocks MCP tool loading

### 14.7 Cross-Repo Knowledge Base
- `knowledge` MCP tool: fan-out queries across all repos in a workspace group via mdkb upstream servers
- `setup`: auto-provisions `mdkb serve` (stdio) for each repo, persists to `mcp-upstreams.json`
- `search`: hybrid BM25 + semantic search across docs, code, symbols, and memory in all group repos
- `code_graph`: cross-repo call graph queries (calls, callers, impact analysis)
- `status`: indexing health for all mdkb instances
- Requires `mdkb` binary on PATH (installed separately)

### 14.8 macOS Dock Badge
- Badge count for attention-requiring notifications (questions, errors)

### 14.9 Tailscale HTTPS
- Auto-detects Tailscale daemon and FQDN via `tailscale status --json` (cross-platform)
- Provisions TLS certificates from Tailscale Local API (Unix socket on macOS/Linux, CLI on Windows)
- HTTP+HTTPS dual-protocol on same port via `axum-server-dual-protocol`
- Graceful fallback: HTTP-only when Tailscale unavailable or HTTPS not enabled
- QR code uses `https://` scheme with Tailscale FQDN when TLS active
- Background cert renewal every 24h with hot-reload via `RustlsConfig::reload_from_pem()`
- Session cookie gets `Secure` flag on TLS connections
- Settings panel shows Tailscale status with actionable guidance

---

## 15. Keyboard Shortcut Reference

### Terminal
| Shortcut | Action |
|----------|--------|
| `Cmd+T` | New terminal tab |
| `Cmd+W` | Close tab / close active split pane |
| `Cmd+Shift+T` | Reopen last closed tab |
| `Cmd+1`–`Cmd+9` | Switch to tab by number |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab |
| `Cmd+L` | Clear terminal |
| `Cmd+C` | Copy selection |
| `Cmd+V` | Paste to terminal |
| `Cmd+Home` | Scroll to top |
| `Cmd+End` | Scroll to bottom |
| `Shift+PageUp` | Scroll one page up |
| `Shift+PageDown` | Scroll one page down |
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
| `Cmd+Shift+Enter` | Maximize / restore active pane |

### Panels
| Shortcut | Action |
|----------|--------|
| `Cmd+[` | Toggle sidebar |
| `Cmd+Shift+D` | Toggle Git Panel |
| `Cmd+Shift+M` | Toggle markdown panel |
| `Cmd+Alt+N` | Toggle Ideas panel |
| `Cmd+E` | Toggle file browser |
| `Cmd+O` | Open file… (picker) |
| `Cmd+N` | New file… (picker for name + location) |
| `Cmd+P` | Command palette |
| `Cmd+Shift+P` | Toggle plan panel |
| `Cmd+,` | Open settings |
| `Cmd+?` | Toggle help panel |
| `Cmd+Shift+K` | Prompt library |
| `Cmd+J` | Task queue |
| `Cmd+Shift+E` | Error log |
| `Cmd+Shift+W` | Worktree manager |
| `Cmd+Shift+A` | Activity dashboard |
| `Cmd+Shift+M` | MCP servers popup (per-repo) |

### Git
| Shortcut | Action |
|----------|--------|
| `Cmd+B` | Quick branch switch (fuzzy search) |
| `Cmd+Shift+D` | Git Panel (opens on last active tab) |
| `Cmd+G` | Git Panel — Branches tab |

### Branches Panel (when panel is focused)
| Shortcut | Action |
|----------|--------|
| `↑` / `↓` | Navigate branches |
| `Enter` | Checkout selected branch |
| `n` | Create new branch |
| `d` | Delete branch |
| `R` | Rename branch (inline edit) |
| `M` | Merge selected into current |
| `r` | Rebase current onto selected |
| `P` | Push branch |
| `p` | Pull current branch |
| `f` | Fetch all remotes |

### File Browser (when focused)
| Shortcut | Action |
|----------|--------|
| `↑/↓` | Navigate files |
| `Enter` | Open file / enter directory |
| `Backspace` | Go to parent directory |
| `Cmd+C` | Copy file |
| `Cmd+X` | Cut file |
| `Cmd+V` | Paste file |
| `Cmd+Shift+F` | Open file browser and activate content search |

### Code Editor (when focused)
| Shortcut | Action |
|----------|--------|
| `Cmd+F` | Find |
| `Cmd+G` | Find next |
| `Cmd+Shift+G` | Find previous |
| `Cmd+H` | Find and replace |
| `Cmd+S` | Save file |

### Ideas Panel (when textarea focused)
| Shortcut | Action |
|----------|--------|
| `Enter` | Submit idea |
| `Shift+Enter` | Insert newline |
| `Cmd+V` / `Ctrl+V` | Paste image from clipboard |
| `Escape` | Cancel edit mode |

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
- Capability-gated access: `pty:write`, `ui:markdown`, `ui:sound`, `ui:panel`, `ui:ticker`, `ui:context-menu`, `ui:sidebar`, `ui:file-icons`, `net:http`, `credentials:read`, `invoke:read_file`, `invoke:list_markdown_files`, `fs:read`, `fs:list`, `fs:watch`, `fs:write`, `fs:rename`, `exec:cli`, `git:read`
- CLI execution API: sandboxed execution of whitelisted CLI binaries (`mdkb`) with timeout and size limits
- Filesystem API: sandboxed read, write, rename, list, tail-read, and watch operations restricted to `$HOME`
- HTTP API: outbound requests scoped to manifest-declared URL patterns (SSRF prevention)
- Credential API: cross-platform credential reading (macOS Keychain, Linux/Windows JSON file) with user consent
- Panel API: rich HTML panels in sandboxed iframes (`sandbox="allow-scripts"`) with structured message bridge (`onMessage`/`send`) and automatic CSS theme variable injection
- Shared ticker system: `setTicker`/`clearTicker` API with source labels, priority tiers (low <10, normal 10-99, urgent >=100), counter badge, click-to-cycle, right-click popover
- Agent-scoped plugins: `agentTypes` manifest field restricts output watchers and structured events to terminals running specific agents (e.g. `["claude"]`)
- Plugin manifest fields use camelCase (`minAppVersion`, `agentTypes`, `contentUri`) — matches Rust serde serialization

### 17.2 Plugin Management (Settings > Plugins)
- **Installed tab:** List all plugins with enable/disable toggle, logs viewer, uninstall button
- **Browse tab:** Discover plugins from the community registry with one-click install/update
- **Enable/Disable:** Persisted in `AppConfig.disabled_plugin_ids`
- **ZIP Installation:** Install from local `.zip` file or HTTPS URL
- **Folder Installation:** Install from a local folder (copies plugin directory into plugins dir)
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
- `tuic://open/<path>` — Open markdown file in tab (iframe SDK only, path validated against repos)
- `tuic://terminal?repo=<path>` — Open terminal in repo (iframe SDK only)

### 17.4.1 TUIC SDK (`window.tuic`)
- Injected automatically into every plugin iframe alongside base CSS and theme variables
- Feature detection: `if (window.tuic)` — `tuic.version` reports SDK version
- `tuic.open(path, {pinned?})` — Open markdown file in tab, optionally pinned
- `tuic.terminal(repoPath)` — Open terminal in repository
- `<a href="tuic://open/...">` and `<a href="tuic://terminal?repo=...">` links intercepted automatically
- `data-pinned` attribute on links sets pinned flag
- Security: paths validated against known repository list

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
- `wiz-stories-kanban` — Kanban board panel for file-based stories with drag-and-drop, filters, and work log timeline

## 18. Mobile Companion UI

Phone-optimized progressive web app for monitoring AI agents remotely. Separate SolidJS entry point (`src/mobile/`) served by the existing HTTP server at `/mobile`.

### 18.1 Architecture
- Separate Vite entry point (`mobile.html` + `src/mobile/index.tsx`)
- Shares transport layer, stores, and notification manager with desktop
- Server-side routing: `/mobile/*` → `mobile.html`, everything else → `index.html`
- Session state accumulator enriches `GET /sessions` with question/rate-limit/busy state
- SSE endpoint (`/events`) and WebSocket JSON framing for real-time updates

### 18.2 Sessions Screen
- Hero metrics header: active session count + awaiting input count with large tabular-nums display
- Elevated session cards with agent icon, status badge, project/branch, relative time
- Rich sub-rows per card: agent intent (crosshair icon) or last prompt (speech bubble), current task (gear icon) with inline progress bar, usage limit percentage
- Question state highlighted via inset gold box-shadow
- Pull-to-refresh spinner via touch events
- Loading skeletons during initial data fetch
- Empty state with instructional hint
- Tap card to open session detail

### 18.3 Session Detail Screen
- Live output via WebSocket with `format=log` (VT100-extracted clean lines, auto-scrolling, 500-line buffer)
- Semantic colorization: log lines are color-coded by type (info, warning, error, diff +/-, file paths) via `classifyLine()` utility
- Search/filter in output: text search bar filters visible log lines in real time
- Rich header: agent intent line (italic), current task line, progress bar, usage percentage (red above 80%)
- Error bar (red tint) when `last_error` is set
- Rate-limit bar (orange tint) with live countdown timer (`formatRetryCountdown`)
- Suggest follow-up chips: horizontal scrollable pills from `suggested_actions`, tap to send
- Slash menu overlay: frosted glass bottom sheet showing detected `/command` entries; tap to send `Ctrl-U` + command + Enter
- Quick-action chips: Yes, No, y, n, Enter, Ctrl-C
- **TerminalKeybar:** context-aware row of special key buttons above the main input. Shows Ctrl+C, Ctrl+D, Tab, Esc, Enter, arrow keys for terminal operations. When the agent is awaiting input, adds Yes/No quick-reply buttons. Consolidated from the former separate QuickActions component
- **CLI command widget:** agent-specific quick commands (e.g., `/compact`, `/status` for Claude Code) accessible via expandable button
- Text command input with 16px font (prevents iOS auto-zoom), `inputmode="text"`
- **Offline retry queue:** `write_pty` calls that fail due to network disconnection are queued and retried when connectivity resumes
- Back navigation to session list

### 18.4 Question Banner
- Persistent overlay when any session has `awaiting_input` state
- Shows agent name, truncated question, Yes/No quick-reply buttons
- Visible on all screens, between top bar and content
- Stacks multiple questions

### 18.5 Activity Feed
- Chronological event feed grouped by time (NOW, EARLIER, TODAY, OLDER)
- Reads from shared `activityStore`
- Throttled grouping: items snapshot every 10s to prevent constant reordering with multiple active sessions; new items/removals trigger immediate refresh
- Sticky section headers, tap to navigate to session

### 18.6 Session Management
- **Session kill:** swipe or long-press a session card to kill/close the PTY session
- **New session:** create a new PTY session from the sessions screen (optional shell/cwd selection)

### 18.7 Settings
- Connection status: connectivity indicator with real-time Connected/Disconnected state
- Server URL display
- Notification sound toggle (localStorage-persisted)
- Open Desktop UI link

### 18.8 PWA Support
- Web app manifest (`mobile-manifest.json`) with standalone display mode
- iOS Safari and Android Chrome Add to Home Screen support
- `apple-mobile-web-app-capable` meta tags
- PNG icons (192x192, 512x512) for PWA installability

### 18.8.1 Push Notifications
- Web Push from TUICommander directly to mobile PWA clients (no relay dependency)
- VAPID ES256 key generation on first enable, persisted in config
- Service worker (`sw.js`) handles push events and notification clicks
- `PushManager.subscribe()` flow with user gesture (click handler) for iOS/Firefox
- Push subscriptions stored in `push_subscriptions.json`, survive restarts
- API endpoints: `POST/DELETE /api/push/subscribe`, `GET /api/push/vapid-key`, `POST /api/push/test`
- Triggers: agent `awaiting_input` (question, orange dot) and `PtyExit` (session completed, purple/unseen dot)
- Deep link: notification click navigates to `/mobile/session/<id>`, opening the specific session detail
- Delivery gate: push is sent whenever the desktop window is **not** focused (minimized, hidden, or on another workspace). This prevents duplicate alerts while the user is at the desktop and still wakes the PWA service worker when the phone is locked
- Rate limited: max 1 push per session per 30 seconds
- Stale subscriptions cleaned on HTTP 410 Gone
- iOS standalone detection: shows "Add to Home Screen" guidance when not installed
- HTTP detection: shows "Push requires HTTPS (enable Tailscale)" when not on HTTPS

### 18.9 Notification Sounds
- Audio playback via Rust `rodio` crate (Tauri command `play_notification_sound`), replacing the previous Web Audio API approach
- Eliminates AudioContext suspend issues on WebKit and works in headless/remote modes
- State transition detection: question, rate-limit, error, completion
- Completion notifications deferred 10s and suppressed when active sub-tasks are running (detected via `⏵⏵`/`››` mode-line prefix)

### 18.10 Visual Polish
- Frosted glass bottom tabs: `backdrop-filter: blur(20px) saturate(1.8)` with semi-transparent background
- Elevated card design: `border-radius: var(--radius-xl)`, `background: var(--bg-secondary)`, margin spacing
- Safe-area-inset padding for notched devices
- `font-variant-emoji: text` on output view — forces Unicode symbols (●, ○, ◉) to render as monochrome text glyphs instead of colorful emoji

### 18.11 Standalone CSS
- Mobile PWA uses its own standalone stylesheet (`src/mobile/mobile.css`), independent from the desktop `global.css`
- Shares core color palette and border radius tokens; differs in font stacks, layout approach, and iOS-specific rules
- WebSocket state deduplication: duplicate state pushes are filtered to reduce unnecessary re-renders

---

## 19. MCP Proxy Hub

TUICommander aggregates upstream MCP servers and exposes them through its own `/mcp` endpoint. Any MCP client (Claude Code, Cursor, VS Code) connecting to TUIC automatically gains access to all configured upstream tools.

### 19.1 Architecture
- TUIC acts as both an MCP server (to downstream clients) and an MCP client (to upstream servers)
- All upstream tools are exposed via the single `POST /mcp` Streamable HTTP endpoint
- Native TUIC tools (`session`, `git`, `agent`, `config`, `workspace`, `notify`, `plugin_dev_guide`) coexist with upstream tools
- Tool routing: names containing `__` are routed to the upstream registry; all others handled natively

### 19.2 Tool Namespace
- Upstream tools are prefixed: `{upstream_name}__{tool_name}`
- Double underscore (`__`) is the routing discriminator — native tool names never contain it
- Tool descriptions are annotated with `[via {upstream_name}]` to identify origin
- Clients always see the merged tool list in a single `tools/list` response

### 19.3 Supported Transports
- **HTTP (Streamable HTTP, spec 2025-03-26)** — connects to any MCP server with an HTTP endpoint
- **Stdio** — spawns local processes (npm packages, Python scripts, etc.) communicating via newline-delimited JSON-RPC

### 19.4 Circuit Breaker (per upstream)
- 3 consecutive failures → circuit opens
- Backoff: 1s → exponential growth → 60s cap
- After 10 retry cycles without recovery → permanent `Failed` state
- Recovery: successful tool call or health check resets the circuit breaker

### 19.5 Health Checks
- Background task probes every `Ready` upstream every 60 seconds via `tools/list` (HTTP) or process liveness check (stdio)
- `CircuitOpen` upstreams with expired backoff are also probed for recovery

### 19.6 Tool Filtering (per upstream)
- Allow list: only matching tools are exposed
- Deny list: all tools except matching ones are exposed
- Pattern syntax: exact match or trailing-`*` prefix glob

### 19.6.1 Per-Repo Scoping
- Each repository can define an allowlist of upstream server names in `RepoSettings.mcpUpstreams`
- 3-layer merge: per-repo user settings > `.tuic.json` (team-shareable) > defaults (null = all servers)
- Quick toggle via **Cmd+Shift+M** popup: shows all upstream servers with status, transport, tool count, and per-repo checkboxes
- Toggling a checkbox immediately persists to repo settings (reactive, no refresh needed)

### 19.7 Hot-Reload
- Adding, removing, or changing upstreams takes effect on save without restarting TUIC or AI clients
- Config diff computed by stable `id` field; only changed entries are reconnected

### 19.8 Credential Management
- Bearer tokens stored in OS keyring (Keychain / Credential Manager / Secret Service)
- Config file (`mcp-upstreams.json`) never contains secrets
- Per-upstream credential lookup at call time

### 19.9 Environment Sanitization (stdio)
- Parent environment is cleared before spawning to prevent credential leakage
- Safe allowlist re-applied: `PATH, HOME, USER, LANG, LC_ALL, TMPDIR, TEMP, TMP, SHELL, TERM`
- User-configured `env` overrides applied on top

### 19.10 SSE Events
- `upstream_status_changed` events emitted on status transitions (connecting, ready, circuit_open, disabled, failed)
- `tools/list_changed` notification emitted when upstream tool lists change, enabling live tool-list updates for connected MCP clients
- Delivered via `GET /events` SSE stream

### 19.11 Metrics (per upstream, lock-free)
- `call_count` — total tool calls routed
- `error_count` — total failed calls
- `last_latency_ms` — last observed round-trip time

### 19.12 Validation
- Names: must match `[a-z0-9_-]+`, must be unique
- HTTP URLs: must use `http://` or `https://` scheme only
- Self-referential URL detection: rejects URLs pointing to TUIC's own MCP port
- Stdio: command must be non-empty
- All errors collected (not just first) and returned to caller
- Respects sound toggle from Settings screen

## 20. Performance

### 20.1 PTY Write Coalescing
- Terminal writes accumulated per animation frame via `requestAnimationFrame` (~60 flushes/sec)
- High-throughput agent output (hundreds of events/sec) batched into single `terminal.write()` calls
- Reduces xterm.js render passes and WebGL texture uploads during burst output
- Flow control (pause/resume at HIGH_WATERMARK) unchanged

### 20.2 Async Git Commands
- All ~25 Tauri git commands run inside `tokio::task::spawn_blocking`
- Prevents git subprocess calls from blocking Tokio worker threads
- `get_changed_files` merged from 2 sequential subprocesses to 1

### 20.3 Watcher-Driven Git Cache
- Unified `repo_watcher` (FSEvents/inotify) monitors entire working tree with per-category debounce
- CategoryEmitter routes events to Git, WorkTree, or Config handlers with trailing debounce
- `.gitignore`-aware filtering prevents unnecessary cache invalidations
- Cache hit ~0.2ms vs git subprocess ~20-30ms
- 60s TTL as safety net for missed watcher events

### 20.4 Process Name via Syscall
- `proc_pidpath` (macOS) / `/proc/pid/comm` (Linux) replaces `ps` fork
- Eliminates ~100 fork+exec/min with 5 terminals open

### 20.5 MCP Concurrent Tool Calls
- `HttpMcpClient` uses `RwLock` instead of `Mutex`
- Tool calls use read lock (concurrent); only reconnect takes write lock

### 20.6 Serialization
- PTY parsed events serialized once with `serde_json::to_value`
- Reused for both Tauri IPC emit and event bus broadcast (was serialized twice)

### 20.7 Frontend Bundle Splitting
- Vite `manualChunks`: xterm, codemirror, diff-view, markdown as separate chunks
- SettingsPanel, ActivityDashboard, HelpPanel lazy-loaded with `lazy()` + `Suspense`
- PTY read buffer increased from 4KB to 64KB for natural batching

### 20.8 Conditional Timers
- StatusBar 1s timer only active when merged PR countdown or rate limit is displayed
- ActivityDashboard snapshot signal uses default equality check (no forced re-render every 10s)

### 20.9 Profiling Infrastructure
- Scripts in `scripts/perf/`: IPC latency, PTY throughput, CPU recording, Tokio console, memory snapshots
- `tokio-console` feature flag for async task inspection
- See `docs/guides/profiling.md`
