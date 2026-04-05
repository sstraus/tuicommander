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

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| **Terminal theme** | — | — | Color theme with preview swatches |
| **Terminal font** | — | JetBrains Mono | 13 bundled monospace fonts: Fira Code, Hack, Cascadia Code, Source Code Pro, IBM Plex Mono, Inconsolata, Ubuntu Mono, Anonymous Pro, Roboto Mono, Space Mono, Monaspace Neon, Geist Mono |
| **Default font size** | — | — | 8–32px slider. Applies to new terminals; existing terminals keep their zoom level. |
| **Split tab mode** | — | — | Separate or unified tab appearance |
| **Max tab name length** | — | — | 10–60 slider |
| **Repository groups** | — | — | Create, rename, delete, and color-code groups |
| **Reset panel sizes** | — | — | Restore sidebar and panel widths to defaults |
| **Copy on Select** | `boolean` | `true` | Auto-copy terminal selection to clipboard |
| **Bell Style** | `none/visual/sound/both` | `visual` | Terminal bell behavior |

## Agents Tab

Each supported agent has an expandable row showing detection status, version, and MCP badge.

| Setting | Description |
|---------|-------------|
| **Agent Detection** | Auto-detects running agents from terminal output patterns. Shows "Available" or "Not found" for each agent. |
| **Run Configurations** | Custom launch configs (binary path, args, model, prompt) per agent. Add, set default, or delete configurations. A config named **"review"** enables the Review button in the PR Detail Popover — its args are interpolated with `{pr_number}`, `{branch}`, `{base_branch}`, `{repo}`, `{pr_url}`. |
| **MCP Integration** | Install/remove TUICommander as MCP server for supported agents. Shows install status with a dot indicator. |
| **Claude Usage Dashboard** | (Claude Code only) Toggle under Features when the Claude row is expanded. Enables rate limit monitoring, session analytics, token usage charts, activity heatmap, and per-project breakdowns. Usage data appears in the status bar agent badge and in a dedicated dashboard tab. |

See [AI Agents](ai-agents.md) for details on agent detection, rate limits, and the usage dashboard.

## GitHub Tab

GitHub authentication and token management:

| Setting | Description |
|---------|-------------|
| **OAuth Login** | Device Flow login — click "Sign in with GitHub", enter code on github.com. Token stored in OS keyring. |
| **Auth Status** | Shows current login, avatar, token source (OAuth/env/CLI), and available scopes |
| **Disconnect** | Clear all GitHub tokens (keyring + env cache). Falls back to next available source. |
| **Diagnostics** | Token source details, scope verification, API connectivity check |

Token priority: `GH_TOKEN` env → `GITHUB_TOKEN` env → OAuth keyring → `gh` CLI config → `gh auth token` subprocess.

## Services Tab

### HTTP API Server

Enable the HTTP API server for external tool integration:
- Serves the REST API and MCP protocol for AI agents and automation tools
- Local MCP connections use a Unix domain socket at `<config_dir>/mcp.sock` — no port configuration needed
- AI agents connect via the `tuic-bridge` sidecar (auto-installed on first launch for Claude Code, Cursor, Windsurf, VS Code, Zed, Amp, Gemini)
- Shows server status (running/stopped) and active session count

### TUIC Tools

Native tools exposed to AI agents via MCP. Each tool can be individually enabled or disabled to restrict what agents can access.

**Collapse tools** (checkbox) — when enabled, replaces the full tool list sent to AI agents with 3 lazy-discovery meta-tools (`search_tools`, `get_tool_schema`, `call_tool`). Cuts the baseline MCP context cost from ~35k tokens to ~500 tokens per agent turn; the agent fetches schemas on demand via BM25-ranked search. Default: off. Toggling refreshes connected clients via `notifications/tools/list_changed`.

Tools:
- **session** — PTY terminal session management
- **git** — Repository state queries
- **agent** — AI agent detection and spawning
- **config** — App configuration read/write
- **workspace** — Repo and worktree queries
- **notify** — User notifications (toast, confirm)
- **plugin_dev_guide** — Plugin authoring reference

### Upstream MCP Servers

Proxy external MCP servers through TUICommander. Their tools appear prefixed as `{name}__{tool}`:
- Add upstream servers via HTTP (Streamable MCP) or stdio (process) transport
- API keys for HTTP upstreams are stored in the OS keychain
- Live status (connecting, ready, circuit open, failed) with tool count and call metrics
- Reconnect and remove controls per upstream
- Per-repo scoping: each repo can define an allowlist of active upstream servers via **Cmd+Shift+M** popup (or repo settings). Empty/null allowlist = all servers active

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
- **Archive Script** — Runs before a worktree is archived or deleted; non-zero exit blocks the operation

### Repo-Local Config (`.tuic.json`)

A `.tuic.json` file in the repository root provides team-shareable settings that override per-repo app settings and global defaults. The file is read-only from TUICommander (edit it in your repo directly).

**Precedence:** `.tuic.json` > per-repo app settings > global defaults

Supported fields: `base_branch`, `copy_ignored_files`, `copy_untracked_files`, `setup_script`, `run_script`, `archive_script`, `worktree_storage`, `delete_branch_on_remove`, `auto_archive_merged`, `orphan_cleanup`, `pr_merge_strategy`, `after_merge`, `auto_delete_on_pr_close`.

User-specific settings (`promptOnCreate`, `autoFetchIntervalMinutes`) are intentionally excluded from `.tuic.json`.

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
