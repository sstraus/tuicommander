# TUICommander — Complete Feature Reference

Published story workflows can start in Plans and Stories through the owning daemon. Run history renders graph positions, decisions, evidence and paged event payloads, with explicit graph recovery and cancellation. IPC/HTTP and the generated `workflow_run` MCP schema share the native run service. Plan start remains visibly unavailable pending plan dispatch.

> Canonical capability inventory. Update this file when adding, changing, or removing user-visible features.
> See [AGENTS.md](../AGENTS.md) for the maintenance requirement.
>
> This document intentionally serves two audiences: users need a searchable overview of what exists, while LLMs and contributors need stable names, shortcuts, settings, and implementation anchors. It is an inventory, not a replacement for the chronological [CHANGELOG](../CHANGELOG.md).

**Current version:** 1.7.7  
**Last verified:** 2026-09-16  
**Recent feature delta:** See the [Unreleased](https://github.com/sstraus/tuicommander/blob/main/CHANGELOG.md#unreleased) and [1.7.7](https://github.com/sstraus/tuicommander/blob/main/CHANGELOG.md#177---2026-09-16) changelog sections for what changed recently. Keep this page focused on the current state; do not duplicate the full changelog here.

Desktop AI Chat uses the terminal composer’s pin and blue play buttons. The pin highlights a parked draft; tooltips identify Park, Restore, Swap, Send and Queue.

AI Chat image paste discovers capabilities on first use and shares clipboard precedence with Ideas and terminal Compose: Finder image filenames allow attachments, while substantive text takes priority.

## ego Perimeter Settings

- Settings > AI Chat edits ego roots, root access, read allowlists, writable
  directories and network mode through the configured CLI.
- Preview comes from ego's effective view for the AI Chat workspace/profile.
  The exec-sandbox badge reports unverified enforcement when ego says `not_checked`
  and renders measured capability arrays without inferring enforcement from backend names.

## How to use this reference

- **Users:** start with the relevant section, then follow its user-guide link.
- **LLMs and contributors:** search for the feature name, shortcut, setting, command, or source anchor. The bullets describe current behavior, including important limitations.
- **Release work:** update the relevant section when behavior changes, and add the chronological explanation to `CHANGELOG.md`.

## User-guide map

| Feature area | User guide |
|---|---|
| Terminal, tabs, splits, and search | [Terminal Features](user-guide/terminals.md) |
| Sidebar, repositories, and branches | [Sidebar](user-guide/sidebar.md) · [Branch Management](user-guide/branches.md) |
| Git worktrees | [Worktrees](user-guide/worktrees.md) |
| AI agents and agent teams | [AI Agents](user-guide/ai-agents.md) · [Agent Teams](user-guide/agent-teams.md) |
| Visual element inspection | [Design Mode](user-guide/design-mode.md) |
| GitHub, PRs, and CI | [GitHub Integration](user-guide/github-integration.md) |
| Smart Prompts and Prompt Library | [Smart Prompts](user-guide/smart-prompts.md) · [Prompt Library](user-guide/prompt-library.md) |
| Settings and shortcuts | [Settings](user-guide/settings.md) · [Keyboard Shortcuts](user-guide/keyboard-shortcuts.md) |
| Plugins and MCP | [Plugins](user-guide/plugins.md) · [MCP Proxy Hub](user-guide/mcp-proxy.md) |
| Remote, mobile, and browser modes | [TUICommander Modes](user-guide/modes.md) · [Remote Access](user-guide/remote-access.md) |
| Setup and recovery | [Getting Started](user-guide/getting-started.md) · [Troubleshooting](user-guide/troubleshooting.md) |
| Project history | [Project Progress](user-guide/project-progress.md) |
| Native plans and stories | [Native plans and stories](user-guide/native-stories.md) |

 Native plans and stories are available through `tuic story`, the `story` MCP tool, desktop IPC, and authenticated HTTP. They support criteria, dependencies, revision-checked transitions, and live-tab claims. WontFix closes cancelled scope but never satisfies a prerequisite; a human can remove a cancelled dependency from a Backlog story. In manual plans, approving a prerequisite immediately releases eligible dependents. Plans with a workflow run require current integration receipts before release. Story import/export is outside this release.

The Plans and Stories dialog provides a manual project view on desktop and in the browser: choose a top-level Markdown plan from the project, add a path or link when needed, create stories, inspect criteria and dependency status, identify transitively abandoned prerequisites, remove a direct cancelled prerequisite, start work without a terminal, check criteria, submit reviews, approve, and block outcomes. It renders the service's cancellation count and all-cancelled flag. A coordinator or independent reviewer may approve after checking the acceptance criteria; the session that claimed a story cannot approve it. Approval records the acting session or local API provenance.

Versioned workflow definitions are available through IPC and HTTP. The Plans and Stories Designer tab can select a seeded draft, add nodes by dragging or clicking, connect outcomes, edit node settings, save, and publish. Published revisions are immutable and validated before use. The two built-in templates are `Story delivery` and `Resolve plan`; automatic graph execution is still under development.

Durable workflow run storage records event history, pinned graph revisions, node attempts, external effect intents, TUIC-run check receipts, and checked integration receipts. Desktop and HTTP clients can start and inspect a run, list a plan's runs, replay events, and send explicit run commands. The Plans and Stories dialog shows a run timeline with paged events. A managed MCP caller can explicitly launch a pinned attempt in a registered isolated worktree, create a plan story with an idempotent proposal key, and submit a typed outcome bound to the spawned session. Story attempts with overlapping or unknown file scope are serialized, and story worktree assignments are recorded before spawn. Dependents remain held until the accepted prerequisite has a current canonical integration receipt; an operator performs the merge explicitly in this slice. An unrelated canonical commit requires a fresh TUIC-computed recertification with pinned checks before release resumes. Reviewer reports can retain criterion-indexed findings and an artifact digest as advisory evidence. A worker input request pauses its run until an operator records an answer and resumes it; valid reports from other running workers remain durable during that pause. The daemon now owns run recovery, serial graph positioning, explicit graph resume, cancellation and idle duration timers under a database owner lock. Serial Agent effects, independent story policy and plan dispatch execute in the owning daemon; public story graph starts and history controls are available. Public API plan starts use slice E dispatch; the plan UI control remains visibly unavailable. E2E evaluation remains pending.

---

## 1. Terminal Management

Terminal prompt colours apply to Chat user messages only. The CLI grid retains prompt scrollbar ticks and answers-only prompt grouping without tinting prompt rows.

- **Tablet keyboard input** — Touch taps and mouse presses focus the same terminal input. Soft-keyboard text and deletion use the shared input handler. Primary presses suppress the browser default canvas focus change.

Terminal text retains stored combining marks through rendering, scrolling,
copy and exact-codepoint search, including marks arriving in a later output
chunk. Search and link positions account for the difference between grid cells
and Unicode string offsets. The existing bound of nine zero-width characters
per cell and the configured history limit still apply.

### 1.0 Design Mode for agent terminals

- Start from an agent terminal's tab context menu or the Command Palette (the palette acts on the active terminal only). While the repository is armed, both offer Stop instead. TUICommander opens a separate Chrome window for the repository's configured development-server URL, or `about:blank` when none is configured.
- Chrome highlights hovered elements. A click collects the selected element's selector, paths, nearby text, HTML snippet, selected styles, the design tokens (CSS custom properties) its matching rules reference with their resolved values, rectangle, source location when development metadata permits, and an optional PNG reference.
- The grab is appended to the bound agent's draft without submitting it; repeated grabs and the user's existing text remain editable. Starting from another agent terminal in the same repository rebinds the Chrome window.
- The tab indicator follows Design Mode status. Browser/PWA clients can request inspection, but Chrome opens on the host running TUICommander. See the [user guide](user-guide/design-mode.md).

### 1.1 PTY Sessions
- Up to 50 concurrent PTY sessions (configurable in Rust `MAX_SESSIONS`)
- Each tab runs an independent pseudo-terminal with the user's shell
- Terminals are never unmounted — hidden tabs stay alive with full scroll history
- Session persistence across app restarts (lazy restore on branch click); only agent tabs are restored — plain shell tabs are discarded and a fresh terminal is spawned instead
- Orchestrated PTYs show a task description above the terminal alongside the last submitted user prompt; MCP callers can supply `pty_description`, while spawn-only orchestration schemas fall back to a compact summary of the task prompt without changing agent launch or prompt-delivery behavior
- Managed-agent automation submits a command with one MCP `session action=submit` call: an idle empty composer is claimed atomically, raw-mode text/Enter framing cannot interleave with another writer, and the same response reports terminal acknowledgement or a precise non-retryable timeout. The action never queues or overwrites a draft; `session action=input` remains write-only compatibility, with a text-to-Enter gap for detected agent sessions
- Agent session restore shows a clickable banner ("Agent session was active — click to resume") instead of auto-injecting the resume command; Space/Enter resumes, other keys dismiss
- Foreground process detection (macOS: `libproc`, Windows: `CreateToolhelp32Snapshot`)
- PTY environment: `TERM=xterm-256color`, `COLORTERM=truecolor`, `LANG=en_US.UTF-8`. A parent `NO_COLOR` is stripped (`sanitize_pty_parent_env`) so a TUICommander launched from Codex does not leak that opt-out into independent sessions; per-command flags and per-agent environment can still request monochrome deliberately
- Pause/resume PTY output (`pause_pty` / `resume_pty` Tauri commands) — suspends reader thread without killing the session

- **Remote replay health** — Stream failure, unreadable frames and initial replay stalls show a persistent error toast. Reconnect success requires a delivered frame; healthy idle terminals have no output-silence deadline.

- **CLI / Chat view** — Claude terminals switch between the grid and a conversation view with docked Compose input, built from the agent's session file (including older prompts, human text/image follow-ups queued while Claude is busy, replies, folded tool cards, image/PDF markers and model-change cards). The grid is hidden, never unmounted.
- **Chat transcript presentation** — Harness notices use compact system notes, image placeholders use attachment chips, adjacent thinking blocks share a disclosure, and TUIC answers retain the CLI highlight. Historical tool cards show status without fabricated execution timing.

### 1.2 Tab Bar
- Create: `Cmd+T`, `+` button (click = new tab, right-click or long press = agent list)
- Close: `Cmd+W`, middle-click, context menu
- Reopen last closed: `Cmd+Shift+T` (remembers last 10 closed tabs)
- Switch: `Cmd+1` through `Cmd+9`, `Ctrl+Tab` / `Ctrl+Shift+Tab`
- Rename: double-click tab name (inline editing)
- Reorder: drag-and-drop with visual drop indicators (works for all tab types: terminal, diff, editor, markdown, plugin panels)
- Tab status dot (left of name): grey=idle, blue-pulse=busy, green=done, purple=unseen (completed while not viewed), orange-pulse=question (needs input), red-pulse=error
- Tab type colors: red gradient=diff, blue gradient=editor, teal gradient=markdown, purple gradient=panel, amber gradient=remote PTY session
- Remote PTY sessions (created via HTTP/MCP) show "PTY:" prefix and amber styling
- Progress bar (OSC 9;4)
- Context menu (right-click): Close Tab, Close Other Tabs, Close Tabs to the Right, Detach to Window, Copy Path
  - Copy Path appears on every file-backed tab — diff, editor, markdown, HTML preview, and a plugin panel opened from a `file://` url — and copies the ABSOLUTE path with `$HOME` shortened to `~`. The tab stores keep `filePath` relative to the tab's filesystem root, so the root is joined back on before copying
  - "Open in Browser" on a `file://` plugin panel hands the file to the OS default application, not to the URL allowlist (which permits only http/https/mailto, because the URLs it was built for come off a PTY)
- **Context menu shortcut chords** — while a context menu is open, pressing a menu item's keyboard shortcut chord (modifier + key, or Enter) fires that action directly without needing to click. Modifier-only keystrokes (Cmd, Shift, etc.) do not close the menu so multi-key chords can form; any other non-matching key closes the menu normally
- Detach to Window: right-click a tab to open it in a floating OS window
  - PTY session stays alive in Rust — floating window reconnects to the same session
  - Closing the floating window automatically returns the tab to the main window
  - Requires an active PTY session (disabled for tabs without a session)
- Overflow menu on scroll arrows (right-click) shows clipped tabs; the `+` button always stays visible regardless of scroll position
- `+` button: click opens a terminal in the active branch; right-click or long press (500 ms) lists the enabled agents (with a submenu per run config) and opens a new tab running the chosen one
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
- Selection follows retained text during output and history eviction; evicted
  endpoints are cleared instead of selecting replacement rows. Copy requests
  rebase against their displayed history snapshot under the backend grid lock.
- Paste to terminal: `Cmd+V`
- **Trailing whitespace trimmed** — All copy paths (Cmd+C, Ctrl+C, copy-on-select) strip trailing spaces from terminal rows
- **Claude gutter normalization** — Multi-line terminal selections remove Claude's repeated non-breaking-space plus `▎` visual margin while preserving isolated block characters and the content's indentation
- **Claude prompt copy** — Selections remove the composer prompt marker and continuation margin, rejoin width-supported wraps, and preserve typed breaks and pasted glyphs. Composer cleanup requires a column-zero origin outside VT soft-wrap continuations; partial body selections remain literal.
- **Copy on Select** — When enabled (Settings > Terminal > Copy on select), selecting text in the terminal automatically copies it to the clipboard. A brief "Copied to clipboard" confirmation appears in the status bar.
- **Copy feedback (Cmd+C)** — Copying via Cmd+C shows "Copied to clipboard" in the status bar, consistent with copy-on-select and Ctrl+C paths.
- **OSC 52 clipboard writes** — Terminal programs (tmux, vim, ssh yank) can set the system clipboard via the OSC 52 escape sequence. Because any displayed file/log can also emit it, each write surfaces a non-blocking "Clipboard updated by &lt;session&gt;" notice, and the behavior can be disabled entirely via Settings > Terminal > "Allow OSC 52 clipboard writes". Suggestion chips (OSC 7770 `suggest=`) carrying shell metacharacters are inserted without auto-Enter so a click cannot silently execute a spoofed command.

### 1.6 Clear Terminal
- `Cmd+L` — clears display, running processes unaffected

### 1.7 Clickable File Paths
- File paths in terminal output are auto-detected and become clickable links
- Terminal directory links open the File Browser at the resolved directory; missing paths show a notification without opening a tab
- Existing absolute paths outside registered repositories, including files under hidden directories, open in the native Markdown viewer or editor from terminal links
- Paths validated against filesystem before activation (Rust `resolve_terminal_path`)
- `.md`/`.mdx` → opens in Markdown panel; preview-capable files (HTML, PDF, images, video, audio, plain text/data) → open in the Preview tab (section 3.15); all other code files → open in the built-in code editor
- `file://` URLs are recognized in addition to plain paths — the prefix is stripped and the path resolved like any other
- OSC 8 hyperlinks: programs that emit hyperlink escape sequences (e.g. Claude Code, modern `ls`) produce clickable links; hover underline spans the full link text (via `terminal_hyperlink_span` backend API)
- Supports `:line` and `:line:col` suffixes; the built-in editor moves to that position even when the file is already open
- Single left-click opens the link instantly (UI-first — opening is a primary action, not gated behind a modifier); drag-select over a link still copies text without opening
- Right-click on a link shows a context menu with **Open** and **Copy link** (copy the resolved path/URL without opening). Right-clicking elsewhere shows the standard terminal context menu
- Recognized extensions: rs, ts, tsx, js, jsx, py, go, java, kt, swift, c, cpp, cs, rb, php, lua, zig, css, scss, html, vue, svelte, json, yaml, toml, sql, graphql, tf, sh, dockerfile, and more

### 1.8 Find in Content
- `Cmd+F` opens search overlay — context-aware: routes to terminal, markdown tab, or diff tab based on active view
- **Terminal:** CLI incremental search with highlight decorations; Chat searches the visible transcript across inline Markdown, skips collapsed thinking bodies, and selects matches with next/previous navigation. Switching views closes search and clears highlights.
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

MCP worktree creation without an explicit session spawn places only its
same-repository caller in the new workspace. Agents working through `git -C`
can declare an existing worktree with `session action=declare_worktree
worktree_path=/absolute/path`. The backend validates ownership, moves only the
caller and persists the association through restart. Shell cwd and other tabs
remain unchanged. See [MCP backend](backend/mcp-http.md).

- Right-click a terminal tab → "Move to Worktree" submenu lists available worktrees (excluding the current one)
- Selecting a worktree sends `cd` to the PTY; OSC 7 auto-reassigns the terminal to the target branch
- Also available via Command Palette: dynamic "Move to worktree: \<branch\>" entries appear when the active terminal belongs to a repo with multiple worktrees
- Only shown when the repo has more than one worktree
- When a worktree is created while an agent is running, **Open Worktree** opens or focuses a terminal rooted there; the agent terminal remains attached to its original branch and working directory

### 1.11 OSC 7 CWD Tracking
- Terminals report their current working directory via OSC 7 escape sequences
- Parsed in the Rust backend from PTY output and stored per-session as `session_cwd`
- When a terminal's CWD falls inside a known worktree path, the session is automatically reassigned to the correct branch in the sidebar
- Sessions spawned into a new sibling worktree refresh their registered repository before final tab placement
- Enables accurate branch association even when the user `cd`s into a different worktree from a single terminal

### 1.12 Kitty Keyboard Protocol
- Supports Kitty keyboard protocol flag 1 (disambiguate escape codes)
- Per-session flag tracking via `get_kitty_flags` Tauri command
- Enables correct handling of `Shift+Enter` (multi-line input), `Ctrl+Backspace`, and modifier key combinations in agents that request the protocol (e.g. Claude Code)

### 1.13 File Drag & Drop
- Drag files from Finder/Explorer onto the terminal area or any panel
- Uses Tauri's native `onDragDropEvent` API (not HTML5 File API — Tauri webviews do not expose file paths via HTML5)
- **Active PTY session:** dropped file paths are forwarded directly to the terminal as text (enables Claude Code image drops and similar workflows)
- **No active PTY session:** `.md`/`.mdx` files open in Markdown viewer, preview-capable files open in the Preview tab (section 3.15), all other files open in Code Editor
- Multiple files can be dropped at once
- Visual overlay with dashed border appears during drag hover
- Global `dragover`/`drop` `preventDefault` prevents the Tauri webview from treating drops as browser navigation (which would replace the UI with a white screen)
- macOS file association: `.md`/`.mdx` files registered with TUICommander — double-click in Finder opens them directly
- **Drag to external apps**: Drag files from the File Browser to external applications (Finder, email clients, etc.) using native OS-level drag via `tauri-plugin-drag`. Works alongside internal drag & drop (tab reorder, split panes)

### 1.14 Cross-Terminal Search
- Type `~` in the command palette (`Cmd+P`) to search text across all open terminal buffers
- Results show terminal name, line number, and highlighted match text
- Selecting a result switches to the correct terminal tab/pane and scrolls to the matched line (centered in viewport)
- Minimum 3 characters after prefix
- Also accessible via the explicit "Search Terminals" command in the palette

### 1.15 Refresh Terminal (`Cmd+Shift+L`)
- Rebuilds the terminal renderer to fix corrupted glyphs (WebGL atlas issues, font rendering artifacts)
- Does not clear content or affect the PTY session — purely a visual refresh
- Action name: `refresh-terminal`

### 1.16 Terminal Bell
- **Terminal Bell** — Configurable bell behavior when the terminal receives a BEL character (`\x07`). Four modes: `none` (silent), `visual` (screen flash animation), `sound` (plays the Info notification sound), `both` (flash + sound). Set `bell_style` in `config.json`; Settings has no control for it.

### 1.17 Alternate-Screen Scrollback
- Fullscreen apps (`gh run watch`, `less`, `man`, TUIs) run on the terminal's alternate screen, which per XTerm semantics has no scrollback — output past the bottom of the window is normally lost and no scrollbar is shown
- TUICommander keeps those lines with user-visible behavior equivalent to iTerm2's "save lines to scrollback in alternate screen mode" option: scrollbar, wheel, and scrollbar drag all work while the app is running
- Output is byte-faithful: only lines that genuinely scroll off the top are kept. An in-place redraw produces no history, while a refresh taller than the viewport (as emitted by `gh run watch`) keeps each overflowing snapshot, including repetitions
- The alternate grid has its own bounded, ephemeral history. It is wiped on every enter/exit and never mixes with the shell's history
- Entering or leaving the alternate screen atomically invalidates scroll, selection, search, link, and row-cache state before the new grid is painted
- Apps with mouse reporting (`vim`, `htop`, `lazygit`, `grok --no-alt-screen`) still receive the wheel themselves — `Shift+wheel` or a scrollbar drag scrolls TUICommander's history
- An inline TUI that enables mouse reporting *without* `1049h` (`grok --no-alt-screen`) is classified as FullscreenTui and excluded from the durable log the same way alt-screen is. Grid history stays so the scrollbar works.

### 1.18 Scrollback History Overlay — removed
The experimental read-only scrollback overlay (`AltScreenHistory`) and its `scrollHistoryEnabled` flag were removed in `86471444`. The terminal's own scrollback and search replace it.

### 1.19 Command Blocks

Terminal output is segmented into command blocks — one per prompt+output cycle. Blocks are detected via OSC 133 shell integration markers (A/C/D sequences) or OSC 7770;block= agent-emitted markers. For Claude Code, heuristic detection synthesizes blocks from tool call headers (`⏺ ToolName(args)`).

- **Scrollbar marks** — Color-coded indicators on the scrollbar for each command block boundary. Provides a visual map of command history at a glance. Toggled by **Show scrollbar marks** in Settings > Terminal (`show_scrollbar_marks`, on by default). The flag covers the history markers — these ticks and the user-prompt ticks below — and deliberately **not** the search-match ticks, which stay visible so a search never silently draws nothing
- **User-prompt scrollbar markers** — A distinct green tick on the scrollbar marks each line where the user submitted a prompt to the agent (recorded from the OSC 7770 `state=prompt` the submit-prompt hook emits; tool-call `state=busy` carries no row via `userPromptLines`). These are separate from command-block boundary marks and help you quickly locate your own prompts in long sessions
- **Timestamp overlay** — Hold `Ctrl+Cmd` to reveal timestamps showing when each block started, displayed as relative time (e.g. "2m ago")
- **Gutter click** — Click the gutter area to select the entire block output for easy copying
- **Block folding** — Collapse/expand block output with `Cmd+Shift+.` toggle. Folded blocks show a summary line. Backend stores fold state per session via `set_block_fold` Tauri command
- **Block-scoped search** — Toggle with `Cmd+Shift+B` to restrict terminal search to the current block only
- **Block navigation** — `Cmd+Shift+Up/Down` jumps between block boundaries
- **Block cap** — Sessions are capped at 500 command blocks; oldest blocks are evicted when the cap is reached
- **Settings** — Configure block features at Settings > Terminal > Terminal: show/hide timestamps, enable/disable folding

### 1.20 Compose Panel (`Cmd+I`)

A multi-line editor docked under the terminal for writing a prompt without fighting the agent's own input box.

- **Chat input** — Chat opens and focuses Compose automatically below the conversation. It stays docked after send or queue and clears submitted text. Chat hides the close and pin controls; `Esc` returns to CLI, where permission prompts must be answered. Returning to CLI restores the tab's previous open/pin state, and unsent drafts survive both directions, including while send or queue is pending
- **Send now** — `Ctrl+Enter` (or the ▶ button) types the text into the composer and submits it immediately, steering whatever the agent is doing
- **Queue for the next idle window** — `Shift+Ctrl+Enter` (or the ☰ button) hands the text to the backend's idle gate instead: it is submitted at once if the agent is already idle, otherwise parked until the agent's next busy→idle transition. This is the way to leave follow-up work for an agent mid-turn without interrupting it
- **Queue badge** — the status bar shows `N queued` while commands are waiting; clicking it discards the whole queue. The count comes from the backend (`state.queued_commands`), so it is accurate across reloads and remote clients
- **Order** — queued commands are typed one per idle window, in the order they were composed; a new one never overtakes one already waiting. Hands-free speech does not use this queue at all — see 9.9
- **Agents only** — queueing is hidden for a plain shell: its idle state says nothing about which program currently owns stdin
- **Pin** — the pin button docks the panel under the terminal instead of over it: the terminal shrinks and the PTY resizes, so no output hides behind the panel. A pinned panel stays open after a send or a queue and removes the sent text, so it can replace the agent's own input box. Text typed while the send runs is kept, and a second send before the first finishes is ignored. When the terminal has no session or the send fails, a toast shows the error and the text stays. `Esc` and `Cmd+I` move the caret between the panel and the terminal instead of closing it. Per tab, not persisted
- **Close button** — the ✕ in the status bar closes the panel and releases the pin; `Esc` closes an unpinned panel

### 1.21 Auto-Standby (Unix)

Idle, unfocused terminals are suspended to stop them consuming CPU and battery. A background checker (every 30s) sends `SIGSTOP` to the entire process group of a session — `kill(-pgid, …)`, so children (dev servers, agent processes) are paused too, not just the shell.

- **Entry conditions (all required)** — timeout enabled (`> 0`), tab not focused, shell state idle, no tracked agent background work, idle for at least the timeout, session startup settled, and not already in standby. Claude/Codex/Gemini/Aider/Grok/pi/OpenCode additionally require confirmed idle (explicit lifecycle marker or stable ready screen); silence-only idle cannot suspend them. Grok distinguishes its active Braille-spinner status row from the persistent `❯` composer.
- **Wake** — `SIGCONT` fires the instant the tab is focused or a message arrives for the agent; the process resumes exactly where it stopped (no session loss, no restart)
- **Safety** — the process-group id is validated before signalling; an unsafe pgid is refused rather than risking a stop sent to the wrong group
- **Pause badge** — suspended tabs show a pause indicator in the tab bar
- **Event** — `session-standby` (`{ session_id, standby }`) emitted on stop/wake
- **Settings** — Settings > General > Power Management > Auto-Standby Timeout (default 5 min; `0` disables)
- **Not Suspend** — standby only `SIGSTOP`s the process group and keeps its memory; [Suspend Tab](#121a-suspend-tab) ends the process

### 1.21a Suspend Tab

A terminal tab can be suspended from its context menu (**Suspend Tab**) or with MCP `session action=suspend`. The PTY and its agent end, so memory and CPU are released, while the tab stays restorable exactly like a tab after a TUICommander restart.

- **Kept on the tab** — cwd, worktree, alias, `tuic_session`, agent type, agent session id and launch command. The tab shows a `zz` badge and a **Suspended** notice with a **Resume** button.
- **Resume** — the **Resume** button or **Resume Tab** menu entry opens a new PTY with the same cwd and, for an agent tab, types the command `verifyAndBuildResumeCommand` builds, the function the restart-restore path uses. A plain shell tab reopens a shell. The command is typed automatically at the first idle prompt, with no restart banner: clicking Resume is the confirmation, and the banner would ask for a second click.
- **Persistence** — `suspended` is stored in the tab's `savedTerminals` record, so a restart restores the tab suspended (a plain shell tab too) and never resumes it by itself.
- **Refusals** — the tab is not suspended while the agent is working (`working`/`starting` or background work), a question awaits input, compose commands are queued, or a plain shell is busy. An agent tab whose session cannot be resumed is refused. MCP returns `Cannot suspend: <reason>` before any event is emitted, and also for a session that already exited.
- **MCP verdict** — `session action=suspend` waits (up to 20 s) for the tab's answer and returns `{ok:true}` or `Cannot suspend: <reason>` when the tab refuses. With no UI attached (a headless `tuic-remote` with no open client) it returns an error instead of reporting success.
- **Event** — `session-suspend-requested` (`{ session_id, request_id }`) from MCP `session action=suspend`; the UI owns the tab and performs the suspend

---

## 2. Sidebar

Automatic repository refreshes start at least five seconds apart per repository. Event bursts retain one trailing refresh; badges can lag by five seconds plus backend read time. Explicit repository additions, branch checkouts and merge cleanup refresh the affected repository immediately, while still waiting for any active refresh to finish.

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
- Worktree removal makes read-only build artifacts removable inside the checkout before Git deletes it; an unregistered directory left behind is reported with its path.
- Stale creation recovery preserves registered Git checkouts on name collisions. Force branch deletion saves an exact-tip archive ref and refuses conflicting archives.
- Background Git actions share a subcommand and per-command flag policy across desktop IPC and HTTP.
- `+` button: click opens a terminal in that branch; long press (500 ms) lists the enabled agents and opens a tab running the chosen one (a shell row has no agents, so there a long press acts as it did before: a click, or the row menu on touch); right-click opens the row menu
- CI ring: proportional arc segments (green=passed, red=failed, yellow=pending)
- PR badge: always shows `#number` plus its highest-priority state when applicable (Draft, Conflicts, CI, review, merged/closed), with state color — click for detail popover
- Diff stats: `+N / -N` additions/deletions. Clicking the stats chip or the `N dirty` chip selects that workspace and opens the Git panel on the Changes tab (it stays open if already there)
- Merged badge: branches merged into main show a "Merged" badge
- Unmerged mark: a small neutral outlined square shows commits outside the default branch even when the diff and dirty counts are zero
- Question indicator: `?` icon (orange, pulsing) when agent asks a question
- Idle indicator: branch icons turn grey when the repo has no active terminals
- Quick switcher badge: numbered index shown when `Cmd+Ctrl` held
- Remote-only branches with open PRs: shown in sidebar with PR badge and inline accordion actions (Checkout, Create Worktree). Additional actions when PR popover is open: Merge, View Diff, Approve, Dismiss
- Dismiss/Show Dismissed: remote-only PRs can be dismissed from the sidebar; a "Show Dismissed" toggle reveals them again
- Branch sorting: main/master/develop always first, then alphabetical; merged PR branches sorted last

### 2.3.1 Nested Terminal Tabs (opt-in)
- **Disabled by default.** Enable it via **Settings → Appearance → Tabs → Nested Terminal Tabs** (`tab_tree_enabled`). The change applies immediately; no restart is required.
- While disabled, the sidebar keeps the normal branch rows and does not render the activity card or nested agent/session rows. Missing nested agents therefore usually means this opt-in setting is still off.
- When enabled, every branch with at least one open terminal session gets an expandable activity card. Enabling the setting does not create sessions: an agent appears only after it is running in a terminal assigned to that branch.
- Agent rows show the agent icon, terminal name, current intent/task (falling back to the last substantial prompt), compact age since the backend's last semantic session activity, and the same busy/idle signal used by the terminal tab and branch icon. Terminal redraws do not reset this age. Plain shells remain visible as terminal rows.
- A sub-agent row shows a muted robot tag with its parent agent in the tooltip; the repo GitHub badge retains the accent color to distinguish its action from session metadata.
- Clicking a row switches to that session.
- Every branch shows its agents by default; collapsing is remembered per workspace. A separate, always-visible chevron toggles the list by click or Enter/Space. The status icon and row select the branch without changing agent visibility.
- Single-session branches can expand too.
- A collapsed list shows its session count beside the chevron; an expanded list hides the count.

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
- **Group park/unpark**: right-click a group header or use the command palette to park or unpark all repos in a group at once
- Parked repos are hidden from the main repository list
- Sidebar footer button opens a popover showing all parked repos
- Unpark a repo from the popover to restore it to the main list

### 2.7a Sidebar Layout (compact / rich)
- Rich repository metadata hides zero PR and worktree counts; positive counts update live
- Toolbar button (left of the filter icon) cycles auto, compact and rich and prints the mode (A, C, R); the mode is saved in UI prefs (`sidebar_density`)
- Compact: one-line rows. Rich: compact-sized navigation with inline branch facts and expanded intent only for working agents. Auto: rich when the conservative expanded-row budget fits the window or the primary pointer is a finger, compact otherwise
- Rich branch row: PR state word and title; last-commit age, ahead/behind the upstream, diff stats, dirty-file count (opens Changes), merged, stale (no commit for 30 days), unknown (removal blocked), unmerged; never stale, merged, dirty or unknown on a main checkout. Compact carries the age, ahead/behind and stale rule on the branch name tooltip
- Rich agent row: state (working, idle, needs input, error) and the agent's intent, task or last prompt; in-session subagents (state, title, tool calls, age; more than 3 fold into a count); TUIC child sessions nested under the parent. Compact carries state and line on the row tooltip
- Rich repo header: current branch, open PR count, worktree count, age of the last remote poll

### 2.8 Active-Only Filter
- Toggled from the filter icon in the toolbar (next to the sidebar collapse button); the icon turns accent-colored while engaged
- When on, the sidebar shows only repositories that have at least one open terminal — empty groups are dropped entirely (no orphaned headers)
- An accent banner at the top of the sidebar makes it unmistakable that repos are hidden, shows a `shown / total` count, and offers "Show all" to clear the filter
- If the filter hides every repo, a dedicated empty state offers "Show all"
- Session-only (not persisted across restarts)

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
- A Markdown document tab, including one opened through `tuic://open`, can be detached from its tab context menu into a resizable window. Its tab remains in the main window; selecting it focuses the document window, and closing that window returns the document to the tab. The detached viewer rereads the file on focus and while open.
- Rendered Markdown links open files and directories in TUICommander, including local symlinks and parent paths; direct UNC hrefs are refused, and paths that reach macOS privacy-protected home directories are not probed. Heading and `file:line` or `file:line:column` targets work without navigating the WebView; web and email links use the system handler
- File list from repository's markdown files
- Clickable file paths in terminal open `.md` files here
- Auto-show: adding any markdown tab automatically opens the Markdown panel if it's closed
- Header bar shows file path (or title for virtual tabs) with Edit button (pencil icon) to open in CodeEditor
- `Cmd+F` search: find text in rendered markdown with highlight navigation (shared SearchBar component)
- **Interactive GFM checkboxes**: `- [ ]`, `- [x]`, and `- [~]` task-list items render as clickable checkboxes. Clicking cycles through unchecked → checked → in-progress → unchecked. Changes are written back to the source `.md` file on disk. The `[~]` state renders as an indeterminate (half-filled) checkbox — non-standard GFM extension for tracking in-progress items
- **Mermaid diagrams**: fenced code blocks with ` ```mermaid ` are rendered as interactive SVG diagrams. Mermaid.js is lazy-loaded on first use with dark theme
- **Inline review comments (tweaks)**: review-comment any passage of a rendered markdown file without leaving the viewer.
  - **Create**: select text in the rendered markdown → a floating **Comment** button appears next to the selection → click it to open an inline popover and type the note (`Ctrl+Enter` to save)
  - **Block comments**: move through the gutter left of a heading, paragraph, individual list item (including task and nested items), quote, table, or code block to highlight its exact Markdown source and reveal its comment button. List-item markers sit on an indented continuation line below the chosen item's own content, preserving the list. Block anchors use the renderer's raw-source ranges, so links and inline HTML do not depend on matching rendered text back to Markdown
  - After each save, block ranges and highlights are refreshed from the updated source even when the visible Markdown is unchanged by the invisible comment markers. Consecutive comments remain attached to the items that were selected.
  - **Send to agent**: the Markdown topbar lists live agents belonging to the same repository. Sending queues one file-scoped instruction through the existing agent idle gate; an idle agent receives it immediately, while a busy agent receives it at its next idle window
  - **View / edit / delete**: commented passages and blocks are highlighted; hovering one shows the comment in a tooltip, clicking it reopens the popover to edit or delete
  - **Storage**: comments live *inside* the `.md` source as HTML-comment markers. Inline comments wrap their source text with `tweak:begin` / `tweak:end`; block comments use a `tweak:block` marker immediately before the target block, while list-item comments use a `tweak:item` marker within that item. Both forms are invisible to standard Markdown renderers, survive round-trips, and are committed with the file. The only escaped sequence is `-->` (→ `--&gt;`)
  - **Save failures**: Markdown files opened by absolute path use the external file writer. A failed comment save shows an error toast and keeps the comment popover and draft open for retry.
  - **LLM-friendly**: the first comment added to a file prepends an invisible HTML-comment header explaining inline, block, and list-item markers, so an AI agent reading the raw file understands them without external context. Existing files with the older header remain readable and editable. The intended workflow is "human highlights + comments → agent applies the feedback to the highlighted text → agent removes the markers"
  - **Rendering**: highlights are wrapped in the DOM *after* markdown parsing, so a selection that straddles inline formatting (`**bold**`, `` `code` ``) stays intact and the highlight spans contiguously. Implemented in `ContentRenderer`, whose consumers are the Markdown panel and the AI Chat transcript

### 3.4 File Browser Panel (`Cmd+E`)
- **Remote file drops**: native OS files dropped onto a connected remote repository are copied through its authenticated daemon connection; sources remain on the Mac. Directory copies require confirmation and existing names are skipped. Each top-level upload is limited to 256 MiB (including archive overhead) and 10,000 entries; symlinks and special files are rejected. Native destination filenames and safe executable permissions are preserved; uploads abort after 30 seconds without data.
- Directory tree of active repository
- **Auto-refresh**: directory watcher detects external file changes (create/delete/rename) and refreshes automatically within ~1s, preserving selection
- Navigation: `↑/↓` (navigate), `Enter` (open/enter dir), `Backspace` (parent dir)
- Breadcrumb toolbar: always-visible path bar with click-to-navigate segments + inline sort dropdown (funnel icon)
- Search filter: text input with `*` and `**` glob wildcard support
- Git status indicators: orange (modified), green (staged), blue (untracked)
- Context menu (right-click): Copy (`Cmd+C`), Cut (`Cmd+X`), Paste (`Cmd+V`), Rename, Delete, Add to .gitignore
- Keyboard shortcuts work when panel is focused (copy/cut/paste)
- Sort dropdown: Name (alphabetical, directories first) or Date (newest first, directories first)
- **View modes**: tree view (default) and flat list — toggle via toolbar buttons. Tree view shows a collapsible hierarchy with lazy-loaded subdirectories on expand. Switching to tree resets to repo root. Search always uses flat results
- Click file to open in code editor tab

#### 3.4.1 Content Search (`Cmd+Shift+F`)
- Full-text search across file contents — toggle from filename search via the `C` button in the search bar
- Options: case-sensitive, regex, whole-word
- Results stream progressively and are grouped by file with match count per file
- Each result row shows file path, line number, and highlighted match context
- Match highlighting stays aligned after Unicode text, including non-BMP emoji
- Click a result to open the file in the code editor at the matched line
- Binary files and files larger than 1 MB are automatically skipped
- Backed by `search_content` Tauri command; results delivered via `content-search-batch` events, each carrying the `search_id` of the panel that asked (the event is global and three panels listen)

### 3.5 Code Editor (CodeMirror 6)
- Opens in main tab area when clicking a file in file browser
- Hold `Cmd` (macOS) or `Ctrl` (Windows/Linux) while hovering to underline a link, then click to open it. HTTP(S) URLs open in the system browser; Markdown links and file paths open in TUICommander. Paths may be absolute, `~/`-prefixed, relative to the edited file, or relative to its repository, with an optional `:line`. Directories open in the File Browser; missing paths show a toast. Other modified clicks keep go-to-definition behavior.
- Syntax highlighting auto-detected from extension (disabled for files > 500 KB)
- Line numbers, bracket matching, active line highlight, Tab-to-indent
- Find/Replace: `Cmd+F` (find), `Cmd+G` / `Cmd+Shift+G` (next/prev), `Cmd+H` (replace), selection match highlighting
- Save: `Cmd+S` (when editor tab is focused)
- Read-only toggle: padlock icon in editor header
- Line wrapping: header toggle or `Alt+Z`; text-like and Markdown files start wrapped, code files start unwrapped; each kind remembers its choice
- Per-tab font zoom: `Cmd+=` / `Cmd+-` changes the active editor by 2px (8–32px); `Cmd+0` resets it to the configured default
- Unsaved changes: dot indicator in tab bar and header
- Disk conflict detection: banner with "Reload" (discard local) or "Keep mine" options
- Auto-reloads silently when file changes on disk and editor is clean
- Undo/Redo: `Cmd+Z` / `Cmd+Shift+Z` with full history
- Code folding: collapse/expand blocks via gutter arrows or `Cmd+Shift+[`/`]`
- Auto-close brackets: typing `(`, `[`, `{`, `"`, `'` inserts matching pair
- Scroll past end: last line can scroll to the top of the viewport
- Block selection: `Alt+drag` for rectangular/column selection with crosshair cursor
- Drop cursor: ghost cursor shown when dragging text over the editor
- Special character highlighting: invisible chars (zero-width spaces, control chars) rendered as placeholders
- CSS color preview: inline color swatches next to hex/rgb/rgba/hsl values
- **Large-file support**: files up to 250 MB open via a dedicated read path (`read_file_editor` / `MAX_EDITOR_LARGE_FILE_SIZE`). Files that exceed this cap are refused up front with an informational notice instead of hanging the UI. Standard syntax highlighting is disabled above 500 KB, but the file still opens and is fully editable
- **Inline git blame** (GitLens-style): a dim italic `author · relative time · summary` annotation at the end of the active line, following the cursor over already-loaded blame data (fetched on load/save/repo-revision via `get_file_blame`, never per keystroke). Lines with uncommitted edits show `You · Uncommitted changes`. On by default (`inline_blame_enabled` config field); no annotation for external (non-repo) files

### 3.6 Ideas Panel (`Cmd+Alt+N`)
- Quick notes / idea capture with send-to-terminal
- `Enter` submits idea, `Shift+Enter` inserts newline
- Per-idea actions: Edit (copies back to input), Queue (agent tabs only), Send to Terminal (sends + return), Delete
- Queue leaves the idea in the agent's Compose FIFO for its next idle window instead of typing it
  into the prompt now, so it never steers the running turn. The action appears only when the active
  tab runs a detected agent; a detached Ideas window always shows it and reports a refusal by toast.
- Mark as used: notes sent to terminal or queued are timestamped (`usedAt`) for tracking
- Badge count: status bar toggle shows count of notes visible for the active repo
- Per-repo filtering: notes can be tagged to a repository; untagged notes visible everywhere
- All-repositories toggle (stacked-layers button, header): shows the ideas of every repository together; in memory only. Badge and "clear completed" follow the visible list; new ideas stay tagged with the active repo
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
- Keyboard shortcuts are now in the Settings > Keyboard Shortcuts page (auto-generated from `actionRegistry.ts`)

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
- Click a commit row to expand and see its full commit message body (multi-line, untruncated) and changed files (via `get_changed_files`)
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
- **M** — Merge selected branch into current. Result is surfaced as a toast: conflict error on failure, "Already up to date" on a no-op, or a success toast with a one-click "Delete branch" action for the now-merged branch
- **r** — Rebase current onto selected branch
- **P** — Push branch (auto-detects missing upstream and sets tracking)
- **p** — Pull current branch
- **f** — Fetch all remotes
- Context menu (right-click): Checkout, Create Branch from Here, Delete, Rename, Merge into Current, Rebase Current onto This, Push, Pull, Fetch, Compare (shows `diff --name-status`)
- **Delete merged**: a broom button (with a count badge of how many qualify) bulk-deletes all local branches already merged into main, behind a confirm dialog listing the targets. Uses the same fresh integration proof as MCP and compares each ref with the proved tip. Content-based proofs require an archive of that tip
- MCP `repo branch_integrations` and `repo branch_integration` report proof, tip, archive requirements and worktree paths. The branch panel and sidebar use the same backend classifier.
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
- Browser mode mounts the same palette and exposes it through the magnifying-glass toolbar button, so it does not depend on the browser forwarding `Cmd/Ctrl+P`
- Browser actions are fail-closed: only commands explicitly verified against the web UI or HTTP transport are shown; native dialogs, detached windows, updater controls, MCP configuration, and user-plugin management remain omitted
- **Search modes**: type `!` to search files by name, `?` to search file contents, `~` to search across all open terminal buffers. File/content results open in editor tab (content matches jump to the matched line). Terminal results navigate to the terminal tab/pane and scroll to the matched line. Leading spaces after prefix are ignored
- Browser filename and content searches use the existing HTTP routes. Content results are correlated with a per-search random ID and republished only inside the requesting page, preventing results from leaking across windows or panels
- **Discoverable search commands**: "Search Terminals", "Search Files", "Search in File Contents" appear as regular palette commands and pre-fill the corresponding prefix
- **QR for Remote Mobile Connection**: opens a large black-on-white QR (in a dialog) that a phone can scan to launch the mobile companion PWA. Reuses the Settings → Remote Access connect flow (`get_connect_url` — token stays server-side); shows a hint when Remote Access is disabled and a network picker for multi-IP machines
- Powered by `actionRegistry.ts` (`ACTION_META` map)

### 3.12 Activity Dashboard (`Cmd+Shift+A`)
- Real-time view of all active terminal sessions in a compact list
- The inline overlay stays close to the detached window's compact width and fits narrow main windows
- An oversized saved detached Activity window opens at its compact 550×650 default after restart; smaller saved sizes remain available
- Each row shows: terminal name, project name badge (last segment of CWD), agent type, status, last activity time
- A PTY spawned by another agent (`agent action=spawn`) shows the same monochrome robot-head icon in the dashboard and nested sidebar row. Its tooltip and accessible name read `Spawned by <parent tab name>`, resolved from either the parent's PTY ID or live TUIC identity; an absent parent tab reads `Spawned by external agent`
- Sub-rows (up to one shown per terminal, in priority order):
  - `currentTask` (gear icon) — current agent task from status-line parsing (e.g. "Reading files"). Suppressed for Claude Code (spinner verbs are decorative)
  - `agentIntent` (crosshair icon) — LLM-declared intent via `intent:` token
  - `lastPrompt` (speech bubble icon) — last user prompt (>= 10 words). Shown only when no `agentIntent` is present
- Status color codes: green=working, yellow=waiting, red=rate-limited, gray=idle
- Ready input composers remain gray/idle when an agent leaves a long-lived background terminal running
- Rate limit indicators with countdown timers
- Click any row to switch to that terminal and close the dashboard
- Relative timestamps auto-refresh ("2s ago", "1m ago")

### 3.13 Error Log Panel (`Cmd+Shift+E`)
- Centralized log of all errors, warnings, and info messages across the app
- Sources: App, Plugin, Git, Network, Terminal, GitHub, Dictation, Store, Config
- Level filter tabs: All, Error, Warn, Info, Debug — uses a **severity threshold**: selecting a level shows that level and everything more severe (e.g. Warn shows Warn + Error intermingled). Each tab has a tooltip describing what it includes
- Source filter dropdown to narrow by subsystem
- Text search across all log messages
- Each entry shows timestamp, level badge (color-coded), source tag, and message
- Copy individual entries or all visible entries to clipboard
- Clear button to flush the log
- Status bar badge shows unseen error/warning count (red, resets when panel opens)
- Global error capture: uncaught exceptions and unhandled promise rejections are automatically logged
- Ring buffer of 1000 entries (oldest dropped when full), Rust-backed — warn/error entries survive webview reloads via `push_log`/`get_logs` Tauri commands
- Also accessible via Command Palette: "Error log"

### 3.14 Plan Detection
- Delivered by the preinstalled external Plan Tracker plugin
- Plans are detected via structured `plan-file` events from the output parser and via a `plans/` directory watcher
- Auto-open: restores the active plan from `.claude/active-plan.json` on startup; new plans opened as background markdown tabs on first detection (no focus change)
- Repo-scoped: only processes plans belonging to the active repository

### 3.15 Preview Tab
- Multi-format file previewer opened from clickable file paths, drag & drop, File Browser, or Command Palette
- File routing handled by `classifyFile()` in `src/utils/filePreview.ts`
- Supported formats:
  - **HTML** — rendered in sandboxed iframe with "Open in browser" button; `Cmd/Ctrl+F` find-in-page uses the shared SearchBar pill (case/regex/whole-word toggles), which drives the iframe over a postMessage bridge and highlights matches in place
  - **PDF** — rendered via asset protocol in embedded iframe
  - **Images** — PNG, JPG/JPEG, GIF, WebP, SVG, AVIF, ICO, BMP — rendered as `<img>` via asset protocol
  - **Video** — MP4, WebM, OGG, MOV — rendered as `<video>` with native controls
  - **Audio** — MP3, WAV, FLAC, AAC, M4A — rendered as `<audio>` with native controls
  - **Text / data** — TXT, JSON, CSV, LOG, XML, YAML, TOML, INI, CFG, CONF — raw text in a `<pre>` block
- Header bar shows shortened file path with **Edit** button (pencil icon — opens file in code editor) and **Open externally** button
- **Reload:** when a web or HTML-preview tab is active, `Cmd/Ctrl+R` reloads its content instead of opening the Run Command dialog
- File content auto-refreshes on repository revision bumps (git change detection)
- Uses Tauri's `convertFileSrc()` asset protocol for binary files, `read_external_file` IPC for text content
- CSP allows `asset:` and `http://asset.localhost` in `frame-src` and `media-src`

### 3.16 Focus Mode (`Cmd+Alt+Enter`)
- Hides sidebar, tab bar, and all side panels to maximize the active tab's content area
- Toolbar and status bar remain visible for repo/branch state and mode exit
- Session-only (not persisted across restarts)
- Toggle again to restore the previous layout

### 3.17 Detachable Panels
- Any panel (AI Chat, Activity Dashboard, Git Panel) can be detached into a separate OS window
- Generic system via `open_panel_window` / `close_panel_window` Rust commands with per-panel adapters
- Two-tier sync: self-sufficient panels (Git Panel) call Rust directly; projection panels (Activity Dashboard) receive state snapshots via `emitTo` at 1 Hz
- Shared `PanelWindowControls` component provides consistent detach/reattach/close buttons across all panels
- Closing a detached window automatically restores the panel to the main window
- Tab bar "Detach to Window" context menu entry for per-tab detach (PTY session stays alive in Rust)
- Markdown document tabs use the same panel-window commands with a window per document; their file path is passed to the detached viewer, which reads from disk and routes linked files back to the main window.
- Generic lifecycle functions: `togglePanel()`, `detachPanel()`, `reattachPanel()` replace per-panel callsites
- `uiStore.detachedPanels` map tracks all detached panels (replaces former `aiChatDetached` boolean)
- Disk-backed panels hand over through their store, not through a live link: the detached window is opened with the params from `detachParams()` and reads its own state on mount, and the main window re-reads it in `onReattach()` when the detached copy closes or reattaches

---

## 4. Toolbar

### 4.1 Sidebar Toggle
- `◧` button (left side) — same as `Cmd+[`
- Hotkey hint visible during quick switcher
- Adjacent **filter icon** (shown while the sidebar is visible) toggles the "Active only" repo filter — see section 2.8

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
- **Messages section** — agent/MCP notices and agent spawns go directly to the bell, with repository scope and an explicit **Open terminal** action when a session is known. Consecutive identical notices from the same origin within five seconds produce one item and one requested sound. User-action toasts are also retained when **Keep toasts in the bell** is enabled. Overflow from the two-card limit is retained even with that setting off; dedicated-domain toasts that explicitly opt out of Messages stay visible or queued.
- **Transient toast placement** — top-right of the terminal area, offset left of a docked panel, with at most two visible cards. Same-kind notices in the same repository/session within five seconds show a ×N summary; every new member restarts its dismissal timer. Errors take a visible slot ahead of informational cards. Tall cards scroll within the upper region so their action buttons remain reachable without covering the bottom input. Clicking a card dismisses it; navigation uses its explicit action.
- **Plugin activity sections** — registered by plugins via activityStore
- Click PR notification: opens full PR detail popover for that branch
- Individual dismiss (×) per notification, section "Dismiss All", auto-dismiss after 5min focused time

### 4.5 IDE Launcher
- Button with current IDE icon — click to open repo/file in IDE
- Dropdown: shows all detected installed IDEs, grouped by category
- Categories: Code Editors, JetBrains, Terminals, Git Tools, System
- JetBrains family: IntelliJ IDEA, PyCharm, WebStorm, GoLand, CLion, PhpStorm, RubyMine, Rider, DataGrip, RustRover, Android Studio, Fleet — launched via their CLI launcher (`idea`, `pycharm`, …) with `--line`/`--column` goto, falling back to `open -a` on macOS when the Toolbox shell scripts aren't on PATH
- File-capable editors (including JetBrains IDEs) open the focused file (from editor or MD tab); others open the repo
- Custom launchers (#71): user-defined entries configured at Settings → General → Custom Launchers (name, executable on `PATH` or absolute, args, per-OS platform, enable toggle), shown under a "Custom" section in the dropdown. Args support placeholder tokens resolved at launch: `{path}`/`{file}` (focused file, else repo), `{repo}`, `{fileDir}` (focused file's directory, else repo), `{cwd}` (focused terminal cwd, else repo), `{home}`, and `{line}`/`{column}` (editor cursor, default 1)
- Run command button: `Cmd+R` (run), `Cmd+Shift+R` (edit & run)

---

## 5. Status Bar

### 5.1 Left Section
- Zoom indicator: current font size (shown when != default)
- Status info text (with pendulum ticker for overflow, pulse animation on new messages)
- CWD path: shortened with `~/`, click to copy to clipboard (shows "Copied!" feedback)
- Unified agent badge with priority cascade:
  1. Rate limit warning (highest): count + countdown timer when sessions are rate-limited
  2. Matching Claude or Codex Usage API ticker: live utilization from the active provider (click opens that provider's dashboard)
  3. PTY usage limit: weekly/session percentage from terminal output detection
  4. Agent name (lowest): icon + name of detected agent
  - Color coding: blue < 70%, yellow 70-89%, red pulsing >= 90%
  - The shared usage ticker is absorbed into the badge only when its provider label matches the active Claude or Codex agent; stale results from the previous provider stay hidden during an asynchronous tab switch
  - The usage poll has no default provider: it stays silent until a Claude or Codex tab is focused, then follows that provider and stays on it while the active tab is a shell. A Codex-only install is therefore never shown a Claude reading (or a Claude "no token") it did not ask for
  - Codex windows are named by duration (`5h`, `7d`) because the API does not label them, and only the account limit is rendered. A plan whose account limit reports a single window shows a single reading; per-model limits stay in the dashboard, where their model name is visible
- Shared ticker area: multi-source rotating messages from plugins with source labels, counter badge (1/3 ▸), click-to-cycle, right-click popover, and priority tiers (low/normal/urgent)
- Non-modal popups (ticker popover, status info balloon, GitHub panel, Smart Prompts dropdown) have no backdrop: wheel and clicks reach the sidebar and terminal while they are open, and an outside press dismisses them without being consumed (shared `useOutsideDismiss` hook)
- Update badge: "Update vX.Y.Z" (click to download & install), progress percentage during download

### 5.2 GitHub Section (center)
- Branch badge: name + ahead/behind counts — click for branch popover
- PR badge: number + highest-priority state label and color — click for PR detail popover
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

### Launch instruction inspector

The terminal context menu offers **Inspect Launch Instructions…**. Collapsible sections show the prospectively captured final launch brief, explicit system instruction arguments/file snapshots and served MCP initialize responses, with recorded source and original UTF-8 byte count. Redacted text is bounded to 32 KiB per section, 64 KiB per receipt and 16 captured sections. Queued briefs are labeled until submission succeeds. Historical launches, shell-launched agents and files the agent reads independently show **Not observable by TUIC**. Receipts belong to live PTY/MCP metadata, survive frontend reloads and end with the backend/session lifetime; workflow preview/hash receipts remain unchanged.

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
| Goose | `goose` | `goose session --resume --name <uuid>` (session-aware) / `goose session --resume` (fallback) |
| Droid (Factory) | `droid` | — |
| pi | `pi` | `pi --continue` |
| Git (background) | `git` | — |

The per-agent **Prevent alternate screen** setting defaults on. New TUIC shells export Claude's environment control and, after a supported-CLI probe, the Codex/Grok `--no-alt-screen` and OpenCode `--mini` choices for manually typed commands. A timed-out help probe is cached for that binary version and its child process tree is stopped, so later launches do not repeat the delay. One Rust policy also covers menu, IPC/HTTP/MCP, worktree, and resume launches. A CLI without a verified control receives no invented option; an unexpected alternate-screen entry emits one warning per session.

### 6.1.1 Session-Aware Resume
When an agent is detected running in a terminal, TUICommander automatically discovers its session ID from the filesystem and stores it per-terminal (`agentSessionId`). On restore, this enables session-specific resume instead of generic fallback commands.

- **Claude Code** — Sessions stored as `~/.claude/projects/<slug>/<uuid>.jsonl`; UUID from filename. Windows drive colons become dashes in the project slug (`C:\Users\foo\bar` → `C--Users-foo-bar`).
- **Gemini CLI** — Sessions stored in `~/.gemini/tmp/<hash>/chats/session-*.json`; `sessionId` field from JSON
- **Codex CLI** — Sessions stored in `~/.codex/sessions/YYYY/MM/DD/rollout-*-<UUID>.jsonl`; UUID from filename. Codex does *not* partition by project, so candidates are filtered on the working directory recorded in the rollout's first `session_meta` record — otherwise a terminal in one project would bind to another project's session. A rollout whose `cwd` can't be read is rejected rather than accepted
- **Goose** — Sessions stored in SQLite (`~/Library/Application Support/Block/goose/sessions/sessions.db`); shell wrapper injects `--name $TUIC_SESSION` for deterministic binding, resume by name

Discovery runs once per terminal on `null→agent` transition. Multiple concurrent agents are handled via a `claimed_ids` deduplication list. On agent exit, the stored session ID is cleared to allow re-discovery on next launch.

Every agent rejects candidates older than 5 minutes (`SESSION_MAX_AGE`), so a terminal opened now never resumes a session abandoned earlier in the day.

### 6.1.2 TUIC_SESSION Environment Variable
Every terminal tab has a stable UUID (`tuicSession`) injected as the `TUIC_SESSION` environment variable in the PTY shell. This UUID persists across app restarts and enables:

- **Automatic session binding**: Shell integration injects wrapper functions that transparently bind agent sessions to the current tab (zsh, bash, fish):
  - **Claude Code**: `claude()` adds `--session-id $TUIC_SESSION`; bypassed when `--session-id`, `--resume`, or `--continue` are explicit
  - **Goose**: `goose()` adds `--name $TUIC_SESSION` to `session` and `run` subcommands; bypassed when `--name`, `-n`, `--resume`, or `-r` are explicit
  - **Session conflict handling**: When an agent reports a session conflict (in-use or not-found), TUICommander creates a `no-session-inject.$TUIC_SESSION` flag file in the config directory. Shell wrappers check for this file and skip `--session-id` injection when it exists — avoiding PTY writes that could corrupt TUI output
- **Automatic resume**: On restore, TUICommander verifies if the session file exists on disk (`verify_agent_session`) before using `--resume $TUIC_SESSION`
- **UI spawn coherence**: When spawning agents via the context menu, `TUIC_SESSION` is used as `--session-id` automatically
- **Custom scripts**: `$TUIC_SESSION` is available as a stable key for any tab-specific state

### 6.2 Agent Detection
- Hand-launched agents are classified and recorded by the backend on desktop and headless hosts; HTTP foreground queries share the desktop detection logic. Plain shells remain ineligible for agent submit and mail wake.
- Protocol completion survives terminal redraws and decorative idle animation; new input and recognized semantic working signals can reopen activity.
- Auto-detection from terminal output patterns
- Multi-agent status line detection via regex patterns anchored to line start: Claude Code (`*`/`✢`/`·` + task text + `...`/`…`), `[Running] Task` format, Aider (Knight Rider scanner `░█` + token reports), Codex CLI (`•`/`◦` bullet spinner with time suffix), Goose (`<message>... (Ctrl+C to interrupt)`), Copilot CLI (`∴`/`●`/`○` indicators), Gemini CLI (braille dots `⠋⠙⠹...`)
- Movement-based activity: BUSY is normally latched/kept by text changing above the input area, user submission, and OSC lifecycle markers, which outrank silence. Ready prompts require a stable 1.5s observation before idle.
- A Claude mail wake submitted after a hook idle cannot leave the session Working indefinitely when the detailed transcript hides the composer: without output or a later busy hook, the indicator returns to idle after five minutes.
- Codex and Claude also use narrow semantic active markers because their current TUIs can freeze or retain an empty composer during real work. Codex scopes `Working … esc to interrupt` to the lowest `›` or `»` composer. Claude requires a spinner-prefixed phase with an ellipsis and parenthesized progress; completed summaries remain idle-safe, and live work can supersede a premature blocking Stop-hook completion.
- Ctrl-C/Escape are interrupt intent only; status changes after the agent confirms interruption, returns to a stable prompt, emits Stop, or exits.
- Status lines rejected when they appear in diff output, code listings, or block comments
- Brand SVG logos for each agent (fallback to capital letter)
- Agent badge in status bar showing active agent
- Binary detection: Rust probes well-known directories via `resolve_cli()` for reliable PATH resolution in desktop-launched apps
- Foreground process detection: `tcgetpgrp()` on the PTY master fd, then macOS `proc_pidpath()` or Linux `/proc/<pid>/exe` with a `/proc/<pid>/comm` fallback to get the binary name. Identities are revoked when a shell returns to the foreground after agent observation; run-config presets remain armed during startup, and transient non-shell helpers preserve identity. Linux updater-replaced executables retain classification after removing the proc ` (deleted)` suffix. Handles versioned binary paths (e.g. Claude Code installs as `~/.local/share/claude/versions/2.1.87`) by recognising the exact `claude/versions/<numeric-version>` layout when the basename is not a known agent; arbitrary agent-named ancestor directories do not classify an executable; Droid is classified explicitly so it receives the agent idle threshold.

### 6.3 Rate Limit Detection
- Provider-specific regex patterns detect rate limit messages
- Status bar warning with countdown timer
- Per-session tracking: rate-limit events are only accepted for sessions where agent activity has been detected (prevents false warnings in plain shell sessions)
- Auto-expire: rate limits are cleared automatically after `retry_after_ms` (or 120s default) without requiring agent output

### 6.4 Question Detection
- An open Ink selection dialog restores a missing question badge with native hooks enabled, including after a later busy hook; repeated paints do not duplicate the notification.
- Recognizes interactive prompts (yes/no, multiple choice, numbered options)
- Tab dot turns orange (pulsing) when awaiting input; sidebar branch icon shows `?` in orange
- Prompt overlay: keyboard navigation (↑/↓, Enter, number keys 1-9, Escape)
- Two detection strategies run in priority order:
  1. **Screen-based** (Strategy 1): reads the live terminal screen, finds the last chat line above the prompt box (delimited by separator lines), checks if it ends with `?`. Works with Claude Code, Codex (`›` prompt), and Gemini (`> ` prompt) layouts
  2. **Silence-based** (Strategy 2, fallback): if terminal output stops for 10s after a line ending with `?`, the session is treated as awaiting input
- Stale candidate clearing: candidates that fail screen verification are purged so the same question can re-fire in a future agent cycle
- Echo suppression: user-typed input echoed by PTY is ignored for 500ms to prevent false question detection
- `extract_question_line()` scans all changed rows (not just the last) for question text, applied in both normal and headless reader threads
- Wrapped `suggest: [ … ]` items remain follow-up actions even when an item contains `?`; they cannot set the question state.
- Question state auto-clears when a `status-line` event fires (agent is actively working, so it's no longer awaiting input)
- Canceling a Codex approval with Esc clears its waiting badge when the idle composer returns, while a later question remains active.

### 6.5 Usage Limit Detection
- Claude Code weekly and session usage percentage (from PTY output patterns)
- Color-coded badge in status bar (blue < 70%, yellow 70-89%, red pulsing >= 90%)
- Integrated into unified agent badge (see section 5.1)

### 6.6 Provider Usage Dashboards
- Native SolidJS component (not a plugin panel — renders as a first-class tab)
- The active Claude, Codex, or Grok badge opens its matching dashboard; `Cmd+Shift+A` opens Claude Usage
- **Rate Limits section:** Live utilization bars from Anthropic OAuth usage API
  - Follows the focused Claude terminal's `CLAUDE_CONFIG_DIR` credential profile; an unresolved profile shows unknown rather than another account's quota
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
- **Codex dashboard:** account rate-limit windows, per-model limits, reset times,
  plan type, credit balance, and token history from the official local Codex
  App Server. The Codex CLI owns OAuth and refresh; TUIC does not read its token.
- **Grok dashboard:** billing-period usage, subscription tier, on-demand spend,
  cap, and prepaid balance from Grok Build's official `_x.ai/billing` ACP
  extension. The short-lived connection is telemetry-only; normal Grok tabs
  remain PTY sessions.
- **Gemini:** terminal rate-limit detection remains available, but there is no
  account dashboard because Gemini CLI has no stable machine-readable account
  quota interface. Session-local `/stats` is not treated as account headroom.

### 6.7 Intent Event Tracking
- Agents declare work phases via `intent: text (Title)` tokens at the start of a row, colorized dim yellow in terminal output. The token is also read when it follows the ack sentence on one row, and when the agent's own wrapping split it across many rows; terminal width distinguishes wrapped text from adjacent indented prose, and input-box chrome is excluded. A long intent keeps its full tab-title event while the Progress journal stores a redacted, capped copy
- MCP instructions request an intent on its own line, at the start of every task and on each material phase change; the terminal Context bar shows it separately from the orchestrator assignment and user prompt
- Session snapshots restore captured intent and the last substantial prompt after reconnect, even while the agent remains idle; live state updates use the same reconciliation path.
- Intent titles may replace spawn-assigned tab labels; only an explicit user rename locks the tab title, including after reconnect
- Colorization is agent-gated (only applied in sessions with a detected agent) to prevent false positives
- Structural tokens stripped from log lines served to PWA/REST consumers via `LogLine::strip_structural_tokens()`
- Structured `Intent` events emitted for LLM-declared work phase tracking
- Centralized debounced busy signal with completion notifications for accurate idle/active status
- HTTP/MCP session origin survives frontend reconnects, keeping orchestration completion chimes muted when configured; BUSY→IDLE and exit share one notification per busy cycle

### 6.8 API Error Detection
- Detects API errors (server errors, auth failures) from agent output and provider-level JSON error responses
- Covers Claude Code, Aider, Codex CLI, Gemini CLI, Copilot, and raw API error JSON from providers (OpenAI, Anthropic, Google, OpenRouter, MiniMax)
- Triggers error notification sound and logs to the Error Log Panel

### 6.9 Agent Configuration (Settings > Agents)
- **ego permissions:** Select `plan`, `default`, `edits`, `auto`, or `yolo` and filesystem sandbox `ro` or `workspace`. Each choice can remain unset to use ego configuration. One backend translator applies these overrides to terminal launches, MCP spawns, and resumes, replacing matching raw flags. AI Chat over ACP uses separate settings.

- Claude and Codex receive process-scoped native status signals at launch by default (`--settings` / `-c notify`), independently switchable per agent. Existing user overrides take precedence and no global settings are changed.
- **Managed workspace trust:** Claude and Codex agent-to-agent spawns accept their new working directory by default, controlled by **Accept workspace trust for managed spawns** for each agent. Codex receives a launch-only project trust override, including through custom launchers that forward arguments; Claude's startup picker is answered from its own PTY after the exact question and default **No, exit** selection appear. Normal user-opened terminals retain each CLI's trust behavior. No agent config file is rewritten.
- **Agent list:** All supported agents with availability status and version detection
- **Run configurations:** Named command templates per agent (binary, args, optional model, env vars). MCP spawn can override the model and environment per child.
- **Default config:** One run config per agent marked as default for quick launching. Codex defaults carry the approvals/sandbox bypass explicitly in editable args, with a warning icon; removing the argument persists.
- **MCP bridge install:** One-click install/remove of `tui-mcp-bridge` into agent's native MCP config file
- **Supported MCP agents:** Claude, Cursor, Windsurf, VS Code, Zed, Amp, Gemini, Codex, Grok, OpenCode, Droid, Goose, pi (through pi-mcp-adapter)
- **Shared settings files are opt-in:** Zed, Amp and Gemini store MCP servers inside their general `settings.json`, so those three are never written automatically — the panel says so and the Install button does it on request
- **Remove all MCP integrations:** Lists every client holding a bridge entry and clears them in one action, so uninstalling TUICommander does not leave a dangling `tuic-bridge` server behind
- **Edit agent config:** Opens agent's own configuration file in the user's preferred IDE
- **Context menu integration:** Right-click terminal > Agents submenu with per-agent run configurations; selected environment variables apply only to the launched agent, including from the sidebar menu
- **Busy detection:** Agents submenu disabled when a process is already running in the active terminal
- **Environment Flags** — Per-agent environment variables injected into every new terminal session. Configure in Settings > Agents > expand an agent > Environment Flags. Useful for setting feature flags like `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` without manual export.

### 6.10 Agent Teams
- **Purpose:** Enables Claude Code's Agent Teams feature to use TUIC tabs instead of tmux panes
- **Approach:** Environment variable `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` injected into PTY sessions, which unlocks Claude Code's TeamCreate/TaskCreate/SendMessage tools. Agent spawning uses direct MCP tool calls (`agent spawn`) instead of the deprecated it2 shim
- **Session lifecycle events:** MCP-spawned sessions emit `session-created` and `session-closed` events so they automatically appear as tabs and clean up on exit
- **Settings toggle:** Settings > Agents > Agent Teams
- **Suggest follow-ups:** Agents can propose follow-up actions via `suggest: [ A | B | C ]` tokens, displayed as floating chip bar
- **Deprecated:** The it2 shim approach (iTerm2 CLI emulation) is commented out — superseded by direct MCP tool spawning

### 6.11 Suggest Follow-up Actions
- **Protocol:** Agents emit `suggest: [ action1 | action2 | action3 ]` at column 0 after completing a task. The whole token sits on one row, bounded by `[ … ]` with no nested brackets — so stray pipes/brackets in surrounding output (mermaid, markdown tables, prose) can never be mis-parsed as items
- **Answer marker:** The MCP initialize instructions (Required Output Markers block) also teach every agent to prefix each sentence that directly answers the user's question with `💬 ` at the start of the row; the answers-only view (`Cmd+Alt+R`) parses it. Always sent, independent of the intent/suggest settings
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
- **Claude background output**: managed MCP peers default to `CLAUDE_CODE_TMPDIR=$HOME/Gits/.tmp/claude/`, created before spawn; explicit inherited, run-config or caller values take precedence.
- Agent-to-agent coordination when multiple agents are spawned in parallel, carried by the `agent` MCP tool — there is no separate `messaging` tool
- **Identity**: Each agent uses its `$TUIC_SESSION` env var (stable tab UUID) as its messaging identity. A headerless external caller may `register` without `tuic_session` to be issued an MCP-scoped UUID, or supply a stable UUID to reclaim an existing identity
- **Actions**: `register` (announce presence, or rename/re-project an auto-bound peer), `list_peers` (discover other agents, optional `path` filter), `send` (message a peer by `to` = tuic_session), `inbox` (poll for messages), `wait` (block until new mail)
- **Dual delivery**: Real-time push via MCP `notifications/claude/channel` over SSE into already working Claude Code turns; idle/completed managed agents and managed non-Claude agents use submitted PTY delivery even when their MCP bridge has an SSE stream; polling fallback via `inbox` is always available
- **Channel support**: TUICommander declares `experimental.claude/channel` capability; spawned Claude Code agents automatically get `--dangerously-load-development-channels server:tuicommander`
- **Lifecycle**: Peer registrations cleaned up on MCP session delete and TTL reap; `PeerRegistered`/`PeerUnregistered` events broadcast via event bus for frontend visibility
- **Limits**: 64 KB max message size, 100 messages per inbox (lifecycle-first eviction; peer mail back-pressure), optional path filtering for `list_peers`
- TUICommander acts as the messaging hub — no external daemon needed
- **Durable task handles**: `agent action=spawn` returns `task_id` and `poll_interval_ms` alongside `session_id`. The `task` MCP tool polls that handle without blocking — `task action=get` returns `{task_id, status, status_message?, result?, error_detail?, poll_interval_ms}` where status is `working|input_required|completed|failed|cancelled` (the last three final), and `task action=cancel` marks the task cancelled **without** killing the agent (`session action=kill` does that). Use this instead of `agent action=wait` / `session action=wait` when work runs past their 300 s cap or when the client may reconnect: the outcome is recorded by the session exit path whether or not anyone was listening. Tasks live for the TUICommander process only — they are deliberately not persisted, because a restart tears down every PTY

### 6.14 AI Chat Panel (`Cmd+Alt+A`) — ego over ACP
**The conversation lives in ego, not in TUICommander.** There is no LLM client
here: no provider, no API key, no tool loop, no sandbox. TUICommander launches
one configured ego binary and speaks ACP to it, per
`plans/ego-integration/archive/plan.md` section 1.

- **One chat across repositories, never bound to a terminal.** A turn ego runs
  outlives any tab and may touch files no tab is showing. Switching repository
  keeps the same conversations; the viewed repository is sent with each prompt.
  The conversation picker lists ego's durable sessions by title and latest
  activity. Desktop and mobile clients connecting to one peer share one ego process, even
  when both connect at the same time.
- Docked on the right, resizable by its left edge (session-scoped width, not
  persisted); detaches into its own window, with `DetachedPlaceholder` in the
  main window meanwhile
- The conversation view loads on first opening, including in a detached window;
  the desktop terminal is ready before this optional view loads
- Multiple chat tabs keep separate ACP sessions, transcripts and composer drafts
  within one panel. Open tabs and the selected tab survive hide/show and detach;
  closing a tab leaves its durable ego conversation available in the picker.
  The focused panel uses `Cmd/Ctrl+T` for a new tab (`Cmd/Ctrl+Alt+T` in browser
  mode) and `Cmd/Ctrl+W` to close one (`Cmd/Ctrl+Alt+W` in browser mode)
- Transcript text and tool output are selectable. User and assistant messages,
  tool outputs and code blocks have copy actions using the terminal clipboard
  adapter. Web links open externally; after backend resolution, file paths use
  the terminal file opener and directory paths open the file browser, including
  from a detached chat. Transcript focus gives `Cmd/Ctrl+A` (select this
  transcript), `Cmd/Ctrl+F` (find), and `Cmd/Ctrl+K` (clear this tab's view)
- **Streamed answers**, reasoning folded into a disclosure, one collapsed tool
  activity line per turn with calls and outputs expandable, and the agent's plan
  replaced whole each time it changes. The view follows new output while the
  reader is at the bottom; collapsed tool rows keep full commands in details
- The connection acknowledgement is hidden; `intent:` is shown as turn status;
  bracketed `suggest:` items on their own line or at the end of an answer become
  buttons that send the selected reply. A sent reply appears once even when ego
  echoes it in chunks. Message Copy appears on hover or keyboard focus
- Session title updates rename the panel header and picker entry. The footer
  shows context-window use and the cumulative cost when ego reports it
- Untitled sessions show a readable prompt or activity-time label in the picker;
  failed and empty turns show a message in the conversation
- **Image paste** stages a removable preview in the composer. Sending forwards
  base64 image content blocks only when the agent advertises image prompts;
  supported PNG, JPEG, GIF and WebP files are capped at 10 MiB per turn
- The composer grows up to a bounded height. Pastes over 200 words are shown as
  numbered markers until Send restores the full text
- `Ctrl+S` in the composer parks text and image previews with a visible chip;
  pressing it again restores or swaps drafts. Sending an intervening prompt
  restores the parked draft. Mobile AI Chat uses a labelled archive icon above the input for this action.
- Mobile AI Chat shares session composer styles, auto-grow, Enter/send behaviour,
  right-side attachment and round Send icons. Suggestions and long text wrap;
  wide tables scroll inside the transcript block.
- Mobile AI Chat transcript links open resolved files or directories in the
  **Files** screen, using the chat workspace as the path base.
- **Permission requests** are answered with one of the option ids ego published.
  Small single-choice **elicitations** use direct answer buttons; other elicitations
  are drawn as a form, and only in `form` mode — the client
  declines every other mode before it reaches a person
- While AI Chat is hidden, its status-bar toggle counts pending permissions and
  forms. Each new question produces one desktop notification, cleared when the
  question settles
- **Session settings** come from the options the session publishes through
  `set_config_option`. The one-row control bar summarizes the model's short name
  and mode; Pause, Resume, Compact and New use named icon buttons. A dialog labels
  every select option and shows its description and current choice. Errors from
  rejected changes appear in the dialog. TUICommander holds no model list
- **Pause, resume and compact** are drawn only when ego advertised each
  extension
- **A stream gap is a state, not a skip**: the panel says it missed part of the
  conversation and offers the one recovery there is — a fresh process replaying
  the history through `session/load`
- Terminal right-click keeps **Explain with AI** and **Fix this error**; both
  write a question about the selection into the composer and open the panel
- Gated by `experimental_features_enabled` (`settingsStore.isAiChatEnabled`,
  off by default) plus a configured `ego_executable`. While that path is empty
  the panel explains why it is inactive and offers **Configure ego** to open the
  ego controls in Settings → General. Provider login and the default model remain
  in Settings → AI Chat. Saving the executable enables chat without an app restart
  (detached windows refresh their settings when focused again)
- Full user guide: [`docs/user-guide/ai-chat.md`](user-guide/ai-chat.md)

What went with the embedded engine (#784-0aec) and did not come back here:

| Removed | Returns as |
|---|---|
| Provider registry, models, slots, API keys in the keyring, Ollama detection | 786-4a6d — configured in ego; TUICommander stores no API key |
| The ReAct agent loop, its 31 tools, `SafetyChecker`, `FileSandbox`, unsafe mode, agent model overrides, cross-session memory injection | nothing. ego runs its own loop and drives terminals from outside, through the `session` MCP tool family |
| Terminal watchers and the cron scheduler (`ai-watchers.json`, `ai-cron.json`) | not scheduled |
| AI diff triage | nothing. #795-320b restored the review but not the triage: one unattended turn gets the whole diff, so there is no per-file classification pass to run |
| PR AI review, changelog generation, improvement scan | **795-320b** — one unattended ego turn each; see 8.3 and 8.10 |
| Smart Prompts `api` execution mode | **787-ee50** — one unattended ego turn; see 10.5 |

### 6.15 Session Knowledge Store (recording only)
Outlived the engine because none of it involves a model, and `pty.rs` is the
producer. Nothing reads it today — it is kept because the history cannot be
re-derived later.
- **Command outcomes** — exit code, duration, CWD, classification, output snippet, auto-correlated error→fix pairs, CWD history, `tui_apps_seen`, terminal mode
- **OSC 133 semantic prompts** feed exact exit codes where the shell supports them; the PTY silence timer records an `Inferred` outcome where it does not
- Persisted to `<config_dir>/ai-sessions/<session_id>.json` with a 2 s debounced flush
- **TUI app detection** via alternate-screen tracking (`ESC[?1049h`/`l`). `TerminalMode::FullscreenTui { app_hint, depth }` is set when the terminal enters vim/htop/lazygit/less/tmux/…
- Code: `src-tauri/src/ai_agent/{knowledge,tui_detect}.rs` — they kept the `ai_agent/` module path because `pty.rs` reads them there

### 6.16 ChoicePrompt Detection
- Claude AskUserQuestion is parsed from its full-bleed Ink footer and numbered options, including rows with descriptions. Its `selection_mode: navigate-enter` tells mobile to move the highlight with arrows and submit with Enter; the mobile screen retains the whole open dialog
- New `ParsedEvent::ChoicePrompt { title, options, dismiss_key, amend_key }` recognises Claude-Code-style numbered confirmation menus (footer matches `Esc to cancel · Tab to amend`)
- Options parsed by regex with optional cursor marker (`❯`, `›`, `>`). Title heuristics require `?` or a verb prefix (`proceed`, `confirm`, `do you want`, …) to avoid matching Markdown numbered lists. Minimum two options
- Destructive labels (`no`, `cancel`, `reject`, `abort`, `deny`, `don't`) flagged for styling
- Piped into `SessionState.choice_prompt`; dispatched to plugins via `pluginRegistry.dispatchStructuredEvent("choice-prompt", …)`; rendered as PWA overlay
- Single-key replies routed through `sendPtyKey()` (`src/utils/sendCommand.ts`) — never `text + \r`. Desktop listener plays a warning sound when the prompt arrives on an inactive tab

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
- Per-repo settings: storage strategy, prompt on create, delete branch on remove, auto-archive, orphan cleanup (safe orphan countdown defaults to 10 seconds), PR merge strategy, after-merge behavior, PR visibility filters (hide drafts/conflicting/CI-failing)
- Setup script: runs once after creation (e.g., `npm install`)
- Archive script: runs before an existing worktree is archived or deleted; non-zero exit blocks the operation. Cleanup of an already missing checkout skips it
- Removal previews distinguish untouched branch history from merged commits, name live sessions, and count uncommitted and untracked files. Automatic PR-close cleanup and merged-worktree archiving skip live or dirty worktrees.
- Merge & Archive: right-click → merge branch into main, then archive or delete based on setting. Conflict cleanup reports `(aborted)` only when `git merge --abort` succeeds; if abort fails, the error includes the manual recovery command.
- Archived worktrees remain usable Git checkouts under `__archived`, including HEAD, reflogs, and initialized submodule refs; locked checkouts are left in place and archived paths are hidden from the active workspace list.
- External worktree detection: monitors `.git/worktrees/` for changes from CLI or other tools
- Remove via sidebar `×` button or context menu (with confirmation)
- **Warm linked worktrees**: every workspace shares refs and objects with the parent. After `git worktree add`, ignored directories such as `node_modules`, `target`, and `.venv` are copied with clonefile/reflink when supported; `.tmp` and `.mdkb` directories, tracked paths, ignored files, nested repositories, and the destination ancestor are never copied
  - Capability is measured against the actual source/destination pair. Unsupported filesystems produce one warning and a valid cold worktree
  - Read-only copied files and directories gain owner write permission in the new worktree only; symlinks are not followed
  - Parent tracked and untracked changes are not carried into the new checkout
  - Shared lifecycle state: sidebar, Worktree Manager, and removal confirmation render one workspace-id keyed backend verdict (`Dirty`, `Merged`, or `Unknown`); sidebar Dirty/Unknown badges explain their meaning on hover or keyboard focus, and unknown blocks removal
  - Desktop IPC, HTTP, and MCP creation payloads report `warm_artifacts.status` (`pending` until the background copy finishes) and state the linked-worktree isolation semantics; workspace path listing reports the current status. A cancelled create marks warming failed; removal and archive cancel queued copies
  - Removal without force keeps dirty worktrees and submodules, refuses an in-progress Git operation, and keeps a branch that changed after the removal check. Submodule commits, stash history, and reflog-only commits receive durable refs in the initialized main-checkout module repository. An uninitialized, misdirected, or deinitialized module with retained Git history stops removal, including nested modules with names different from their paths. A registered checkout whose directory is missing has an explicit lifecycle verdict and needs confirmed force to prune its registration; cleanup skips its inaccessible archive script. Dirty-file force still proves branch safety and never overrides a lock by itself
  - Branch deletion accepts default-branch ancestry, ancestry in the checked-out integration branch, patch equivalence, or a merged GitHub PR whose fetched head contains the local tip
  - MCP `repo action=branch_delete` removes an integrated local branch without a worktree after the same safety proof; it preserves remote refs and refuses checked-out, current, default, or changed local refs
  - **Worktree Manager panel** (`Cmd+Shift+W` or Command Palette → "Worktree manager"):
  - Dedicated overlay listing all worktrees across all repos with metadata: branch name, repo badge, PR state (open/merged/closed), dirty stats, last commit timestamp
  - Dirty, merged, and unknown badges from the shared progressive refresh
  - Orphan worktree detection with warning badge and Prune action; pruning refuses in-progress Git operations and detached commits absent from durable refs
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
- Auto-show PR popover filters out CLOSED and MERGED PRs (configurable in Settings > Git & GitHub > Pull Requests). The popover is non-modal: wheel scrolling over the sidebar keeps terminal keyboard focus; an outside press dismisses it and reaches the underlying control.

### 8.2 CI Checks
- Ring indicator with proportional segments
- Individual check names and status in PR detail popover
- Labels with GitHub-matching colors

### 8.3 PR Detail Popover
- Title, number, link to GitHub
- Author, timestamps, state, merge readiness, review decision
- CI check details, labels, line changes, commit count
- View Diff button: opens PR diff as a dedicated panel tab with collapsible file sections, dual line numbers, and color-coded additions/deletions
- AI Review: one unattended ego turn over the PR diff (#795-320b). Findings are listed with severity, path and line; each one with a concrete line can be
  selected and posted to GitHub as an inline review comment. Findings below the confidence threshold (default `0.7`, `TUIC_REVIEW_CONFIDENCE_THRESHOLD`
  overrides) never leave the backend. TUICommander stores no API key and names no model — that is ego's configuration. When ego is not reachable the
  popover shows ego's own sentence, never an empty finding list
- Merge button: visible when PR is open, approved, CI green — merges via GitHub API. Merge method auto-detected from repo-allowed methods; auto-fallback to squash on HTTP 405 rejection
- Approve button: submit an approving review via GitHub API (remote-only PRs)
- Post-merge cleanup dialog: after merge, offers checkable steps (switch to base, pull, delete local/remote branch)
- Review button: if the branch's active agent has a run config named "review", spawns a terminal running the interpolated command with `{pr_number}`, `{branch}`, `{base_branch}`, `{repo}`, `{pr_url}`. Hidden when no matching config exists
- Triggered from: sidebar PR badge, status bar PR badge, status bar CI badge, toolbar notification bell

### 8.4 PR Visibility Filters
- Global settings (Settings > Git & GitHub > Pull Requests): hide draft PRs, hide conflicting PRs, hide CI-failing PRs
- Per-repo overrides (Settings > [Repo] > PR Visibility): tri-state toggle per filter (Show / Default / Hide)
- Default = inherit from global setting, shown in parentheses (e.g. "Draft PRs (Show)")
- Resolution chain: per-repo override → global setting
- TriStateToggle component: 3-position pill switch (left=hide, center=default, right=show) matching existing toggle style

### 8.5 Auto-Heal
- When a PR on a branch with an active agent terminal becomes blocked, auto-heal hands the problem to the agent:
  - **CI failure** — fetches failure logs and injects them with a fix prompt
  - **Merge conflict** (`mergeable === "CONFLICTING"`) — injects a resolve-conflicts prompt
- Toggle per-branch via pill-switch toggle in PR detail popover (visible when CI is failing or the PR is conflicting), styled consistently with CI check item rows
- Fetches completed failed-job logs directly via the GitHub Actions jobs API and configured CircleCI jobs, including when sibling jobs keep the workflow run in progress; logs are sanitised and truncated before injection
- Waits for agent to be idle/awaiting input before injecting
- Max 3 delivered attempts per block cycle, then stops and logs a warning; log-fetch and terminal-delivery failures do not consume the budget
- Enabling while already blocked kicks off a heal immediately
- Attempt counter visible in PR detail popover
- Status tracked per-branch in `BranchState.ciAutoHeal`

### 8.6 PR Notifications
- Types: Merged, Closed, Conflicts, CI Failed, Changes Requested, Ready
- Toolbar bell with count badge
- Individual dismiss or dismiss all
- Click to open PR detail popover

### 8.7 Merge PR via GitHub API
- Merge PRs directly from TUICommander without switching to GitHub web
- Configurable merge strategy per repo: merge commit, squash, or rebase (Settings > Repository > Worktree tab)
- Merge method auto-detected from repo's allowed methods via GitHub API (`get_repo_merge_methods`); auto-fallback to squash on HTTP 405 rejection. Squash is preferred first when several methods are allowed (`src/utils/prMerge.ts`)
- Merge stays available while CI is still running: `canMergePr` requires the PR to be open, non-draft, approved, and free of *definitively failed* checks — pending checks do not hide the action
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

### 8.8 Auto-Delete Branch on PR Close
- Per-repo setting: Off (default) / Ask / Auto
- Triggered when GitHub polling detects PR merged or closed transition
- If branch has a linked worktree, removes worktree first then deletes branch
- Safety: never deletes default/main branch; dirty worktrees always escalate to ask mode
- Uses safe `git branch -d` (refuses unmerged branches)
- Deduplication prevents double-firing on the same PR

### 8.9 GitHub Issues Panel
- Issues displayed in a collapsible section within the GitHub panel alongside PRs
- Filter modes: Assigned (default), Created, Mentioned, All, Disabled
- Filter persisted in app config (`issue_filter` field) and configurable in Settings > Git & GitHub > Issues
- Each issue shows: number, title, state (OPEN/CLOSED), author, labels, assignees, milestone, comment count, timestamps
- Labels rendered with GitHub-matching colors (background opacity 0.7, contrast-aware text color)
- Issue actions: Open in GitHub, Close/Reopen, Copy issue number
- Expand accordion to see full details (milestone, assignees, labels, timestamps)
- Skeleton loading rows shown during first fetch
- Empty state message when no issues match the filter
- MCP HTTP endpoint: `GET /repo/issues?path=...` returns issues JSON
- MCP HTTP endpoint: `POST /repo/issues/close` closes an issue

### 8.10 GitHub Ops Dashboard
- Dedicated GitHub Ops dashboard tab with live columns for PR review findings, auto-fix sessions, conflict assists, improvement proposals, and CI / merge readiness.
- The review column shows Working, Done with a finding count, or ego's own failure sentence. It deliberately never names a model: one unattended turn is all ego reports.
- Improvement scans run one unattended ego turn over local repo context with focus modes: `refactor`, `testing`, and `perf`. At most five proposals survive one scan.
- Proposals are notification-first: scan results emit `proposals-ready` over desktop events and `/events` SSE; that event is the *only* path that publishes them into the store — the scan's return value is for the caller, not for the panel. No GitHub issue is created automatically.
- Each proposal can be promoted to a GitHub issue only through an explicit user action, using the existing authenticated issue creation path.

### 8.10a Changelog Generation
- Button in the GitHub panel header opens the Changelog modal, which runs one unattended ego turn over the merged PRs since the last tag (#795-320b).
- The result is markdown plus a structured JSON split; the modal renders the markdown and offers Copy and Save (`CHANGELOG-ai.md`). HTTP and MCP callers get both halves.
- Ego answering in prose only is a valid answer — the JSON half is `null` and the markdown still renders. Ego being unreachable shows ego's own sentence, never a blank changelog.

### 8.11 Polling
- Active window: every 30 seconds
- Hidden window: every 2 minutes
- API budget: ~2 calls/min/repo

### 8.12 Token Resolution
- Priority: `GH_TOKEN` env → `GITHUB_TOKEN` env → OAuth keyring token → `gh_token` crate → `gh auth token` CLI
- `gh_token` crate with empty-string bug workaround
- Fallback to `gh auth token` CLI

### 8.13 OAuth Device Flow Login
- One-click GitHub authentication from the Settings > Git & GitHub page
- Uses GitHub OAuth App Device Flow (no client secret, works on desktop)
- Token stored in OS keyring (macOS Keychain, Windows Credential Manager, Linux Secret Service)
- Requested scope: `repo`
- Shows user avatar, login name, and token source after authentication
- Logout removes OAuth token, falls back to env/gh CLI
- On 401: auto-clears invalid OAuth token and prompts re-auth

### 8.14 Multiple Accounts (github.com + GitHub Enterprise)
GitHub integration is **account-centric**: TUICommander can manage N accounts and each workspace repo is explicitly bound to the account that monitors it (a persisted binding, not derived live from `origin`).

**Account kinds**
- **Ambient github.com default** — the account you authenticate with via the OAuth device flow above (or `GH_TOKEN`/`gh` CLI). Behaves exactly as before; a github.com-only user sees zero change.
- **Additional github.com accounts** — extra named github.com logins added via the device flow (Settings → Git & GitHub → *Additional GitHub Accounts* → "Add another github.com account").
- **GitHub Enterprise Server (GHE)** — added by host + a pasted **Personal Access Token** (no per-host OAuth App). Validated against `https://{host}/api/v3/user`; PAT stored in the OS keyring under `github/account/{id}/token`.

**Repository bindings** (Settings → Git & GitHub → *Repository Bindings*)
- Each workspace repo resolves to one of: **Bound** (shows the account + *Unbind*), **NeedsBind** (a candidate chooser — no silent `origin` pick when multiple GitHub remotes/accounts match), **NeedsAccount** (a github.com repo with no account yet → points to setup), or **Unmonitored**.
- A single matching account auto-confirms; ambiguity always asks. Worktrees of a repo share the main checkout's binding.

**Per-account isolation** (hybrid model)
- github.com keeps the global breaker/viewer/rate/cooldown state byte-for-byte; each GHE account gets its own `ghe_state` (circuit breaker, viewer login, rate budget).
- The poller groups active repos by account and runs one batch per account, so a 401 / rate-limit / fault on one account never opens another's breaker or blocks its polling.
- Cooldown keys are account-scoped (`{account_id}:owner/repo` for GHE; `owner/repo` unchanged for cloud); github.com logout clears only cloud cooldowns; removing an account drops only its token, record, bindings, and caches.

**Limitations**
- REST + GraphQL (PRs, CI, issues, merge, approve, issue comments) work against bound GHE repos. `gh`-CLI-assisted CI-failure-log fetching (CI Auto-Heal) is disabled with a clear message for non-github.com accounts.

Backend: `github_account.rs` (`GitHubHost`, account model, binding store, `resolve_repo_account`), commands `github_list_accounts` / `github_add_account` / `github_remove_account` / `github_bind_repo` / `github_unbind_repo` / `github_list_bindings` / `github_resolve_repo`.

---

## 9. Voice Dictation

The `tuic-dictation` Rust crate implements audio, transcription and speech; the desktop application retains the commands, browser transport and event emission.

### 9.1 Whisper Inference
- Local processing via `whisper-rs` (no cloud)
- macOS: GPU-accelerated via Metal
- Linux: CPU (optional CUDA/Vulkan build feature)
- Windows: CPU-only (the whisper.cpp Vulkan backend's shader build is broken on the Windows CI runner; `vulkan` will be re-enabled once stabilized)

### 9.2 Models
| Model | Size | Quality |
|-------|------|---------|
| small | ~488 MB | Good |
| small.en | ~488 MB | Good (English-only) |
| large-v2 | ~3.0 GB | Highest accuracy (slow) |
| large-v3-turbo | ~1.6 GB | Best (recommended, default) |

### 9.3 Push-to-Talk
- Default hotkey: `F5` (configurable, registered globally)
- Mic button in status bar: hold to record, release to transcribe
- On macOS, releasing Fn stops microphone capture in Rust before the WebView handles the event. Losing window focus releases a held hotkey; a late Fn start is refused. The file log records release-to-stop latency without storing spoken text.
- Transcribed text inserts into the focused input element (textarea, input, contenteditable); falls back to active terminal PTY when no text input has focus. Focus target captured at key-press time.

### 9.4 Streaming Transcription
- Real-time partial results during push-to-talk via adaptive sliding windows
- First partial within ~1.5s, subsequent windows grow to 3s for quality
- Streaming skips only all-zero audio windows; the shared RMS and speech-confidence gates reject quiet/no-speech audio. A trailing pause does not discard the preceding speech.
- Floating toast shows partial text above status bar during recording, with a live microphone meter beside the partial text. The level is an RMS reading curved as `sqrt(rms * 20)` and clamped to 0–1 so ordinary speech is visible rather than pinned near zero, published through an atomic so the UI never blocks audio capture
- 200ms audio window overlap (`keep_ms`) carries context across windows for continuity
- Final transcription pass on full captured audio at key release
- Before the final pass, the hands-free 20 ms frame and sustained-duration rule rejects short bursts; push-to-talk uses the configured transcription RMS floor for frame activity so quiet sustained speech still reaches Whisper. Speech still open at key release is kept.
- A skipped final pass shows its specific speech-gate reason in the dictation status; an empty successful pass reports `no speech detected`.
- Hallucination filter (`transcribe.rs`) as the backstop after the RMS gate: quiet audio makes Whisper emit a subtitle credit in whatever language it guessed. Short thanks (`grazie`, `thank you`, `merci`, `danke`, `спасибо`, …) are dropped only when they are the entire transcript, so a dictated sentence containing one survives; channel boilerplate (`amara.org`, `sottotitoli e revisione a cura di`, `thanks for watching`, …) is dropped anywhere in the text. Covers all 11 languages in `WHISPER_LANGUAGES` because the default setting is `auto`

### 9.5 Microphone Permission Detection (macOS)
- On first use, checks microphone permission via macOS TCC (Transparency, Consent, and Control) framework
- Permission states: `NotDetermined` (will prompt), `Authorized`, `Denied`, `Restricted`
- If denied, shows a dialog guiding the user to System Settings > Privacy & Security > Microphone with an "Open Settings" button
- Linux/Windows: always returns `Authorized` (no TCC framework)

### 9.6 Configuration
- Enable/disable, hotkey, language (auto-detect or explicit), model download
- Audio device selection
- Text correction dictionary (e.g., "new line" → `\n`)
- **Auto-send** — Enable in Settings > Voice to automatically submit (press Enter) after transcription completes.
- Spoken-reply voice, hands-free activation phrase and hold-back delay — see 9.11.

### 9.7 Speech Engines and Assets
- **Microsoft Edge voices are the default engine.** No download; the voice list comes from the service, filtered by the dictation language, with a Listen button, and the chosen voice persists. It needs the internet and sends the reply text to Microsoft's speech service (stated in the settings). Offline or a service error shows a clear message in the Voice section, never silence or a hang; a cancelled or over-budget reply stops promptly.
- **Expert > Speech engine** switches to Pocket TTS (local) or an external command. A fresh install gets Edge; an installation with a chosen Pocket voice, the Pocket runtime on disk or a configured speech command keeps that engine. Echo cancellation, hush, volume, levelling and the voice choice apply to all engines.
- The rest of this section describes the Pocket engine's assets.
- Spoken replies need two downloads beside the Whisper models: a **language bundle** (ONNX graphs, tokenizer, voice — about 130 MB, French about 390 MB) and **ONNX Runtime** itself (42 MB). Neither ships with the app.
- Bundled languages: English, French, German, Italian, Portuguese and Spanish, each with the voice Kyutai lists as native to it (alba, estelle, juergen, giovanni, rafael, lola). French exists upstream only as the larger 24-layer model.
- The catalogue is an **allowlist compiled into the binary**, not a fetched manifest: every file is pinned to an immutable upstream revision and to a sha256, and a download whose bytes do not match that hash is refused instead of installed.
- Downloads report progress, can be cancelled, and install **atomically** — the bytes land in a staging directory, are verified there, and only then replace the installed version. An interrupted or corrupt download never appears ready.
- Three states, told apart on purpose: `absent` offers a download, `incomplete` names the files that are missing, `ready` is the only one a language can be spoken from.
- Replacing or deleting a language waits for any reply being spoken to finish, then unloads its graphs. The 125 MB an engine holds resident is released on deletion and at shutdown.
- **Every voice.** Each language has 26 Kyutai voices. One ships in the language bundle; the other 25 are separate downloads (about 6 MB each, 25 MB for French) that the user chooses in Settings > Voice > Voices, grouped as Installed, Downloadable and Yours.
- **Your own voice files.** "Add voice file…" imports a `.safetensors` voice into the current language. The file is checked against that language's model before it is stored, and a file that does not fit is refused with the reason. TUICommander does not create or clone voices.
- **Listen** plays a short sample of the selected voice on this machine's speaker, without a hands-free conversation and without changing the saved voice. It is refused while a hands-free reply is being spoken.
- Sources and licensing: the graphs come from a public Hugging Face export of Kyutai's Pocket TTS weights; the voices come from Kyutai's ungated repository without the voice-cloning weights. Both are CC-BY-4.0, attributed in `THIRD_PARTY_NOTICES.md`.

### 9.8 Spoken Replies
- **Spoken replies** defaults on and can be disabled from Voice Settings, the mobile conversation header or mobile Settings. Off keeps dictation armed, cancels the speech queue, and enforces text replies server-side. Edge 401/403 rejections start a five-minute cool-down across conversations.
- Arming hands-free also opens a **reply queue** for that conversation, so the model can answer out loud. Arming without a working voice still works — that is ordinary dictation — and the reason is reported rather than discovered on the first reply.
- **The user can talk over a reply.** Speaking stops the moment the capture loop hears them, and their words land in the new turn rather than behind the sentence they interrupted. Echo cancellation runs first, so a reply never interrupts itself on its own voice coming back through the microphone.
- **Accepting a reply is not the user hearing it.** Every reply gets an id and one of six fates: `queued`, `rendering`, `speaking`, `finished`, `interrupted` or `failed`. Only `finished` means somebody heard it to the end, and only the audio device going quiet can produce it.
- A reply written for a turn the user has already talked over is **refused, not spoken** over whatever they said next.
- **Every reply is at one level.** A loudness stage levels each reply, brings it to the **Voice volume** (-30 to -12 dB, default -18) and limits the peaks to -1 dBFS, so no slider position clips. **Levelling** (Off to Strong, default 67%) sets how much the quiet and loud words of one reply are evened out. A change applies to the next reply and does not stop speech.
- Speech belongs to a conversation, not to the application. A model reaches it through the `voice` MCP tool and may drive only the conversation armed for its own terminal; a connection bound to no terminal is refused rather than allowed to speak into whichever conversation happens to be armed.
- The `voice` tool is always listed, on every build, and answers `available: false` with a reason when it cannot speak — discovery does not depend on tool-list change notifications, which not every client acts on.

### 9.9 One language, end to end
- **The dictation language decides everything**: what Whisper transcribes, which language the model is told to answer in, and which voice speaks the answer. There is no separate speech language and the model cannot override it.
- **A spoken turn is typed into the agent at once, even while it is working** — the same as a line you type by hand into a busy agent, which the agent queues or takes mid-turn itself. It never waits in the Compose queue. Only an open permission dialog or text you are typing in the terminal holds it; it stays in the hands-free panel and is typed the moment they are gone, joined with anything you said meanwhile.
- Every hands-free turn reaches the model as `<what you said> (reply in <Language>)`. It is part of the typed entry, so turning optional hints off does not remove it.
- With **Auto**, the language is the one Whisper actually detected, and it is shown. Before the first turn there is none — spoken replies are unavailable and say so, rather than falling back to English.
- A language TUICommander transcribes but ships no voice for is **named, never substituted**: the status says which bundle is missing instead of answering in a language the user is not speaking.
- **Which voice** speaks is a setting; which *language* speaks is not. A language that ships several voices offers them in the Dictation panel; left untouched, the setting means "the first voice this language ships". A named voice the language does not ship is refused with the list of the ones it does, rather than replaced by a voice nobody chose.
- Changing the language or the voice **stops the reply written for the previous one** mid-sentence — a sentence half said in one voice does not finish in another. Changing any other dictation setting leaves the voice alone.

### 9.10 The model is told when you start and stop talking
- **Notify model when hands-free changes** (on by default): arming tells the agent that what arrives from now on was spoken and that it can answer out loud; disarming tells it to go back to text.
- Both notices are typed like a spoken turn — at once, even into a busy agent — and land in the terminal the conversation was armed for, not in whatever tab you focused since. A start notice held by a dialog goes before any spoken turn; a stop notice that meets a dialog is dropped, because the conversation it belonged to is already over.
- Change your mind before the agent reads the start notice and it is **withdrawn**, with no stop notice behind it. What the agent ends up holding always matches the mode you are actually in.
- **Your own start notice**: replace the text the agent reads at the start with your own instructions (for example a reply language or a length limit). Leave it empty, or press **Reset to default**, to send the built-in text. A notice written on several lines is sent as one line.
- Turning the option off silences both notices and **nothing else**: disarming still stops the voice, drops queued audio and refuses late replies.
- **Push-to-talk is untouched.** Holding the hotkey never starts continuous listening, never uses the activation phrase, never makes speech available and never notifies anyone.

### 9.11 Driving it from Settings > Voice
- **One titled section per job, speech-to-text and text-to-speech kept apart:** Dictation (enable, hotkey, auto-send), Speech recognition (input device, Whisper model, language, voice tuning), Auto-Corrections, Hands-free conversation, Spoken replies. Each section holds its own advanced settings; there is no shared "Advanced" section.
- **Spoken replies** lists the speech languages and the ONNX runtime with their state, size and Download / Repair / Cancel / delete, plus the voice to speak with. There is **no** speech-language control — the language is the Whisper one, and a second control would be a second source that disagrees with it.
- **Conversation controls** — Start conversation is an accent action, Stop is distinct, and a coloured Running/Stopped indicator accompanies the backend phase.
- **Hands-free conversation** picks the terminal to talk to, starts and stops the mode, and shows the live phase (waiting, capturing, transcribing, holding back, delivered), the turn being held back, what is being spoken and any error. It also holds the activation phrase and the hold-back delay.
- **Breath and continuation:** 1.5 s of quiet closes an utterance. With an activation phrase, a turn waits at least 5 s for speech that continues without repeating the phrase; the displayed pending text grows before it reaches the agent as one message.
- **Earcons** — an 80 ms blip when a spoken turn reaches the agent, and a softer, lower one when the activation phrase drops a turn, so the user knows without looking. Web Audio on the desktop and in a browser alike, played only by the client whose microphone holds the conversation. Too short for the capture VAD to take as speech (`min_speech_ms` is 200 ms). On by default; the `hands_free_earcons` dictation setting turns them off.
- **Opening the panel never opens the microphone**, and neither does starting the app. Nothing arms by itself; the mode starts only when you press Start (here or in the Command Palette) or say the activation phrase in a conversation you already armed.

### 9.12 Starting it from the Command Palette
- **"Start hands-free conversation" / "Stop hands-free conversation"** — one entry (`toggle-hands-free`, category Dictation), labelled for what it will do. Starting binds the **active** terminal; an active tab with no live session starts nothing. Hidden on the desktop while dictation is disabled; available in browser mode, where it uses the tab's own microphone.
- Unbound by default; a user shortcut for `toggle-hands-free` runs the same action.
- One function (`src/actions/handsFreeConversation.ts`) reuses the store's arm/disarm, so the audio owner and earcon priming are the same as in Settings.
- **Push-to-talk and continuous listening are two mechanisms, not a mode switch.** Push-to-talk is the hotkey; continuous is Start. There is no setting that flips between them.
- **The dictation hotkey is also the stop control.** Pressed while a conversation is running, it ends the whole thing — capture, the transcript waiting out its hold-back or held by a dialog, and whatever is being spoken. Turns already typed stay in the terminal. Pressed otherwise it records as usual.
- **Hands-free works in a browser too, on the browser's own hardware.** A tab opens its microphone and speaker and streams them to TUICommander, so a conversation started from a laptop is heard and answered *there*. The two sides never mix: arming from a browser cannot open the microphone on the machine running TUICommander, and arming from the desktop cannot take audio from a browser. A tab whose owner has no audio connection is refused rather than quietly served the server's hardware, and closing the tab ends the conversation it held. The global dictation hotkey and the microphone-device list stay desktop-only — a browser cannot register a system hotkey, and that device list belongs to the other machine.

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

AI automation layer with 29 built-in context-aware prompts. Each prompt includes a description explaining what it does. Prompts auto-resolve git context variables and execute via inject (PTY write), shell script (direct run), headless (one-shot subprocess), or API (direct LLM call) mode.

- **Open**: `Cmd+K` or toolbar lightning bolt button
- Drawer with category filtering (All/Custom/Recent/Favorites), search by name/description, and enable/disable toggles
- Prompt rows show inline badges: execution mode (inject/shell/headless/api), built-in, placement tags
- Prompts are context-aware: 31 variables auto-resolved from git, GitHub, and terminal state
- **Variable Input Dialog**: unresolved variables show a compact form with variable name + description before execution
- **Edit Prompt dialog**: full editor with name, description, content textarea, variable insertion dropdown (grouped by Git/GitHub/Terminal with descriptions), placement checkboxes, execution mode, auto-execute, and keyboard shortcut capture
- **Inject target**: when a prompt is not submitted immediately, the target selects the review surface: the **Compose box** (default) or editable text in the **Terminal**
- **Auto-execute**: when enabled, a prompt submits exactly once through agent-aware `sendCommand`, regardless of its review target. When disabled, it remains editable. Explicit **Insert** and **Insert & Run** actions override the saved setting.
- **API execution mode runs one unattended ego turn (#787-ee50)**: it used to call LLM providers directly over HTTP from TUICommander; that client, its provider registry and its keyring entry were deleted with the embedded AI engine (#784-0aec). It now goes through `acp_one_shot_prompt` — launch the configured ego, one `session/new` with **no MCP server**, one prompt, shut down — and the turn's final text goes to the prompt's `outputTarget`. Nothing streams: a Smart Prompt runs with no panel open. A `headless` prompt whose resolved agent is `api` takes the same one path. Every permission request and elicitation is declined the instant it arrives, because a seat nobody takes is a turn that never ends; the count comes back as `declined` so an empty answer caused by a refused tool is reported as that rather than as an empty answer. The turn is abandoned after 300s
- **No LLM API config**: there is no global provider/model/API key in `Settings > Agents` any more, and TUICommander stores no API key. The model an `api` prompt runs on is ego's default, set in `Settings > AI Chat` (786-4a6d). With no ego binary named, the mode refuses and names `Settings > General` for the binary and `Settings > AI Chat` for the model

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

- **Inject** (default): routes the resolved prompt text to the active terminal. **Auto-execute** decides whether the action submits. Submissions are idle-gated (configurable via `requiresIdle`) and use agent-aware Enter semantics. Review-only actions are not idle-gated; the **Target** sub-option places their editable text in the Compose box (default) or directly in the terminal input. If the Compose box is unavailable, the terminal input is the fallback.
- **Shell script**: executes the prompt content directly as a shell script via `execute_shell_script` Tauri command. No agent involved — runs content as-is via `sh -c` (macOS/Linux) or `cmd /C` (Windows) in the repo directory. Output routed via `outputTarget`. 60-second timeout cap. No prerequisites (no terminal, agent, or API config needed)
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
- A normal click or Enter follows the saved auto-execute setting; double-click and **Insert & Run** force one submission, while **Insert** always keeps the result editable
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

The navigation groups the global pages by task. Each group is a static label row above its pages:

| Group | Pages |
|-------|-------|
| Application | General (**11.1**), Appearance (**11.2**), Notifications (**11.5**) |
| Workspace | Terminal (**11.9**), Keyboard Shortcuts (**11.6**), Git & GitHub (**11.10**) |
| AI | Agents (**11.7**), AI Chat (**11.8**), Voice (section 9), Smart Prompts (**10.5**) |
| Integrations | MCP (**11.3**), Remote Access (**11.3**), Remote Machines (**11.3**), Plugins (**17.2**) |
| Repositories | One page per configured repository (**11.4**) |

### 11.0 Search
- Search box at the top of the page list; filters every setting across every page at once
- Each result shows the setting name and its `Page › Section` trail; selecting one opens that page and scrolls to the field
- Repository pages are not indexed — a global box cannot know which repository a query means
- Settings the current build does not render (the AI Chat page while Experimental Features is off, for example) report no match instead of opening an absent page
- A result inside an expert setting shows an **Expert** badge; opening it reveals that setting (see **11.11**)
- The index is committed, not scanned from the DOM: only one page mounts at a time, and mounting the rest would fire CLI status, mdkb status, GitHub and audio probes on every keystroke. A drift test re-derives it from the sources, so a setting added without indexing fails CI

### 11.1 General
- Language: the locales that ship a message catalog, each named in its own language. The pick persists to `config.json` and re-renders every translated string without a reload. Locales with no catalog are not listed — they would render English while claiming to be translated — so the list holds only English until more catalogs land
- Show agent context bar
- Confirmations: quit, close tab (only when a process is running — agents or busy shell; idle shells close immediately)
- Power management: prevent sleep when busy, auto-standby timeout, content indexing strategy
- Updates: auto-check, update channel, check now
- TUIC CLI install/uninstall (see **21**), Code Intelligence (MDKB, see **14.8**)
- ego executable (**ego** section, after Code Intelligence): the ego binary the AI Chat panel talks to over ACP. Desktop: status line with Select…/Clear; browser: a text field. Always shown, also while Experimental Features is off
- Default IDE, custom launchers (see **4.5**)
- Experimental Features: one master toggle, no sub-flags. It opts in to the AI Chat panel, the **AI Chat** settings page (**11.8**) and SSH Tunnels. The three AI sub-flags went with the embedded engine (#784-0aec)

### 11.2 Appearance
- Split tab mode: separate / unified
- Tab ordering mode: grouped-by-type (default, tabs grouped by kind), terminals-first (terminals left, others freely interleaved), free (any tab anywhere)
- Cycle all tab types, nested terminal tabs
- Max tab name length: 10-60 slider
- Repository groups: create, rename, delete, color-coded
- Layout: reset panel sizes (restore sidebar and panel widths to defaults)
- UI legend: reference for colors, symbols and badges

### 11.3 MCP, Remote Access and Remote Machines
Three pages under **Integrations**. They were one "Services & MCP" tab; each page now mounts only its own content, and the MCP and Remote Access pages share one status poll.
- **MCP** — HTTP API server: always active on IPC listener (Unix domain socket on macOS/Linux, named pipe `\\.\pipe\tuicommander-mcp` on Windows). TCP port only for remote access
- **MCP** — MCP connection info: bridge sidecar auto-installs configs for supported agents (Claude Code, Cursor, etc.)
- **MCP** — TUIC native tool toggles: enable/disable individual MCP tools (`session`, `agent`, `task`, `remote`, `repo`, `ui`, `plugin_dev_guide`, `config`, `debug`) to restrict what AI agents can access
- **MCP** — Upstream MCP Servers: add/edit/remove upstream MCP servers (HTTP or stdio with optional `cwd`), per-upstream enable/disable, reconnect, credential storage via OS keyring, live status dots, tool count and metrics. Saved upstreams auto-connect on boot. The MCP popup's "Manage in Settings" opens this page at this section
- MCP Per-Repo Scoping: each repo can define which upstream MCP servers are relevant via an allowlist in repo settings (3-layer: per-repo > `.tuic.json` > defaults). Null/empty allowlist = all servers. Quick toggle via **Cmd+Shift+M** popup
- **Remote Access** — port, username, password (bcrypt hash), URL display, QR code, token duration, IPv6 dual-stack, Tailscale HTTPS, cloud relay
- **Remote Machines** — `tuic-remote` connections over SSH or a direct URL; the page lists SSH hosts discovered from `~/.ssh/config` and `known_hosts` (hashed entries are only counted), shows the loaded agent keys, probes hosts on demand and prefills the Add form from a click
- Voice dictation has its own **Voice** page (section 9)

### 11.4 Repository Settings (per-repo)
- Display name
- Worktree tab: storage strategy, prompt on create, delete branch on remove, auto-archive, orphan cleanup, PR merge strategy, after-merge action (each overridable from global defaults)
- Scripts tab: setup script (post-worktree), run script (`Cmd+R`), archive script (pre-archive/delete hook), and optional local development-server URL for Design Mode
- Repo-local config: `.tuic.json` in repo root provides team-shared settings. Three-tier precedence: `.tuic.json` > per-repo app settings > global defaults. **Scripts (setup, run, archive) are intentionally excluded from `.tuic.json` merging** — arbitrary script execution by a checked-in file poses a security risk; scripts are always sourced from the local per-repo app settings only

### 11.5 Notifications
- Master toggle, volume (0-100%)
- Per-event: question, error, completed, warning, info
- Test buttons per sound
- Reset to defaults
- **Keep toasts in the bell** — mirrors toasts into the bell's Messages section (see **4.4**). Outside the audio block, because the bell is visual and must stay configurable without an audio device

### 11.6 Keyboard Shortcuts
- Settings > Keyboard Shortcuts page (`Cmd+,` to open Settings), also accessible from Help > Keyboard Shortcuts (same editor)
- All app actions listed with their current keybinding
- Click the pencil icon to rebind — inline key recorder with pulsing accent border
- Conflict detection: warns when the new combo is already bound to another action, with option to replace
- Overridden shortcuts highlighted with accent color; per-shortcut reset icon to revert to default
- "Reset all to defaults" button at the bottom
- Custom bindings stored in `keybindings.json` in the platform config directory
- Auto-populated from `actionRegistry.ts` (`ACTION_META` map) — new actions appear automatically
- **Global Hotkey:** configurable OS-level shortcut to toggle window visibility from any application. Set in the "Global Hotkey" section at the top of the Keyboard Shortcuts page. No default — user must configure. Toggle: hidden/minimized → show+focus, visible but unfocused → focus, focused → instant hide (no dock animation). Cmd and Ctrl are distinct modifiers. Uses `tauri-plugin-global-shortcut` (no Accessibility permission required on macOS). Hidden in browser/PWA mode.

### 11.7 Agents
- See **6.9 Agent Configuration** for full details
- Claude Usage Dashboard enable/disable toggle (under Claude agent section)

### 11.8 AI Chat
ego's own configuration; the `ego_executable` path and the `ai_chat_workspace` folder (AI Chat workspace, empty = home directory, absolute paths only) are on General (**11.1**). Shown only while Experimental Features is on, because that flag is what offers the AI Chat panel — the one place `ego` is reachable from.
- Reads and writes **ego's** configuration by running ego: `config ls --json`, `models --json`, `doctor --json`, and `config set model="<slug>"`. All three reads must succeed, so the page is never a partial picture
- Default model: a picker over every model ego knows, grouped by provider, with unavailable models disabled. A write is followed by a fresh read, so what is shown is what ego persisted. It survives a restart because ego holds it, not TUICommander
- Refresh from providers: `ego models --refresh`, the only action in TUICommander that reaches a provider over the network — and it is ego that reaches it. Opt-in; opening the page does not
- Per-provider rows: how many models are usable, and ego's own words for why the rest are not (once per distinct reason)
- Credential state from `ego doctor`: stored, expired (ego renews it on its next run), missing, or "could not read the store" — which is deliberately not the same as an empty store
- **No API key enters TUICommander**: none is stored, none reaches the OS keyring, and no provider HTTP call is made from this process. `ego auth login <provider>` runs in a terminal tab opened by the provider's Login button, typed as a command and answered by the person in the terminal, so the secret never passes through TUICommander
- `model` is the only writable key, exposed as its own operation rather than a key/value pair, so no caller over IPC or HTTP can reach `sandbox` or `permissions.judge`
- Four failure states, each distinct: ego not configured (names the field to fill), a configured path that will not start, an ego command that failed (shown with the command, exit code and its verbatim output), and a transport fault that is not attributed to ego

### 11.9 Terminal
- Theme: terminal and app color theme, color swatches. Bundled themes include **Commander** (default and fallback for an unknown key), **Paper** (key `vscode-light`, the light theme), **Minimal Kiwi** (dark green-tinted background with muted warm accents) and **Ink** (key `clean`; Orca-style neutral dark: `#0a0a0a` canvas, `#171717` surfaces, white accent, Ghostty ANSI palette, neutral tab-type tints keyed on `html[data-theme]`). The UI uses `antialiased` font smoothing with 0.01em tracking on every theme
- Shell
- Terminal font: bundled monospace fonts (JetBrains Mono default), default font size (8-32px slider), font weight, cursor style (bar / block / underline)
- Copy-on-select toggle (auto-copy selection to clipboard), OSC 52 clipboard writes, block timestamps (elapsed-time label per command block while Ctrl+Cmd is held), block folding (gates the Toggle Block Fold shortcut and its palette entry), scrollbar marks, scrollback reflow on resize

### 11.10 Git & GitHub
- GitHub authentication (see **8.13**), additional accounts and repository bindings (see **8.14**)
- Pull requests: auto-show PR popover, hide draft / conflicting / CI-failing PRs, auto-delete on PR close
- Issues: show issues, issue filter
- Repository defaults: base branch, copy ignored files, copy untracked files (two separate toggles), setup/run/archive scripts
- Worktree defaults: storage strategy, prompt on create, delete branch on remove, auto-archive, orphan cleanup, PR merge strategy, after-merge behavior, auto-fetch interval

### 11.11 Expert Mode
- **Expert** switch in the Settings header; persisted as the UI pref `settings_expert_mode` (default off)
- An expert setting is hidden in basic mode only while its value equals the config default. A modified value always stays visible, so an override is never hidden
- A setting the user edits stays shown until Settings is opened again, also when it is set back to the default in the same open. A value that differs only until the page loads its config is not an edit
- Defaults come from the read-only `get_config_defaults` command (`GET /config/defaults`). While they are unknown, or a lookup fails, the setting stays visible
- A search result for an expert setting carries an **Expert** badge; opening it opens the page, reveals the setting and scrolls to it. The reveal lasts until Settings is opened again and does not change the switch
- Expert settings per page: General (auto-standby timeout, content indexing, update channel); Notifications (master volume, audio output device); Terminal (shell, font weight, OSC 52 clipboard, block folding, scrollbar marks, scrollback reflow); Git & GitHub (auto-delete on PR close, copy ignored files, copy untracked files, storage strategy, auto-archive merged, orphan cleanup, after-merge behavior, auto-fetch interval, the "Add another GitHub account" button while no additional account exists); Agents (collect project progress, and per agent idle-close delay, auto-retry, native status signals, install hooks globally, track intent, collect progress, suggested follow-ups, headless command template, Claude environment flags); Voice (long-press threshold, auto-send, input device, level gate, speech confidence gate, hold-back, notify model on hands-free, start notice); MCP (collapse tools); Remote Access (port, session token duration, IPv6). Full table: [Settings → Expert Mode](user-guide/settings.md#expert-mode)
- Every section keeps at least one basic setting, so no heading and no page hides in basic mode

---

## 12. Persistence

### 12.1 Rust Config Backend
All data persisted to platform config directory via Rust:
- `app_config.json` — general settings
- `notification_config.json` — sound settings
- `ui_prefs.json` — sidebar visibility/width
- `repo_settings.json` — per-repo worktree/script settings
- `repositories.json` — repository list, groups, branches (shared by debug and
  release builds, like every other file here)
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

MCP `session output` reports continuation instructions for retained output pages.
Text pages use scrollback positions; raw pages use source-byte positions and mask
secrets before slicing. Local HTTP and remote MCP retain the same paging contract.
Buffer eviction is reported; it does not create another output store.

- REST API on localhost for external tool integration
- Exposes terminal sessions, git operations, agent spawning
- WebSocket streaming, Streamable HTTP transport
- Used by Claude Code, Cursor, and other tools via MCP protocol
- `tuic-bridge` ships as a Tauri sidecar; auto-installs MCP configs on first launch for Claude Code, Cursor, Windsurf, VS Code, Codex, Grok, opencode, Droid, goose and pi — but only for the ones actually installed on the machine. Zed, Amp and Gemini keep MCP inside their general `settings.json` and wait for an explicit install. JSON configs are edited member-by-member, never reserialized, so comments, key order and indentation survive (see [MCP auto-install](backend/config.md#mcp-bridge-auto-install))
- Local connections use Unix domain socket (`<config_dir>/mcp.sock`) on macOS/Linux or named pipe (`\\.\pipe\tuicommander-mcp`) on Windows; TCP port reserved for remote access only
- Unix socket lifecycle is crash-safe: RAII guard removes the socket file on `Drop`; bind retries 3× (×100 ms) removing any stale file before each attempt; liveness check uses a real `connect()` probe so a dead socket from a crashed run never blocks MCP tool loading

### 14.7 Cross-Repo Knowledge Base
- Knowledge base functionality is available via the `mdkb` MCP upstream server (configure in MCP Upstreams settings)
- Provides hybrid BM25 + semantic search across docs, code, symbols, and memory
- Call graph queries (calls, callers, impact analysis) via `code_graph` tool
- Requires `mdkb` binary on PATH (installed separately)

### 14.8 Code Intelligence (MDKB integration)
- Go-to-definition: Cmd+Click on symbols in the editor navigates to the definition via `mdkb_goto_definition`. Holding Cmd (macOS) / Ctrl underlines the symbol under the cursor (`cm-hover-link`) as a click affordance; the underline clears on release or when the pointer leaves the editor, and its position is remapped through edits so it never goes stale
- Find references: Shift+F12 finds all callers of a symbol via `mdkb_references` (uses code_graph callers query)
- Symbol outline: file-level symbol tree via `mdkb_outline` (functions, types, structs)
- Install/uninstall managed from Settings → General → Code Intelligence
- `is_available()` checks binary existence on disk (not cached path) — survives external uninstalls
- The daemon ping version must match the installed binary; an older detached daemon is restarted automatically after upgrades
- Homebrew-managed installs show `brew uninstall mdkb` guidance instead of silent failure
- Graceful fallback: all commands return empty results when mdkb is unavailable
- A daemon that goes silent cannot wedge the app: every request/response exchange is bounded (10 s), the liveness probe runs on a shorter 2 s leash, and a connection cut short by a deadline is abandoned rather than reused — a half-read stream would answer the next question with the previous reply
- The shared daemon lock is held only either side of a query, never across it, so one stalled call cannot make unrelated Code Intelligence calls wait out its deadline. Daemon startup stays exclusive on purpose, so two callers never race two spawns

### 14.9 macOS Dock Badge
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
| `Cmd+Ctrl+Backspace` | Return to last terminal — toggles back to the previously focused terminal, switching repo/branch if needed (`focus-last-terminal`) |
| `Cmd+Alt+←` / `Cmd+Alt+→` | Terminal history back / forward — walks the last 10 activated terminals across repos/branches, browser-style: stepping does not reorder the history and activating another terminal drops the forward part; closed terminals leave it. The toolbar has matching arrow buttons (disabled at the ends, tooltip names the target). The Mobile Sessions screen has a sort button (Default / Recent) next to the search lens; Recent orders by the sessions last opened on that device (`history-back`, `history-forward`) |
| `Cmd+U` | Jump to next waiting terminal — cycles to the next terminal awaiting input (agent question/error) across all repos/branches, switching context as needed; does nothing if none are waiting (`jump-waiting-terminal`) |
| `Cmd+L` | Clear terminal |
| `Cmd+Shift+L` | Refresh terminal (fix glyphs) |
| `Cmd+C` | Copy selection |
| `Cmd+V` | Paste to terminal |
| `Cmd+Home` | Scroll to top |
| `Cmd+End` | Scroll to bottom |
| `Shift+PageUp` | Scroll one page up |
| `Shift+PageDown` | Scroll one page down |
| `Cmd+R` | Run saved command |
| `Cmd+Shift+R` | Edit and run command |
| `Cmd+Shift+.` | Toggle block folding |
| `Cmd+Shift+Up` | Jump to previous block |
| `Cmd+Shift+Down` | Jump to next block |
| `Cmd+Shift+B` | Toggle block-scoped search |

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
| `Cmd+Alt+Enter` | Focus mode (hide sidebar, tab bar, panels) |

### AI
| Shortcut | Action |
|----------|--------|
| `Cmd+Alt+A` | Toggle AI Chat panel (`toggle-ai-chat`) |
| `Ctrl+S` (AI Chat composer) | Park, restore, or swap the current prompt draft |
| `Cmd+Enter` (panel focused) | Send message |
| `Esc` (panel focused) | Cancel in-flight stream |

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
| `Cmd+,` | Open settings |
| `Cmd+?` | Toggle help panel |
| `Cmd+Shift+K` | Prompt library |
| `Cmd+J` | Task queue |
| `Cmd+Shift+E` | Error log |
| `Cmd+Shift+W` | Worktree manager |
| `Cmd+Shift+A` | Activity dashboard |
| `Cmd+Shift+M` | MCP servers popup (per-repo) |
| `Cmd+I` | Toggle compose panel |
| `Cmd+Alt+L` | Toggle outline panel |

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
| `Cmd+=` / `Cmd+-` / `Cmd+0` | Zoom editor text in, out, or reset |
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

### Recording a Custom Combo
Every shortcut above is rebindable from Help > Keyboard Shortcuts (see
[`docs/user-guide/keyboard-shortcuts.md`](user-guide/keyboard-shortcuts.md)).
On macOS, `Ctrl+Tab` and `F13`–`F20` never reach the WebView — AppKit consumes the
first for native tab cycling and simply does not forward the rest — so a
`keydown` listener sees nothing. `src-tauri/src/native_keys.rs` installs a single
`NSEvent` monitor that catches both and re-emits them (`ctrl-tab`,
`native-key-down`), which is what makes `F13`–`F20` recordable for both per-action
shortcuts and the Global Hotkey. Keys macOS itself claims before the process
(`F14`/`F15` keyboard illumination) still need remapping in System Settings.

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
- Capability-gated access: `pty:write`, `pty:read`, `ui:markdown`, `ui:sound`, `ui:panel`, `ui:ticker`, `ui:context-menu`, `ui:sidebar`, `ui:file-icons`, `ui:file-preview`, `net:http`, `credentials:read`, `invoke:read_file`, `invoke:list_markdown_files`, `fs:read`, `fs:list`, `fs:watch`, `fs:write`, `fs:rename`, `fs:scan`, `fs:delete`, `exec:cli`, `git:read`
- CLI execution API: sandboxed execution of whitelisted CLI binaries (`mdkb`) with timeout and size limits
- Filesystem API: sandboxed text read, base64 binary read, write, rename, list, tail-read, and watch operations restricted to `$HOME`; binary reads keep a 10 MiB default and accept a per-call budget capped at 512 MiB
- HTTP API: outbound requests scoped to manifest-declared URL patterns (SSRF prevention)
- Credential API: cross-platform credential reading (macOS Keychain, Linux/Windows JSON file) with user consent
- Panel API: rich HTML panels in sandboxed iframes (`sandbox="allow-scripts allow-same-origin"`) with structured message bridge (`onMessage`/`send`), transferable buffer ownership, and automatic CSS theme variable injection
- Hidden plugin and URL tab iframes unload and reload when shown again, preventing background pages from blocking terminal input on the shared WebContent main thread
- Explicit `http`/`https`/`mailto` clicks in inline plugin panels and HTML previews open outside the app; blocked navigation from cross-origin dashboard frames shows a toast directing users to the tab's Open in Browser action
- Shared ticker system: `setTicker`/`clearTicker` API with source labels, priority tiers (low <10, normal 10-99, urgent >=100), counter badge, click-to-cycle, right-click popover
- Agent-scoped plugins: `agentTypes` manifest field restricts output watchers and structured events to terminals running specific agents (e.g. `["claude"]`)
- Output watchers match in Rust on the PTY reader thread: the frontend pushes its pattern set (`set_plugin_output_watchers`), Rust assembles and cleans the lines, and the WebView is only woken for a line that matched. Rust is the only line assembler, so a watcher that registers mid-line still sees that line whole. A pattern the Rust `regex` crate cannot express (lookaround, backreferences) is reported back and keeps matching in the WebView, which then receives every line
- Watcher sets are per client (max 8): a desktop window and a browser tab keep independent sets, and browser/PWA clients receive watcher matches as well
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
- `docx-preview` plugin: previews Word `.docx`/`.dotx` files as clean HTML using Mammoth.js
- `xlsx-preview` plugin: previews Excel `.xlsx`/`.xlsm`/`.xlsb`/`.xls` and OpenDocument `.ods` spreadsheets as sortable per-sheet tables using SheetJS
- `sqlite-viewer` plugin: opens `.db`, `.sqlite`, `.sqlite3`, `.db3`, and `.s3db` files in a self-contained sql.js/WebAssembly viewer with schema and index browsing, per-column filters, sorting, native pagination, bounded read-only queries, visual query plans, CSV copy, and explicit atomic inline-edit saves for primary-key tables

### 17.4 Deep Links (`tuic://`)
- `tuic://install-plugin?url=https://...` — Download and install plugin (HTTPS only, confirmation dialog)
- `tuic://open-repo?path=/path` — Activate a repo already in the sidebar; a folder that is not in it yet is added after one confirmation (this is what `tuic <dir>` sends)
- `tuic://settings?tab=plugins` — Open Settings to specific tab
- `tuic://open/<path>` — Open a Markdown file in a native tab. MCP `ui action=tab` also accepts absolute Markdown paths outside registered repositories; the iframe SDK keeps its repository path validation.
- Focused absolute `tuic://open`/`tuic://edit` targets switch to their owning registered repository so the native file tab remains visible; background opens preserve the current repository
- MCP `ui action=tab` tabs, including external native files and HTML/URL previews, bind to the calling session's registered repository when known. Focused opens switch there so the tab stays reachable; background opens preserve the current view. Unpinned tabs hide in other repositories and return when their repository is selected; pinned tabs remain visible across repositories.
- Native MCP file tabs use the supplied `id` for identity: different ids remain separate even when they target the same path, and repeating an id updates that tab. Tabs remain visible when another terminal is selected in the same repository, including before a workspace is chosen.
- `tuic://terminal?repo=<path>` — Open terminal in repo (iframe SDK only)
- **`tuic://cmd/{tool}/{action}?{params}`** — MCP gateway for external automation (scripts, Shortcuts, browser pages). Routes to the same tool/action handlers as the MCP server. Gating is default-deny:
  - **Read-only / notify actions** (e.g. `session/list`, `session/status`, `repo/list`, `agent/inbox`, `ui/toast`) run silently without a dialog
  - **Destructive or unknown actions** (anything not in the safe list) require a confirmation dialog before executing — prevents a malicious page from acting unattended
  - **`config/save` and `debug/invoke_js`** are blocked entirely and never execute even with user confirmation
  Source: `src/deep-link-handler.ts` (`SAFE_COMMANDS`, `BLOCKED_COMMANDS`); Rust backstop: `deep_link_mcp_call` in `src-tauri/src/lib.rs`

### 17.4.1 TUIC SDK (`window.tuic`)
- Injected automatically into every plugin iframe (inline and same-origin URL mode)
- Feature detection: `if (window.tuic)` — `tuic.version` reports SDK version
- **Files:** `tuic.open(path, {pinned?})`, `tuic.edit(path, {line?})`, `tuic.getFile(path): Promise<string>`
- **External URLs:** `tuic.openUrl(url)` accepts `http`, `https`, and `mailto` only
- **Path resolution:** relative paths resolve against active repo; absolute paths match longest repo prefix; `../` traversal outside repo root is blocked
- **Repository:** `tuic.activeRepo()` returns active repo path; `tuic.onRepoChange(cb)` / `tuic.offRepoChange(cb)` for live updates
- **Terminal:** `tuic.terminal(repoPath)` — open terminal in repository
- **UI feedback:** `tuic.toast(title, {message?, level?, sound?})` — native toast notifications with optional sound (info blip, warn double-beep, error descending sweep); `tuic.clipboard(text)` — copy to clipboard from sandboxed iframe
- **Messaging:** `tuic.send(data)` / `tuic.onMessage(cb)` — bidirectional host↔plugin communication; `PanelHandle.send` reaches SDK listeners through the host-message envelope
- **Plugin lifecycle:** panel tabs and bridges close on plugin unload; state observers receive active branch, shell state and awaiting-input transitions
- **HTTP scope:** `net:http` requires a non-empty `allowedUrls` manifest declaration
- **Theme:** `tuic.theme` — current theme as JS object (camelCase CSS vars); `tuic.onThemeChange(cb)` for live updates
- `<a href="tuic://open/...">` and `<a href="tuic://terminal?repo=...">` links intercepted automatically
- `data-pinned` attribute on links sets pinned flag
- Interactive test page: `docs/examples/sdk-test.html` (see `docs/tuic-sdk.md` for launch instructions)

### 17.5 Preinstalled External Plugins
- **Plan Tracker** — Detects agent plan files from structured events and opens them as background tabs
- **Stories Ticker** — Shows the active repository's open story count in the shared status ticker
- Both are seeded once during migration, then remain independently uninstallable and updateable through the plugin catalog

> **Note:** Claude Usage Dashboard was promoted from a plugin to a native SolidJS feature (see section 6.6). It is managed via Settings > Agents > Claude > Usage Dashboard toggle.

### 17.6 Example External Plugins
See `examples/plugins/` for reference implementations:
- `hello-world` — Minimal output watcher example
- `auto-confirm` — Auto-respond to Y/N prompts
- `ci-notifier` — Sound notifications and markdown panels
- `repo-dashboard` — Read-only state and dynamic markdown
- `report-watcher` — Generic report file watcher with markdown viewer
- `claude-status` — Agent-scoped plugin (`agentTypes: ["claude"]`) tracking usage and rate limits
- `wiz-kanban` — Wiz framework plugin: kanban board for managing the workflow of plans, stories, and reviews with drag-and-drop

### 17.7 Claude Wakeup Plugin
Agent-scoped plugin (`agentTypes: ["claude"]`) that wakes Claude Code when it stalls without asking a question. Ships in `plugins/claude-wakeup/`.

- **Idle detection:** After 20 s of shell idle with no pending question, no active sub-tasks, and no choice prompt, sends a verification message to the agent
- **Typing suppression:** Every busy→idle transition resets the idle clock, so keystroke-generated shell-state blips prevent false wakes
- **Done detection (primary):** Watches the busy-cycle duration after a wake — short cycle (<8 s) = agent acknowledged ("done"), long cycle (≥8 s) = agent continued working
- **Done detection (secondary):** OutputWatcher fast-path for agents that emit a clean `done` line
- **Disarm/re-arm:** Disarms after confirmed done; re-arms only when the user gives new input after the disarm timestamp and the agent works >10 s
- **Limits:** Max 3 wakes per stall, max 12 per session lifetime
- **Dashboard:** Markdown stats panel with wake counts, done rate, active session state, and history
- **Pause/Resume:** Via Activity Center toggle (transient, not persisted)
- **Configuration:** `data/config.json` — `idleThresholdMs`, `maxWakes`, `maxWakesEver`, `doneMaxBusyMs`, `checkIntervalMs`, `minBusyDurationMs`, `questionStaleMs`, `pendingTimeoutMs`
- **Capabilities:** `pty:write`, `pty:read`, `ui:ticker`, `ui:markdown`

## 18. Mobile Companion UI

Phone-optimized progressive web app for monitoring AI agents remotely. Separate SolidJS entry point (`src/mobile/`) served by the existing HTTP server at `/mobile`.

### 18.1 Architecture
- Separate Vite entry point (`mobile.html` + `src/mobile/index.tsx`)
- Shares transport layer, stores, and notification manager with desktop
- Sessions opens by default; the five bottom tabs are Sessions, Chat, Files,
  Progress, and Activity. Settings opens from the app-bar overflow.
- Chat opens the cross-repository chat directly. Read ego's streamed ACP
  conversation with shared transcript cards and collapsed activity, answer
  pending interactions, and switch among titled saved conversations.
- Server-side routing: `/mobile/*` → `mobile.html`, everything else → `index.html`
- Session state accumulator enriches `GET /sessions` with question/rate-limit/busy state
- SSE endpoint (`/events`) and WebSocket JSON framing for real-time updates

### 18.2 Sessions Screen
- Hero metrics header: active session count + awaiting input count with large tabular-nums display
- Search button at the top right filters the live session list by name, repository path, worktree path, branch, or agent type; closing search restores the full list
- Waiting sessions appear first; the list preserves order within waiting, idle-agent, and shell groups. Session names keep their original casing, and busy state reads "Working".
- The new-session sheet selects Claude Code, Codex, or Gemini, searches repositories, and opens the created agent session. Its close button and backdrop dismiss the sheet.
- Question banners identify the session and repository; the header counter opens the first waiting session.
- Elevated session cards with agent icon, status badge, project/branch, relative time
- Rich sub-rows per card: agent intent (crosshair icon) or last prompt (speech bubble), current task (gear icon) with inline progress bar, usage limit percentage
- Question state highlighted via inset gold box-shadow
- Pull-to-refresh spinner via touch events
- Loading skeletons during initial data fetch
- Empty state with instructional hint
- Tap card to open session detail

### 18.2.1 Files Screen
- Select a configured repository and browse its directories one level at a time
- Repository paths truncate from the left and reveal the complete path on long press; normal folders precede hidden folders
- Search recursively for files by name or repository-relative path from the tree
- File view and editor keep Back, file name, and Edit or Cancel/Save on one row with 44 px buttons; the editor fills the remaining height and soft-wraps lines
- From a session header's overflow menu, open Files at that session's worktree root, or at the registered repository containing its working directory; Back returns to the still-mounted session and preserves its output and draft
- Markdown paths in session output open here through the desktop terminal-path resolver. Absolute, relative, `file://`, and `tuic://open//` references are supported, including line numbers. Files outside registered repositories are refused with a toast naming the path.
- Show an explicit error when the session has no repository path, no registered repository contains its working directory, or the directory request fails
- Open `.md` files as rendered Markdown using the desktop's shared `ContentRenderer`; other UTF-8 text files remain plain text
- Tap a task-list checkbox (a 44 px target around the box) in a rendered `.md` file to toggle `- [ ]` / `- [x]` / `- [~]` on disk; tap a paragraph, heading or list item to select it and add a comment, saved as the same `<!--tweak:block:...-->` / `item` marker the desktop Markdown tab writes
- Every mobile write (checkbox, comment, Edit → Save) goes through `write_file_if_unchanged`: if the file changed on disk since it was read, nothing is written, the view reloads and a notice asks to repeat the change (an edit draft or comment draft is kept)
- Switch from View to Edit for source changes, then save and return to View
- Refuse files over 1 MB before reading and show a clear message for binary or non-text files

### 18.3 Session Detail Screen
- Mirrored sessions stream through the connected desktop server to their owning daemon, so the phone stays on its HTTPS origin; the session kill action reaches that owner too
- Live output via WebSocket with `format=log` (VT100-extracted clean lines, auto-scrolling, 500-line buffer)
- HTTP and HTTPS links in output open in the phone's external browser; Markdown path controls open the Files editor.
- Source-width prose rows are rejoined before the phone wraps them; short lines, lists, and box-drawing blocks retain their layout
- When a mobile output line wraps, its continuation keeps the line's leading spaces or tabs; unindented prose and horizontally scrolling box-drawing blocks retain their layout
- Semantic colorization: log lines are color-coded by type (info, warning, error, diff +/-, file paths) via `classifyLine()` utility
- Search/filter in output: text search bar filters visible log lines in real time
- Compact 56 px header: desktop agent logo with a state dot, session display name, repository/branch, state, elapsed activity time, tasks count, and an overflow menu. Tapping the name reveals intent and current task in a transient sheet. Tasks opens current work and intent history; overflow Progress opens this session's filtered journal in a bottom sheet. Files, output search, Ideas, quick commands, usage, copy ID, and terminate remain in overflow. The terminal keeps its height because these panels overlay it.
- Error bar (red tint) when `last_error` is set
- Rate-limit bar (orange tint) with live countdown timer (`formatRetryCountdown`)
- Suggest follow-up chips: horizontal scrollable pills from `suggested_actions`, tap to send
- Slash menu overlay: the keybar `/` inserts at the composer selection through the same input path as typing; both sync to the PTY and show only detected agent menu entries with navigation. Surrounding draft text is preserved. Removed commands are hidden, and the close button dismisses the menu. Picking a command fills the input; submitting waits for the agent's Enter gap
- Quick-action chips: Yes, No, y, n, Enter, Ctrl-C
- **TerminalKeybar:** 44 px tall, horizontally scrollable row of `/`, Ctrl+C, Tab, Esc, arrow and Enter keys above the main input, with no visible scrollbar. When the agent is awaiting input, it adds Yes/No quick-reply buttons. The composer input and Send button also have 44 px touch targets. After the session ends, the keybar and composer are disabled and the stale status badge is hidden
- **CLI command widget:** agent-specific quick commands (e.g., `/compact` for Claude Code and `/status` for Codex) accessible via expandable button
- Text command input with 16px font (prevents iOS auto-zoom), `inputmode="text"`
- **Offline retry queue:** `write_pty` calls that fail due to network disconnection are queued and retried when connectivity resumes
- Back navigation to session list
- Header Files button opens the session repository without going through the repository picker

### 18.4 Question Banner
- Persistent overlay when any session has `awaiting_input` state
- Shows agent name, truncated question, Yes/No quick-reply buttons
- Visible on all screens, between top bar and content
- Stacks multiple questions

### 18.5 Activity Feed
- Chronological event feed grouped by time (NOW, EARLIER, TODAY, OLDER)
- Local 24-hour timestamps, minute-scale run durations, and singular block counts
- Hydrates persisted activity from the server when the mobile tab opens; dismissed items stay hidden and current live items remain visible
- Throttled grouping: items snapshot every 10s to prevent constant reordering with multiple active sessions; new items/removals trigger immediate refresh
- Sticky section headers, tap to navigate to session

### 18.6 Session Management
- **Session kill:** swipe or long-press a session card to kill/close the PTY session
- **New session:** create a new PTY session from the sessions screen (optional shell/cwd selection)
- **Progress:** lists projects with journal entries by recent activity, then shows and switches their saved entries without a desktop repository selection
- Long Progress messages collapse to four lines with a More/Less control on mobile

### 18.7 Settings
- Connection status: connectivity indicator with real-time Connected/Disconnected state
- Server URL display
- App and server versions, plus a persistent Dark/Light choice using desktop theme colors and a separate server-side mobile preference
- Notification sound toggle (localStorage-persisted)
- Open Desktop UI link

### 18.8 PWA Support
- Mobile session and AI Chat composers accept camera photos and files from one
  picker. Binary files stream to `.tuic/attachments/` in the session working
  directory and are excluded from Git locally; older files are cleaned at close.
- Installed Android Chrome PWAs accept shared files through the manifest share
  target and stage them in AI Chat. Safari on iOS has no Web Share Target support.
- Web app manifest (`mobile-manifest.json`) with standalone display mode
- iOS Safari and Android Chrome Add to Home Screen support
- `apple-mobile-web-app-capable` meta tags
- PNG icons (192x192, 512x512) for PWA installability

### 18.8.1 Push Notifications
- Web Push from TUICommander directly to mobile PWA clients (no relay dependency)
- VAPID ES256 key generation on first enable; the private key is held in the OS credential vault and the public key in config
- Service worker (`sw.js`) handles push events and notification clicks
- `PushManager.subscribe()` flow with user gesture (click handler) for iOS/Firefox
- Push subscriptions stored in `push_subscriptions.json`, survive restarts
- API endpoints: `POST/DELETE /api/push/subscribe`, `GET /api/push/vapid-key`, `POST /api/push/test`
- Triggers: an explicit managed-agent `progress type=blocked` report (with its question text), a parsed agent question when a real title is available, `PtyExit` (session completed), and a still-pending ACP permission or form elicitation. A bare AskUserQuestion hook can signal awaiting without text; it does not spend the push limit before the title arrives
- Deep link: notification click navigates to `/mobile/session/<id>`, opening the specific session detail
- ACP interaction pushes link to `/mobile?repo=…&session=…`, selecting the correct Chat conversation; answered requests and ordinary ACP activity do not push
- Visible notifications use one tag per session, so a new question replaces only that session's earlier notification; generic alerts keep a separate shared tag
- Delivery gate: push is sent when the desktop window is unfocused or macOS HID input has been idle for at least two minutes. An active desktop suppresses duplicate alerts; platforms without HID idle information retain the focus gate
- Question and completion pushes share one 30-second limit per session
- ACP interaction pushes use the same 30-second limit for each conversation
- A managed session's free-text mobile reply uses the atomic `session submit` path and retains the draft if the session rejects it. It can answer a confident question while queued automated messages stay parked. Numbered choices keep their key-input path. For Codex `request_user_input`, the session header opens the queued question; its title and options appear in the transient choice overlay, and numbered answers use one PTY key. The `Other` choice opens Codex notes for a typed answer. The control uses the existing header, preserving all terminal rows
- Claude AskUserQuestion publishes its actual choices in the same mobile overlay; a tap follows the backend's arrow-and-Enter selection contract instead of using generic Yes/No keys
- Stale subscriptions cleaned on HTTP 410 Gone
- iOS standalone detection: shows "Add to Home Screen" guidance when not installed
- HTTP detection: shows "Push requires HTTPS (enable Tailscale)" when not on HTTPS

### 18.9 Notification Sounds
- Session notification sounds play only while the Sessions list and PWA document are visible; session detail, Chat, Files, Progress, Activity, and Settings remain silent. Muted transitions are tracked without replay, and deferred completions recheck visibility before playback.
- Audio playback via Rust `rodio` crate (Tauri command `play_notification_sound`), replacing the previous Web Audio API approach
- Eliminates AudioContext suspend issues on WebKit and works in headless/remote modes
- Native tones end on an exact zero-amplitude sample and keep the output stream alive for a 100 ms silent tail, avoiding end-of-chime clicks when CoreAudio releases a short-lived stream
- One shared 500 ms anti-spam gate covers every sound type, preventing different tones in a notification burst from overlapping; explicit Settings test playback bypasses it
- State transition detection: question, rate-limit, error, completion
- Completion notifications deferred 10s and suppressed when active sub-tasks are running (detected via `⏵⏵`/`››` mode-line prefix)
- In the desktop app, an unfocused agent question also sends a native OS notification naming its terminal. The Tauri notification plugin handles desktop delivery; macOS retains the native response handle through a desktop command so a click returns to the terminal. Permission is checked once, and focused windows and repeated notices within five seconds are suppressed
- **Sounds:** `question` (C5→E5 chime), `completion` (C5→E5→G5 arpeggio), `error` (E4→C4), `warning` (A4 double-tap), `info` (single G5 pluck), and `attention` — a triangular G4→G4→E5 callback with two short knocks and a longer rise. Native and browser/PWA playback share the motif and 0.8 gain; each engine applies its own envelope. The repeated opening is immediately recognizable while the softer timbre avoids the old square buzzer's harshness. Meant for an agent that is working unattended and is blocked on the user
- Each sound has its own on/off toggle and Test button in Settings > Notifications, and all of them honour the global volume and chosen output device
- **Agents can raise them over MCP**: `ui action=toast sound="attention"` (see 19.x `ui` tool). `sound: true` still means "the tone matching `level`"; a name overrides it. These agent notices appear in the bell rather than as transient toasts; consecutive identical notices within five seconds play once. The sound plays through this scheme, so a muted sound stays muted no matter who asked for it

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
- Native TUIC tools (`session`, `agent`, `task`, `remote`, `repo`, `ui`, `plugin_dev_guide`, `config`, `debug`) coexist with upstream tools
- Tool routing: names containing `__` are routed to the upstream registry; all others handled natively

### 19.1.1 Lazy Tool Discovery (`collapse_tools`)
- When `collapse_tools: true` (Settings > MCP > TUIC Tools > "Collapse tools"), the full tool list is replaced with 3 meta-tools: `search_tools`, `get_tool_schema`, `call_tool`
- Grok sessions (`clientInfo.name` matching `grok-shell-*`) receive the same 3 meta-tools automatically because Grok rejects nested qualified names such as `tuicommander__upstream__tool`; this per-session compatibility mode leaves the global setting and other clients unchanged, and the bridge restores it after TUIC reconnects
- Cuts MCP context from ~35k tokens to ~500 tokens per agent turn; agent fetches schemas on demand via BM25-ranked search
- BM25 index backed by `AppState::tool_search_index` (rebuilds automatically when the tool set changes)
- Settings native-tool switches and descriptions come from the backend MCP registry, including disabled tools; newly registered tools appear automatically. Every native tool can be disabled.
- Safety filters (`disabled_native_tools`, upstream allow/deny) enforced at both discovery and dispatch time — agents cannot bypass filters by calling `call_tool` directly
- Toggling fires `notifications/tools/list_changed`; compatible connected clients refresh automatically, while clients that ignore the notification may require a reconnect

### 19.2 Tool Namespace
- Upstream tools are prefixed: `{upstream_name}__{tool_name}`
- Double underscore (`__`) is the routing discriminator — native tool names never contain it
- Tool descriptions are annotated with `[via {upstream_name}]` to identify origin
- Clients always see the merged tool list in a single `tools/list` response

### 19.3 Supported Transports
- **HTTP (legacy Streamable HTTP, revision 2025-11-25)** — connects to any MCP server with an HTTP endpoint and sends `MCP-Protocol-Version` on every POST
- **Stdio (legacy revision 2025-11-25)** — spawns local processes (npm packages, Python scripts, etc.) communicating via newline-delimited JSON-RPC

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
- **Keyring warm-up** — resolved bearer token cached in memory after the first read. Health checks (every 60 s) and tool calls reuse the cache, invalidated on 401 and re-populated after token refresh. Eliminates repeated macOS Keychain permission prompts
- Config file (`mcp-upstreams.json`) never contains secrets
- Per-upstream credential lookup at call time
- OAuth 2.1 token sets persisted as structured JSON in the keyring (`{"type": "oauth2", "access_token", "refresh_token", "expires_at"}`)

### 19.8.1 OAuth 2.1 Upstream Authentication
- Full RFC 9728 (Protected Resource Metadata) + RFC 8414 (Authorization Server Discovery) flow with PKCE S256
- `UpstreamAuth::OAuth2 { client_id, scopes, authorization_endpoint?, token_endpoint? }` joins `Bearer` as a credential type; endpoints auto-discovered from the resource server's `WWW-Authenticate` challenge when omitted
- Completion via native deep link `tuic://oauth-callback?code=…&state=…` — callbacks never touch the WebView console
- `TokenManager` shared across every `HttpMcpClient` refresh path with a per-upstream semaphore that defeats thundering-herd refresh. 60 s expiry margin; `None expires_at` treated as valid
- `UpstreamError::NeedsOAuth { www_authenticate }` transitions the registry to `needs_auth`; the Settings MCP page shows an *Authorize* button
- Auto-triggered OAuth is gated behind explicit user consent; a blocking in-app confirm dialog surfaces the Authorization Server origin and prevents the pending flow from being cancelled behind the prompt
- Status values extended: `authenticating` ("Awaiting authorization…") + `needs_auth`
- Tauri commands: `start_mcp_upstream_oauth`, `mcp_oauth_callback`, `cancel_mcp_upstream_oauth`

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
- Paint triggers coalesced per animation frame via `requestAnimationFrame` (~60 repaints/sec)
- High-throughput agent output (hundreds of events/sec) batched into single grid frame updates
- Reduces canvas render passes during burst output
- Flow control (pause/resume at HIGH_WATERMARK) unchanged

### 20.2 Async Git Commands
- All ~25 Tauri git commands run inside `tokio::task::spawn_blocking`
- Prevents git subprocess calls from blocking Tokio worker threads
- `get_changed_files` merged from 2 sequential subprocesses to 1

### 20.3 Watcher-Driven Git Cache
- `repo_watcher` (FSEvents/inotify) monitors the working tree with per-category debounce. macOS/Windows use one recursive watch; Linux splits into pruned non-recursive working-tree watches (skipping `node_modules`/`target`/gitignored, with new dirs added dynamically) plus targeted `.git` watches (root + `refs`/`worktrees`, never `objects`/`logs`), to avoid inotify event storms (issue #82)
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
- Vite `manualChunks`: terminal, codemirror, diff-view, markdown as separate chunks
- SettingsPanel, ActivityDashboard, HelpPanel lazy-loaded with `lazy()` + `Suspense`
- PTY read buffer increased from 4KB to 64KB for natural batching

### 20.8 Conditional Timers
- StatusBar 1s timer only active when merged PR countdown or rate limit is displayed
- ActivityDashboard snapshot signal uses default equality check (no forced re-render every 10s)

### 20.9 Profiling Infrastructure
- Scripts in `scripts/perf/`: IPC latency, PTY throughput, CPU recording, Tokio console, memory snapshots
- `tokio-console` feature flag for async task inspection
- See `docs/guides/profiling.md`

### 20.10 Process Monitor
- Reports CPU% and resident memory (RSS) for TUIC and every child process tree, each row attributed to the session that owns it
- Agent lifecycle also classifies the owning process tree: meaningful background descendants keep `agent_state=working` while an input-ready terminal may remain `shell_state=idle`; persistent `mdkb`, `tuic-bridge`, and `node_repl` helper subtrees plus Claude's standalone timed `caffeinate -i -t <seconds>` assertion are excluded by executable name or authoritative argv path. A `caffeinate` invocation that wraps a command remains meaningful. A descendant that started within 60s of the agent itself is also excluded whatever its name, which covers the daemons no list can anticipate — `codex-code-mode-host`, or an MCP server launched through `npm exec`; platforms that report no process age fall back to the name rule alone A ready observation waits for a newer shared process snapshot, and polling stops once no probe or background work remains
- Unix: a single batched `ps -o pid,rss,%cpu` query across all PIDs (not one stat per process); Windows: per-process working-set size via the platform API
- Process statistics use HTTP `GET /process/stats` (JSON `{ session_id, name, pid, rss_kb, cpu_pct }`), while `GET /process/monitor` serves a self-contained HTML dashboard with no build step or external assets. The removed MCP `session action=process_stats` returns an error naming the HTTP replacement.
- Frontend `ProcessManagerModal` opens the dashboard in-app
- Use to diagnose which agent/terminal is driving high CPU or memory

### 20.11 Runtime Diagnostics (CPU watchdog + diagnostic mode)
- **Always-on CPU watchdog** (zero overhead when idle): polls `getrusage(RUSAGE_SELF)` every 5s and logs a full snapshot when TUIC's own CPU stays above 80% for 10+ consecutive seconds. PTY children (cargo, rustc, …) are separate OS processes and don't count toward the measurement
- **Sleep/wake aware**: inter-tick gaps over 30s are treated as the machine having been asleep (lid closed) and skipped, so stale tokio-timer ticks after wake don't trigger false spikes or idle cascades
- **Diagnostic mode** (toggleable at runtime, off by default): emits a health snapshot every 30s and alerts on FD/thread growth trends. Each snapshot includes: `cpu_pct` (TUIC self only, via `RUSAGE_SELF`), `children_cpu` (aggregate %cpu of all PTY child process trees + the hottest individual child — note the CPU watchdog spike trigger intentionally ignores children, so a hot `cargo`/agent only surfaces here), thread count, FD count, PTY session count, content-index build state, semaphore permits, sessions with grid frames outstanding (`session×count`, from the `GridGate` counters), event-bus subscriber count, and `head_emits_suppressed` (repo-watcher `head-changed` emits skipped by the resolved-HEAD-target guard — a climbing value signals a filesystem-event storm)
- **Frontend liveness** (always on, desktop only): the WebView beats every 5s from its main thread; six missed beats log `Frontend unresponsive: no heartbeat for Ns` exactly once, and the return beat logs a matching recovery line. Sleep/wake re-baselines the clock so a lid-close is never charged to the frontend. Recover with `POST /debug/reload_webview` — a native-side navigation that works while the JS thread does not, and keeps every PTY session (they live in the backend). Recovery remembers only a URL on the app origin, including the configured development origin. Native navigations log their trigger, action, and target URL; each frontend initialization logs the document navigation type and start time. Frontend: `src/utils/frontendHeartbeat.ts`; backend: `src-tauri/src/frontend_liveness.rs`, `src-tauri/src/webview_recovery.rs`
- Control via HTTP: `POST /diagnostics {"enabled":true}` to toggle, `GET /diagnostics` for status, `GET /logs?source=diagnostics` to read the snapshots
- Developer agent capture scenarios record installed CLIs in isolated headless debug sessions; fixture promotion preserves bytes and checks ordered expected states through the replay oracle. See `scripts/agent_capture/README.md`.

- Raw PTY captures can be directed to an absolute `TUIC_CAPTURE_DIR` for isolated regression evidence; the capture status reports the chosen directory.
- Catches known failure patterns: IPC flush loops, content-index CPU saturation, a blocked or dead WebView main thread (missed heartbeats — *not* grid frames outstanding, which a hidden terminal produces on purpose by never acking), FD/thread leaks, and sleep/wake false-idle cascades
- Backend: `src-tauri/src/cpu_watchdog.rs`

## 21. CLI Companion (`tuic`)

### 21.1 Overview
- Standalone Rust binary embedded as a sidecar, installed to system PATH
- Combines VS Code-style file opening, tmux-style session management, and agent orchestration
- Cross-platform: macOS, Linux, Windows
- Communicates via IPC (Unix socket / Windows named pipe) with the running TUICommander instance
- Auto-launches TUICommander if not running

### 21.2 Editor Mode
- `tuic [path]` — open file or directory (VS Code/Zed style)
- `tuic open --goto file:line:col` — open at specific position
- `tuic open --wait` — block until file closed ($EDITOR support)
- `tuic diff <a> <b>` — diff view

### 21.3 Session Management (tmux-compatible)
- `tuic ls` / `tuic new` / `tuic kill` / `tuic send` / `tuic capture`
- `tuic resize <id> WxH` / `tuic pause` / `tuic resume`
- Targets accept UUIDs, ID prefixes, or session names
- tmux key name translation (Enter, C-c, Space, etc.)

### 21.4 Agent Orchestration
- `tuic agent spawn <type> <prompt> [--repo <path>]` — spawn AI agent on an initial prompt
- `tuic agent ls` — list running agents
- `tuic agent send <peer-uuid> <message>` — deliver to a registered peer's inbox through the registry, the same path as the MCP `agent action=send` tool. Reports `Delivered` only when something surfaced the message; an `inbox_only` route reads `Buffered`
- `tuic agent type <id> <message>` — type into an agent's terminal and submit, with the text and the Enter as separate writes (raw-mode TUIs treat a combined `text\r` as an unsent prefill)

### 21.5 tmux Compatibility Mode
- `tuic alias` creates `tmux → tuic` symlink; `argv[0]` detection switches to compat mode
- Supports: `new-session`, `list-sessions`, `kill-session`, `kill-server`, `send-keys`, `capture-pane`, `resize-pane`, `attach-session`, `has-session`
- Tools expecting tmux (e.g. Claude Code `--tmux`) transparently use TUICommander

### 21.6 Installation
- First-run prompt on app launch (one-time, dismissible)
- Settings > General > TUIC CLI (install/uninstall button with status)
- Auto-update on app startup (silent, no elevation prompt)
- Paths: `/usr/local/bin/tuic` (macOS/Linux), `%LOCALAPPDATA%\Microsoft\WindowsApps\tuic.exe` (Windows)
- `tuic install-cli` / `tuic alias` for self-service

## 22. Remote Daemon (`tuic-remote`) — Beta

### 22.1 Overview
- Standalone headless binary for running TUICommander on servers without a desktop environment
- Same HTTP/WebSocket API as the desktop app's remote access feature
- No Tauri dependency — pure Rust binary
- Available as GitHub Release artifacts for Linux x64/ARM64, macOS ARM, and Windows x64
- `tuic-bridge` is published for every one of those targets too, and belongs next
  to the daemon: the daemon writes an MCP entry naming it into the config of each
  agent installed on the machine, so an agent launched there gets the full
  `tuicommander` tool surface (`session`, `repo`, `progress`, `agent`, peer mail)
- Agents reach the daemon over its local IPC endpoint (`mcp.sock`, or the
  `tuicommander-mcp` named pipe) — never over the authenticated TCP port
- Runs the desktop's background tasks that a machine needs: process snapshots,
  standby parking, content indexing and boot pre-warm, tool search index, upstream
  MCP auto-connect and health checks, CPU watchdog, maintenance sweep. It does not
  run the WebView watchdog (no WebView) or command-knowledge persistence (no
  route exposes it). The embedded assistant's scheduler and watcher engine are
  not on this list any more — they were deleted outright in #784-0aec

### 22.2 Configuration
- Without `--instance`, uses the desktop app's existing platform config directory,
  keyring service `tuicommander`, user `vault`, and legacy migrations unchanged
- `--instance <id>` selects an immutable isolated namespace before password setup
  or startup: files under `<platform-app-config>/instances/<id>/` and keyring
  service `tuicommander-instance-<id>`, user `vault`
- Instance IDs are lowercase ASCII DNS labels of 1–63 characters with
  alphanumeric ends and optional internal hyphens; `default` is reserved
- Named instances start empty and never fall back to or migrate default/legacy
  files or credentials. A release daemon exits before binding if its named OS
  keyring vault cannot be opened
- Default port: 9877 (overridable via `TUIC_PORT` env var)
- `--set-password` performs interactive password setup (bcrypt hashed) inside
  the instance selected earlier on the same command line
- LAN auth bypass always disabled in headless mode (security hardening)
- `--bind <ip>` selects the listening address; desktop-managed deployments pin
  it to `127.0.0.1`
- `--survive-secs <n>` exits after that many seconds without SSE or WebSocket
  clients; client activity resets the timer
- `--no-agent-configs` skips host agent configuration for ephemeral and installed
  desktop-managed daemons
- `TUIC_PAIRING_TOKEN` is consumed once as the daemon session token and removed
  from the environment before child processes can inherit it

### 22.3 TLS
- Manual TLS via `services.tls` in the instance's `config.json` (`mode: "manual"`,
  plus cert and key PEM paths)
- No TLS by default — use a reverse proxy or Tailscale for production

### 22.4 Lifecycle
- Graceful shutdown on SIGINT/SIGTERM/SIGHUP, with a pid file removed on exit
- Binds TCP, starts background tasks (MCP session reaper, upstream health checks)
- Fails fast if port is already in use
- Optional survive timer preserves sessions across short disconnects, then exits
  when its configurable idle window expires

## 23. SSH Tunnel Manager

### 23.1 Supervised Tunnels
- Managed SSH processes with automatic lifecycle supervision
- Tunnel states: Starting, Connected, Reconnecting, Stopped, Error
- Readiness check: SSH must survive 500 ms and every local forward must accept TCP connections before the tunnel is considered connected (30-second limit)
- Graceful shutdown: SIGTERM with 5s grace period, then SIGKILL escalation
- SSH agent forwarding: auto-discovers `SSH_AUTH_SOCK` for key-based auth

### 23.2 Reconnection with Exponential Backoff
- Automatic retry on retryable failures (network down, connection refused, timeout)
- Exponential backoff: 1s base, doubling per attempt, capped at 30s
- Jitter: +/-25% per delay to prevent thundering herd
- Maximum 10 retries before stopping; counter resets on successful connection
- Non-retryable failures (auth denied, host key mismatch, port in use) stop immediately

### 23.3 Exit Classification
- Stderr-based pattern matching classifies SSH exit reasons (AuthFailed, HostKeyMismatch, PortInUse, ConnectionRefused, NetworkDown, Timeout, UserKilled)
- Exit code used as fallback when stderr is empty
- Classification drives retry decisions — only network-related failures are retried

### 23.4 Audit Logging
- SQLite database with WAL mode for concurrent-safe, high-performance event logging
- Event types: Started, Connected, Disconnected, Error, Retry, Stopped
- Query by tunnel ID (most recent N events) or by time range
- Automatic rotation: configurable retention period deletes old events
- Indexed on `tunnel_id` and `timestamp` for fast lookups

### 23.5 Profile Configuration
- TOML-based profiles with name, host, port, user, identity file, and port forwards
- Global scope: `<config_dir>/tunnels/*.toml` — available across all repos
- Per-repo scope: `<repo>/.tuic/tunnels/*.toml` — overrides global profiles with same ID
- Forward types: Local (`-L`) and Remote (`-R`) port forwarding
- Options: ServerAliveInterval (default 15s), ServerAliveCountMax (default 3), StrictHostKeyChecking (Yes/AcceptNew)
- Validation: duplicate bind ports, empty fields, port range (1-65535)
- Pre-spawn port availability check for local forwards

### 23.6 Tauri IPC Commands
- All tunnel management exposed as native Tauri IPC commands (`tunnels/tauri_commands.rs`) — profile CRUD, start/stop, status, audit log, SSH config host parsing, SSH agent key listing
- Desktop app uses IPC directly; browser mode falls back to HTTP endpoints

### 23.7 Auto-Connect
- Profiles with `auto_connect: true` start automatically on app launch
- Hydration runs once during startup, guarded against duplicate calls
- Non-blocking: failures are logged but don't prevent app startup

### 23.8 SSH Agent Detection
- Auto-detects SSH agent type from `SSH_AUTH_SOCK`: 1Password, Secretive, GPG Agent, generic SSH Agent
- Lists loaded keys via `ssh-add -l` (fingerprint, comment, key type)
- Shown in the tunnel editor for identity verification

### 23.9 Orphan SSH Process Cleanup
- `check_local_port()` distinguishes `PermissionDenied` (privileged ports) from `AddrInUse`
- `kill_ssh_on_port()` finds SSH processes holding a port via `lsof`, verifies with `ps`, sends SIGTERM
- Only kills confirmed `ssh` processes — never unrelated services

### 23.10 Statusbar Shield
- Grey shield icon when tunnel profiles exist but none are connected
- Green shield with count badge when tunnels are connected
- Clicking the shield opens the Tunnels Panel

### 23.11 Shutdown on Exit
- `TunnelManager::shutdown_all()` called on `RunEvent::Exit`
- Stops all supervisors and clears the tunnel map — no orphaned SSH processes after app close

### 23.12 UI
- **TunnelsPanel** — List of tunnel profiles with status badges and start/stop controls
- **TunnelEditorModal** — Create and edit tunnel profiles with form validation; file browse dialog for identity file; remote host pre-populated from tunnel host when adding forwards; type-aware Local/Remote forward endpoint fields; numeric input mode for port fields
- **TunnelStatusBadge** — Color-coded status indicator (green=connected, blue=starting, orange=reconnecting, red=error, grey=stopped)
- **Command Palette** — `toggle-tunnels` action registered for quick access

## 24. Remote Connection Manager

### 24.1 Connection Types
- **SSH** — Connects via SSH tunnel to a remote `tuic-remote` daemon; auto-creates port forwarding
  - Fields: host, SSH port (default 22), SSH user, optional identity file, remote daemon port (default 9877), deploy policy, survive time
  - The host picker probes deduplicated SSH config hosts on demand and labels
    shell, no-shell, auth-failed and unreachable results; free text remains valid
- **Direct** — Connects to a `tuic-remote` daemon URL directly (for Tailscale, LAN, or VPN scenarios)
  - Fields: URL, auth username
- **Update & restart remote** — For either connection type, compare the
  daemon's `/health` build SHA-256 with the release asset or a matching local
  build, show an out-of-date badge, confirm the number of live sessions that
  will be lost, deploy, and verify the new hash after automatic reconnect.
  Direct uses an authenticated, size-limited upload; SSH uses SCP. Windows
  in-process replacement is explicitly unsupported.
- **Auto-update remote daemons** — An opt-in per-connection setting. On connect,
  update an out-of-date daemon only when it reports zero live PTY sessions.
  With live sessions, show the count and offer the manual update without a queue.
  Progress, success and errors appear in Remote Machines. One update owns each
  connection at a time: manual and automatic updates both show in-progress,
  and overlapping requests over IPC, HTTP, or MCP receive an error.

### 24.2 Storage
- Connections persisted in `<config_dir>/connections.json`
- Atomic writes via temp file + rename
- Each connection has UUID, name, transport, auth username, enabled flag,
  `deploy` (`never | on_connect | installed`), `survive_secs` and
  `auto_update` (defaults to false)
- The Basic Auth **password** goes to the OS credential vault (`Credential::RemoteConnection`), keyed by the connection UUID — never to `connections.json`, never readable back, and deleted with the connection
- Desktop-managed SSH deployments use a separate vault pairing token. It is the
  daemon session token, never appears in `connections.json`, and survives a
  desktop restart so Connect can rejoin the same daemon

### 24.3 Authentication
- Both desktop and `tuic-remote` require credentials on protected TCP requests, including loopback and LAN. Origin/Host checks reject foreign websites and DNS rebinding before any handler. Local IPC retains its existing trust.
- `tuic-remote` authenticates every TCP request: the headless build has no loopback bypass and `run_remote` forces `lan_auth_bypass` off, so an SSH tunnel does not make it local. `GET /health` is the only unauthenticated route
- On connect, the backend trades the vault password for the daemon's session token (`GET /api/auth/session-token`, Basic Auth) — in Rust, so the password never reaches the WebView
- The token is appended as `?token=` to HTTP, the terminal WebSocket and the `/events` SSE stream by the single helper `withRemoteToken` (`transportRuntime.ts`). A WS upgrade cannot set a header and `Access-Control-Allow-Origin: *` rules out credentialed cookies, so the query string is the only credential all three share
- Held in memory only, never persisted: the daemon mints a new one on every restart, and a 401 from the status poll triggers one re-authentication
- Status separates **Not authenticated** (reachable, credentials rejected) from **Error** (unreachable). An unauthenticated connection starts no poll, no event bridge and routes no calls

### 24.4 Connect and Install
- **Deploy on connect** resolves the host target with `uname`, downloads and
  caches the matching release asset, compares its SHA-256, copies only when
  needed, then launches it on loopback with the configured survive time
- SSH tunnel, remote exec and SCP share the TUIC-owned
  `~/.ssh/tuic-%C` ControlMaster socket, so one authenticated connection serves
  the whole flow without exceeding Unix socket path limits. The master persists
  only for the life of the process that owns it, preventing a completed one-shot
  command from making a later supervised tunnel exit immediately
- **Install** writes a systemd user unit and protected environment file on Linux,
  or a protected launchd plist on macOS. **Uninstall** stops and removes them
- Deployment publishes `deploying` status with the current step. Failures retain
  the step and remote log tail in `error`; they are never misreported as an auth failure

### 24.5 Remote Repositories and Terminals
- Repos can be assigned to a remote connection; sidebar shows remote badge
- Remote repository browsing starts at the remote host's reported home directory and exposes directory permission errors without blocking manual path entry
- Terminals on remote repos route WebSocket I/O through the connection's base URL
- A terminal launch failure is shown in its pane, and a failed grid stream shows an error toast; the remote session owner is registered before the stream attaches
- **One choke point decides the machine.** `resolveOwningConnection` (`transportRuntime.ts`) reads the call's own arguments — a session id first, then a repository path — and answers which connection owns it. Both entry points ask it: `rpc()` for the HTTP transport and `invoke()` for the desktop IPC path, which would otherwise short-circuit straight to the local backend. A call site cannot forget to route, because it never routes
- Path→connection resolution reuses `resolveRepoPathFor` (deepest registered repo or linked worktree wins); session→connection goes through the terminal's `repoPath`, falling back to its `cwd` while ownership reconciliation has not run yet
- A command with no HTTP route (`INTENTIONALLY_UNMAPPED`) stays local and warns once — routing it would replace a working call with a throw
- A call on a repo whose connection is down fails with `Remote connection <id> not connected`; it is never answered by the local backend
- `canvasTerminalTransport.ts` takes the `connectionId` and derives both the base URL and the token from it
- Status polling runs against `/api/version`, not `/health`: only a route behind the auth middleware can tell a working connection from a rejected one

### 24.6 Event Mirror
- Remote MCP toasts are retained once in the desktop Messages bell with `[connection name]` before the title. Their level and requested notification sound are preserved; Open terminal resolves the peer to its live remote PTY and never selects a local tab. Unknown or closed sessions leave focus unchanged. Toasts arriving after disconnect are dropped, with no replay queue. Malformed remote titles/messages are discarded without logging their content. Deduplication includes the connection id, so identically named hosts retain separate notices.
- `remote_mirror.rs` runs one task per connected connection: it reads the daemon's `GET /sessions` and then its `/events` stream, in Rust
- The stream carries **no** `types=` filter, and every frame is repeated on the local bus under the daemon's own event name — a client cannot tell a mirrored event from a local one, so the existing handlers raise the same badge, the same notification and the same queue gate, and a new event type crosses for free
- Mirrored sessions appear in `list_active_sessions` and `GET /sessions` beside local ones, each carrying `connection_id` — the only field that says which machine runs it
- The stream is re-seeded after every reconnect, so a gap in the SSE cannot leave a stale badge; losing the connection announces each mirrored session closed and then drops it
- Every mirrored payload is stamped `__tuic_origin`, and a frame that already carries one is dropped: a mirrored event never crosses a second hop, so two machines pointed at each other stop after one repeat instead of looping. A body that is not a JSON object cannot be stamped and is dropped for the same reason
- The **desktop window** hears only `session-state-changed` and `session-closed`. Those handlers are session-scoped; `session-created`, `ui-tab`, `worktree-created`, `worktree-removed` and `repo-changed` mutate local state — a phantom tab per remote session, a workspace in the local store, git work on a path this machine does not have. They still reach the local bus and `/events`
- `connect` refuses a base URL whose `/health` reports this process's own `instance_id`: a machine cannot mirror itself. A daemon too old to publish the field still connects

---

## 25. Generators

Secure value generators accessible from the command palette (`open-generators` action).

### 25.1 Available Generators
- **Password** — Configurable length, character classes (uppercase, lowercase, digits, symbols)
- **UUID v4** — Standard random UUID (RFC 4122)
- **UUID v7** — Time-ordered UUID (RFC 9562)
- **ULID** — Universally unique lexicographic identifier
- **CUID2** — Collision-resistant unique identifier
- **JWT Secret** — 256-bit random hex key
- **TOTP Secret** — RFC 4226 base32 secret (160-bit)
- **Nano ID** — URL-friendly random ID with configurable length
- **Slug** — adjective-noun-NNNN random slug
- **Ed25519 Key Pair** — Public + private key pair

### 25.2 Architecture
- All generation happens in the Rust backend (`generators.rs`) via the `ring` crate for cryptographic randomness
- Frontend is a modal dialog with copy-to-clipboard and regenerate actions
- Password and Nano ID have configurable options (length, character classes)

## 26. ACP Client for ego

Backend-only so far: TUICommander can drive an [ego](https://github.com/sstraus/ego)
agent over the Agent Client Protocol (v1). No frontend surface yet.

### 26.1 What it does
- Launches a supervised ego child per connection and initializes it, refusing
  anything the agent did not advertise **before** a byte reaches the wire
- Durable sessions: new, list, load, resume, fork, close, delete
- Turns: prompt, cancel, per-turn config options, and the ordered stream of what
  the agent said, replayable from any sequence a bounded journal still holds
- The agent's questions back — permission and elicitation — parked so a person
  answers them without freezing the connection, and listed so a client that was
  not running when they were asked still finds them
- ego's own extensions: `_ego/pause`, `_ego/resume`, `_ego/compact`, each gated
  on the version ego advertised

### 26.2 Where it is reachable
- Desktop: 23 `acp_*` Tauri commands (`docs/api/tauri-commands.md`)
- Browser/PWA/remote: an identical route per command under `/acp`
  (`docs/api/http-api.md`), including the same error bodies
- The turn stream is a dedicated Channel on the desktop and a dedicated
  WebSocket in the browser; `/events` carries only the low-frequency
  `acp-notice` wake signal
- Text sent during a running turn uses advertised ego v1 ACP steering and enters
  the current query at the next provider request. Accepted steering is shown only
  through ego’s user-message echo and is never resent. A not-busy response uses
  the normal prompt path. Attachments, unavailable steering, and rejections
  retain the shared queue.
- Queued prompt submissions share one FIFO per ACP session across desktop and phone;
  queued entries are visible and removable from either view, and active-turn
  cancellation is shared. Queue snapshots and events omit image bytes.

### 26.3 Process authority
The one binary this may launch is the `ego_executable` setting, read at each
connect. It is not an argument of any command or route, so no request — local
or remote — can choose what the host runs. An empty setting refuses every
connect rather than failing later inside a spawn.
The optional `ego_profile` setting selects one profile in ego's user
configuration at ACP launch. Empty adds no `--profile` argument. TUIC sends no
profile rules or allow/deny policy in `session/new`. A workspace's `.tuic.json`
may select `ego_profile`; TUIC passes that name and the explicit machine selection
as `_meta.ego.{profile,ceilingProfile}`. Ego owns restrictive composition of mode,
sandbox and perimeter. Its returned warnings appear as AI Chat warning notices.
Missing machine selection or missing ego acknowledgement refuses attachment.

## 27. Terminal Progress

- One append-only journal per project with each new entry attributed to its source PTY when known: entries are written once, then kept or deleted. No pause, clear, correction, revision, deduplication or Markdown export
- Three kinds. Agents report `done` and `blocked`; TUICommander writes `intent` from the agent's own `intent:` marker, so an agent that never calls the tool still leaves a trail
- Compact MCP `progress` tool (`type`, `text`, optional `step`) that appends and toasts in one call, stays directly callable in collapsed tool mode, and refuses `intent` — that kind is observed, not claimed
- An imperative reporting obligation in `initialize` rather than only a tool description: the descriptive version recorded zero entries across 39 repositories
- One SQLite database in the configuration directory with the project as a column. Nothing is written inside a repository, so Progress produces no repository-change event, no Git-exclude entry and no indexing pass
- Managed workspaces resolve to their parent project, so a worktree and its repository share one history; a directory belonging to no registered project is not recorded at all
- A dialog, not a panel: one newest-first list for the active PTY by default, a selector for other PTYs and the repository aggregate, queried once per selected view
- A last-visit divider frozen while the dialog is open, so it never moves under the line being read
- Blocked entries in red, `intent` entries muted, one blocked-only filter, per-entry deletion scoped to the project
- Aggregate notification-bell count plus exactly one live toast, silent by default; Progress entries are not duplicated into MESSAGES
- On desktop, `done` and `blocked` entries also send a native OS notification with the project and entry text while TUICommander is unfocused. A macOS click opens Progress at that project and terminal. `intent`, hand-off, and message entries remain silent; identical notices within five seconds are coalesced
- The notification bell always offers Terminal Progress, including with zero unread updates; `Cmd/Ctrl+Shift+P` and the command palette open the same dialog
- `progress_tracking` gate: a global setting ANDed with a per-agent override. Global off removes the tool from every agent's tool list

### Remote MCP session ownership and peer mail

Desktop MCP discovers configured remote PTYs and peers, routes output and semantic
submit to the owning daemon, and delivers connection-qualified peer mail through an
authenticated desktop hub. Local mail remains independent of the hub.

### Answers-only View

Use **Toggle answers-only view** (`Cmd+Alt+R` on macOS) to read selectable marked answers and their tracked prompts. Turns without marked answers are omitted. Output before the first tracked prompt remains available as a prompt-less turn, from the retained history base. If no answers qualify, the view shows a one-line notice. Toggle the view again to return to the terminal.

## Private secret forms

The `secret` MCP tool opens a separate native form for requested sensitive
fields and returns names/status only. Approved argv commands receive values in
the child environment; pipe output is masked for exact and common encoded
values, including wraps. User consent binds exact argv, names
and directory. Values and templates live only until exit. One-time nonce links
support browser entry on the existing application origin and transport. Use
HTTPS for phone entry. TUIC inspection tools are gated while a form is open. See
[Private secret forms](user-guide/secrets.md) for limits and consent.

Workflow safety includes owned check-tree teardown, credential-derived operator authority, and receipt-based plan completion that reopens after canonical ref movement.

Workflow and dependency writes probe Git before opening write transactions, then reject concurrent run or story revision changes. Canonical ref changes invalidate integration evidence on the next read.

### Stored terminal marker coordinates

Command boundaries and user prompt ticks retain their physical row after capped scrollback evicts older output. Answers-only history uses the same stable prompt coordinates.

## Headless Telegram channel

Any MCP-bound TUIC agent opts in with `telegram register`; one volatile
registration replaces the previous agent with one native notice. It ends on
MCP session end, PTY close, foreground-agent exit or daemon restart.
Allowlisted private text arrives as native peer mail. Without an agent the bot
replies "Nessun agent registrato" and drops the text; strangers stay silent.
The registered agent can stream concise activity, send exact replies and offer
opaque buttons. Proactive sends use one allowlisted destination. Only the polling
cursor persists; live mint deployment remains pending. See
[Telegram channel](design/telegram-channel.md).
### Telegram Settings

Desktop Settings → Telegram and Mobile Settings → Telegram setup provide token replacement/getMe check, one-use ten-minute pairing or explicit private-chat IDs, read-only registered-agent status, enable and safe status. Only `tuic-remote` owns polling. Tokens stay in owner-only private files and never return to the UI.

Connected daemon notices carry their host identity. MCP confirmation responses and ACP permission/elicitation answers return to that daemon; disconnected questions disappear without changing local connections. AI Chat shows remote questions separately, with their ACP connection identity. Remote GitHub transitions fetch PR data from the repository owner, notify once, and do not run local repository automation. GitHub polling also works on the headless daemon. MCP upstream health refreshes use a separate host snapshot rather than this machine’s editable configuration.

- AI Chat mobile push: pending questions and ego notice cards share the 30-second per-conversation limit.

The graph runtime foundation now persists and replays pinned activations, serial predecessor history, edge decisions and repair counters. Executable validation requires Pause resume targets and deterministic final checks. Fork/all-Join schema and execution wait for slice G; scheduling and designer controls for these settings remain in development. Pause resolution respects uncertain-effect and pending-input fences, and serial history is bounded. Old pre-contract runs support inspect/cancel only.

AI Chat offers **Fork from here** beside a reply when ego advertises message-point forks. The new tab keeps history through that reply’s completed turn; the parent remains open.

Forked AI Chat tabs label inherited history. A separator marks where the child’s own conversation begins, including after loading saved history.

The AI Chat conversation picker groups fork and compaction descendants beneath their ancestors. Deleted immediate parents remain visible as disabled “Deleted conversation” rows.

Closed refusal turns show one plain-text card with the agent’s existing ACP refusal text. A refusal without text shows a generic refusal message.

Browser HTML and Markdown previews load local images only from open repositories and their linked worktrees, through the authenticated repository image route. Images outside those roots show “Image unavailable: outside open repositories.” Desktop asset loading is unchanged.
Workflow graph slice C: the owning daemon executes pinned serial Agent/Judge/Loop/Pause/Notify/Join nodes through the existing RunStore ledger and managed launch fences. Named sol/sonnet profiles are required. New unsupported Gate and plan node publications are refused visibly. Manual pauses suspend active duration. Slice F supplies public story graph starts and history controls; D/E add independent approval and plan dispatch; graph completion alone never closes a native story.


### Workflow story policy (slice D)

The daemon runs pinned deterministic checks before accepting an independent reviewer approval. Native approval history preserves the reviewer actor and revision; the run retains the reviewed artifact and check receipts. A workflow implementer cannot approve its story after exit or claim release. Integration remains an explicit operator action.


### Workflow plan dispatch (slice E)

The owning daemon executes pinned plan coordinator and Create Stories visits through the existing proposal effects. Disjoint story children share a bounded project wave; overlapping or unknown scopes wait. A dependency starts only after its accepted revision has a current explicit Git integration receipt. Done alone and WontFix never release it. Approved children wait for an operator merge. The final plan Judge runs the pinned canonical checks and binds verification to the current plan fingerprint. Failed checks route through a bounded replan with a fresh coordinator attempt. Public start/history controls use the same graph entry point; parallel graph branches follow in slice G.

- **Run incidents and next steps:** Plans and Stories run history connects recorded workflow and bound-agent failure/input outcomes to their cause, identifiers and manual next action. Session/task evidence is transient; no automatic incident retry.

- **Conversation launch options:** AI Chat can give one conversation its own ego profile, executable and workspace, persisted with a dedicated peer identity. Creation and reopen are also available over HTTP.
- **Inbox read audit:** MCP agent inbox reads log caller, owner and message ids at INFO without mail bodies.

Terminal stream reconnect notices identify the session and show the transport’s current attempt (up to 10). A received grid frame or confirmed empty replay removes only that stream notice; socket opening alone does not. Exhausted retries leave a persistent failure notice.

In rich mode, sessions idle for more than two hours fold into an expandable count. Backend activity timestamps determine age; busy sessions, awaiting input, unread output, selected rows and parents of visible children stay visible. Expanding preserves row order. Compact mode does not fold idle sessions.

Rich working agent rows reserve two clamped intent lines. Idle, awaiting-input and error rows stay on one line with status dots; their full intent, task or prompt remains in the tooltip. Compact rows keep their one-line layout.

Rich navigation keeps compact typography and padding. Branch identity and facts share one line, with names truncated and full PR/lifecycle facts in tooltips. Repository metadata disappears when PR and worktree counts are both zero. Header controls use compact sizing to protect repo names. Returned subagents fold into an expandable count independently of running work.

In rich mode, ungrouped repositories with open terminals appear first, followed by an Idle section. Manual order is preserved inside each section. Idle headers start on one line; disclosure reveals their branches without changing activity or saved order. Headers count agents in the backend working state, independently of open-terminal membership.

Rich active/idle sections also apply independently inside each existing repository group. Group membership and group ordering remain intact; active-only filtering hides idle-only groups and empty Idle headings.

Rich merged linked worktrees with no sessions and verified lifecycle facts use a one-line summary: Merged, N uncommitted, Clean up. Live, unmerged, or unknown worktrees retain full rows. Clean up enters the existing lifecycle preflight and confirmation workflow; cancellation preserves the worktree.

Rich parent terminal rows show N agents in each other repository containing live children. Clicking reveals the child group, repository and workspace, then selects a child through existing terminal navigation. Session placement and same-branch nesting stay intact; closed children disappear from summaries.

Browser Add Repository provides a server-side directory picker using the existing filesystem routes. Root HTML navigation offers form login; touch-capable iPad Safari selects the mobile interface even with its desktop Mac user agent. API authentication is unchanged.

### Automation Once schedules (Rust foundation)

Definitions can schedule one wall-clock date-time in an IANA zone. Rust rejects
past instants on creation and spring gaps, resolves folds to the earlier instant,
and keeps consumed Once definitions for inspection. The backend supplies one
preview instant and completed schedule state. Public automation controls arrive
with the scheduler API and dialog.
### Automations dialog (backend integration pending)

Machine-local scheduled-run editor with search, backend cadence/zone preview,
prechecks, workspace and duration controls, pause/resume, Run now, confirmed
delete and recent status history. Open **Automations** from the command palette.
Step 8 API integration is required before use; see [Automations](user-guide/automations.md).
