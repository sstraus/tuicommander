<p align="center">
  <img src="assets/logo.png" alt="TUICommander" width="128" />
</p>

<h1 align="center">TUICommander</h1>

<p align="center">
  <strong>The native platform for AI coding agents.<br>Run, monitor, and extend Claude Code, Codex, Aider, and more — no IDE required.</strong>
</p>

<p align="center">
  <a href="https://github.com/sstraus/tuicommander/releases/latest"><img src="https://img.shields.io/github/v/release/sstraus/tuicommander?style=flat-square&color=blue" alt="Release"></a>
  <a href="https://github.com/sstraus/tuicommander/releases/tag/tip"><img src="https://img.shields.io/badge/nightly-tip-orange?style=flat-square" alt="Nightly"></a>
  <a href="https://github.com/sstraus/tuicommander/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/sstraus/tuicommander/ci.yml?style=flat-square&label=CI" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/sstraus/tuicommander?style=flat-square" alt="License"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey?style=flat-square" alt="Platform">
  <img src="https://img.shields.io/badge/rust-backend-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-v2-24C8D8?style=flat-square&logo=tauri&logoColor=white" alt="Tauri v2">
</p>

<p align="center">
  <a href="https://github.com/sstraus/tuicommander/releases/latest"><strong>Download</strong></a> &bull;
  <a href="https://github.com/sstraus/tuicommander/releases/tag/tip"><strong>Nightly</strong></a> &bull;
  <a href="docs/FEATURES.md"><strong>All Features</strong></a> &bull;
  <a href="docs/plugins.md"><strong>Plugin API</strong></a> &bull;
  <a href="docs/"><strong>Docs</strong></a>
</p>

---

<p align="center">
  <img src="assets/tui-screenshot.png" alt="TUICommander — multiple agents, split panes, sidebar with repos" width="900" />
</p>

---

## The problem

You're running Claude Code in one terminal, Aider in another, Codex in a third. One hit a rate limit 10 minutes ago and you didn't notice. Another is waiting for a Y/N confirmation. You switch to VS Code to check a diff. You open GitHub to see if CI passed.

The more agents you run, the harder it gets to keep track.

## The solution

**TUICommander is one native window for all of it.** Terminals, diffs, PRs, CI status, agent monitoring — everything an AI-assisted workflow needs, without an IDE.

---

## Why TUICommander (vs. tmux, VS Code, Warp)

| Pain point | TUICommander | tmux / terminal tabs | VS Code |
|---|---|---|---|
| Agent hits rate limit silently | Auto-detects, shows countdown, switches to fallback agent | You don't know until you look | No detection |
| Agent asks a question and stalls | Tab indicator changes, notification sound, overlay with options | Silent stall | Silent stall |
| Need isolated workspace per agent | Git worktree auto-created per branch — each agent works on its own copy | Manual `git worktree add` | Manual |
| Checking what agents changed | Inline diff panel, PR status, CI rings — right next to the terminal | Switch to another tool | Built-in, but agents run externally |
| Running 5+ agents on the same project | Sidebar shows every branch with status, split panes, activity dashboard | Pane hell | Terminal tab hell |
| External AI tools need terminal access | MCP server exposes everything via HTTP/WebSocket/SSE | Not possible | Limited extensions |

---

## Core: Agent Orchestration

TUICommander detects and monitors **10 AI coding agents** automatically:

Claude Code, Codex CLI, Aider, Gemini CLI, Amp, Jules, Cursor Agent, OpenCode, Warp Oz, ONA

### What "monitoring" actually means

- **Rate limit detection** — Provider-specific patterns (Claude "overloaded", Gemini "429", OpenAI "too many requests"). Status bar shows countdown timers per session.
- **Fallback chains** — Configure primary + fallback agents. When Claude hits a rate limit, auto-switch to Gemini. Auto-recovery checks every 5 minutes.
- **Question detection** — Recognizes Y/N prompts, numbered options, inquirer-style menus. Tab shows `?` icon, notification sound fires, overlay appears with keyboard navigation (`↑/↓`, `Enter`, `1-9`).
- **Usage tracking** — Claude Code weekly/session limit percentage: blue < 70%, yellow 70–89%, red pulsing >= 90%.
- **Activity dashboard** (`Cmd+Shift+A`) — Real-time view of all sessions: agent type, status (working / waiting / rate-limited / idle), last activity timestamp.

### The workflow

```
Add repo → Click branch → Worktree auto-created → Terminal opens in isolated copy
                                                   ├── Run Claude Code
                                                   ├── Split pane → Run Aider
                                                   ├── Switch branch → Run Codex
                                                   └── Activity dashboard shows all 3
```

Each branch gets its own git worktree. Each agent works in isolation. Switch branches and your terminals are preserved — switch back and they're exactly as you left them.

---

## Core: See What Your Agents Changed

The feedback loop happens *in the same window*, not in a browser tab:

- **Diff panel** (`Cmd+Shift+D`) — Working tree diff or any of the last 5 commits. Click a file for inline diff.
- **PR status** — Sidebar shows PR badges (open/merged/closed/draft), CI ring (green/red/yellow arcs), review state (approved/changes requested). Click for detail popover.
- **PR notifications** — Toolbar bell with rich popovers: merged, conflicts, CI failed, changes requested. Auto-dismiss when PRs close.
- **Clickable file paths** — File references in terminal output become links. `.md` opens in the viewer, code files open in the editor at the right line.
- **Built-in code editor** — CodeMirror 6 with syntax highlighting, disk conflict detection. Edit without leaving the window.

All git data comes from GitHub GraphQL API and `.git/` file reads — no subprocess polling, no `gh` CLI dependency.

---

## Core: Extend Everything

### MCP HTTP Server

TUICommander exposes a **Model Context Protocol server** on localhost:

- REST, WebSocket, and SSE endpoints
- Terminal sessions, git operations, agent spawning — all accessible
- Used by Claude Code, Cursor, and other AI tools to interact with your terminals

This means external AI tools can read your terminal output, send commands, check git status — through a standard protocol.

### Plugin System

Obsidian-style plugins with hot reload and a community registry:

- **4 capability tiers** — From read-only watchers to PTY write access
- **Terminal output watchers** — Regex patterns that trigger actions
- **Activity Center** — Plugins contribute notifications to the toolbar bell
- **Markdown providers** — Plugins can render custom panels
- **Filesystem API** — Sandboxed read/list/watch within `$HOME`
- **Community registry** — Browse and install with one click, or via `tuic://install-plugin?url=...`

[Plugin Authoring Guide →](docs/plugins.md)

---

## Everything Else

<details>
<summary><strong>Terminal features</strong> — 50 sessions, splits, detach, find, persistence</summary>

- Up to 50 concurrent PTY sessions, each with independent zoom (8–32px)
- Split panes: vertical (`Cmd+\`) or horizontal (`Cmd+Alt+\`) with drag-resize
- Detachable tabs: float any terminal into its own OS window, re-attaches on close
- Find in terminal (`Cmd+F`): regex, case-sensitive, whole word, match navigation
- Session persistence: terminals survive restarts with lazy restore on branch click
- Tab management: reorder by drag, rename by double-click, reopen last 10 closed tabs
- International keyboard support
</details>

<details>
<summary><strong>Git integration</strong> — Worktrees, lazygit, quick actions</summary>

- Auto-create worktrees per branch with configurable base branch and setup scripts
- Lazygit: inline (`Cmd+G`), split pane (`Cmd+Shift+L`), or floating window
- Git Operations Panel (`Cmd+Shift+G`): pull, push, fetch, stash, merge, checkout, conflict resolution
- Repository groups: named, colored, collapsible, drag-and-drop reordering
- Park repos: temporarily hide repos you're not using
- Quick branch switcher: hold `Cmd+Ctrl`, press `1-9` to switch instantly
</details>

<details>
<summary><strong>Voice dictation</strong> — Local Whisper, push-to-talk, GPU-accelerated</summary>

- On-device speech-to-text via whisper-rs — no cloud, no API keys
- GPU-accelerated on macOS (Metal), CPU fallback on Windows/Linux
- 5 model sizes from tiny (75 MB) to large-v3-turbo (1.6 GB)
- Push-to-talk hotkey (default `F5`) or hold the mic button
- Text correction dictionary for domain-specific substitutions
</details>

<details>
<summary><strong>Productivity</strong> — Palette, keybindings, prompts, IDE launchers</summary>

- Command palette (`Cmd+Shift+P`): fuzzy search across all actions with recency ranking
- Configurable keybindings: remap any shortcut in Settings
- Prompt library (`Cmd+K`): saved prompts with `{{variable}}` substitution (`{{diff}}`, `{{branch}}`, `{{cwd}}`, custom)
- IDE launchers: open in VS Code, Cursor, Zed, or any detected editor with one click
- Ideas panel (`Cmd+N`): quick notes with send-to-terminal
- Run commands (`Cmd+R`): per-repo saved commands
- 13 bundled monospace fonts
</details>

<details>
<summary><strong>Remote access</strong> — Browser UI, MCP server, deep links</summary>

- Access TUICommander from a browser on another device (WebSocket terminal streaming)
- Basic Auth with bcrypt-hashed passwords
- QR code with local IP for quick mobile/tablet connection
- `tuic://` deep link scheme: install plugins, open repos, navigate settings
</details>

> **Full feature reference:** **[docs/FEATURES.md](docs/FEATURES.md)**

---

## Get started

**[Download the latest release](https://github.com/sstraus/tuicommander/releases/latest)** — macOS builds are signed and notarized.

Want the bleeding edge? The **[Nightly](https://github.com/sstraus/tuicommander/releases/tag/tip)** is rebuilt on every push to `main`.

<details>
<summary>Build from source</summary>

**Prerequisites:** Node.js 22+, Rust toolchain, [Tauri CLI](https://tauri.app/start/)

```bash
npm install
npm run tauri dev      # Development with hot reload
npm run tauri build    # Production build
npm test               # Run tests
```

See [docs/guides/development-setup.md](docs/guides/development-setup.md) for platform-specific instructions.
</details>

## Built with

Rust + [Tauri v2](https://tauri.app) backend, [SolidJS](https://solidjs.com) UI, [xterm.js](https://xtermjs.org) + WebGL terminals, [CodeMirror 6](https://codemirror.net) editor, [whisper-rs](https://github.com/tazz4843/whisper-rs) dictation, [Vite](https://vite.dev) + LightningCSS build.

## Documentation

| | |
|---|---|
| [Getting Started](docs/user-guide/getting-started.md) | First-run guide |
| [Features](docs/FEATURES.md) | Complete feature reference with all keyboard shortcuts |
| [AI Agents](docs/user-guide/ai-agents.md) | Agent detection, fallback chains, rate limits |
| [Plugin API](docs/plugins.md) | Build plugins for TUICommander |
| [HTTP API](docs/api/http-api.md) | REST/WebSocket/SSE endpoints |
| [Architecture](docs/ARCHITECTURE.md) | System design and component overview |
| [Development Setup](docs/guides/development-setup.md) | Build from source |

---

<p align="center">TUICommander is in active development — new features land weekly.<br><a href="https://github.com/sstraus/tuicommander/releases">Follow the releases</a> to see what's new.</p>

## License

[MIT](LICENSE) &copy; 2026 Stefano Straus
