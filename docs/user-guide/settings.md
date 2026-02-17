# Settings

Open settings with `Cmd+,`. Settings are organized into tabs.

## General Tab

### Terminal Font

Choose the monospace font for terminal display. Bundled fonts (no installation needed):

- JetBrains Mono (default)
- Fira Code
- Hack
- Cascadia Code
- Source Code Pro
- IBM Plex Mono
- Inconsolata
- Ubuntu Mono
- Anonymous Pro
- Roboto Mono
- Space Mono

### Default Font Size

8px to 32px. Applies to new terminals. Existing terminals keep their individual zoom level.

### Shell

Custom shell path (e.g., `/bin/zsh`, `/usr/local/bin/fish`). Leave empty for system default:
- macOS: `/bin/zsh`
- Linux: `$SHELL` or `/bin/bash`
- Windows: `powershell.exe`

### Default IDE

IDE used for "Open in..." actions:

**Editors:** VS Code, Cursor, Zed, Sublime Text, Vim, Neovim, Emacs
**IDEs:** IntelliJ IDEA, WebStorm, PyCharm, GoLand, Rider, CLion, RustRover, Android Studio
**Terminals:** iTerm2, Warp, Kitty, Alacritty, Hyper, WezTerm
**Git:** GitKraken, GitHub Desktop, Tower, Sourcetree, Fork
**Utilities:** Finder, Terminal

### Terminal Theme

Color theme for terminals. Multiple themes available.

### Confirmations

- **Confirm before quitting** — Show dialog when closing app with active terminals
- **Confirm before closing tab** — Ask before closing terminal tab

## Agents Tab

### Primary Agent

The default AI coding agent for new sessions:
- Claude Code
- Gemini CLI
- OpenCode
- Aider
- Codex

### Auto-Recovery

When enabled, TUI Commander periodically checks if the primary agent becomes available again after switching to a fallback.

Recovery interval: 5 minutes.

### Reset to Primary

Force reset the active agent back to the primary agent immediately.

## Services Tab

### MCP HTTP Server

Enable an HTTP API server on localhost for external tool integration:
- Exposes terminal sessions, git operations, and agent spawning
- Used by Claude Code, Cursor, and other tools via MCP protocol
- Shows server status (running/stopped), port, and active session count

### Remote Access

Enable HTTP/WebSocket access from other devices on your network:

- **Port** — Default 9876, range 1024-65535
- **Username** — Basic Auth username
- **Password** — Basic Auth password (stored as bcrypt hash)
- **URL** — Shown after configuration: `http://<ip>:<port>`

### Voice Dictation

See [Voice Dictation](dictation.md) for full details.

- Enable/disable dictation
- Hotkey configuration (default: F5)
- Language selection
- Model selection and download
- Audio device selection
- Text corrections dictionary

## Repository Settings

Per-repository settings accessed via sidebar `⋯` → "Repo Settings".

### Worktree Tab

- **Display Name** — Custom name shown in sidebar
- **Base Branch** — Branch to create worktrees from (auto-detect, main, master, develop)
- **Copy ignored files** — Copy .gitignored files to new worktrees
- **Copy untracked files** — Copy untracked files to new worktrees

### Scripts Tab

- **Setup Script** — Runs once after worktree creation (e.g., `npm install`)
- **Run Script** — On-demand script launchable from toolbar with `Cmd+R`

## Notification Settings

- **Enable Audio Notifications** — Master toggle
- **Volume** — 0-100%
- **Per-event toggles:**
  - Agent asks question
  - Error occurred
  - Task completed
  - Warning
- **Test buttons** — Test each sound individually
- **Reset to Defaults** — Restore default notification settings
