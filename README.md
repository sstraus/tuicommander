<p align="center">
  <img src="assets/logo.png" alt="TUICommander" width="128" />
</p>

<h1 align="center">TUICommander</h1>

<p align="center">
  <strong>The IDE that understands AI agents.<br>Run parallel agents on isolated branches with full observability.</strong>
</p>

<p align="center">
  <a href="https://github.com/sstraus/tuicommander/releases/latest"><img src="https://img.shields.io/github/v/release/sstraus/tuicommander?style=flat-square&color=blue" alt="Release"></a>
  <a href="https://github.com/sstraus/tuicommander/releases/tag/tip"><img src="https://img.shields.io/badge/nightly-tip-orange?style=flat-square" alt="Nightly"></a>
  <a href="https://github.com/sstraus/tuicommander/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/sstraus/tuicommander/ci.yml?style=flat-square&label=CI" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/sstraus/tuicommander?style=flat-square" alt="License"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey?style=flat-square" alt="Platform">
  <img src="https://img.shields.io/badge/rust-backend-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-v2-24C8D8?style=flat-square&logo=tauri&logoColor=white" alt="Tauri v2">
  <a href="https://github.com/sstraus/tuicommander/actions/workflows/audit.yml"><img src="https://img.shields.io/github/actions/workflow/status/sstraus/tuicommander/audit.yml?style=flat-square&label=audit" alt="Dependency Audit"></a>
  <a href="https://github.com/sstraus/tuicommander/stargazers"><img src="https://img.shields.io/github/stars/sstraus/tuicommander?style=flat-square" alt="Stars"></a>
</p>

<p align="center">
  <a href="https://tuicommander.com"><strong>Website</strong></a> &bull;
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

You're running Claude Code in one terminal, Aider in another, Codex in a third. One hit a rate limit 10 minutes ago and you didn't notice. Another is waiting for a Y/N confirmation. You switch between windows and still lose track.

The more sessions you run, the worse it gets. The tooling doesn't understand what's happening inside the terminal.

## The solution

**TUICommander is an AI-native IDE** — designed from the ground up for multi-agent development. Agents, code, diffs, PRs, CI status, and usage analytics live in one window. No context switching. No lost threads.

AI-native means the agents are not an afterthought. Rate limit detection, question recognition, session-aware resume, and usage tracking are core — not plugins.

---

## What makes it different

### Run many AI sessions in parallel

Launch Claude Code on five branches at once — or mix agents. Each session runs in its own Git worktree — no context collision, no stash conflicts, no "which terminal was that?" moments.

- Up to 50 terminal sessions running simultaneously
- Each session works on its own isolated copy of the repo
- Activity dashboard showing every session at a glance

### Agent observability — not just terminals

TUICommander auto-detects **10 AI coding agents** (Claude Code, Codex CLI, Aider, Gemini CLI, Amp, Cursor Agent, OpenCode, Droid, Goose, Grok) and understands what they're doing:

- **Rate limit detection** — Provider-specific patterns with countdown timers per session.
- **Question detection** — Y/N prompts, numbered options, inquirer-style menus. Tab indicator, notification sound, keyboard overlay.
- **Usage tracking** — Claude Code weekly/session limits with color-coded thresholds. Full Claude Usage Dashboard with rate limit bars, 7-day chart, 52-week heatmap, per-project breakdown.
- **Activity dashboard** — Every session at a glance: agent type, status (working / waiting / rate-limited / idle), last activity.
- **Session-aware resume** — Auto-discovers agent session IDs from disk (Claude Code, Gemini CLI, Codex CLI). Resume exactly where you left off.

No other tool knows that your agent is stuck.

### Git worktrees, fully managed

Click a branch in the sidebar. TUICommander auto-creates a git worktree — a fully isolated copy of the repo. The terminal opens *inside* it. Switch to another branch: your previous terminals are preserved. Switch back: they're exactly as you left them.

- Automatic worktree creation on branch click
- Worktree Manager — PR status, dirty file counts, last commit at a glance
- Post-merge cleanup dialog — delete branch, archive worktree in one click
- Configurable base branch, setup scripts, auto-fetch intervals

### See what your agents changed — without leaving

The feedback loop happens in the same window:

- **Git Panel** — Staging, inline commit, blame with age heatmap, canvas commit graph, stashes, branches with ahead/behind counts.
- **Diff views** — Side-by-side, unified, or scroll-all-files. Word-level highlighting, hunk and line-level restore.
- **PR management** — Merge via GitHub API with auto-detected merge method. Post-merge cleanup.
- **GitHub Issues** — Filter by assigned/created/mentioned, labels with colors, close/reopen.
- **CI Auto-Heal** — When CI fails, TUICommander fetches failure logs and injects them into the agent for automatic fix.
- **Built-in code editor** — Syntax highlighting, find/replace, disk conflict detection, a VS Code-style change-overview ruler, and a Cmd/Ctrl+hover go-to-definition affordance.
- **File browser** — Directory tree, content search (grep), git status indicators.

### Built-in AI Chat & autonomous agent

A conversational AI companion that sees your terminal as you see it. Ask about errors, get code suggestions, or let the autonomous agent take the wheel and drive your terminal directly.

- Multi-provider: Ollama (local, free), Anthropic, OpenAI, OpenRouter, or any compatible endpoint
- Autonomous AI Agent (ReAct loop) with 30+ tools: read screen, send input, edit files, search code, run commands
- Session knowledge: the agent learns from your terminal history — commands, errors, fix patterns
- Live cost tracking: prompt/completion tokens and estimated cost per turn
- Per-terminal chat state, conversation history, detachable panel for multi-monitor

### MCP Proxy Hub — one connection for all your tools

Aggregate all your MCP servers into a single endpoint. Claude Code, Cursor, VS Code — each agent connects once and gets access to every upstream tool.

- Auto-configures Claude Code, Cursor, Windsurf, VS Code, Zed, Amp, Gemini
- Circuit breakers, health checks, hot-reload per upstream
- Credential management via OS keyring, OAuth 2.1 support
- Tool filtering: whitelist or blacklist per upstream server

### Control from anywhere

A **mobile companion PWA** lets you monitor agents from your phone, answer questions with one tap, and track rate limits in real time.

- QR code scan for instant LAN connection
- Tailscale auto-HTTPS or E2E-encrypted cloud relay — no VPN or port forwarding
- Live WebSocket output, suggest follow-up chips, slash menu

### Agents that coordinate

**Agent Teams** — Claude Code's sub-agents run as native TUICommander tabs instead of tmux panes. Full session awareness, output parsing, and question detection for each sub-agent.

**Inter-agent messaging** — When multiple agents work in parallel, they discover peers, send messages, and receive push notifications through TUICommander as the coordination hub.

### Automate repetitive workflows

**29 built-in Smart Prompts** turn common tasks into one-click operations: Smart Commit, Review Changes, Create PR, Fix CI, and more. Context variables like branch, diff, and PR data are resolved automatically.

- Inject mode (PTY write), headless mode (subprocess), or shell script mode (direct execution)
- 35 context variables auto-resolved from git, GitHub, terminal, and file context
- Create your own prompts with the same variable system

### Talk to your agents

On-device speech-to-text powered by whisper-rs. No cloud service, no API keys, no data leaving your machine.

- GPU-accelerated on macOS (Metal) and Windows (Vulkan), CPU on Linux (optional CUDA/Vulkan)
- Push-to-talk hotkey — text injected into the active terminal
- 4 model sizes from Small (488 MB) to Large V2 (3 GB), incl. large-v3-turbo (1.6 GB)

### Extend everything

**Plugin system** — Obsidian-style plugins with hot reload and a community registry:

- 5 capability tiers from read-only watchers to scoped Tauri invoke
- TUIC SDK v1.0: `tuic.activeRepo`, `tuic.toast`, `tuic.onRepoChange` and more
- Terminal output watchers with regex triggers
- Status bar tickers, custom panels, notification contributions
- Browse and install with one click

[Plugin Authoring Guide →](docs/plugins.md)

### Built to be scripted — CLI, HTTP, and MCP control surface

TUICommander isn't a black box. Everything you click, you can also drive from a script, another tool, or an AI agent.

- **`tuic` CLI companion** — Open files with cursor goto (`tuic src/main.rs:42:8`), manage sessions (`ls` / `new` / `kill` / `send`), orchestrate agents (`spawn` / `ls` / `send`), plus a tmux-compatibility alias mode. Installs from Settings, auto-updates on launch.
- **HTTP API** — REST + WebSocket + SSE on a local port: list/create/close sessions, stream live output, spawn agents, read terminal grids and scrollback, query process CPU/RSS. Script TUIC from anything that can hit a socket.
- **MCP control surface** — TUIC is itself an MCP server. Connected agents get `session`, `agent`, `repo`, and `ui` tools — including `drive_agent` (atomic send → wait-for-idle → read) and delta cursors that return only new output. *(Distinct from the MCP Proxy Hub above, which aggregates your upstream servers.)*
- **Custom "Open in…" launchers** — Define your own editor/tool commands with placeholder tokens: `{file}`, `{repo}`, `{fileDir}`, `{cwd}`, `{home}`, `{line}`, `{column}`. iTerm2, Tower, and the full JetBrains family ship built in.

[CLI guide →](docs/user-guide/cli.md) &bull; [HTTP API →](docs/api/http-api.md) &bull; [MCP server →](docs/backend/mcp-http.md)

---

## How it compares

| Capability | Ghostty / Kitty | Warp | Cursor IDE | Claude Desktop | TUICommander |
|---|---|---|---|---|---|
| Terminal sessions | Yes | Yes | Yes | No | Yes (50) |
| AI coding agents | No | Partial | Built-in | Built-in | Any agent (10 detected) |
| Parallel agents | No | No | Limited | No | Unlimited |
| Git worktree orchestration | No | No | No | No | Automatic |
| Agent observability | No | No | No | No | Real-time |
| Global Workspace | No | No | Yes | No | Multi-repo |
| Usage dashboard | No | No | No | Basic | Full (heatmap, per-project) |
| Remote access (phone/tablet) | No | No | No | Mobile app | PWA + E2E relay |
| Voice dictation | No | No | Extension | Built-in | Local Whisper |
| MCP Proxy Hub | No | No | No | No | Built-in |
| Plugin system | No | No | Extensions | No | Hot reload + SDK |
| GitHub Issues & PR management | No | No | Extension | No | Built-in |
| Built-in AI Chat | No | Built-in | Built-in | Built-in | Multi-provider (beta) |
| CI Auto-Heal | No | No | No | No | Built-in |

---

<details>
<summary><strong>Terminal features</strong> — 50 sessions, splits, detach, find, persistence</summary>

- Up to 50 concurrent PTY sessions, each with independent zoom (8–32px)
- Split panes: vertical (`Cmd+\`) or horizontal (`Cmd+Alt+\`), up to 4 panes, drag-resize
- Detachable tabs: float any terminal into its own OS window, re-attaches on close
- Find in terminal (`Cmd+F`): regex, case-sensitive, whole word, match navigation
- Cross-terminal search: type `~` in command palette to search all open terminal buffers
- Session persistence: terminals survive restarts with lazy restore on branch click
- Tab management: reorder by drag, rename by double-click, reopen last 10 closed tabs
- Tab status dots: idle, busy, done, unseen, question, error
- Copy on select, configurable bell (visual/sound/both), scroll shortcuts
- International keyboard support, Kitty keyboard protocol
- 11 bundled monospace fonts (JetBrains Mono, Fira Code, Hack, Cascadia Code, and more)
</details>

<details>
<summary><strong>Git integration</strong> — Worktrees, Git Panel, PR management</summary>

- Auto-create worktrees per branch with configurable base branch and setup scripts
- Worktree Manager (`Cmd+Shift+W`): all worktrees across all repos, orphan detection, batch operations
- Git Panel (`Cmd+Shift+D`): staging, commit, log with canvas commit graph, stashes, branches, blame with age heatmap
- PR management: merge via GitHub API, auto-detect merge method, post-merge cleanup dialog
- GitHub Issues: filter by assigned/created/mentioned, labels with colors, close/reopen
- Auto-delete branch on PR close, CI Auto-Heal, PR notifications
- Repository groups: named, colored, collapsible, drag-and-drop reordering
- Park repos: temporarily hide repos you're not using
- Quick branch switcher: hold `Cmd+Ctrl`, press `1-9` to switch instantly
- Auto-fetch on configurable interval
</details>

<details>
<summary><strong>Productivity</strong> — Smart Prompts, palette, keybindings, dictation</summary>

- Smart Prompts (`Cmd+Shift+K`): 29 built-in AI automation prompts with auto-resolved context variables
- Command palette (`Cmd+P`): fuzzy search all actions, files (`!`), file contents (`?`), terminal buffers (`~`)
- Configurable keybindings with chord support and conflict detection
- Claude Usage Dashboard: rate limits, 7-day chart, 52-week heatmap, per-project breakdown
- Prompt library (`Cmd+K`): saved prompts with variable substitution
- IDE launchers: open in VS Code, Cursor, Zed, JetBrains, iTerm2, Tower, or any detected tool — plus user-defined custom launchers with `{file}`/`{repo}`/`{line}`/`{column}` tokens
- Ideas panel (`Cmd+Alt+N`): quick notes with image paste and send-to-terminal
- Voice dictation: streaming on-device Whisper with partial results
- Focus mode (`Cmd+Alt+Enter`): maximize active tab, hide sidebar and panels
</details>

<details>
<summary><strong>Developer & automation</strong> — CLI, HTTP/MCP control, command blocks, generators, process manager</summary>

- `tuic` CLI: file open with cursor goto, session/agent orchestration, tmux-compat alias mode, auto-update
- HTTP API (REST + WebSocket + SSE): sessions, live output stream, agent spawn, terminal grid/scrollback ops, process stats
- MCP control surface: `session` / `agent` / `repo` / `ui` tools, `drive_agent` atomic send→wait→read, delta cursors for incremental reads
- Custom launchers: user-defined exec + args with `{file}`/`{repo}`/`{fileDir}`/`{cwd}`/`{home}`/`{line}`/`{column}` placeholder tokens
- Command blocks: terminal output segmented per prompt+output cycle — semantic scrollbar marks, fold (`Cmd+Shift+.`), block search (`Cmd+Shift+B`), navigate (`Cmd+Shift+Up/Down`)
- Generators (command palette): Password, UUID v4/v7, ULID, CUID2, JWT secret, TOTP secret, Nano ID, Slug, Ed25519 keypair — generated natively in Rust
- Process Manager: CPU% and RSS for TUIC and every child process tree, with a self-contained HTML monitor
- Runtime diagnostics: CPU-spike watchdog, FD/thread-leak detection, health snapshots via the local logs endpoint
- Auto-standby: SIGSTOP idle + unfocused PTY groups after N minutes, SIGCONT on focus or agent message
</details>

> **Full feature reference:** **[docs/FEATURES.md](docs/FEATURES.md)**

---

## Get started

**[Download the latest release](https://github.com/sstraus/tuicommander/releases/latest)** — macOS builds are signed and notarized.

Install via terminal:

```bash
# macOS (Homebrew)
brew install sstraus/tap/tuicommander

# macOS / Linux (shell)
curl -fsSL https://tuicommander.com/install.sh | sh

# Windows (PowerShell)
irm https://tuicommander.com/install.ps1 | iex
```

Want the bleeding edge? The **[Nightly](https://github.com/sstraus/tuicommander/releases/tag/tip)** is rebuilt on every push to `main`.

<details>
<summary>Build from source</summary>

**Prerequisites:** Node.js 24+, Rust toolchain, [Tauri CLI](https://tauri.app/start/)

```bash
pnpm install
pnpm tauri dev      # Development with hot reload
pnpm tauri build    # Production build
pnpm test               # Run tests
```

See [docs/guides/development-setup.md](docs/guides/development-setup.md) for platform-specific instructions.
</details>

## Built with

Rust + [Tauri v2](https://tauri.app) backend, [SolidJS](https://solidjs.com) UI, native terminal via [alacritty_terminal](https://crates.io/crates/alacritty_terminal) + canvas rendering, [whisper-rs](https://github.com/tazz4843/whisper-rs) dictation, [Vite](https://vite.dev) + LightningCSS build. ~80 MB RAM.

## Documentation

| | |
|---|---|
| [Getting Started](docs/user-guide/getting-started.md) | First-run guide |
| [Features](docs/FEATURES.md) | Complete feature reference with all keyboard shortcuts |
| [AI Agents](docs/user-guide/ai-agents.md) | Agent detection, rate limits, question detection |
| [Plugin API](docs/plugins.md) | Build plugins for TUICommander |
| [HTTP API](docs/api/http-api.md) | REST/WebSocket/SSE endpoints |
| [Architecture](docs/ARCHITECTURE.md) | System design and component overview |
| [Development Setup](docs/guides/development-setup.md) | Build from source |

---

<p align="center">Apache 2.0 licensed. Zero telemetry. Runs locally.<br><a href="https://github.com/sstraus/tuicommander/releases">Follow the releases</a> — new features land weekly.</p>

## License

[Apache 2.0](LICENSE) &copy; 2026 Stefano Straus
