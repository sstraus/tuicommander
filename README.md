<p align="center">
  <img src="assets/logo.png" alt="TUI Commander" width="128" />
</p>

# TUI Commander

**A desktop terminal orchestrator for running dozens of AI coding agents in parallel.**

Manage Claude Code, Gemini CLI, Aider, OpenCode, and Codex side by side -- each in its own terminal with independent zoom, git worktree isolation, and automatic rate limit recovery.

![TUI Commander](assets/screenshot.png)

Built with **Tauri + SolidJS + xterm.js + Rust** for native performance on macOS, Linux, and Windows.

---

## Why TUI Commander?

Running multiple AI agents across repos means juggling terminals, losing track of which agent hit a rate limit, and constantly switching contexts. TUI Commander puts everything in one window:

- **50+ terminals** with tab navigation and per-pane zoom
- **Git worktree isolation** -- each task gets its own branch without stashing
- **Rate limit detection** -- knows when an agent is throttled and can auto-fallback
- **Audio alerts** -- hear when an agent needs your attention
- **Prompt library** -- save, search, and reuse prompts with variable substitution

## Features

### Terminal Management
- Tabbed terminals with `Cmd+1-9` quick switch, `Cmd+T` to open, `Cmd+W` to close
- Per-pane font zoom (`Cmd+Plus/Minus/0`) -- read one terminal at 20px while keeping others at 14px
- Reopen accidentally closed tabs with `Cmd+Shift+T`
- Tab context menu with close and rename operations
- WebGL-accelerated rendering via xterm.js

### Multi-Agent Orchestration
- **5 agents supported:** Claude Code, Gemini CLI, OpenCode, Aider, Codex
- Automatic agent detection from terminal output
- Rate limit detection with provider-specific patterns (API limits, 429s, quota errors)
- Configurable fallback chains -- when one agent hits a limit, switch to the next
- Error handling strategies with retry logic

### Git Integration
- Repository sidebar with branch status and CI indicators
- Git worktree management -- spin up isolated workspaces per task
- Built-in diff viewer with syntax highlighting
- Markdown file viewer for docs and READMEs
- Git operations panel (`Cmd+Shift+G`) for common git workflows
- Branch rename dialog and quick branch switcher (`Cmd+Ctrl+1-9`)
- PR detail popover with status checks and review info

### Prompt Library
- Save prompts with categories, favorites, and search (`Cmd+K`)
- Variable substitution with `{variableName}` syntax
- Keyboard-driven: navigate with arrows, insert with Enter

### Voice Dictation
- Speech-to-text via Whisper integration
- Configurable model selection and language
- Global hotkey support

### Settings & Customization
- Centralized settings panel (`Cmd+,`) with tabs for appearance, agents, notifications, dictation, and services
- 13 bundled monospace fonts (JetBrains Mono, Fira Code, Hack, Cascadia Code, and more)
- Audio notifications with per-event volume control (question, error, completion, warning)
- IDE launcher integration (VS Code, Cursor, Zed, etc.)
- Window position and size remembered across launches

### Extras
- lazygit integration (`Cmd+G`) with split-pane mode
- Run saved shell commands (`Cmd+R`)
- Task queue panel (`Cmd+J`)
- Searchable help panel (`Cmd+?`)
- MCP (Model Context Protocol) server bridge

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
| `Cmd+D` | Toggle diff panel |
| `Cmd+M` | Toggle markdown panel |
| `Cmd+G` | Open lazygit |
| `Cmd+Shift+G` | Git operations panel |
| `Cmd+J` | Task queue |
| `Cmd+,` | Settings |
| `Cmd+?` | Help panel |
| `Cmd+Ctrl+1-9` | Quick branch switch |

## Getting Started

```bash
# Install dependencies
npm install

# Run in development (desktop app)
npm run tauri dev

# Build for production
npm run tauri build

# Run tests
npm test
```

## Tech Stack

| Layer | Technology | Why |
|-------|-----------|-----|
| UI | SolidJS | Fine-grained reactivity, no virtual DOM overhead |
| Terminal | xterm.js + WebGL | Canvas-rendered terminal with addon ecosystem |
| Backend | Rust + Tauri v2 | Native PTY management, async I/O with tokio |
| Build | Vite + LightningCSS | Fast HMR in dev, optimized production builds |

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for design decisions.

## Supported Agents

| Agent | Command | Rate Limit Detection |
|-------|---------|---------------------|
| Claude Code | `claude` | API rate limit, overloaded |
| Gemini CLI | `gemini` | 429 errors, quota exceeded |
| OpenCode | `opencode` | Rate limit patterns |
| Aider | `aider` | Rate limit patterns |
| Codex | `codex` | Rate limit patterns |

## License

MIT
