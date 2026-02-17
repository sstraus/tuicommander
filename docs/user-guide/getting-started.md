# Getting Started

## What is TUI Commander?

TUI Commander is a desktop terminal orchestrator for running multiple AI coding agents in parallel (Claude Code, Gemini CLI, Aider, OpenCode, Codex). Built with Tauri + SolidJS + xterm.js + Rust.

**Key capabilities:**
- Up to 50 concurrent terminal sessions
- Git worktree isolation per branch
- GitHub PR monitoring with CI status
- AI agent detection and fallback chains
- Voice dictation with local Whisper
- Prompt library with variable substitution
- Split terminal panes
- Remote access via HTTP/WebSocket

## First Launch

1. **Add a repository** — Click the `+` button at the top of the sidebar, or use the "Add Repository" option. Select a git repository folder.

2. **Select a branch** — Click a branch name in the sidebar. If it's not the main branch, TUI Commander creates a git worktree so you work in an isolated copy.

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

**Repository entry:**
- Click to expand/collapse branch list
- Click again to toggle icon-only mode
- `+` button: Create new worktree for a new branch
- `⋯` button: Repo settings, remove repository

**Branch entry:**
- Click: Switch to this branch (shows its terminals)
- CI ring: Shows CI check status (green/red/yellow segments)
- PR badge: Shows PR number with color-coded state
- Stats: Shows +additions/-deletions

## Next Steps

- [Keyboard Shortcuts](keyboard-shortcuts.md) — All shortcuts
- [Terminal Features](terminals.md) — Tabs, splits, zoom, copy/paste
- [Settings](settings.md) — All configuration options
- [Voice Dictation](dictation.md) — Push-to-talk setup
- [GitHub Integration](github-integration.md) — PR monitoring, CI rings
