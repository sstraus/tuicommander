# Getting Started

## What is TUICommander?

TUICommander is a desktop terminal orchestrator for running multiple AI coding agents in parallel (Claude Code, Gemini CLI, Aider, OpenCode, Codex). Built with Tauri + SolidJS + xterm.js + Rust.

**Key capabilities:**
- Up to 50 concurrent terminal sessions with split panes and detachable tabs
- 10 AI agents supported (Claude Code, Codex, Aider, Gemini, Amp, and more)
- Git worktree isolation per branch
- GitHub PR monitoring with CI status and notifications
- Obsidian-style plugin system with community registry
- Voice dictation with local Whisper (no cloud)
- Command palette, configurable keybindings, prompt library
- Built-in file browser and code editor
- MCP HTTP server for external AI tool integration
- Remote access from a browser on another device
- Auto-update, 13 bundled fonts, cross-platform (macOS, Windows, Linux)

## First Launch

1. **Add a repository** — Click the `+` button at the top of the sidebar, or use the "Add Repository" option. Select a git repository folder.

2. **Select a branch** — Click a branch name in the sidebar. If it's not the main branch, TUICommander creates a git worktree so you work in an isolated copy.

3. **Start typing** — The terminal is ready. Your default shell is loaded. Type commands, run AI agents, or execute scripts.

4. **Open more tabs** — Press `Cmd+T` (macOS) or `Ctrl+T` (Windows/Linux) to add more terminal tabs for the same branch.

## Workflow Overview

```
Add Repository → Select Branch → Worktree Created → Terminal Opens
                                                    ├── Run AI agent
                                                    ├── Open more tabs
                                                    ├── Split panes
                                                    └── View diffs/PRs
```

Each branch has its own set of terminals. When you switch branches, your previous terminals are preserved and hidden. Switch back and they reappear exactly as you left them.

## Sidebar

The sidebar shows all your repositories and their branches.

**Repository groups:**
- Repositories can be organized into named, colored groups
- Drag a repo onto a group header to move it
- Right-click a group to rename, change color, or delete it
- Groups collapse/expand by clicking the group header

**Repository entry:**
- Click to expand/collapse branch list
- Click again to toggle icon-only mode (shows initials)
- `+` button: Create new worktree for a new branch
- `⋯` button: Repo settings, remove, move to group

**Branch entry:**
- Click: Switch to this branch (shows its terminals)
- Double-click the branch name: Rename branch
- CI ring: Shows CI check status (green/red/yellow segments)
- PR badge: Shows PR number with color-coded state — click for detail popover
- Stats: Shows +additions/-deletions

**Git quick actions** (bottom of sidebar when a repo is active):
- Pull, Push, Fetch, Stash buttons — run the git command in the active terminal

## Next Steps

- [Terminal Features](terminals.md) — Tabs, splits, zoom, detachable tabs, find-in-terminal
- [Sidebar](sidebar.md) — Repos, branches, groups, park repos, quick branch switcher
- [AI Agents](ai-agents.md) — Agent detection, rate limits, fallback chains, question detection
- [Keyboard Shortcuts](keyboard-shortcuts.md) — All shortcuts and how to customize them
- [Command Palette & Activity Dashboard](command-palette.md) — Fuzzy-search actions and monitor sessions
- [File Browser & Code Editor](file-browser.md) — Browse files, edit code, git status
- [GitHub Integration](github-integration.md) — PR monitoring, CI rings, notifications
- [Git Worktrees](worktrees.md) — Worktree workflow, configuration
- [Lazygit](lazygit.md) — Three modes for lazygit integration
- [Prompt Library](prompt-library.md) — Template management, variables
- [Plugins](plugins.md) — Install, browse, and manage plugins
- [Voice Dictation](dictation.md) — Push-to-talk setup
- [Remote Access](remote-access.md) — Access from a browser on another device
- [Settings](settings.md) — All configuration options
