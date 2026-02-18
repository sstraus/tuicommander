# Changelog

All notable changes to TUI Commander will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Planned
- **Tab scoping per worktree** - Each worktree/branch will have its own isolated set of tabs instead of sharing a global tab list

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
