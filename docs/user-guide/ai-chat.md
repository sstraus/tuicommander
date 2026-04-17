# AI Chat

AI Chat is a conversational AI companion that lives next to your terminals and sees what they see. Unlike spawning a full agent (Claude Code, Aider, …) in a PTY, the chat panel gives you a quick explain / summarise / suggest loop that shares the exact screen state of your active terminal.

Two progressive capability levels ride on the same panel:

| Level | What it does |
|-------|--------------|
| **Chat** (default) | Streaming Q&A with terminal context injection. You ask, the model sees the last N lines, replies in markdown. "Run this" and "Copy" on every code block. |
| **Agent** (ReAct loop) | The model *acts*: it reads the screen, sends input/keys, waits for patterns, asks for approval before destructive commands. Pause / resume between iterations. Built on six tools exposed both internally and as `ai_terminal_*` MCP tools. |

The same panel switches modes — no separate UI.

## Opening the panel

- **Hotkey:** `Cmd+Alt+A` (macOS) / `Ctrl+Shift+I` (others) — toggle.
- **Toolbar:** chat icon in the right section of the toolbar.
- **Context menu:** right-click a terminal → *Send selection to AI Chat* or *Explain this error*.

The panel docks on the right. Width is remembered per window (`aiChatPanelWidth`).

## Providers

AI Chat speaks to four provider families plus a custom endpoint. Switch in `Settings > AI Chat > Provider`:

| Provider | Default base URL | Notes |
|----------|------------------|-------|
| **Ollama** (local) | `http://localhost:11434/v1/` | Auto-detected — the settings tab shows live status and the model list pulled from `GET /api/tags`. No API key required. |
| **Anthropic** | `https://api.anthropic.com` | Direct Messages API. API key from Anthropic console. |
| **OpenAI** | `https://api.openai.com/v1` | Chat Completions. |
| **OpenRouter** | `https://openrouter.ai/api/v1` | Single key, many models. |
| **Custom** | *(editable)* | Any OpenAI-compatible endpoint. |

### Model recommendations

| Use case | Local (Ollama) | API |
|---|---|---|
| Explain output, quick Q&A | Qwen 2.5 7B, Llama 3.3 8B | Haiku, GPT-4o-mini |
| Generate commands, review diffs | Qwen3-Coder 14B | Sonnet, GPT-4o |
| Agent loop (tool calling) | DeepSeek R1 32B, Qwen 27B | Sonnet, Opus |

API keys are stored in the OS keyring under service `tuicommander-ai-chat` — never written to disk in plaintext.

## Context injection

Every turn the backend assembles a compact context from the currently-attached terminal:

- **Clean screen text** — last `context_lines` rows from the `VtLogBuffer` (ANSI-stripped, TUI alternate-screen suppressed). Default: 150. Tune in settings.
- **Session state** — shell busy / idle, CWD, last exit code, detected agent type, terminal mode (`Shell` vs `FullscreenTui`).
- **Recent parsed events** — errors, questions, rate limits, status lines.
- **Git context** — branch, short diff stats, staged file list (same variables Smart Prompts use).
- **Session knowledge** (when the agent has been active) — compact markdown summary of recent command outcomes, error→fix pairs, TUI apps seen.

Attach / detach the terminal via the dropdown at the top of the panel (or right-click → *Attach to AI Chat*). "Auto-attach" picks the focused terminal each turn.

## Conversations

- Each panel session gets a fresh `chatId` on open.
- Save explicitly via the menu → `list_conversations` / `load_conversation` / `delete_conversation`.
- Hard cap: **100 messages** per conversation in memory; older messages are evicted FIFO. Saved conversations keep the full history on disk.
- Streaming uses a Tauri `Channel<ChatStreamEvent>` — you see tokens as they arrive. Cancel mid-stream with the stop button or `cancel_ai_chat`.

## Run-this, copy, and actions

Every fenced code block in the AI reply has a small toolbar:

| Action | Effect |
|--------|--------|
| **Run** | Sends the block to the attached terminal via `sendCommand()` (handles Ink raw mode). Disabled when no terminal is attached. |
| **Copy** | Clipboard. |
| **Insert** | Prepends the block to the current prompt input (for refinement). |

Language hints in the fence control button visibility — a ` ```text ` block hides *Run*.

## Agent mode (ReAct)

Flip the panel into agent mode via the header toggle or the command palette (`Agent: start`). Give it a goal ("set up pnpm and install deps", "fix the failing test") and press Enter. The loop:

1. Assemble context.
2. Ask the LLM with six tools available: `read_screen`, `send_input`, `send_key`, `wait_for`, `get_state`, `get_context`.
3. Dispatch tool calls — each appears as a collapsible card in the panel.
4. Record outcomes into the session knowledge store.
5. Stop on `end_turn` or when cancelled.

### Safety gates

The `SafetyChecker` trait inspects every would-be `send_input`. Three verdicts:

- **Allow** — common commands, `ls`, `git status`, `cargo build`, editor launches …
- **NeedsApproval** — destructive patterns (`rm -rf`, `git reset --hard`, `git push --force`, `DROP TABLE`, `dd of=`, package uninstall …). The panel shows a *Pending approval* card with a one-line reason. Approve / reject with a click or `approve_agent_action`.
- **Block** — hard-coded refusals (e.g. `rm -rf /`, `:(){ :|:& };:`).

Pause / resume any time — the loop cleanly stops between iterations.

### Session knowledge

As the agent runs, the `SessionKnowledgeBar` footer shows live telemetry:

- Commands run this session (count).
- Last 5 outcomes with kind badges (Success / Error / TuiLaunched / Timeout …).
- Last 5 errors with inferred `error_type` (e.g. `rust-error-borrow`, `npm-missing-module`).
- TUI mode indicator + list of TUI apps seen.

OSC 133 semantic prompts (`OSC 133;A/B/C/D`) feed accurate exit codes when the shell supports them (modern `bash`/`zsh`/`fish` with the integration enabled). Without OSC 133, the PTY silence timer records an `Inferred` outcome so the loop still learns.

Knowledge persists to `<config_dir>/ai-sessions/<session_id>.json` with a 2 s debounced background flush. Reopening a session rehydrates the store.

### Knowledge history overlay

Click **History** next to the `SessionKnowledgeBar` to open a two-pane browser over every persisted session on disk — not just the currently active one. Useful for "find the command that fixed the build error last week":

- **Sessions list** (left) — sorted by most recent activity, showing command count, error count, and last CWD.
- **Detail pane** (right) — one card per command with kind badge, timestamp, exit code, duration, CWD, output snippet, and a **copy** button.
- **Filters** — debounced full-text search (matches command, output, inferred `error_type`, and `semantic_intent`), `errors only` checkbox, date window (`24h` / `7d` / `30d` / `all`).

Esc closes. Backed by the `list_knowledge_sessions` + `get_knowledge_session_detail` Tauri commands.

## External MCP surface (`ai_terminal_*` tools)

The same six ReAct tools are exposed to external MCP clients (Claude Code, Cursor, …) through the TUICommander MCP server:

| Tool | Purpose |
|------|---------|
| `ai_terminal_read_screen` | Last N rows of clean text (secrets redacted). |
| `ai_terminal_send_input` | Send a command — **always** prompts for user confirmation. |
| `ai_terminal_send_key` | Send a single special key — **always** prompts for confirmation. |
| `ai_terminal_wait_for` | Wait for regex match or screen stability. |
| `ai_terminal_get_state` | Structured `SessionState`. |
| `ai_terminal_get_context` | ~500-char compact context summary. |

Input tools are refused while the internal agent loop is active on that session, so an external agent can't fight the internal one for the same PTY.

## Settings reference (`Settings > AI Chat`)

| Field | Stored in | Notes |
|-------|-----------|-------|
| Provider | `ai-chat-config.json` | `ollama` / `anthropic` / `openai` / `openrouter` / `custom` |
| Model | `ai-chat-config.json` | Free-text; settings tab populates suggestions per provider |
| Base URL | `ai-chat-config.json` | Pre-filled per provider, editable |
| Temperature | `ai-chat-config.json` | Default `0.7` |
| Context lines | `ai-chat-config.json` | Default `150`. Raise for richer context, lower for smaller prompts. |
| API key | OS keyring (`tuicommander-ai-chat` / `api-key`) | Masked with eye-toggle. "Test connection" validates the key + base URL. |
| Experimental: enrich command blocks | `ai-chat-config.json` (`experimental_ai_block_enrichment`) | Default off. When on, each completed OSC 133 block is sent to the provider for a one-line `semantic_intent`. Rate-limited to ~10/min, silent on failure. |

## Keyboard shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd+Alt+A` | Toggle panel |
| `Cmd+Enter` (panel focused) | Send message |
| `Esc` (panel focused) | Cancel in-flight stream |
| *(palette)* Agent: start / stop / pause / resume | Agent-mode control |

## Files & storage

| Path | Purpose |
|------|---------|
| `<config_dir>/ai-chat-config.json` | Provider, model, base URL, temperature, context budget |
| `<config_dir>/ai-chat-conversations/<id>.json` | Saved conversation bodies |
| `<config_dir>/ai-sessions/<session_id>.json` | Per-session knowledge store (browsable from the History overlay) |
| OS keyring (`tuicommander-ai-chat` / `api-key`) | Provider API key |

## See also

- [`docs/backend/mcp-http.md`](../backend/mcp-http.md) — `ai_terminal_*` MCP tools + OAuth 2.1 upstream auth.
- [`docs/api/tauri-commands.md`](../api/tauri-commands.md) — Full Tauri command reference for chat + agent.
- [`docs/backend/pty.md`](../backend/pty.md) — PTY lifecycle, OSC 133, TUI detection, silence-based idle.
- [`ideas/ai-assisted-terminal.md`](../../ideas/ai-assisted-terminal.md) — Original 3-level plan (Level 1 = Chat, Level 2 = Agent, Level 3 = Knowledge).
