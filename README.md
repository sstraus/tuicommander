<p align="center">
  <img src="assets/logo.png" alt="TUICommander" width="128" />
</p>

<h1 align="center">TUICommander</h1>

<p align="center">
  <strong>The native platform for AI coding agents.<br>Run, monitor, and extend Claude Code, Codex, Aider, and more — no IDE required.</strong>
</p>

<p align="center">
  <a href="https://github.com/sstraus/tui-commander/releases"><img src="https://img.shields.io/github/v/release/sstraus/tui-commander?style=flat-square&color=blue" alt="Release"></a>
  <a href="https://github.com/sstraus/tui-commander/releases/tag/tip"><img src="https://img.shields.io/badge/nightly-tip-orange?style=flat-square" alt="Nightly"></a>
  <a href="https://github.com/sstraus/tui-commander/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/sstraus/tui-commander/ci.yml?style=flat-square&label=CI" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/sstraus/tui-commander?style=flat-square" alt="License"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey?style=flat-square" alt="Platform">
  <img src="https://img.shields.io/badge/rust-backend-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-v2-24C8D8?style=flat-square&logo=tauri&logoColor=white" alt="Tauri v2">
</p>

---

<p align="center">
  <img src="assets/tui-screenshot.png" alt="TUICommander — multiple agents, split panes, sidebar with repos" width="900" />
</p>

---

<p align="center"><em>TUI stands for Terminal User Interface — because your AI agents live in the terminal, and so does TUICommander.</em></p>

## Your Terminal Agents Deserve a Real Home

If you use **Claude Code**, **Gemini CLI**, **Aider**, or any terminal-based AI agent, you know the drill: a dozen terminal windows, no idea which agent is stuck on a rate limit, context-switching between repos, and an IDE open just to check a diff or read a markdown file.

**TUICommander replaces all of that.** A native desktop app built in Rust — fast, lightweight, cross-platform — specifically designed for running AI coding agents in parallel.

Review diffs, read markdown, inspect code, manage branches — without leaving the app. Need your full IDE? Launch VS Code, Cursor, or Zed from any repo with one click. But you'll find you reach for it less and less.

## Why TUICommander

**Built for agents, not adapted from a text editor.** Every feature exists because multi-agent workflows demand it:

- **50+ terminals in one window** — tabbed, split, zoomable, each with its own font size
- **Agent-aware** — detects which agent is running, tracks rate limits and usage, alerts you when an agent needs input
- **Built-in code review** — diff viewer with syntax highlighting and markdown renderer, right next to your terminals
- **Git worktree isolation** — one click to spin up an isolated branch per task, each agent gets its own workspace
- **Rate limit resilience** — configurable fallback chains: when Claude hits a limit, switch to Gemini or Aider automatically
- **Plugin system** — Obsidian-style API with 4 capability tiers, hot-reload, and a community registry
- **Native and cross-platform** — Rust backend, runs on macOS, Linux, and Windows with the same performance

## Features

### Terminals
- `Cmd+T` / `Cmd+W` / `Cmd+1-9` / `Cmd+Shift+T` — open, close, switch, reopen
- Per-pane font zoom — read one terminal at 20px while others stay at 14px
- Vertical and horizontal split panes
- WebGL-accelerated rendering via xterm.js
- Session persistence — terminals survive app restarts

### Agent Orchestration
- **Claude Code, Gemini CLI, OpenCode, Aider, Codex** — detected automatically
- Usage limit badges — color-coded (blue/yellow/red pulsing) with percentage
- Question detection — sidebar indicator + dock badge (macOS) when an agent waits for input
- Fallback chains — automatic agent switching on rate limits
- Keep-awake — prevents system sleep while agents are working

### Code & Docs Viewer
- **Diff viewer** (`Cmd+D`) — syntax-highlighted diffs for the current repo, inline
- **Markdown viewer** (`Cmd+M`) — render READMEs, docs, and specs without leaving the app
- Review what your agents changed without opening an IDE or a browser

### Git & GitHub
- Repository sidebar with branch, CI, and PR status at a glance
- Git worktree management — isolated workspaces without stashing
- GitHub GraphQL API — PR status, CI checks, review decisions (no `gh` CLI needed)
- Git operations panel (`Cmd+Shift+G`) — commit, push, branch from the keyboard
- PR detail popover with merge state, status checks, and review info
- Branch switcher (`Cmd+Ctrl+1-9`) and rename dialog

### Productivity
- **Prompt library** (`Cmd+K`) — save, search, reuse prompts with `{variable}` substitution
- **Voice dictation** — on-device Whisper with Metal GPU acceleration, push-to-talk hotkey
- **Ideas panel** (`Cmd+N`) — capture thoughts alongside your terminals
- **IDE launcher** — VS Code, Cursor, Zed, Neovim with one click
- **Audio alerts** — per-event notifications (question, error, completion, warning)

### Plugin System

TUICommander ships with an [Obsidian-style plugin API](docs/FEATURES.md) — extend the app without touching the core:

- **4 capability tiers** — from read-only state access to PTY write and Tauri commands, each gated by manifest declaration
- **Watch terminal output** — plugins can match regex patterns on PTY output and react in real time
- **Markdown providers** — plugins can render custom content in the markdown panel (dashboards, reports, tracking)
- **Activity Center** — plugins contribute items to the notification bell (plan steps, story status, CI results)
- **Hot-reload** — edit a plugin, see changes immediately
- **Community registry** — browse, install, and update plugins from Settings
- **Deep links** — `tuic://install-plugin?url=...` for one-click install from the web

**Built-in plugins:** Plan Tracker (detects Claude Code plan files). **Example plugins:** auto-confirm prompts, CI notifier, repo dashboard, story tracker.

See [`examples/plugins/`](examples/plugins/) for reference implementations.

### MCP & Integrations
- **MCP server** — external AI tools can control terminals and query repos via HTTP
- **Auto-update** — one-click install with download progress
- **Nightly builds** — always-fresh `tip` release from `main`
- 13 bundled monospace fonts, centralized settings (`Cmd+,`)

> TUICommander is in active development. New features land weekly — follow the [releases](https://github.com/sstraus/tui-commander/releases).

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd+T` | New terminal |
| `Cmd+W` | Close terminal |
| `Cmd+Shift+T` | Reopen closed tab |
| `Cmd+1-9` | Switch to tab N |
| `Cmd+Shift+[/]` | Previous / next tab |
| `Cmd+[` | Toggle sidebar |
| `Cmd++/-/0` | Zoom in / out / reset |
| `Cmd+L` | Clear terminal |
| `Cmd+K` | Prompt library |
| `Cmd+R` | Run saved command |
| `Cmd+D` | Diff viewer |
| `Cmd+M` | Markdown viewer |
| `Cmd+N` | Ideas panel |
| `Cmd+G` | lazygit |
| `Cmd+Shift+G` | Git operations |
| `Cmd+J` | Task queue |
| `Cmd+,` | Settings |
| `Cmd+?` | Help |
| `Cmd+Ctrl+1-9` | Quick branch switch |

> On Windows and Linux, substitute `Ctrl` for `Cmd`.

## Supported Agents

| Agent | Binary | What TUICommander Tracks |
|-------|--------|--------------------------|
| Claude Code | `claude` | API rate limits, overloaded errors, weekly/session usage % |
| Gemini CLI | `gemini` | 429 errors, quota exceeded, RESOURCE_EXHAUSTED |
| OpenCode | `opencode` | Rate limit patterns |
| Aider | `aider` | Rate limit patterns |
| Codex | `codex` | Rate limit patterns |

## Getting Started

### Download

Grab the latest stable build from [Releases](https://github.com/sstraus/tui-commander/releases/latest). macOS builds are signed and notarized.

Want the bleeding edge? The **[Nightly](https://github.com/sstraus/tui-commander/releases/tag/tip)** is rebuilt on every push to `main` — same signing and notarization, fresh off the branch.

### Build from Source

```bash
npm install
npm run tauri dev      # Development with hot reload
npm run tauri build    # Production build
npm test               # Run tests
```

> Prerequisites: Node.js, Rust toolchain, Tauri CLI

## Tech Stack

| Layer | Technology |
|-------|-----------|
| **UI** | SolidJS — fine-grained reactivity, no virtual DOM |
| **Terminal** | xterm.js + WebGL — canvas rendering with addon ecosystem |
| **Backend** | Rust + Tauri v2 — native PTY management, async I/O |
| **Build** | Vite + LightningCSS — fast HMR, optimized production builds |

## License

[MIT](LICENSE)
