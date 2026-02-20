# Changelog

All notable changes to TUI Commander will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- **Windows compatibility** - Full cross-platform support: platform-aware shell escaping (cmd.exe vs POSIX), Windows foreground process detection via `CreateToolhelp32Snapshot`, Windows paths in `resolve_cli`, IDE detection/launch for Windows, `if exist` syntax for lazygit config detection
- **Repository groups context menu** - Right-click any repo to "Move to Group" with "New Group..." option that creates and assigns in one step
- **PromptDialog component** - Reusable native dialog replacing `window.prompt()` which doesn't work in Tauri's webview; used for terminal rename and group creation
- **Lazy terminal restore** - Terminal sessions no longer eagerly restore on app startup; they materialize only when clicking the branch in the sidebar
- **Check for Updates menu** - "Check for Updates" item in both app menu and Help menu, wired to the updater store
- **Repo watcher** - Shared file watcher for automatic GitOperationsPanel refresh when repository files change
- **Context menu submenus** - ContextMenu component now supports nested children for submenu rendering
- **`github-release` Makefile target** - One-command releases via `make publish-github-release`

### Changed
- **CLI resolution via `resolve_cli`** - All `git` and `gh` command invocations now route through `resolve_cli()` for reliable PATH resolution in desktop-launched apps
- **Tab creation UX** - `+` button creates new tab on click; split options on right-click only
- **Data persistence guard** - `save()` now blocks before `hydrate()` completes to prevent nuking persisted `repositories.json`

### Fixed
- **Window-state corruption** - Guard against zero-dimension or off-screen windows from persisted state causing PTY garbage output
- **IDE detection in release builds** - `resolve_cli` probes well-known directories when tools aren't on PATH
- **Multi-byte UTF-8 panic** - Fixed panic on multi-byte chars in rate-limit debug output
- **International keyboard support** - Terminal now handles intl keyboard input correctly; reduced rate-limit false positives
- **Tab drag-and-drop** - Fixed broken DnD caused by Tauri's internal drag handler
- **Updater error message** - Friendly message when no published releases exist instead of raw error
- **Drag-over visual feedback** - Added visual indicators when dragging repos over group sections

### Planned
- **Tab scoping per worktree** - Each worktree/branch will have its own isolated set of tabs instead of sharing a global tab list

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
- Initial TUI Commander implementation
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
