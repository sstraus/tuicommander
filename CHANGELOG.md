# Changelog

All notable changes to TUICommander will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.6.0] - 2026-02-28

### Added
- **Plugin filesystem write/rename** — New `fs:write` and `fs:rename` capabilities allow plugins to write and rename files within `$HOME` with path-traversal validation
- **Plugin panel message bridge** — `openPanel()` accepts `onMessage` callback for structured iframe→host messaging; `PanelHandle.send()` delivers host→iframe messages. Replaces fragile global `window.addEventListener("message")` pattern
- **Plugin panel CSS theme injection** — CSS custom properties (`--bg-*`, `--fg-*`, `--border*`, etc.) are automatically injected into plugin panel iframes, so plugins inherit the app theme without manual color copying
- **Auto-delete branch on PR close** — Per-repo setting (off/ask/auto) to automatically delete local branches when their GitHub PR is merged or closed. Handles worktree cleanup, dirty-state escalation, and main-branch protection
- **Worktree system overhaul** — Configurable storage strategies (sibling, app dir, inside-repo), three creation flows (dialog with base ref, instant, right-click quick-clone), hybrid branch naming (`{source}--{random}`), merge & archive workflow, external worktree detection via `.git/worktrees/` monitoring, per-repo worktree settings with global defaults
- **Centralized error log panel** — Ring-buffer logger captures all errors, warnings, and info from app, plugins, git, network, and terminal subsystems. Filterable overlay panel with level tabs, source dropdown, and text search. Status bar badge shows unseen error count. Keyboard shortcut: `Cmd+Shift+E` ([solution doc](docs/solutions/integration-issues/centralized-error-logging.md))
- **Plugin log forwarding** — Plugin `host.log()` calls now appear in the centralized error log panel alongside app-wide logs
- **Agent-scoped plugins** — `agentTypes` manifest field restricts plugin output watchers and structured event handlers to terminals running specific agents (e.g. `["claude"]`). Universal plugins (empty array) continue to receive all events
- **File browser → Markdown viewer routing** — `.md`/`.mdx` files opened from the file browser now open in the Markdown panel instead of the code editor
- **Plugin CLI execution** — `exec:cli` capability allows plugins to run whitelisted CLI binaries (sandboxed: allowlist, timeout, stdout limit, trusted-directory validation)
- **Session prompt tracking** — Built-in `sessionPromptPlugin` reconstructs user-typed input from PTY keystrokes and displays in Activity Center
- **Input line buffer** — Rust-side virtual line editor (`input_line_buffer.rs`) reconstructs typed input from raw PTY keystroke data, supporting cursor movement, word operations, and Kitty protocol sequences
- **mdkb Dashboard plugin** — External installable plugin for viewing mdkb knowledge base status, memories, and configuration
- **API error detection** — Output parser detects API errors (5xx, auth failures) from agents (Claude Code, Aider, Codex CLI, Gemini CLI, Copilot) and provider-level JSON error formats (OpenAI, Anthropic, Google, OpenRouter, MiniMax). Triggers error notification sound and logs to centralized error panel
- **Rust-backed log ring buffer** — Warn/error entries survive webview reloads via `push_log`/`get_logs` Tauri commands
- **Switch Branch submenu** — Main worktree context menu with dirty-tree stash prompt and running-process guard
- **Merged badge** — Branches merged into main show a "Merged" badge in the sidebar
- **Info notification sound type** — Added "info" to per-event notification sounds
- **Tab bar overflow menu** — Right-click scroll arrows to see clipped tabs; `+` button always stays visible
- **Focus-aware dictation** — Transcribed text inserts into focused input element instead of always targeting terminal PTY
- **Auto-fetch interval** — Per-repo setting to periodically `git fetch --all` in the background (5/15/30/60 min), keeping branch stats and ahead/behind counts fresh without manual intervention
- **LLM intent declaration** — Agents emit `[[intent: <action>]]` tokens that the output parser captures and displays in the Activity Dashboard, showing real-time work intent alongside user prompts
- **Mobile Companion UI** — Phone-optimized PWA at `/mobile` for monitoring AI agents remotely. Session list with status cards, live output with quick-reply chips, question overlay banner, activity feed, notification sounds. Installable via Add to Home Screen on iOS Safari and Android Chrome

### Changed
- Agent session restore now shows a clickable banner instead of auto-injecting the resume command
- Migrated ~200 `console.error`/`console.warn` calls to centralized `appLogger` across terminal, hooks, stores, UI components, plugins, and utilities (waves 1-4)
- Activity Dashboard shows last user prompt (>= 10 words) as sub-row with tooltip, now native Rust implementation
- OSC 8 hyperlinks in terminal now open in system browser correctly

### Fixed
- Worktree removal now respects the `deleteBranchOnRemove` setting instead of always deleting the local branch
- File path link underline no longer flickers on mouse hover (cached link provider)
- Rate limit and usage limit badges no longer trigger redundantly on terminal resize
- Terminal focus no longer silently switches to a terminal from another repo
- Rapid branch switching no longer creates duplicate terminals (serialization lock)
- HEAD-changed events during branch rename no longer lose terminal state
- Push-to-talk race condition — fast key release no longer drops transcription
- Claude usage timeline gaps — flush orphan tokens from active sessions
- Merged branch detection hardened with file I/O probing and 5s TTL cache
- **Activity Dashboard state inconsistencies** — `setActive()` no longer resets `shellState` to null; busy flag reconciliation on every PTY chunk prevents "—" status for working terminals; agent polling now covers all terminals (not just the active one)
- **Rate-limit false positives** — Added `line_is_source_code()` guard so agents reading `output_parser.rs` no longer trigger their own rate-limit patterns
- **False "awaiting input" indicator** — Silence-based question detector threshold raised from 5s to 10s; added `line_is_likely_not_a_prompt()` guard to filter code, markdown, and long lines

### Removed
- `showAllBranches` toggle (replaced by Switch Branch submenu)
- `sessionPromptPlugin` built-in (replaced by native Rust last-prompt tracking)

### Documentation
- FEATURES.md: documented tab pinning, branch sorting, Kitty keyboard protocol, PTY pause/resume, MCP registration with Claude CLI

### Security
- **Plugin exec binary resolution hardened** — Removed `which`/`where` PATH lookup; binary resolution now uses only hardcoded trusted directories with symlink canonicalization to prevent symlink attacks
- **Plugin exec stderr truncated** — Error messages from failed CLI commands now truncate stderr to 256 bytes to prevent leaking secrets

### Housekeeping
- **Removed dead wizStoriesPlugin built-in** — Extracted to external plugin; orphaned source and tests cleaned up
- **Replaced wiz-specific example plugins** — `wiz-stories` and `wiz-reviews` examples replaced with generic `report-watcher` and `claude-status` (demonstrates agentTypes)
- **Ideas audit** — Reclassified 4 ideas: PR Merge Readiness → done, Worktree Status Refresh → done (implemented via revision-based reactivity), Structured Agent Output → rejected (requires upstream adoption), Analytics/Editor Settings clarified (editors done, analytics deferred)
- **Plugins submodule updated** — registry.json and README cleaned up, mdkb-dashboard added

### Planned
- **Tab scoping per worktree** — Each worktree/branch will have its own isolated set of tabs instead of sharing a global tab list

### Infrastructure
- **Nightly workflow: move tip tag** — Cleanup job now force-moves the `tip` git tag to the current commit before building, so the release always points to HEAD
- **Makefile: unified CI targets** — Replace `build-github-release` / `publish-github-release` / old `github-release` with two clean targets: `make nightly` (push + tip tag) and `make github-release BUMP=patch` (version bump + tag + CI + publish)
- **Makefile: github-release fixes** — `cargo check` stderr no longer suppressed; run ID lookup matches by commit SHA to avoid race conditions

---

## [0.5.4] - 2026-02-24

### Terminal

- **Ghostty terminal identity** — Switch from kitty to ghostty for Claude Code's terminal detection allow-list (CC v2.1.52 compatibility)
- **Shift+Enter multi-line input** — Sends `\x1b\r` (ESC+CR) for multi-line newlines in Claude Code and other CLI apps
- **Shift+Tab focus fix** — Prevents browser focus navigation while letting xterm send CSI Z to PTY
- **Kitty flags initial sync** — Race condition fix: query kitty flags on listener attach to avoid missed push events
- **Tab close focus transfer** — Closing the active tab now properly focuses the next tab via `handleTerminalSelect` (includes `ref.focus()`)

### Infrastructure

- **Transport layer compliance** — `get_kitty_flags` routed through `usePty`/`transport.ts` with HTTP handler for browser mode
- **Linux CLI resolution** — Added `/usr/bin` to `extra_bin_dirs` for minimal desktop environments
- **Nested session guard** — `env_remove("CLAUDECODE")` prevents "cannot launch inside another CC session" error

### Fixed

- **Windows clippy errors** — Unused variables and collapsible ifs
- **rAF close-all guard** — Prevent crash when concurrent tab closes race with deferred focus callback

---

## [0.5.0] - Unreleased

### Plugin System

- **External plugin loading** — Plugins live in `~/.config/tui-commander/plugins/{id}/` and are loaded at runtime via the `plugin://` URI scheme; hot reload when files change on disk
- **Plugin Settings tab** — Install plugins from a ZIP file or URL, enable/disable, uninstall, view per-plugin logs
- **Community registry / Browse tab** — Discover and install plugins from `sstraus/tuicommander-plugins`; 1-hour TTL cache with manual refresh
- **`tuic://` deep link scheme** — `tuic://install-plugin?url=…`, `tuic://open-repo?path=…`, `tuic://settings?tab=…`
- **Per-plugin error logging** — 500-entry ring-buffer logger per plugin; errors from lifecycle hooks and watchers captured automatically
- **Capability-gated PluginHost API** — Tier 1 (activity/watchers), Tier 2 (read-only state), Tier 3 (PTY write, markdown panel, sound), Tier 4 (whitelisted Tauri invoke)
- **Built-in plugin toggle** — Plan and Stories plugins can be disabled from Settings → Plugins
- **Activity Center bell** — Toolbar bell replaces the plan button; plugins contribute sections and items; supports per-item dismiss and "Dismiss All"
- **4 sample plugins** in `examples/plugins/` demonstrating all capability tiers
- **Plugin filesystem API** — `fs:read`, `fs:list`, `fs:watch` capabilities for sandboxed file access within `$HOME` (10 MB limit, glob filtering, debounced watching via `notify`)
- **Plugin data HTTP endpoint** — `GET /api/plugins/{id}/data/{path}` exposes plugin data to external HTTP clients

### Terminal

- **Detachable terminal tabs** — Float any terminal tab into an independent OS window; re-attach on close
- **Find in Terminal** (`Cmd+F`) — In-terminal search overlay with match count and navigation
- **Configurable keybindings** — Remap any shortcut in Settings → Keyboard Shortcuts; persisted to `~/.config/tui-commander/keybindings.json`
- **iTerm2-style Option key split** — macOS: left Option sends Meta (for Emacs/readline), right Option sends special chars; configurable per repo
- **Per-repo terminal meta hotkeys** — Override Option key behavior per repository in Settings

### Settings Panel

- **Split-view layout** — Vertical nav sidebar + content pane replaces the old dialog
- **Repos in Settings nav** — Each repo appears as a nav item with deep-link open support
- **Keyboard Shortcuts tab** — Browse and rebind all app actions
- **About tab** — App version, links, acknowledgements
- **Appearance tab** — Absorbs former Groups tab; theme, color, font settings in one place
- **Global repo defaults** — Set base branch, color, and other defaults; per-repo settings override only what differs

### File Browser & Editor

- **File browser panel** (`Cmd+E`) — Tree view of the active repository with git status indicators, copy/cut/paste, context menu
- **CodeMirror 6 code editor** — Full editor panel with tab system, syntax highlighting, and file browser integration
- **Markdown edit button** — Pencil icon in MarkdownTab header opens the file in the code editor
- **Clickable file paths** — File references in diff and code panels open in the editor or focused in the IDE
- **Panel search** — Search within code and diff panels
- **Mutually exclusive panels** — File browser, Markdown, and Diff panels are now mutually exclusive to save screen space
- **Drag-resize** — Panel dividers are draggable

### Git & GitHub

- **Diff panel commit dropdown** — Select any recent commit to diff against; Working / Last Commit scope toggle
- **PR notification rich popover** — Click the bell to see PR title, CI status, review state, and open in browser
- **Plan file detection** — Toolbar button lights up when an agent creates a plan file in the active repo
- **GitHub API rate limit handling** — Graceful backoff and UI indicator when GitHub API rate limit is hit

### Agent Support

- **New agents** — Amp, Jules, Cursor, Warp, Ona; brand SVG logos for all supported agents
- **Silence-based question detection** — Recognizes interactive prompts for unrecognized agents via output silence heuristic
- **MCP tools consolidation** — 21 individual MCP tools replaced by 5 meta-commands

### Cross-Platform

- **Windows compatibility** — Platform-aware shell escaping (cmd.exe vs POSIX), foreground process detection via `CreateToolhelp32Snapshot`, Windows paths in `resolve_cli`, IDE detection/launch, `if exist` syntax for lazygit config detection

### Other Added

- **Command Palette** (`Cmd+Shift+P`) — Fuzzy search across all app actions with recent-first ordering
- **Activity Dashboard** (`Cmd+Shift+A`) — Real-time view of all terminal sessions and agent status
- **Park Repos** — Right-click any repo to park it; sidebar footer button shows parked repos with badge count
- **Repository groups context menu** — Right-click any repo to "Move to Group" with "New Group..." option
- **Lazy terminal restore** — Terminal sessions materialize only when clicking a branch, not on startup
- **Check for Updates menu** — In both app menu and Help menu
- **Repo watcher** — Shared file watcher for automatic panel refresh on `.git/` changes
- **Context menu submenus** — ContextMenu supports nested children
- **Remote access QR code** — Shows actual local IP address; HTTPS-only install links; firewall reachability check
- **Auto-hide closed/merged PRs** — PR notifications for closed or merged PRs are automatically dismissed

### Changed

- **Display name** — "TUI Commander" renamed to "TUICommander" across the codebase
- **UX density** — Tighter status bar (22px), toolbar (35px macOS), and sidebar row spacing to match VS Code density
- **Browser/remote mode** — Full compatibility with MCP session events, CORS for any origin, IPv4 binding
- **Status bar icons** — All text labels replaced with monochrome SVG icons; buttons reordered
- **HelpPanel** — Simplified to app info and resource links; keyboard shortcuts moved to Settings
- **Sidebar design** — Flat layout; harmonized git actions and footer; SVG branch/asterisk icons
- **Tab creation UX** — `+` button creates new tab; split options on right-click only
- **CLI resolution** — All `git` and `gh` invocations route through `resolve_cli()` for reliable PATH in release builds
- **Diff panel shortcut** — Remapped from `Cmd+D` to `Cmd+Shift+D`
- **Data persistence guard** — `save()` blocks until `hydrate()` completes to prevent wiping `repositories.json`

### Fixed

- **Lazygit pane ghost terminal** on close
- **xterm fit() minimum dimensions** — Guard prevents crash on zero-size terminal
- **Terminal reattach fit** after floating window closes
- **Splash screen timing** — Deferred removal until stores are fully hydrated
- **Markdown viewer refresh** — Viewer now refreshes after saving a file in the code editor
- **Window-state corruption** — Guard against zero-dimension or off-screen persisted state causing PTY garbage
- **IDE detection in release builds** — `resolve_cli` probes well-known directories
- **Multi-byte UTF-8 panic** — Fixed in rate-limit debug output
- **International keyboard support** — Correct handling of intl input; fewer rate-limit false positives
- **Tab drag-and-drop** — Fixed by working around Tauri's internal drag handler
- **Left Option key state leak** — Reset on `altKey=false` to prevent stuck Meta state
- **PromptDialog hidden on mount** — Dialog now shows correctly when first rendered
- **Browser-mode init freeze** — Fixed hang when session cookie expires
- **Silent failures and memory leak** — P1 issues resolved (floating promises, missing cleanup)
- **Drag-over visual feedback** — Group sections show drop indicator during drag
- **Tab store mutual exclusivity** — Fixed markdown wheel scroll by enforcing only one tab store active at a time
- **Browser mode PTY creation** — Fixed ConnectInfo extraction and keybinding conflicts in remote mode

---

## [0.3.0] - 2026-02-19

### Added
- **Auto-update** - Check for updates on startup via tauri-plugin-updater, download progress badge in status bar, one-click install and relaunch
- **Prevent system sleep** - keepawake integration prevents sleep while agents are working (configurable in Settings)
- **Usage limit badge** - Detects Claude Code "You've used X% of your weekly/session limit" messages and displays a color-coded badge in status bar (blue < 70%, yellow 70-89%, red pulsing >= 90%)
- **Ideas panel** - Renamed Notes to Ideas with lightbulb icon, send-to-terminal and delete actions
- **Terminal session persistence** - Terminal sessions survive app restarts, with activeRepoPath live-sync
- **GitHub GraphQL API** - Replaced `gh pr list` CLI with direct GraphQL for PR statuses, CI checks, and token resolution
- **HEAD file watcher** - Watches `.git/HEAD` for branch changes instead of polling
- **Build & release targets** - Makefile targets for `build-github-release` and `publish-github-release`

### Changed
- **Git status via file reads** - Read branch and remote URL from `.git` files instead of subprocess for better performance
- **Status bar overflow** - Handles long content gracefully
- **Color picker** - Added to settings for theme customization
- **Default theme** - Changed to VS Code Dark, reordered theme lists

### Fixed
- **Empty GitHub token** - Filter empty strings from `gh_token` crate, fall back to `gh auth token` CLI
- **Agent resume commands** - Updated resume commands for OpenCode and Aider
- **Download progress bar** - Fixed layout in Dictation Settings
- **PTY environment** - Set `TERM=xterm-256color`, `COLORTERM`, and `LANG` for proper color and UTF-8 support
- **Branch name overflow** - Text ellipsis on long branch names in sidebar
- **Branch name styling** - Font size and color consistency
- **Worktree button** - Disabled during creation to prevent double-clicks
- **CI builds** - Linux `libasound2-dev` dependency, macOS notarization, Windows process group guard

---

## [0.2.0] - 2026-02-18

### Added
- **Terminal context menu** - Split right/left/down/up, reset terminal, change title
- **PR state badges** - Replace CI ring with merge/review state badges in sidebar
- **PR clickable links** - PR number opens GitHub in browser
- **Rate limit warning** - Badge in status bar when AI agents hit rate limits
- **Question detection** - Recognizes interactive prompts in terminal, shows ? icon in sidebar
- **Dock badge count** - macOS dock badge for attention-requiring notifications
- **Auto-show PR popover** - Optional setting to auto-display PR details
- **Splash screen** - Branded loading screen on app start
- **Repo header context menu** - Right-click on repo header in sidebar
- **Smart branch terminal spawn** - Auto-spawns terminal only on first branch select; respects user intent when all tabs are closed

### Changed
- **Design system tokens** - Migrated all hardcoded CSS values to CSS custom properties
- **WCAG AA compliance** - Theme-aware text-on-color system, contrast fixes across all UI elements
- **Standardized sizing** - Consistent button, badge, and input dimensions
- **Sci-fi worktree names** - More creative auto-generated worktree names
- **OSC title cleaning** - Filters shell script noise and extracts useful command names
- **Lazygit tab naming** - Explicitly sets tab name to avoid polluted OSC titles
- **Tauri webview build targets** - Optimized build configuration

### Fixed
- **macOS "Restored session" message** - Suppressed by setting TERM_PROGRAM in PTY
- **PR popover layout** - Improved readability and positioning
- **Hotkey macOS symbols** - Correct modifier symbol translation
- **Sidebar badge layout** - Proper alignment and spacing
- **MCP HTTP port conflict** - Server falls back to localhost on port conflict
- **WebGL canvas fallback** - Graceful degradation when WebGL addon fails
- **ErrorBoundary crash screen** - Shows recovery UI instead of blank screen

---

## [0.1.0] - 2026-02-04

### Added
- Initial TUICommander implementation
- **Multi-terminal support** - Up to 50 concurrent PTY sessions
- **Repository sidebar** - Hierarchical view of repositories with branches and worktrees
- **Tab bar** - Terminal tabs with keyboard shortcuts (Cmd+1-9)
- **Git worktree integration** - Create and manage git worktrees from the UI
- **Per-pane zoom** - Independent font size control per terminal (Cmd+Plus/Minus)
- **VS Code / Claude Code launchers** - Quick access buttons in status bar
- **Markdown preview panel** - Toggle with MD button
- **Diff preview panel** - Toggle with Diff button
- **Session persistence** - Terminals maintain state across tab switches
- **Branch-terminal association** - Terminals are tracked per branch

### Architecture
- **Frontend**: SolidJS + TypeScript + Vite
- **Backend**: Tauri (Rust) for native performance
- **Terminal**: xterm.js with WebGL renderer
- **State Management**: Custom stores (terminals, repositories)

### Known Issues
- Tabs from all worktrees visible when switching branches (fix planned)
