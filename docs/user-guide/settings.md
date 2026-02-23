# Settings

Open settings with `Cmd+,`. Settings are organized into tabs.

## General Tab

| Setting | Description |
|---------|-------------|
| **Language** | UI language |
| **Default IDE** | IDE for "Open in..." actions. Detected IDEs grouped by category: Editors (VS Code, Cursor, Zed, Sublime, Vim, Neovim, Emacs), IDEs (IntelliJ, WebStorm, PyCharm, GoLand, etc.), Terminals (iTerm2, Warp, Kitty, etc.), Git (GitKraken, GitHub Desktop, Tower, etc.) |
| **Shell** | Custom shell path (e.g., `/bin/zsh`, `/usr/local/bin/fish`). Leave empty for system default. |
| **Confirm before quitting** | Show dialog when closing app with active terminals |
| **Confirm before closing tab** | Ask before closing terminal tab |
| **Prevent sleep when busy** | Keep machine awake while agents are working |
| **Auto-check for updates** | Check for new versions on startup |
| **Auto-show PR popover** | Automatically display PR details when switching branches. Only shows for OPEN pull requests — CLOSED PRs are hidden, and MERGED PRs fade after 5 minutes of user activity. |
| **Repository defaults** | Base branch, file handling, setup/run scripts applied to new repos |

## Appearance Tab

| Setting | Description |
|---------|-------------|
| **Terminal theme** | Color theme with preview swatches |
| **Terminal font** | 13 bundled monospace fonts (JetBrains Mono default): Fira Code, Hack, Cascadia Code, Source Code Pro, IBM Plex Mono, Inconsolata, Ubuntu Mono, Anonymous Pro, Roboto Mono, Space Mono, Monaspace Neon, Geist Mono |
| **Default font size** | 8–32px slider. Applies to new terminals; existing terminals keep their zoom level. |
| **Split tab mode** | Separate or unified tab appearance |
| **Max tab name length** | 10–60 slider |
| **Repository groups** | Create, rename, delete, and color-code groups |
| **Reset panel sizes** | Restore sidebar and panel widths to defaults |

## Agents Tab

Each supported agent has an expandable row showing detection status, version, and MCP badge.

| Setting | Description |
|---------|-------------|
| **Agent Detection** | Auto-detects running agents from terminal output patterns. Shows "Available" or "Not found" for each agent. |
| **Run Configurations** | Custom launch configs (binary path, args, model, prompt) per agent. Add, set default, or delete configurations. |
| **MCP Integration** | Install/remove TUICommander as MCP server for supported agents. Shows install status with a dot indicator. |
| **Claude Usage Dashboard** | (Claude Code only) Toggle under Features when the Claude row is expanded. Enables rate limit monitoring, session analytics, token usage charts, activity heatmap, and per-project breakdowns. Usage data appears in the status bar agent badge and in a dedicated dashboard tab. |

See [AI Agents](ai-agents.md) for details on agent detection, rate limits, and the usage dashboard.

## Services Tab

### MCP HTTP Server

Enable an HTTP API server on localhost for external tool integration:
- Exposes terminal sessions, git operations, and agent spawning
- Used by Claude Code, Cursor, and other tools via MCP protocol
- Shows server status (running/stopped), port, and active session count

### Remote Access

Enable HTTP/WebSocket access from other devices on your network. See [Remote Access](remote-access.md) for full setup guide.

### Voice Dictation

See [Voice Dictation](dictation.md) for full details.

## Keyboard Shortcuts Tab

Browse and rebind all app actions:

- Every registered action is listed with its current keybinding
- Click any action row and press a new key combination to rebind it
- Custom bindings are stored in `keybindings.json` in the platform config directory
- Auto-populated from the action registry — new actions appear automatically

See [Keyboard Shortcuts](keyboard-shortcuts.md) for the full reference and customization guide.

## Plugins Tab

Install, manage, and browse plugins. See [Plugins](plugins.md) for the full guide.

- **Installed** — List all plugins with enable/disable toggle, logs viewer, uninstall
- **Browse** — Discover and install from the community registry

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
