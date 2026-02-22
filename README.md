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
  <a href="docs/"><strong>Docs</strong></a>
</p>

---

<p align="center">
  <img src="assets/tui-screenshot.png" alt="TUICommander — multiple agents, split panes, sidebar with repos" width="900" />
</p>

---

<p align="center"><em>TUI stands for Terminal User Interface — because your AI agents live in the terminal, and so does TUICommander.</em></p>

## Stop juggling terminal windows

You're running Claude Code in one terminal, Aider in another, Codex in a third. You have no idea which one hit a rate limit 10 minutes ago. You switch to VS Code just to check a diff. You open GitHub to see if CI passed.

**TUICommander is one window for all of it.** Built in Rust. Native on macOS, Linux, and Windows.

## What you get

**10 agents, one dashboard.** Claude Code, Codex, Aider, Gemini CLI, Amp, Jules, Cursor Agent, OpenCode, Warp Oz, ONA — all detected and monitored automatically. Rate limits, usage percentages, "waiting for input" alerts. When one agent stalls, fallback chains switch to the next.

**See what your agents changed.** Built-in diff viewer and markdown renderer. Review code, read docs, check specs — right next to the terminal that produced them. No IDE tab-switching.

**Git that actually helps.** Sidebar shows every repo with branch, PR status, and CI state. Worktrees for parallel work — each agent gets its own isolated branch. GitHub PR details, review status, merge state — all via GraphQL, no `gh` CLI needed.

**A platform, not just a terminal.** Obsidian-style plugin system with hot-reload and a community registry. Plugins can watch terminal output, render markdown panels, push notifications, and control PTY sessions. MCP server exposes everything to external AI tools via HTTP.

**Everything else you'd expect.** Voice dictation (on-device Whisper, no cloud). Prompt library with variable substitution. Session persistence across restarts. Auto-update. 13 bundled monospace fonts. Launch VS Code / Cursor / Zed with one click when you need them.

## Get started

**[Download the latest release](https://github.com/sstraus/tuicommander/releases/latest)** — macOS builds are signed and notarized.

Want the bleeding edge? The **[Nightly](https://github.com/sstraus/tuicommander/releases/tag/tip)** is rebuilt on every push to `main`.

<details>
<summary>Build from source</summary>

```bash
npm install
npm run tauri dev      # Development with hot reload
npm run tauri build    # Production build
npm test               # Run tests
```

Prerequisites: Node.js, Rust toolchain, Tauri CLI
</details>

## Built with

Rust + [Tauri v2](https://tauri.app) backend, [SolidJS](https://solidjs.com) UI, [xterm.js](https://xtermjs.org) + WebGL terminals, [Vite](https://vite.dev) + LightningCSS build.

See [docs/](docs/) for architecture, plugin API, and keyboard shortcuts.

---

<p align="center">TUICommander is in active development — new features land weekly.<br><a href="https://github.com/sstraus/tuicommander/releases">Follow the releases</a> to see what's new.</p>

## License

[MIT](LICENSE)
