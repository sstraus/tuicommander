# Changelog

All notable changes to TUICommander will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [1.0.7] - 2026-04-24

### Added
- **Manual MCP configuration panel** — Expandable "Manual MCP configuration" section in Settings > Services > TUIC Tools shows the `tuic-bridge` binary path and a ready-to-paste JSON snippet for manual MCP client setup. Copy button included.
- **Copy-on-select settings toggle** — UI toggle in Settings > General > Terminal section to enable/disable copy-on-select behavior (previously only in Appearance tab).
- **Copy feedback for Cmd+C** — The `handleCopy` capture-phase listener (Cmd+C) now shows "Copied to clipboard" in the status bar, matching the existing behavior of copy-on-select and Ctrl+C paths.

### Fixed
- **Terminal copy trailing whitespace** — All copy paths (Cmd+C, Ctrl+C on Windows, copy-on-select, keyboard shortcut handler) now strip trailing spaces that xterm.js pads to the terminal width.
- **Windows path support across frontend** — Replaced all raw string path operations (`startsWith("/")`, `.split("/").pop()`, template literal joins) with cross-platform `pathUtils` helpers (`isAbsolutePath()`, `pathBasename()`, `joinPath()`, `pathStartsWith()`, `pathStripPrefix()`). Affects 15+ files: App.tsx, FileBrowserPanel, CodeEditorTab, MarkdownTab, HtmlPreviewTab, PaneTree, ChangesTab, resolveTuicPath, planPlugin, mdTabs, diffTabs, editorTabs, useAppInit, useFileDrop, useGitOperations.
- **Git path traversal validation** — Simplified `validate_paths_within_repo` to use depth-counting instead of full path canonicalization, correctly rejecting `../` escapes without needing the file to exist on disk.
- **`$HOME` restriction removed from `list_markdown_files` and `read_file`** — The artificial `$HOME` boundary blocked access to repos on external drives and non-standard locations. The user IS the trust boundary for a local desktop app.
- **Session conflict flag-file approach** — Replaced `maybe_reset_tuic_session` (which wrote `export TUIC_SESSION=...` directly to the PTY) with a flag-file mechanism. Now creates `no-session-inject.$TUIC_SESSION` in the config dir; shell wrappers (zsh/bash/fish) check for this file before injecting `--session-id`. Eliminates PTY writes that could corrupt TUI output.

### Added (tests)
- **pathUtils comprehensive test suite** — 40+ tests covering `isAbsolutePath`, `normalizeSep`, `pathStartsWith`, `pathStripPrefix`, `joinPath`, `pathParts` with Unix, Windows, UNC, and mixed-separator cases.
- **Windows path tests for planPlugin, mdTabs** — Drive-letter CWD resolution, mixed separators, absolute path preservation.

## [1.0.6] - 2026-04-18

### Added
- **Run config Edit/Delete menu** — The Delete button on each run config in Settings → Agents has been replaced with a `···` dropdown containing Edit and Delete. Edit opens an inline form for name, command, and args (with cross-agent duplicate name validation). Env editing continues to live on the dedicated "Env" button.
- **Auto-inject session binding for Claude Code and Goose** — Shell integration (zsh, bash, fish) wraps `claude` and `goose` commands with functions that transparently inject session identifiers (`--session-id $TUIC_SESSION` for Claude, `--name $TUIC_SESSION` for Goose `session`/`run` subcommands), ensuring deterministic 1:1 tab↔session mapping. Wrappers are bypassed when the user explicitly passes session/resume flags.
- **Goose CLI agent support** — Full integration for Block's Goose CLI: foreground process detection, status line spinner parsing (`(Ctrl+C to interrupt)` pattern), session-aware resume via `--name`, MCP client identification, Settings panel entry, and agent icon/badge.
- **Claude Wakeup plugin** — Agent-scoped external plugin (`plugins/claude-wakeup/`) that wakes Claude Code when it stalls without asking a question. Sends a verification prompt after 20 s of idle, detects "done" replies via busy-cycle duration (short <8 s = done, long ≥8 s = continued), disarms until next user turn. Typing suppression: every busy→idle transition resets the idle clock, preventing false wakes during keystroke-generated shell-state blips. Max 3 wakes per stall, 12 per session. Markdown stats dashboard. Configurable thresholds via `data/config.json`.
- **Agent cron scheduler** — Time-triggered agent tasks with cron expressions. Define jobs (cron + goal) in Settings > AI Chat > Scheduler. Persisted to `ai-cron.json`, ticks every 30 s. Tauri commands: `load_scheduler_config`, `save_scheduler_config`.
- **Agent model overrides per task phase** — Route different models to different tool phases (`plan`, `search`, `read`, `write`) in the AI Agent loop. Configure in Settings > AI Chat. Stored as `agent_model_overrides` in `ai-chat-config.json`.
- **PTY orchestration tools for multi-agent control** — Swarm agents can spawn, monitor, and coordinate PTY sessions via MCP `agent` and `session` tools.
- **Cross-session memory injection** — `build_cross_session_section()` scans prior sessions in the same repo and injects a summarised memory block into the agent system prompt.
- **`search_tools` / `call_tool` MCP bridge** — Speakeasy lazy-discovery meta-tools (`search_tools`, `get_tool_schema`, `call_tool`) replace the full tool list when `collapse_tools` is enabled, cutting MCP context from ~35k to ~500 tokens.
- **Unsafe mode** — Lock icon in the AI Chat header toggles `TrustLevel::Unrestricted`, bypassing approval prompts and sandbox. Confirmation dialog + red header indicator.
- **Agent cost tracking UI** — Live usage footer in AI Chat: prompt tokens (↑N), completion tokens (↓N), estimated cost ($X.XXXX), cache hit rate.
- **`search_code` BM25 tool** — Semantic search over repo files via `content_index`, available as the 13th agent tool and via `ai_terminal_search_code` MCP.
- **AI Chat conversation history panel** — Slide-in list of saved conversations with title, terminal name, message count, date. Click to load.
- **AI Chat per-terminal state** — Each terminal maintains independent chat history, streaming state, and conversation ID (keyed by `tuicSession`). Frozen-state banner when no terminal is focused.
- **AI Chat detachable panel** — Detach the AI Chat panel into a separate Tauri window for multi-monitor workflows. Click the detach icon in the panel header to pop it out; the main window shows a placeholder with a "Bring back" button. Cross-window state sync via a Rust-side ChatRegistry using `Channel<ChatEvent>` fan-out (no `app.emit` for high-frequency data). The detached window shares the same conversation state — streaming chunks, messages, and errors are projected in real-time.
- **Refresh terminal** (`Cmd+Shift+L`) — Rebuilds the terminal renderer to fix corrupted WebGL glyphs without clearing content.
- **Tab drag reorder for all tab types** — Non-terminal tabs (diff, editor, markdown, plugin panels) can now be reordered via drag-and-drop.
- **`get_input_buffer_content` Tauri command** — Read the terminal input line buffer; whitelisted for plugins with `pty:read` capability.
- **Keyring warm-up** — Bearer tokens cached in memory after first keyring read, eliminating repeated macOS Keychain prompts.

### Changed
- **Session restore filters plain shell tabs** — On restart, only terminals with an active agent session (`agentType` set) are restored. Plain shell tabs are discarded and a fresh terminal is spawned, eliminating ghost tabs that couldn't resume anything.

### Fixed
- **Sidebar "Add Agent" launch now respects run configs** — Launching an agent from the branch context menu previously wrote the raw command string directly to the PTY via `pty.write(cmd + "\r")`, bypassing `sendCommand()` and missing the Ctrl-U prefix required by Ink-based agents. Now routes through `pty.sendCommand()` like the active-terminal path.
- **Intent parsing for custom command aliases (C2, c, wrappers)** — `classify_agent` only matches literal binary names like `"claude"`, so sessions launched via aliases, symlinks, or wrapper scripts never flipped `agent_active_for_parse` on and silently dropped `intent:`/`suggest:` tokens. `PtyConfig` now carries an optional `agent_type` that pre-seeds `SessionState.agent_type` at PTY creation, and `get_session_foreground_process` falls back to the pre-seeded type when it sees a non-shell process that `classify_agent` doesn't recognise.
- **`--session-id` injection for custom Claude commands** — `buildAgentLaunchCommand` now accepts an `agentType` parameter and injects `--session-id` for any command the user has mapped to the Claude agent type, not only commands whose binary name starts with `claude`.
- **Resume session uses the default run config** — Clicking resume in the top bar previously hardcoded `claude --resume <id>`, ignoring whatever custom command/args the user configured as their default run config. `buildResumeCommand` / `verifyAndBuildResumeCommand` now swap the binary for `runConfig.command` and append `runConfig.args` after the resume flag (e.g. `c2 --resume <id> --model claude-opus-4-6`).
- **Activity Dashboard stale row content** — The dashboard snapshotted full row data (intent, status, last-prompt, task) every 10 s, so intent/status updates lagged up to 10 s behind reality. Now the snapshot stabilises only the sort order (list of ids); row contents are read live from the store on each render.
- **Context menu stealing terminal focus** — Right-clicking a panel (e.g. File Browser "Copy Path") left focus on the panel after the menu closed, making the terminal unresponsive to keystrokes. The context menu now captures `document.activeElement` on open and restores focus on close.
- **AI Chat message list compression** — Chat panel content was visually compressing instead of scrolling when many messages were present. Fixed with `min-height: 0` on the flex container and `flex-shrink: 0` on children.
- **15 Clippy errors from Rust 1.95** — Fixed `unnecessary_sort_by` (→ `sort_by_key` with `Reverse`), `collapsible_match` (→ match guards), and `collapsible_if` (→ `&&` chains) across 10 files.
- **Cmd+modifier+Enter sending `\r` to PTY** — Prevented Cmd+Shift+Enter and Cmd+Alt+Enter from injecting carriage returns into the terminal.
- **MCP stdio proxy "0 tools"** — `StdioMcpClient.rpc()` now matches JSON-RPC responses by `id`, skipping server notifications emitted between request and response. Previously a single interleaved notification (e.g. `notifications/tools/list_changed`) would be consumed as the `tools/list` response, silently yielding 0 tools.
- **macOS keychain prompt spam** — `HttpMcpClient` now caches the resolved bearer token in memory after the first keyring read. Health checks (every 60s) and tool calls reuse the cache instead of hitting the OS keychain each time. Cache is invalidated on 401 and re-populated after token refresh.
- **Tilde expansion in all user-supplied paths** — `std::process::Command` and `std::fs` do not expand `~`; paths like `~/bin/mdkb` failed with ENOENT. Added `crate::cli::expand_tilde()` and applied it in 11 sites: MCP stdio client (command, args, cwd), PTY (shell, cwd), agent spawn (binary_path, cwd), headless prompts (command, args, repo_path), shell scripts (repo_path), MCP HTTP session/agent/transport (cwd), worktree setup scripts (cwd), plugin exec (cwd validation), and plugin filesystem (path validation).
- **Silent "0 tools" diagnostic** — Both stdio and HTTP MCP clients now log `warn!` when `tools/list` response is missing `result.tools`, instead of silently returning an empty tool list via `unwrap_or_default()`.

### Added (tests)
- **Claude Wakeup unit tests** — 18 tests covering `canWake` state machine, typing-resets-lastIdleAt (the main false-wake bug), done detection via busy-cycle duration, and re-arm after disarm logic.
- **Stdio RPC id-matching tests** — Two tests verifying that `rpc()` correctly skips interleaved server notifications (1 and 3 notifications) and returns the matching `tools/list` response.
- **`expand_tilde` unit tests** — Tests for `~/path` expansion and no-op cases (absolute paths, relative paths, `~other_user`).

## [1.0.6] - 2026-04-18

### Added
- **AI Chat knowledge history overlay** (`#1387-e745`) — Two-pane modal browser for persisted `SessionKnowledge`: sessions list (sorted by most recent activity) + per-command detail pane. Debounced full-text search matches command, output snippet, inferred `error_type`, and opt-in `semantic_intent`. Filters: errors-only checkbox, date window (24h / 7d / 30d / all). Per-command card shows kind badge, timestamp, CWD, exit code, duration, output snippet, and a copy-command button. Launched from a "History" button next to the `SessionKnowledgeBar`; Esc closes. Backed by two Tauri commands: `list_knowledge_sessions(filter?, limit?)` and `get_knowledge_session_detail(session_id)` — served from the in-memory store when active and from disk (`<config_dir>/ai-sessions/`) otherwise.
- **Experimental AI block enrichment** (`#1389-b547`) — Opt-in Settings flag (`experimental_ai_block_enrichment`, default off). When enabled, every completed OSC 133 D block is enqueued to a bounded `mpsc` worker that asks the active AI Chat provider for a one-line `semantic_intent` and writes it back to the `CommandOutcome`. `CommandOutcome` gains a stable `id: u64` (dense per session) so the worker can target the exact record. Per-minute rate limit (~10/min), silent drop on full queue or disabled setting — never blocks the PTY path. Intent lines surface in the knowledge history overlay.
- **Drag & drop files onto folders in File Browser** — Dropping OS files/folders on a directory row in the File Browser moves them into that directory (hold `⌥`/`Alt` on macOS, `Ctrl` elsewhere, to copy instead). Name conflicts are silently skipped. Recursive transfers of directories prompt for confirmation. Enabled by flipping `dragDropEnabled` to `true` and routing Tauri's `onDragDropEvent` payload — which carries absolute OS paths — through a hit-tester. Terminal drops now paste shell-quoted absolute paths (previously only the basename was available via the browser `File.path` non-standard field, which broke commands referencing the dropped file).
- **AI Chat panel** — Conversational AI companion that sees the terminal as the user sees it. Side panel + settings tab (`Cmd+Alt+A` toggle, action `toggle-ai-chat`). Streaming markdown with syntax-highlighted code blocks and "Run this" action back to the active PTY. Multi-provider: Ollama (local, auto-detected on `localhost:11434`), Anthropic Claude, OpenAI, OpenRouter, custom base URL. Per-turn context assembly pulls `VtLogBuffer` clean text (configurable line budget), `SessionState`, recent `ParsedEvent`s, git branch/diff. API keys stored in OS keyring (service `tuicommander-ai-chat`). Conversations persisted to disk with load/list/delete. Terminal context menu actions (send selection / send error) and global hotkey. Settings at `Settings > AI Chat` (provider, model, base URL, temperature, context lines).
- **AI Agent loop (ReAct, Levels 2 + 3)** — Autonomous agent that observes a terminal and acts through six terminal tools: `read_screen`, `send_input`, `send_key`, `wait_for`, `get_state`, `get_context`, plus six Level 3 filesystem tools: `read_file`, `write_file`, `edit_file`, `list_files`, `search_files`, `run_command`. Filesystem access confined by `FileSandbox` path jail scoped to the repo root. `run_command` executes in a sandboxed subprocess with timeout. File-write safety checker blocks writes to sensitive paths (`.env`, credentials, CI configs). Pause/resume between iterations, user approval card for destructive commands (detected by `SafetyChecker`: `rm -rf`, `git reset --hard`, `DROP TABLE`, force push, `dd`, …). Structured command parser extracts executable + args for precise safety classification. Tool-call cards in `AIChatPanel` with collapsible output. Conversation schema v2 persists tool-call records alongside messages. `tokio-tracing` observability spans across the entire agent module.
- **Session knowledge store** — Per-session accumulator: command outcomes with exit code, duration, CWD, classification (`Success` / `Error{error_type}` / `TuiLaunched{app_name}` / `Timeout` / `UserCancelled` / `Inferred`), auto-correlated error→fix pairs, CWD history, TUI apps seen, terminal mode. Fed by OSC 133 semantic prompt markers (`OSC 133;A/B/C/D`) with an inferred-outcome fallback from the silence timer when OSC 133 is absent. Injected into the agent system prompt as a compact markdown summary. Surfaced in the UI as a collapsible `SessionKnowledgeBar` under the chat panel. Persisted to `<config_dir>/agent-knowledge/<session_id>.json` with a 2 s debounced background flusher. Tauri command: `get_session_knowledge`.
- **TUI app detection** — `tui_detect.rs` heuristics track alternate-screen enter/leave (`ESC[?1049h`/`l`) to classify `TerminalMode` as `Shell` or `FullscreenTui { app_hint, depth }`. Known signatures (vim, htop, lazygit, less, tmux, claude, …) populate `tui_apps_seen`. Agent tool set adapts: in TUI mode `send_input` prefers `send_key` semantics and `wait_for` polls the rendered screen rather than lines-since.
- **`ai_terminal_*` MCP tools (external agent surface)** — Six tools exposed to external MCP clients (Claude Code, Cursor, …) so a remote agent can observe and drive a TUICommander terminal: `ai_terminal_read_screen`, `ai_terminal_send_input`, `ai_terminal_send_key`, `ai_terminal_wait_for`, `ai_terminal_get_state`, `ai_terminal_get_context`. `send_input`/`send_key` always prompt the user and are rejected while the internal agent loop is active on the target session. Output passes through secret redaction.
- **ChoicePrompt parser variant** — New `ParsedEvent::ChoicePrompt { title, options, dismiss_key, amend_key }` detects Claude-Code-style numbered confirmation menus (footer matches `Esc to cancel · Tab to amend`; options match `[❯›>] <digit>[.)] <label>`; minimum two options; title heuristics require `?` or a verb prefix like "do you want"/"proceed"/"confirm"). Destructive labels (`no`, `cancel`, `reject`, `abort`, `deny`, `don't`/`do not`) flagged for styling. Piped into `SessionState.choice_prompt`, dispatched to plugins via `pluginRegistry.dispatchStructuredEvent("choice-prompt", …)`, and rendered as a PWA overlay. Desktop listener plays a warning sound when the prompt arrives on an inactive tab.
- **MCP upstream OAuth 2.1** — Full RFC 9728 (Protected Resource Metadata) + RFC 8414 (Authorization Server Discovery) flow for upstream MCP servers. `TokenManager` handles PKCE S256, code exchange, and refresh with a per-upstream semaphore that defeats thundering-herd refresh. `OAuthFlowManager` drives the PKCE dance, pending-flow state machine, and a localhost dev callback server. Native deep link `tuic://oauth-callback` completes the flow in the desktop app. `UpstreamAuth::OAuth2 { client_id, scopes, authorization_endpoint?, token_endpoint? }` joins `Bearer` as a credential type. `UpstreamError::NeedsOAuth { www_authenticate }` is surfaced on 401 so the registry transitions the upstream to `needs_auth` and the UI shows an "Authorize" button (AS origin displayed in the consent dialog to defend against AS mix-up). Tokens are persisted to the OS keyring as structured JSON (`OAuthTokenSet` with `expires_at`). Auto-triggered OAuth is gated behind explicit user consent.
- **OAuth 2.1 auth selector (Services settings)** — Per-upstream auth picker (`none` / `bearer` / `oauth2`). OAuth fields: client ID, scopes, optional authorization + token endpoints (defaults come from discovery). Live status includes `authenticating` ("Awaiting authorization…"). "Authorize" and "Cancel" buttons wired to the three OAuth commands.
- **Open with Default App (File Browser)** — Generic "Open with Default App" context-menu entry on any filesystem item defers to the OS handler. Complements the existing code-editor / markdown / HTML routing for known extensions.
- **Open File / Folder / Path + expanded Tools menu** — New app menu entries for opening arbitrary files, folders, and paths from disk, plus reorganised Tools submenu.
- **Plugin iframe reload** — Right-click context menu entry and `Cmd/Ctrl+R` reload for plugin iframes.
- **Smart Prompts Shell Script mode** — New "Shell script (direct run)" execution mode runs prompt content directly as a shell command without any AI agent. Executes via system shell (`sh`/`cmd`) in the repo directory with 60s timeout. Supports all context variables (`{branch}`, `{repo_path}`, etc.) and output targets (clipboard, toast, panel). Ideal for automating CLI pipelines like branch cleanup, linting, or metrics collection
- **TUIC SDK v1.0 expansion** — Plugin iframes now have access to the full SDK: `tuic.activeRepo()`, `tuic.onRepoChange()`, `tuic.getFile()`, `tuic.toast()`, `tuic.clipboard()`, `tuic.send()`/`tuic.onMessage()`, `tuic.theme`/`tuic.onThemeChange()`. Relative paths resolve against the active repo with traversal guard. SDK is auto-injected into same-origin URL-mode iframes. Interactive test page at `docs/examples/sdk-test.html`
- **Visual toast notifications** — `tuic.toast()` (SDK) and MCP `ui action=toast` now display visual toast notifications in the bottom-right corner with auto-dismiss (4s) and click-to-dismiss. Optional `sound` parameter plays a synthesized notification tone per level (info = soft blip, warn = double beep, error = descending sweep)
- **Multi-format Preview tab** — Clickable file paths, drag & drop, and File Browser now open preview-capable files in a dedicated tab: HTML (sandboxed iframe), PDF, images (PNG/JPG/GIF/WebP/SVG/AVIF/ICO/BMP), video (MP4/WebM/OGG/MOV), audio (MP3/WAV/FLAC/AAC/M4A), and plain text/data (TXT/JSON/CSV/LOG/XML/YAML/TOML/INI/CFG/CONF). File routing via `classifyFile()` utility.
- **Focus mode** (`Cmd+Alt+Enter`) — Hides sidebar, tab bar, and all side panels to maximize active tab content. Toolbar and status bar remain visible. Session-only (not persisted).
- **PWA echo classification** — Smart PTY input line sync for the mobile PWA: `classifyEcho()` rejects stale prefix echoes, accepts superset echoes (tab completion) immediately, and holds unrelated text during a 300 ms grace window after writes settle. Eliminates duplicate/deleted characters under latency.

### Security
- **`write_external_file` restricted to home-dir allowlist** — Tauri command now refuses writes outside `$HOME` to eliminate drive-wide write via crafted paths.
- **`stat_path` TCC guard (macOS)** — Returns a deterministic permission error instead of silently failing when the path is in a TCC-protected directory without access.
- **Smart Prompts `env_clear` + allowlist** — Shell and headless execution paths now clear the environment and re-inject a small allowlist before spawning, preventing repo-controlled env vars from leaking into subprocesses.
- **Smart Prompts shell-quoting** — Repo-controlled variables are shell-quoted in shell mode; eliminates injection through `{branch}`/`{repo_path}` containing metacharacters.
- **Smart Prompts run-config args argv-safe** — Argv-form execution (no intermediate shell), metacharacters in args now literal.
- **MCP headless/api prompt routes localhost-only** — The `/headless/prompt` and `/api/prompt` routes refuse non-localhost peers.
- **Settings API key masking** — API key fields are masked by default with an eye toggle to reveal; prevents shoulder-surfing while editing.
- **Duplicate env-var key detection** — Settings validates that each env-var key is unique per run config before save.
- **MCP OAuth hardening** — Unified `redirect_uri` between registry and commands (`#1260`); share `TokenManager` across http-client refresh paths (`#1270`); accept `None expires_at` as valid (`#1269`); defend against Authorization Server mix-up and display AS origin in the consent dialog (`#1268`); dropped dead constant-time comparison and documented the desktop-deep-link threat model (`#1266`).
- **File-stem ID validation** — Conversation and knowledge file IDs are now validated against path traversal patterns (`..`, `/`, `\`), blocking crafted IDs from escaping their storage directory.
- **OSC 133 data sanitization + secret redaction** — OSC 133 prompt markers are stripped from tool event output. API keys and tokens in terminal content are redacted before being persisted or sent to the AI provider.

### Changed
- **Experimental feature flags** — Settings > General now has an "Experimental Features" section with a master toggle and sub-flag for AI Chat. Features gated behind `experimental_features_enabled` are hidden until the user explicitly opts in.

### Fixed
- **Ctrl-U injection shell-family aware** — Prefix is now selected based on the detected shell family (POSIX/Windows) rather than the host platform, preventing bogus clears when mixing PowerShell with a POSIX shell or WSL.
- **`suggest:` parser robustness** — Dewraps the `suggest:` keyword when split across a newline on narrow terminals; accepts the wrap tail; bounds the overlay pipe-heuristic. Avoids an unconditional `String` allocation in `dewrap_suggest_keyword` (perf).
- **Keybindings capture cancel / replace** — Global hotkey restored when capture is cancelled; conflict-replace now unbinds the previous action explicitly and compares canonical action names.
- **Branch switch resilience** — `handleBranchSelectInner` body wrapped in `try/finally` so UI always returns to a clean state on failure.
- **Per-PR diff loading state** — Loading state is now scoped per-PR instead of being shared across all expanded PRs.
- **Plugin panel tabs evicted on repo switch** — Non-pinned plugin-panel tabs no longer leak across repositories.
- **App init repo-revision bump** — `repo-changed` now bumps the revision synchronously, avoiding a race where panels rendered with stale state.
- **AI Chat listener leaks** — Disposed stale `onCleanup` listeners and removed direct DOM manipulation hazards in `AIChatPanel` that could leak event handlers across re-renders.
- **Agent session identity binding** — Agent loop now binds to a specific PTY session ID at start, preventing tool calls from writing to a different terminal if tabs are switched mid-run.
- **Agent filesystem I/O off async runtime** — File system operations in the agent module moved to `spawn_blocking` with `Acquire`/`Release` atomics, avoiding blocking the Tokio runtime on large reads/writes.
- **PWA real-time idle/busy indicator** — Mobile PWA now subscribes to SSE `shell-state` events for live busy/idle status updates instead of polling.
- **Clipboard copy race condition** — Fixed race where clipboard copy could grab the wrong selection source when multiple terminals competed for the selection.
- **Cmd+Alt+letter shortcut combos** — `Cmd+Alt+<letter>` hotkey combinations now register correctly on macOS; AI Chat toggle button repositioned next to the mic button.

## [1.0.5] - 2026-04-12

### Added
- **PWA bidirectional live sync** — CommandInput in browser mode now uses delta-based bidirectional sync with the PTY, including echo deduplication and a slash dropup menu for command suggestions

### Fixed
- **Ghost terminals from resurrected store entries** — Prevented phantom terminal sessions from appearing when store entries were resurrected after session close
- **MCP debug invoke_js schema** — `__TUIC__` global is now hinted in the invoke_js schema for MCP debug introspection
- **vt-log-total emitted as bare number** — PTY event now emits the total as a plain number instead of a JSON object wrapper
- **PWA agent_type unified to "claude"** — Consistent `agent_type` value across the entire codebase
- **PWA slash menu detection** — Improved slash menu detection accuracy and fixed tab close countdown in browser mode
- **Cache-keepalive counter reset** — Counter now resets on user-input events instead of arbitrary triggers, preventing phantom busy→idle loops

### Changed
- **GitHub OAuth app** — Moved to TUICommander organization

## [1.0.4] - 2026-04-11

### Added
- **GitHub Issues panel** — Unified GitHub panel now shows issues alongside PRs. Filter by Assigned (default), Created, Mentioned, All, or Disabled. Each issue displays labels with GitHub-matching colors, assignees, milestone, and comment count. Actions: open in browser, close/reopen, copy number. Filter setting persisted in config
- **Terminal fontSize inheritance** — New terminal tabs inherit font size from the active terminal instead of always using the global default
- **Interactive GFM checkboxes** — Task-list items (`- [ ]`, `- [x]`, `- [~]`) in the Markdown panel are now clickable. Clicking cycles through unchecked → checked → in-progress (indeterminate) → unchecked. Changes are written back to the source file on disk. Source-line mapping ensures correct checkbox identification even with fenced code blocks
- **Dynamic debug store registry** — All frontend stores self-register via `debugRegistry.ts`. MCP `invoke_js` can now call `__TUIC__.stores()` and `__TUIC__.store(name)` for runtime introspection without manual bridge additions

### Fixed
- **Issue filter hydration race** — `issueFilter` now reads from `settingsStore` as single source of truth, eliminating a race where `githubStore` could read the pre-hydrate default
- **Shared issue action error** — Error messages for issue close/reopen are now scoped per-issue instead of shared across all expanded issues
- **Smart prompt variable guard** — Unresolved variables are now always flagged even when `contextVariables` are provided
- **Diagnostics crash in Settings** — Replaced non-null assertions (`!`) with optional chaining (`?.`) in GitHubTab diagnostics section to prevent crash when auth state hasn't resolved
- **Agent transition events missed on direct switches** — Switching directly between agents (e.g. claude → codex) without first returning to idle now correctly emits `agent-stopped` for the previous agent and `agent-started` for the new one. Previously, plugins filtered on a single `agentType` (like cache-keepalive) kept leaking internal state across the switch
- **xterm scrollbar still hidden on untouched terminals** — The previous CSS-only `.fade` override missed terminals that had never been interacted with (xterm's `_hide(e)` early-returns before adding `.fade` on the first reveal). Replaced with a JS observer that sources overflow from the xterm buffer model and forces inline opacity, covering streaming agents in background tabs
- **Overlay rectangles stuck during scrollback** — Suggest/intent row overlays no longer pin to the viewport top when scrolling through scrollback; the observer now listens to `onScroll` in addition to `onRender`

## [1.0.3] - 2026-04-10

### Added
- **Native file drag & drop** — Files dropped onto the window now use Tauri's native `onDragDropEvent` API, providing absolute OS paths instead of bare filenames. Dropped files write to the active PTY (for running agents) or open in the appropriate tab. Browser mode falls back to HTML5 drag events
- **Remote tab auto-close countdown** — When a remote (MCP) session closes, the tab name shows a live countdown (e.g. "PTY: Session 2 (45s)") before auto-removing after 60 seconds

### Fixed
- **Remote session tabs not appearing** — Fixed `pendingLocal` guard that blocked all `session-created` events when any local terminal had a null `sessionId` (e.g. during PTY reconnect after page reload). Now uses `browserCreatedSessions` set for accurate local/remote distinction
- **Remote tabs invisible for non-repo paths** — Sessions spawned from directories outside any tracked repository (e.g. `/tmp`) now fall back to the currently active repo/branch instead of creating an invisible orphan tab
- **Phantom question notifications** — Question detection now inspects only the single last chat line above the prompt instead of scanning 15 lines deep. Prevents false notifications from the user's own prior `?`-ending input or stale agent content across turn boundaries
- **xterm scrollbar disappearing** — Override xterm v6's auto-fade scrollbar behavior so the vertical scrollbar stays visible whenever there is scrollback content, instead of hiding when idle

## [1.0.2] - 2026-04-08

### Added
- **Global Workspace (experimental)** — Cross-repo workspace mode (`Cmd+Shift+X`) that promotes terminals from any repo into a unified view. Auto-layout distributes promoted terminals across panes. Sidebar globe icon indicates promoted state. Deactivating restores per-repo layouts. Repo isolation preserved in TabBar and pane persistence
- **Multi-monitor (POC)** — Secondary window with pane-only layout for multi-display setups
- **TUIC SDK `edit` command** — `tuic://edit/path` deep link opens files for editing in the host app
- **MCP debug introspection** — Full MCP log coverage and `eval_js` action in the debug tool for runtime diagnostics

### Changed
- **Markdown panel shortcut** — Changed from `Cmd+M` to `Cmd+Shift+M` to avoid conflict with macOS system minimize. All docs, tests, and keybinding defaults updated
- **Settings IPC** — `structuredClone` for plain-JS store paths eliminates redundant settings IPC round-trips

### Fixed
- **Suggest bar reliability** — Rewrote `conceal_suggest` as a simple single-chunk stream filter (no cross-chunk buffering that caused scroll freezes). Handles `\n`-delimited and Ink `\r`-segment layouts. Also: deferred PTY creation via ResizeObserver, stale actions cleared on user input, number prefixes stripped from chip display
- **Terminal focus restoration** — Focus returns to the active terminal after closing any modal dialog (CommandPalette, SmartPromptsDropdown, PromptDialog, PromptDrawer, BranchSwitcher, RunCommandDialog, RenameBranchDialog, CreateWorktreeDialog) and after PaneTree tab switches
- **Repo-switch freeze** — Eliminated visibility thundering herd that caused UI freeze when switching repositories with many terminals
- **Idle detection** — Replaced chrome-tick guard with spinner keepalive; tuned agent idle thresholds to prevent indicator flicker while maintaining responsive idle transitions
- **Plugin panel CSP** — URL-mode plugin tabs use `src=` attribute instead of fetch-inject to avoid Content Security Policy blocking
- **Tab ordering** — Tabs now follow spatial pane layout order in split mode
- **Keepalive replay** — Shell-state replayed after agent detection completes, preventing stale state after keepalive reconnection
- **Plugin localhost URLs** — `fetch_tab_html` now allows localhost URLs for local development dashboards

## [1.0.1] - 2026-04-06

### Added
- **Open file / New file** — `Cmd+O` opens a file picker and routes the result through the standard extension-based dispatch (`.md`/`.mdx` → markdown tab, `.html`/`.htm` → HTML preview tab, other → code editor). `Cmd+N` prompts for name + location, creates the empty file, and opens it the same way. Both also available from the command palette under the "File" category.
- **`file://` URLs in terminal** — Clickable `file:///…` URLs in terminal output are now recognized alongside plain paths. The prefix is stripped and the path resolved via the existing `resolve_terminal_path` flow.
- **Idle branch icons** — Sidebar branch icons (star, branch, worktree) turn grey when the repo has no active terminals, making it easy to spot repos with running sessions at a glance.
- **TUIC SDK for plugin iframes** — Every plugin iframe now receives `window.tuic`, a lightweight API for host integration. `tuic.open(path, {pinned?})` opens markdown files, `tuic.terminal(repoPath)` opens terminals, and `<a href="tuic://open/...">` links are intercepted automatically. Paths validated against known repos.
- **CSS popover tooltips** — Native popover-based tooltips for MCP tool descriptions, replacing title attributes with styled, multi-line popover panels
- **URL-mode plugin panels** — Plugin panels can now load external URLs directly via the `url` parameter in the MCP `ui` tool, enabling embedded dashboards like Mission Control
- **PWA reconnection banner** — Mobile PWA shows a reconnect dialog when the server goes down, with auto-reconnect on server recovery
- **Service worker fetch interception** — PWA service worker intercepts fetch requests for offline splash page and push subscription persistence across updates

### Changed
- **Ideas panel shortcut** — Moved from `Cmd+N` to `Cmd+Alt+N` so `Cmd+N` can serve the universal "New file" convention. Users with an override in `keybindings.json` keep their existing binding.
- **PWA push gate** — Web Push delivery is now gated on desktop window focus instead of a PWA heartbeat. Notifications fire whenever the desktop window is not focused (phone locked, screen off, etc.), finally matching the intended "wake the service worker" semantics. Removed the now-unused `POST /api/push/heartbeat` endpoint.
- **MCP tool consolidation** — 12 native MCP tools consolidated to 8 with unified dispatch routing, config defaults, and tmux-optimized meta-tool descriptions. Reduces tool-selection overhead for AI agents.
- **Pane layout persistence** — Split pane layouts now survive app restarts and branch switches, with terminal ID remapping on lazy restore

### Fixed
- **Duplicate history on WebSocket catch-up** — The raw PTY WebSocket handler racily registered its live subscription against the ring-buffer snapshot read, causing bytes written during the small gap to appear in both the catch-up replay and the live stream. Serialized `ring.write` + `ws_clients` broadcast under the same lock on both the producer and consumer sides, eliminating the duplication window.
- **Agent settings checkboxes** — Custom checkbox styles for agent settings toggles now render correctly across all themes
- **Flaky race condition test** — `ws_catchup_no_duplicate_with_concurrent_writer` test stabilized with explicit subscriber-attached synchronization barrier

## [1.0.0] - 2026-04-04

### Added
- **HTTP compression** — Gzip and Brotli compression for all HTTP responses >860 bytes via tower-http `CompressionLayer`. Auto-negotiated via `Accept-Encoding`
- **PWA lazy loading** — Mobile terminal view loads only the last 100 lines initially, then lazy-loads older output on scroll-up with viewport anchoring
- **MCP Debug Tool** — Dev-only `debug` MCP tool with `agent_detection`, `logs`, and `sessions` actions for diagnosing PTY and agent detection issues
- **Smart Prompt Variables** — 12 new context variables: `remote_url`, `current_user`, `repo_owner`, `repo_slug`, `dirty_files_count`, `branch_status`, `pr_author`, `pr_labels`, `pr_additions`, `pr_deletions` (31 total)
- **Variable Insertion Dropdown** — Smart prompt editor includes a dropdown below the content textarea with all available variables grouped by Git/GitHub/Terminal, with descriptions; click inserts `{variable}` at cursor position
- **Prompt Editor Tags** — Prompt rows in Cmd+K drawer show inline badges for execution mode (inject/headless), built-in status, and placement tags
- **Dev Debug Console** — `window.__debug` exposed in dev mode with all SolidJS stores + Tauri `invoke`/`listen` for browser console debugging
- **File Browser Tree View** — Toggle between flat list and tree view in the file browser panel. Tree view shows a collapsible hierarchy with lazy-loaded subdirectories on first expand
- **Diff Scroll View** — All-files continuous scroll view showing every changed file (staged + unstaged) with collapsible sections, per-file stats, and clickable filenames. Toggle via toolbar or `Cmd+Shift+G`
- **Command Palette File Search** — Type `!` in the command palette (`Cmd+P`) to search files by name, `?` to search file contents with highlighted matches. Results open in editor tabs
- **Update Progress Dialog** — Modal progress bar during update downloads with percentage and status text
- **Cross-Terminal Search** — Type `~` in the command palette to search text across all open terminal buffers. Results show terminal name, line number, and highlighted match. Selecting a result navigates to the terminal and scrolls to the matched line
- **Search Mode Commands** — Explicit "Search Terminals", "Search Files", and "Search in File Contents" commands in the palette make prefix modes discoverable
- **Global Hotkey Validation** — Frontend validates key combos before sending to Tauri, showing clear error messages for unsupported keys (e.g. `<` on ISO keyboards) instead of cryptic parser errors
- **Global Hotkey** — Configurable OS-level shortcut to toggle window visibility from any app. Set in Settings > Keyboard Shortcuts. Toggle cycles: hidden → show+focus, unfocused → focus, focused → hide. Uses `tauri-plugin-global-shortcut`; hidden in browser/PWA mode
- **Unified Repo Watcher** — Single recursive watcher per repository with per-category debounce (Git/WorkTree/Config), replacing separate HEAD and index watchers. Uses `notify-debouncer-full` with `.gitignore`-aware filtering
- **Gitignore Hot-Reload** — Editing `.gitignore` rebuilds the watcher's ignore filter without restart
- **File Icon Provider** — New `ui:file-icons` plugin capability; `tuic-vscode-icons` plugin provides VS Code-style file icons in the file browser tree view
- **Plan Auto-Open** — Restores active plan from `.claude/active-plan.json` on startup; `plans/` directory watcher detects new plan files created externally
- **macOS TCC Access Dialog** — Shows a guided dialog when macOS denies access to a repository directory, pointing the user to Full Disk Access settings

### Changed
- **Structured event tokens** — New plain-prefix format (`intent:`, `action:`, `suggest:`) replaces bracket syntax (`[[intent:...]]`). Both formats supported for backward compatibility. Plain-prefix parsing is agent-gated to prevent false positives from CLI output
- **MCP system prompt** — Agents now receive `action:` token instruction alongside `intent:` and `suggest:`, all documenting column-0 requirement

### Fixed
- **Terminal Scroll Lock** — Fixed viewport jumping away from scroll position when output arrives while scrolled up. Root cause: xterm's auto-scroll during writes falsely disengaged the ViewportLock
- **Terminal Close Confirmation** — Shells in startup (.zshrc/.zprofile loading) no longer trigger close confirmation; only shells that have completed initialization and are running a user process (agents, htop, npm) prompt for confirmation
- **File Browser Repo Switch** — File browser now updates instantly when switching between repositories via sidebar; previously showed stale files from the old repo due to unbatched reactive updates and an async race condition
- **Clipboard Paste** — Restored `tauri-plugin-clipboard-manager` so Cmd+V paste works in terminals (WebView requires explicit capability for `navigator.clipboard.readText()`)
- **Agent Detection** — Claude Code installs its binary as a version number (`~/.local/share/claude/versions/2.1.87`); `process_name_from_pid` now scans parent directory names when the basename doesn't match a known agent
- **HMR Session Loss** — Vite HMR reloads no longer close PTY sessions; `beforeunload` in Tauri mode skips session cleanup so `list_active_sessions` can re-adopt surviving sessions
- **Git Panel Label** — "Changes" section renamed to "Changes (unstaged)" for clarity
- **Diff Tab Focus** — Opening a diff tab now deactivates terminal, markdown, and editor tabs to prevent keyboard conflicts
- **Plan File Events** — Plan-file events with absolute paths now recognized regardless of CWD
- **iPad Touch Scroll** — Terminal output view now scrolls with touch gestures on iPad; `touch-action: pan-y` overrides the global `manipulation` that blocked iOS pan recognition
- **Sidebar Double-Tap** — Repo and branch selection on iOS no longer requires two taps; `:hover` rules that caused iOS sticky hover wrapped in `@media (hover: hover)`
- **Tab Double-Tap** — Same iOS sticky hover fix applied to tab bar (tab highlight, close button reveal, specialized tab types)
- **Shell State Idle** — Status line ticks from Claude Code no longer block idle detection; idle fires after 3s of real output silence even with status line ticking
- **Usage Exhausted** — New `ParsedEvent::UsageExhausted` detects "out of extra usage" messages with optional reset time
- **Agent Detection Speed** — Event-driven detection on shell-state transitions (immediate on idle, 500ms debounce on busy) replaces 3s polling; ~30x fewer syscalls
- **Agent Tracking Leak** — Module-level `discoveryAttempted` and `nullStreak` maps cleaned up when terminal is removed
- **VtLogBuffer Cursor** — `total_lines()` is now a monotonic counter that doesn't decrease on eviction, fixing paginated reads for mobile/REST clients

### Changed
- **Watcher Backend** — Upgraded from `notify-debouncer-mini` to `notify-debouncer-full`; deleted legacy `head_watcher` module
- **Smart Prompts Management** — Settings tab removed; all management consolidated in the Cmd+K drawer (edit, enable/disable, create, delete)
- **Prompt Drawer UI** — Compact font sizing aligned with command palette conventions; editor dialog layout improved with side-by-side execution mode + auto-execute fields
- **Tailscale HTTPS** — Auto-detects Tailscale daemon, provisions TLS certificates via Local API, serves HTTP+HTTPS on same port (dual-protocol). QR code uses `https://` with Tailscale FQDN when TLS active. Background cert renewal every 24h. Cross-platform (macOS, Linux, Windows)
- **PWA Push Notifications** — Web Push from TUICommander directly to mobile PWA clients. VAPID key generation, push subscription management via `/api/push/*` endpoints, service worker with push/notificationclick handlers. Rate limited (1 per session per 30s). iOS standalone detection with guidance
- **Smart Prompts** — AI automation layer with 24 built-in context-aware prompts
  - Toolbar dropdown (Cmd+Shift+K) with category grouping and search
  - SmartButtonStrip in Git Panel Changes tab and PR Detail Popover
  - Command Palette integration (all prompts with "Smart:" prefix)
  - Branch context menu integration
  - Inject mode: PTY write into active agent with idle check
  - Headless mode: one-shot agent execution with configurable per-agent templates
  - Auto-resolved context variables ({diff}, {branch}, {pr_number}, etc.)
  - Settings > Smart Prompts tab for management (enable/disable, edit, reset to default)
  - Settings > Agents: headless command template per agent
- **MCP Per-Repo Scoping** — Each repo can define which upstream MCP servers are relevant via an allowlist in repo settings (3-layer: per-repo > `.tuic.json` > defaults). Quick toggle via **Cmd+Shift+M** popup with live status, transport badges, tool counts, and per-repo checkboxes
- **Side-by-Side Diff Viewer** — Split and unified view modes with `@git-diff-view/solid`, word-level highlighting, and synchronized scrolling. Toggle persisted in ui-prefs.
- **Hunk & Line-Level Restore** — Revert individual hunks or selected lines in working tree and staged diffs via `git apply --reverse`. Click lines to select, shift-click for ranges, floating action bar with line count.

- **Smart Prompts API Mode** — New "API (LLM direct)" execution mode calls LLM providers directly via HTTP API (genai crate), no terminal or agent CLI needed. Global provider/model/API key config in Settings > Agents. Per-prompt system prompt. Supports OpenAI, Anthropic, Gemini, OpenRouter, Ollama, and any OpenAI-compatible endpoint. API key stored in OS keyring
- **Notification Bell Enhancements** — CI recovery ("CI Passed") notifications, background git operation results, worktree creation events. Empty state shows "No notifications" instead of 1px dropdown.
- **TCP Port Retry** — MCP HTTP server tries up to 3 adjacent ports when the configured port is busy, with clear error message on failure.
- **Base Branch Tracking** — Branches store a base ref in git config (`tuicommander-base`), showing ahead/behind relative to base in sidebar. "Update from base (rebase)" in context menu. Inline branch create form includes a base ref selector with grouped Local/Remote refs. Auto-fetches remote refs before creation
- **Edit File Button** — Diff tab toolbar includes "Edit file" button to open the file in the default editor
- **OSC 8 File Links** — Terminal `file://` URIs from OSC 8 hyperlinks now open in the system file opener
- **Tailscale Recheck** — Settings > Services tab includes a "Recheck" button for Tailscale HTTPS status
- **Profiling Infrastructure** — Scripts in `scripts/perf/` for IPC latency, PTY throughput, CPU recording, Tokio console, and memory snapshots. See `docs/guides/profiling.md`

### Changed
- **PTY write coalescing** — Terminal writes are accumulated per animation frame via `requestAnimationFrame` (~60/sec) instead of calling `terminal.write()` for every PTY event (hundreds/sec during burst output). Reduces xterm.js render passes and WebGL texture uploads
- **Async git commands** — All ~25 git commands converted to async with `tokio::task::spawn_blocking`, preventing git subprocess calls from blocking Tokio worker threads. `get_changed_files` merged from 2 sequential subprocesses to 1
- **Watcher-driven git cache** — `repo_watcher` invalidates git caches immediately on file system changes instead of relying on 5s TTL. TTL raised to 60s as safety net for missed watcher events. Most IPC calls hit cache (~0.2ms) instead of spawning git (~20-30ms)
- **Process name via syscall** — `proc_pidpath` (macOS) / `/proc/pid/comm` (Linux) replaces `ps` fork for terminal process detection. Eliminates ~100 fork+exec/min with 5 terminals open
- **MCP RwLock** — MCP upstream `HttpMcpClient` uses `RwLock` instead of `Mutex`. Tool calls use read lock (concurrent); only reconnect takes write lock
- **Double serialization eliminated** — PTY parsed events serialized once with `serde_json::to_value`, reused for both Tauri IPC emit and event bus broadcast
- **PTY read buffer** — Increased from 4KB to 64KB for natural batching of burst output, reducing IPC events during high-throughput agent output
- **Bundle splitting** — Vite `manualChunks` splits xterm, codemirror, diff-view, markdown into separate chunks. Lazy-load SettingsPanel, ActivityDashboard, HelpPanel with `lazy()` + `Suspense`
- **Conditional StatusBar timer** — 1s timer only runs when merged PR or rate limit is active, eliminating ~60 signal writes/min during normal operation
- **ActivityDashboard reactivity** — Removed `{ equals: false }` from snapshot signal; SolidJS default equality check prevents unnecessary `<For>` diffs every 10s
- **SmartButtonStrip** — Extracted as reusable split button component with dropdown, spinner, click-outside, error callbacks, last-used memory. Integrated in git-changes, git-branches, pr-popover

### Fixed
- **Browser mode parsed events** — Structured events (suggest, status-line, rate-limit, question, progress, etc.) now work in browser/remote mode via WebSocket, not just Tauri desktop.
- **Stale suggestion chips** — Follow-up suggestions no longer reappear from buffer re-scans during resize/tab-switch; requires agent idle state.
- **Git spawn error diagnostics** — "Spawn failed" errors now include the working directory path in the log message for easier debugging.
- **MCP upstream URL overflow** — Long URLs in the Services tab no longer push action buttons off-screen; URLs now truncate with ellipsis
- **Stale worktree pruning** — `get_worktree_paths` now runs `git worktree prune` before listing and skips entries whose directory no longer exists on disk
- **Commit textarea auto-expand** — Textarea grows with content using `scrollHeight`, switches to scrollable when exceeding max-height
- **Agent polling race** — Fixed `useAgentPolling` early return that prevented interval creation when sessionId was set after terminal add. Added 3-poll debounce before clearing agent status
- **False idle from silence timer** — Chrome-chunk arrivals no longer reset the silence timer, preventing false idle transitions during streaming output
- **Tailscale cert fallback** — Falls back to CLI cert provisioning on macOS App Store builds where Local API is unavailable
- **rustls CryptoProvider** — Explicitly install `ring` CryptoProvider at startup to prevent "no process-level CryptoProvider" panic

## [0.9.9] - 2026-04-02

### Added
- **Copy on select** — Auto-copy terminal selection to clipboard (enabled by default in Settings > Appearance)
- **Terminal bell styles** — Configurable bell: none, visual (screen flash), sound (via notification system), or both
- **Scroll shortcuts** — Cmd+Home (top), Cmd+End (bottom), Shift+PageUp, Shift+PageDown
- **Zoom pane** — Cmd+Shift+Enter maximizes/restores the active split pane
- **Ctrl+Tab / Ctrl+Shift+Tab** — Native tab switching via macOS NSEvent monitor (bypasses WKWebView interception)
- **Dictation auto-send** — Option to automatically press Enter after transcription completes
- **Environment flags UI** — Per-agent environment variable injection from Settings > Agents
- **--bare flag** — CLI option for minimal startup
- **ANSI anomaly logging** — Diagnostic logging for unusual terminal escape sequences (scroll-jump investigation)

### Changed
- **Watcher v3** — Replaced `notify-debouncer-full` with raw `RecommendedWatcher` and manual per-category debounce
- **PTY rendering** — Replaced DiffRenderer with cursor-up clamping for simpler escape sequence handling
- **Bell implementation** — Moved from xterm.js built-in `bellStyle` option to manual `onBell` handler with notification system integration
- **Prompt library shortcut** — Menu accelerator corrected from Cmd+K to Cmd+Shift+K (Cmd+K is clear scrollback)
- **Git panel shortcut** — Menu accelerator corrected from Cmd+Shift+G to Cmd+Shift+D (Cmd+Shift+G is diff scroll)
- **Tab switching** — Removed Cmd+Shift+[/] defaults (unreliable on non-US keyboards), Ctrl+Tab is now primary

### Fixed
- **Rate limit warning** stuck in status bar after expiry
- **Terminal CWD** falls back to active repo path when PTY reports no working directory
- **Agent events** — Plugin system now emits `agent-started` / `agent-stopped` events correctly
- **Copy-on-select** feedback — Shows "Copied to clipboard" in status bar
- **Keyboard shortcuts help** — Added 12 missing shortcuts to the help panel
- **Documentation** — Corrected Cmd+K → Cmd+Shift+K references across 6 doc files, updated tab switching docs

## [0.9.7] - 2026-03-26

### Added
- **Sidebar Plugin Panels** — New `ui:sidebar` capability lets plugins register collapsible panel sections in the sidebar below the branch list. Panels display structured data (items with icon, label, subtitle, badge, context menu) scoped per-repo. Built-in plan plugin migrated from Activity Center to sidebar panel
- **Multi-target context menu actions** — Plugins can now register actions in branch, repo, and tab context menus (not just terminal). New `registerContextMenuAction()` API with target types and typed context
- **Open in GitHub** — Branch and repo right-click context menus now include "Open in GitHub" (opens branch/repo on github.com) and "Open PR" (direct link to the PR if one exists)
- **Startup notification suppression** — PTY sessions now suppress Question, RateLimit, and ApiError notifications during the initial output burst (e.g. `claude --continue` replaying conversation history). Grace ends after 5s without output or 120s max

### Fixed
- **Plugin double-dispose crash** — Plugin disposables are now idempotent; calling `dispose()` twice no longer crashes with "undefined is not an object (evaluating 'listeners[eventId].handlerId')"
- **awaitingInput not cleared on idle→busy** — Question notifications are now properly cleared when the agent resumes work (idle→busy transition). The null→busy case is excluded since the agent hasn't been idle yet

## [0.9.6] - 2026-03-25

### Added
- **Inter-Agent Messaging** — New `messaging` MCP tool for agent-to-agent coordination. Agents register with their `$TUIC_SESSION` identity, discover peers via `list_peers`, and exchange messages via `send`/`inbox`. Dual delivery: real-time push via MCP channel notifications (SSE) when `--dangerously-load-development-channels` is active, plus polling fallback via inbox. Spawned Claude Code agents automatically get the channels flag. TUICommander acts as the messaging hub — no external daemon needed
- **Multi-instance socket coexistence** — Multiple TUICommander instances (e.g. release + dev build) now coexist safely. First instance binds `mcp.sock`, subsequent instances fall back to `mcp-{pid}.sock`. Bridge auto-discovers live sockets with `TUIC_SOCKET` env override. Stale sockets cleaned on startup
- **Enriched health endpoint** — `/health` now returns `uptime_secs`, `session_count`, and `socket_path` for monitoring
- **Session close reasons** — `session-closed` events include a `reason` field (`process_exit`, `explicit_close`) for debugging session lifecycle

### Changed
- **Terminal scroll tracking** — Consolidated 20 iteratively-patched scroll fixes into a self-contained `ScrollTracker` class with 26 unit tests. Replaces inline `trackedScrollState`, `lastKnownVisible`, and `updateTrackedScroll` with a testable state machine that handles visibility inference, alternate buffer guards, and re-entrancy suppression

### Fixed
- **Terminal scroll lock** — New write-based `ViewportLock` keeps the viewport anchored when user scrolls up to read. Programmatic scrolls (from agent output) are intercepted during `terminal.write()` and restored via xterm's `scrollToLine()` API. Zero overhead when at bottom
- **Session tab visibility** — MCP-created sessions now match to repos using ancestor path matching (subdirectory of repo root or worktree), fixing a race condition with branch stats loading
- **Question detection** — Removed `q.starts_with(t)` prefix match that could produce false positive ghost notifications on short screen rows
- **PTY creation consolidation** — Shell PTY creation in MCP transport now delegates to `spawn_pty_session`, fixing a missing `last_output_ms` insertion for REST-created sessions

## [0.9.5] - 2026-03-23

### Added
- **GitHub OAuth Login** — New "GitHub" tab in Settings with one-click Device Flow authentication. Stores token securely in OS keyring (macOS Keychain, Windows Credential Manager, Linux Secret Service). Eliminates manual PAT management and missing-scope issues. Token resolution priority: env vars → OAuth keyring → gh CLI
- **Branch Panel** — New Branches tab (4th tab) in the Git Panel with full branch management: checkout (with dirty-worktree stash/force/cancel dialog), create, delete (safe + force), rename, merge, rebase, push (auto-sets upstream), pull, fetch, inline search, context menu, stale dimming (>30 days), merged badge, ahead/behind counts, prefix folding, and recent branches from reflog. `Cmd+G` opens the Git Panel directly on the Branches tab; clicking the sidebar "GIT" vertical label also lands on Branches
- **Worktree Agent Bridge** — MCP `worktree action=create` now returns a `cc_agent_hint` field for Claude Code clients, guiding CC to spawn a subagent that works in the worktree using absolute paths. Works around CC's inability to change working directory mid-session
- **Auto-retry on API errors** — Terminal sessions automatically retry when the AI provider returns server errors (5xx, rate limits). Configurable per-agent in Settings
- **Plans Panel** — Scans `plans/` directory to populate the PlanPanel with project plans
- **HTML Preview** — New panel for previewing HTML files with "Open in Browser" action
- **Cmd+Q confirmation** — Shows a confirmation dialog when quitting with active terminal sessions

### Fixed
- **Terminal scrollbar jank** — Eliminated a redundant native scrollbar on the xterm viewport that was updating out of sync with xterm v6's custom scrollbar widget, causing a visible thumb-resize flash on each write
- **Terminal scroll stability** — Seven distinct root causes for viewport-jump-to-line-0 identified and fixed: escape-sequence jumps, buffer contraction drift, baseY staleness on idle sessions, alternate buffer corruption, hidden terminal viewportY drift, hidden→visible transition guards, and WebGL atlas rebuild timing
- **Suggest overlay** — Added close button (X) and anchored the suggest token regex to start-of-line to prevent false matches
- **File path linking** — Terminal file paths followed by sentence punctuation (`.`, `,`, `)`) are now correctly clickable
- **GitHub settings** — "Connect to GitHub" button now appears after disconnect or fetch failure
- **Config test initializer** — Added missing `auto_retry_on_error` field

### Removed
- **Lazygit integration** — Replaced by the native Branch Panel. `Cmd+G` is reassigned to the Branches tab

### Security
- Updated `tar` crate 0.4.44 → 0.4.45 to fix RUSTSEC-2026-0067/0068

## [0.9.4] - 2026-03-19

### Added
- **Cross-repo knowledge base** — New `knowledge` MCP tool powered by mdkb. Actions: `setup` (auto-provisions mdkb upstream per repo), `search` (hybrid BM25+semantic fan-out across repo groups), `code_graph` (cross-repo call graph queries), `status` (indexing status). Provisioned upstreams persist in `mcp-upstreams.json`
- **Stdio upstream `cwd` field** — MCP upstream servers using stdio transport can now specify a working directory. Required for mdkb which uses cwd as project root
- **Boot-time upstream auto-connect** — Saved MCP upstream servers in `mcp-upstreams.json` now connect automatically on app launch (previously required UI interaction)
- **CI Auto-Heal** — When CI checks fail on a branch with auto-heal enabled and an active agent terminal, TUICommander fetches the failure logs via `gh run view --log-failed`, waits for the agent to be idle, and injects the logs with a fix prompt. Up to 3 attempts per cycle. Toggle per-branch in the PR detail popover
- **PWA WebSocket auto-reconnect** — When the browser closes the WebSocket (e.g. mobile backgrounding), the terminal now auto-reconnects with exponential backoff (1s→30s, up to 10 attempts). A pulsing "Reconnecting" banner shows progress. PTY sessions survive on the server — no data loss
- **Chunked backlog streaming** — Initial PTY catch-up on WebSocket connect is now sent in 64KB chunks instead of one giant frame. Supports `?offset=N` for delta-only catch-up on reconnect, skipping already-received data

### Fixed
- **MCP stale session auto-recovery** — When a `tools/call` or SSE request arrives with a session ID the server no longer recognizes (e.g. after app restart), the session is re-registered automatically instead of returning a `-32600` error. Only requests missing the header entirely are rejected

### Changed
- **MCP tool rationalization** — Removed `git` tool (thin CLI wrappers CC does natively). Replaced with `github` tool (`prs` for batched PR+CI, `status` for cross-repo aggregate) and `worktree` tool (`list`, `create` with optional `spawn_session`, `remove`). Tool count 7→8, action count 24→21
- **Session output now includes exit status** — `session action=output` returns `exited` (bool) and `exit_code` (number|null) so agents know when a teammate has finished
- **Workspace list includes ahead/behind** — `workspace action=list` now returns `ahead`/`behind` counts for repos with remotes, eliminating follow-up calls

## [0.9.3] - 2026-03-18

### Added
- **Dictation instant mode** — Long-press threshold slider now starts at 0 (was 200ms). When set to 0, any keypress activates dictation immediately without short-press pass-through. UI shows "Instant" label

### Fixed
- **Terminal scroll jump on long sessions** — After long idle periods, the terminal viewport would jump to line 0 on any resize event. Root cause: `trackedScrollState.baseY` drifted from reality because xterm's `onScroll` doesn't fire when `baseY` grows while the user is scrolled up. Now updated on every write callback
- **Worktree orphan cleanup** — When a linked worktree was deleted externally, its terminals remained live in the store, preventing the stale branch from being removed from the sidebar. Now closes orphaned terminals automatically
- **MCP server instructions for agent teams** — Restored server identity ("terminal session orchestrator") and explicit `session action=create` / `agent action=spawn` workflow steps that were removed in the v0.9.2 slim-down. Added Claude Code-specific hint for teammate PTY creation (conditional on clientInfo)
- **Post-merge cleanup branch switch** — `switch_branch` invoke used wrong parameter name (`branch` instead of `branchName`), causing the post-merge cleanup step to fail silently
- **Tauri invoke parameter mismatches** — Fixed 5 broken `invoke()` calls: `close_pty` used `id` instead of `sessionId` (RepoSection, PrDetailPopover), `write_pty` used `id` instead of `sessionId` (pluginRegistry), `write_plugin_data` and `read_plugin_data` used `plugin_id` instead of `pluginId`
- **Close PTY error resilience** — Terminal close loops in worktree cleanup, PrDetailPopover, and RepoSection now catch errors from already-dead PTY sessions instead of aborting the entire cleanup
- **Bridge version not bumped** — `make bump` now includes `src-tauri/crates/tuic-bridge/Cargo.toml`

## [0.9.2] - 2026-03-18

### Added
- **Dictation long-press hotkey** — Replaced tauri-plugin-global-shortcut with tauri-plugin-user-input for push-to-talk activation. Now supports 140+ keys (vs the limited set before) and long-press detection: short press passes through as normal input, holding the key beyond a configurable threshold (default 400ms) starts dictation. Key repeat is automatically filtered. Threshold is adjustable in Settings > Dictation (200–1000ms)
- **Cmd+F search in diff panels** — DiffTab now supports `Cmd+F` text search via SearchBar + DomSearchEngine, matching the markdown viewer search experience
- **Copy Path in viewer tab context menus** — Right-click on diff, markdown (file type), and editor tabs to copy the file path to clipboard
- **Click-to-diff in Git Panel** — Changes tab: clicking a file row opens its diff directly. Log tab: clicking a file in an expanded commit opens its diff at that commit hash

### Changed
- **Slimmer MCP server instructions** — Removed redundant tool table from MCP instructions (tool schemas already describe actions), switched from markdown tables to compact lists

### Fixed
- **File operations in worktrees** — Markdown viewer, file browser, code editor, and git panels now correctly resolve file paths against the worktree directory instead of the main repo root. Previously, opening a markdown file or browsing files while on a linked worktree branch would fail with "file not found" or show files from the wrong branch
- **MCP bridge socket path** — tuic-bridge was looking for the Unix socket in `tuicommander/` instead of `com.tuic.commander/`, preventing MCP connections from Claude Code and other agents
- **Text selection in diff panels** — DiffTab and PrDiffTab now allow text highlighting and copying via `user-select: text`
- **Submodule entries in working tree status** — Submodules no longer appear as regular files in the Changes tab
- **SearchBar placeholder encoding** — Fixed literal `\u2026` showing instead of ellipsis character in "Find…" placeholder
- **SearchBar counter text wrapping** — "No results" text no longer wraps to a second line
- **PR badge click on non-active repo** — Clicking a PR status badge (e.g. "Conflicts") on a branch belonging to a non-active repo now correctly opens the PR detail popover instead of silently doing nothing
- **PWA input duplication with agents** — Live PTY sync sent Ctrl-U bundled with text in a single PTY write. Cooked-mode shells (bash/zsh) handled this correctly, but raw-mode apps (Claude Code/Ink, Aider) don't process Ctrl-U when bundled with text in the same read — causing progressive input duplication. Live sync is now disabled for detected agent sessions

## [0.9.1] - 2026-03-16

### Added
- **File browser content search** (`Cmd+Shift+F`) — full-text search across file contents with case-sensitive, regex, and whole-word options. Results stream progressively and are grouped by file. Click any result to open the file at the matched line. Binary files and files >1 MB are automatically skipped
- **Color picker for group colors** — Visual color picker dialog with 8 preset swatches, native browser color input, and clear button. Shared `ColorSwatchPicker` component used in sidebar and settings tabs ([#9](https://github.com/sstraus/tuicommander/pull/9), thanks @antoniovizuete)

### Fixed
- **File drag & drop** — Drag & drop now works correctly in Tauri. Replaced broken HTML5 `File.path` (undefined in Tauri webviews — Electron-only API) with `getCurrentWebview().onDragDropEvent()` which provides real absolute paths. When a terminal has an active PTY session, dropped files are forwarded as paths to the terminal (enabling Claude Code image drops). Otherwise, `.md`/`.mdx` files open in the Markdown viewer and all others in the Code Editor. A global `dragover`/`drop` `preventDefault` prevents the browser-navigation white screen when dropping onto non-terminal panels
- **Terminal scroll jump to top** — Resizing the terminal while scrolled up with pending output data no longer jumps to line 0. Root cause: `doFit()` mixed a fresh `buf.baseY` (inflated by incoming writes) with a stale `trackedScrollState.viewportY`, producing an inflated `linesFromBottom` that went negative when `fitAddon.fit()` shrank `newBase`. Fixed by using `trackedScrollState.baseY` for both sides of the subtraction
- **MCP Unix socket robustness** — A stale socket file from a crashed previous run no longer blocks MCP tool loading. `SocketGuard` RAII struct removes the socket on `Drop` (crash-safe cleanup). Bind retries up to 3 times (×100 ms) removing any stale file before each attempt. `get_mcp_status` liveness check upgraded from `socket_path().exists()` to a real `UnixStream::connect()` probe — preventing the bridge from returning `tools: []` against a dead socket
- **File browser: content search mode toggle icon** — The `C`/`F` mode toggle button in the file browser search bar was invisible due to a missing CSS size rule on the SVG. Fixed with explicit `width: 14px; height: 14px` on `.modeToggle svg`
- **`.tuic.json` scripts exclusion (security)** — Scripts (`setup`, `run`, `archive`) are never merged from the repo-local `.tuic.json` file. Only worktree and workflow settings are team-overridable; scripts must be configured locally per-developer to prevent arbitrary code execution via a checked-in config file
- **Windows: CMD window flash** — Background process spawns (git, agent detection, plugin execution, `where` lookups) no longer flash visible console windows on Windows. Applied `CREATE_NO_WINDOW` flag to all background `Command::new` callsites. Interactive spawns (IDE/terminal launches) unaffected. MCP stdio server stderr now forwarded to tracing on Windows instead of being silently dropped ([#7](https://github.com/sstraus/tuicommander/issues/7))
- **CI: Windows clippy** — Sidecar stub now creates `.exe` variant on Windows, fixing Tauri build.rs resource resolution
- **Tests: 34 broken test expectations** aligned with current implementation (mock mismatches, timing, security-excluded `.tuic.json` scripts)

## [0.9.0] - 2026-03-14

### Added
- **Git Panel** — Tabbed side panel (`Cmd+Shift+D`) replacing the Git Operations Panel floating overlay and standalone Diff Panel. Three tabs: Changes (staging/unstaging, commit with amend, discard, glob filter, per-file diff counts), Log (virtual scroll + Canvas commit graph with lane assignment and Bezier connections), Stashes (apply/pop/drop). History and Blame are collapsible sub-panels within Changes (not separate tabs). Keyboard navigation: Escape to close, Ctrl/Cmd+1–3 to switch tabs
- **Canvas-based commit graph** — Visual commit graph in the Log tab rendered on Canvas with lane assignment, 8-color palette, ref badges, and Bezier curve connections between parent/child commits
- **Ideas panel image paste** — `Ctrl+V` / `Cmd+V` pastes clipboard images into notes. Images saved to disk, displayed as thumbnails, and sent as absolute paths when forwarding to terminal (so AI agents can read them). Supports PNG, JPEG, WebP, GIF up to 10 MB. Image-only notes (no text) are allowed. Cleanup on delete
- **Ideas panel in-place edit** — Edit now preserves note identity (no ID change). `Escape` cancels edit mode
- **Archive script** — Per-repo lifecycle hook that runs before a worktree is archived or deleted; non-zero exit blocks the operation. Configurable via Settings → Repository → Scripts or `.tuic.json`
- **Repo-local config (`.tuic.json`)** — Team-shareable configuration file in the repository root. Three-tier precedence: `.tuic.json` > per-repo app settings > global defaults. Covers base branch, scripts, worktree storage, merge strategy, and more
- **PR Review button** — Review button in the PR Detail Popover spawns a terminal running the agent's "review" run config with interpolated PR variables (`{pr_number}`, `{branch}`, `{base_branch}`, `{repo}`, `{pr_url}`). Shown only when an active agent has a run config named "review"

### Changed
- **DiffPanel removed** — Standalone Diff Panel replaced by the Git Panel's Changes tab. `Cmd+Shift+D` now opens the Git Panel
- **Git Operations Panel removed** — Floating overlay replaced entirely by the docked Git Panel
- **Git Panel: History/Blame as sub-panels** — History and Blame moved from separate tabs to collapsible sub-panels within the Changes tab, reducing tab count from 5 to 3
- **Updater: beta channel removed** — Only stable and nightly update channels remain
- **Resume banner UX** — Now accepts Space or Enter to resume agent session; other keys dismiss the banner
- **Shell state derivation** — Moved shellState (busy/idle) from frontend timer-based derivation to Rust-authoritative AtomicU8 CAS transitions. Frontend syncs on remount via `get_shell_state` Tauri command
- **tuic-bridge standalone crate** — Extracted from main binary into an independent workspace crate for cleaner builds

### Fixed
- **Shell state false oscillation** — Mode-line ticks no longer cause false busy/idle transitions; question notifications fire correctly; completion sound suppressed when terminal is awaiting input; no blue tab flash on resize when idle
- **Split panes visible behind overlay** — Split panes now hide when an overlay tab (Git Panel, Settings, etc.) is active
- **Scroll position lost on fit** — Terminal scroll position is always restored after `fitAddon.fit()`
- **Repo watchers not started at runtime** — HEAD and repo watchers now start when adding a repository at runtime (not only on app launch)
- **Commit graph scope** — Graph follows HEAD only, matching the commit log scope
- **Plugin CORS** — `plugin://` protocol responses now include CORS headers for cross-origin access
- **Merged branch false positives** — Branches at the same SHA as main are excluded from the merged list
- **Intent body double space** — Intent body text trimmed after SGR strip to prevent leading/trailing whitespace
- **Plan-file notification spam** — Info sound no longer fires repeatedly from repeated plan-file detections
- **PTY output filtering** — Replaced `chrome_only` heuristic with per-row content check; replaced overly broad `)? ` filter with targeted code-try regex
- **Font inconsistency** — File list fonts harmonized to `--font-md` (13px) across all panels
- **Remote PR button overflow** — Button row layout and dismiss UX fixed in remote-only PR popover

### Performance
- **Branch select** — Three hot paths optimized in `handleBranchSelectInner`
- **File system** — `list_directory` and `search_files` made async; `search_files` rewritten with `ignore` crate for gitignore-aware walking; removed per-entry `canonicalize` overhead
- **Incremental compilation** — Enabled for release profile to speed up iterative builds
- **Git Panel IPC** — Suppresses fetch calls when panel is hidden

## [0.8.2] - 2026-03-11

### Added
- **TUIC_SESSION env var** — Every terminal tab gets a stable UUID injected as `TUIC_SESSION` in the shell. Use `claude --session-id $TUIC_SESSION` for tab-bound sessions that resume automatically on restart. Supported agents: Claude Code, Gemini CLI, Codex CLI
- **Git Operations Panel redesign** — Complete rewrite with 400px panel, rich status card (branch, ahead/behind, staged/changed/stash counts, last commit), background execution via `run_git_command`, inline feedback bar, searchable BranchCombobox, Create Branch form, rebase/cherry-pick in-progress UI, monochrome SVG icons, keyboard navigation (Escape to close, autofocus)
- **`get_git_panel_context` Tauri command** — Single IPC round-trip for all Git Operations Panel data (cached 5s TTL)
- **BranchCombobox shared component** — Searchable combobox with keyboard navigation for branch selection
- **File drag & drop** — Drag files from Finder/Explorer onto the terminal area to open them with the appropriate viewer (`.md`/`.mdx` → Markdown, others → Code Editor). Visual overlay during drag hover. Supports standalone files outside any repo
- **Markdown file association** — `.md`/`.mdx` files registered with TUICommander on macOS. Double-clicking a markdown file in Finder opens it directly in TUICommander
- **File browser: auto-refresh** — Directory watcher (notify crate) detects external file changes and refreshes automatically within ~1s, preserving selection by path
- **File browser: sort dropdown + UI polish** — Sort toggle replaced with compact inline dropdown (funnel icon) next to breadcrumb path. Parent row ("..") is now smaller and subtler
- **Claude Usage: rate-limit headers fallback** — Falls back to unified rate-limit response headers when per-model header data is unavailable, improving accuracy of the usage dashboard

### Changed
- **Updater: beta/nightly check moved to Rust** — `check_update_channel` replaces `fetch_update_manifest`. URLs hardcoded in Rust (SSRF-safe), 15s timeout, 64 KB size cap, typed results. TS store is now a pure state consumer with no URL constants or error regex
- **Post-merge cleanup: auto-stash** — Switch step now auto-stashes uncommitted changes instead of blocking. Dialog shows inline warning with optional "Unstash after switch" checkbox
- **Mobile Activity Feed: throttled grouping** — Items snapshot every 10s to prevent constant reordering with multiple active sessions
- **Claude Usage dashboard adaptive layout** — Dashboard adapts gracefully when only partial usage data is available, preventing empty columns

### Fixed
- **Ghost question notifications on PWA** — Question state now auto-clears when agent resumes work (status-line event)
- **Mobile table rendering** — Box-drawing characters preserve alignment via horizontal scroll (`white-space: pre`)
- **Mobile emoji rendering** — Unicode symbols (●, ○, ◉) forced to text presentation via `font-variant-emoji: text`
- **Updater CSP bypass** — Beta/nightly update manifest now fetched via Rust backend instead of the webview, bypassing CSP restrictions that blocked update checks. Missing channel releases shown as info, not error
- **Question detection reliability** — Stale pending state cleared so re-asked questions can refire; repaint-triggered false re-fires suppressed; screen-based detection via `last_chat_line` for accuracy; generalized prompt style recognition for Codex and Gemini
- **Rate-limit false positives** — Rate-limit events now gated on agent presence; terminals with no active agent no longer show spurious rate-limit badges
- **Rate-limit auto-expire** — Stale `rate_limited` state automatically clears after `retry_after_ms` elapses, preventing the badge from persisting beyond the actual limit window
- **Notification sound deduplication** — Plan-file info sound no longer fires multiple times per event; notification sounds decoupled from parsed event handlers to prevent double-firing
- **Sidebar auto-close** — Sidebar's focus-loss auto-close no longer dismisses the post-merge cleanup dialog mid-workflow

## [0.7.1] - 2026-03-08

### Changed
- **Notification sounds moved to Rust** — Audio playback moved from JS Web Audio API to Rust `rodio` crate. Eliminates AudioContext suspend issues on WebKit and works reliably in both Tauri and headless/remote modes
- **Transport table-driven mapping** — `mapCommandToHttp` refactored from 370-line switch to declarative `COMMAND_TABLE` for easier maintenance
- **Agent Teams simplified** — it2 shim infrastructure commented out; Agent Teams now uses env-var-only approach (`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`) with direct MCP tool spawning
- **Mobile TerminalKeybar consolidated** — QuickActions merged into context-aware TerminalKeybar with agent-specific Yes/No buttons and Enter key for Ink TUI navigation
- **Intent token colorization** — Embedded ANSI codes from Ink renderers are now stripped from intent body text so dim-yellow color is uniform
- **Question detection simplified** — Removed challenged threshold; all silence-based questions use a single 10s timeout

### Fixed
- **Intent tokens visible in PWA** — `[[intent:...]]` and `[[suggest:...]]` structural tokens are now stripped from log lines served to PWA/REST consumers
- **PTY echo false question detection** — User-typed input echoed by PTY no longer triggers the silence-based question detector (500ms suppression window)
- **Headless reader question detection** — `extract_question_line` now applies to HTTP-created sessions, not just Tauri-spawned ones
- **Mobile input echo** — CommandInput sends `Ctrl-U` + text + Enter atomically to prevent duplicate echo
- **PluginManifest field naming** — TypeScript PluginManifest fields aligned to Rust serde camelCase serialization (`minAppVersion`, not `min_app_version`)
- **Notification subtask detection** — Parser now recognizes `⏵⏵` (U+23F5) prefix in addition to `››` (U+203A) for Claude Code active subtask counting; restored 10s notification deferral
- **Agent session lifecycle events** — MCP-spawned sessions now emit `session-created` and `session-closed` events so they appear as tabs and clean up correctly

## [0.7.0] - 2026-03-06

### Added
- **Markdown search** — `Cmd+F` now works in the markdown viewer with DOM-based text search, cross-element matching, highlight navigation, case/regex/whole-word toggles, and shared SearchBar component (also used by terminal search)
- **VT100 log extraction for mobile** — New `VtLogBuffer` per session uses a full VT100 parser to extract clean log lines from PTY output. Alternate-screen TUI apps (vim, htop, Claude Code) are suppressed — no garbled screen renders in mobile output. Accessible via `GET /sessions/:id/output?format=log` (returns `{lines, total_lines}`) and `WS /sessions/:id/stream?format=log` (catch-up on connect, then 200ms polling frames as `{type:log,lines:[...],offset:N}`). Mobile `OutputView` component now uses `format=log` for both initial fetch and live streaming
- **Session-aware agent resume** — When an agent is detected running in a terminal, TUICommander automatically discovers its session UUID from the filesystem and persists it per-terminal. On restore, uses the agent-specific `--resume <uuid>` for exact session matching. Supported: Claude Code (`~/.claude/projects/`), Gemini CLI (`~/.gemini/tmp/`), Codex CLI (`~/.codex/sessions/`). Multiple concurrent agents are handled via deduplication. Non-discoverable agents (Aider, Amp, etc.) fall back to their static resume commands. Context-menu "Launch Agent" now auto-executes the launch command without requiring a banner click
- **MCP bridge as Tauri sidecar** — `tuic-bridge` ships with the app and auto-configures MCP on first launch for Claude Code, Cursor, Windsurf, VS Code, Zed, Amp, Gemini
- **MCP `tools/list_changed` SSE notification** — Connected MCP clients receive live tool-list updates when upstream tool lists change
- **MCP Proxy Hub** — TUICommander now aggregates upstream MCP servers and exposes them through its own `/mcp` endpoint. Configure HTTP and stdio upstream servers in Settings > Services > MCP Upstreams; their tools are automatically available to any MCP client (Claude Code, Cursor, VS Code) connecting to TUIC. Features: tool namespace prefixing (`{upstream}__{tool}`), per-upstream tool allow/deny filters, circuit breaker (3 failures → open, 1s–60s exponential backoff, 10 retries → permanent failure), 60-second health checks, hot-reload on config save, credential storage via OS keyring, environment sanitization for stdio children, SSE status events, and self-referential URL detection
- **Worktree Manager panel** — Dedicated overlay (`Cmd+Shift+W` or Command Palette) listing all worktrees across repos with branch name, repo badge, PR state, dirty stats, and last commit timestamp. Features: orphan detection with Prune action, repo filter pills + text search, multi-select with batch delete and batch merge & archive, single-row actions (Open Terminal, Delete, Merge & Archive). Main worktrees have destructive actions disabled
- **Terminal CWD tracking via OSC 7** — Terminals detect working directory changes via OSC 7 escape sequences. When a terminal cd's into a known worktree, the tab automatically reassigns to that worktree's branch. Supports restart recovery via Rust-side cwd persistence.
- **Remote PTY session tab styling** — Sessions created via HTTP/MCP now display with amber tab color and "PTY:" name prefix for instant visual distinction from local terminals
- **Multi-agent status line detection** — Output parser now recognizes status lines from Claude Code (✢/·/asterisk), Aider (Knight Rider scanner + token reports), Codex CLI (bullet spinner), GitHub Copilot CLI (∴/●/○ indicators), Gemini CLI, Amazon Q, and Cline (braille dots). Tab titles update correctly for all supported agents
- **MCP workspace tool** — New `workspace` MCP tool with `list` (all open repos with groups, worktrees, branch, dirty status) and `active` (currently focused repo) actions
- **MCP notify tool** — New `notify` MCP tool with `toast` (temporary notification with info/warn/error level) and `confirm` (blocking confirmation dialog, localhost-only) actions
- **Plugin context menu actions** — Plugins can register custom actions in the terminal right-click "Actions" submenu via `host.registerTerminalAction()`. Actions receive a context snapshot (sessionId, repoPath) captured at right-click time, support dynamic `disabled` callbacks, and auto-cleanup on plugin unload. Requires new `ui:context-menu` capability
- **Plan Panel** (`Cmd+Shift+P`) — New right-side panel showing plan files for the active repository. Plans are detected from agent output via structured events, filtered by active repo, and auto-open as background tabs on first detection. Frontmatter is stripped from rendered content. Panel visibility and width persist across restarts
- **Agent Teams it2 shim** — Bash shim at `~/.tuicommander/bin/it2` emulates iTerm2 CLI for Claude Code Agent Teams. Supports `session split`, `run`, `close`, and `list`. PTY env injection sets `ITERM_SESSION_ID`, `TERM_PROGRAM`, and prepends shim to `PATH`. Enable via Settings > General > Agent Teams
- **Suggest follow-up actions** — Agents can propose follow-up actions via `[[suggest: ...]]` tokens. Desktop shows a floating chip bar (SuggestOverlay) with 30s auto-dismiss; mobile shows horizontal scrollable pills above CommandInput. Configurable in Settings > Agents
- **Mobile companion UI redesign** — Hero metrics header with active/awaiting counts, elevated session cards with rich sub-rows (intent, task, progress, usage), error and rate-limit info bars with live countdown, suggest follow-up chips, frosted glass bottom tabs, connection status in settings, 16px input font to prevent iOS auto-zoom
- **Quick Branch Switch** (`Cmd+B`) — Fuzzy-search dialog to switch branches instantly. Shows all local and remote branches for the active repo with current/remote/main badges. Remote branches auto-checkout as local tracking branches
- **Move terminal to worktree** — Right-click a terminal tab → "Move to Worktree" submenu to move the terminal to a different worktree. Also available via Command Palette with dynamic "Move to worktree: <branch>" entries
- **Customizable keybindings** — Click the pencil icon next to any shortcut in Help > Keyboard Shortcuts to rebind it. Conflict detection, per-shortcut reset, and "Reset all to defaults" button. Overrides persist in `keybindings.json`
- **Tip of the Day improvements** — Expanded from 18 to 31 tips covering all discoverable features. Larger fonts, brighter colors, sliding dot window (max 7 visible). Fixed click-through bug on arrows and dots
- **Post-merge cleanup dialog** — After merging a PR from the popover, a stepper dialog offers checkable steps: switch to base branch (with dirty state detection), pull (ff-only), delete local branch (closes terminals first), delete remote branch (handles "already deleted" gracefully). Steps execute sequentially via Rust backend (not PTY). Available from both local PR popover and remote-only PR popover. Also replaces the old MergePostActionDialog for worktree cleanup — when `afterMerge=ask`, the same unified dialog includes an archive/delete worktree step with an inline selector
- **Unseen terminal status dot** — Purple dot on terminals that completed work while the user was viewing a different terminal. Clears when the terminal is selected. Branch/worktree icons in the sidebar also show purple when containing unseen terminals
- **PR diff panel tab** — View Diff button in PR popover opens a dedicated panel tab with collapsible file sections, dual line numbers, and color-coded additions/deletions
- **Dismiss/Show Dismissed for remote-only PRs** — Hide irrelevant remote PRs from the sidebar; "Show Dismissed" toggle brings them back
- **Approve button for remote-only PRs** — Submit an approving review via GitHub API directly from the PR popover
- **Slash menu detection** — Output parser detects `/command` menus from screen bottom rows; mobile PWA renders a native bottom-sheet overlay for selection
- **GitHub merge method auto-detection** — Merge method selected from repo's allowed methods via GitHub API; auto-fallback to squash on HTTP 405 rejection
- **Mobile PWA enhancements** — TerminalKeybar (Ctrl+C/D/Tab/Esc/arrows), CLI command widget (agent-specific quick commands), offline retry queue for write_pty, session kill/new, search/filter in output, semantic log line colorization, slash menu overlay, connectivity indicator, isolated CSS (`mobile.css`), WebSocket state deduplication

### Changed
- **Progressive worktree loading** — `refreshAllBranchStats` now uses two-phase progressive loading. Phase 1 (`get_repo_structure`) returns worktree paths and merged branches instantly, so WorktreeManager rows appear immediately. Phase 2 (`get_repo_diff_stats`) fills in diff stats and timestamps progressively. Auto-archive of merged worktrees runs after Phase 1 instead of waiting for all stats
- **MCP cross-platform IPC transport** — MCP server uses Unix domain socket on macOS/Linux and named pipe (`\\.\pipe\tuicommander-mcp`) on Windows. `tuic-bridge` sidecar now works on all platforms. Bridge path is verified and updated on every app launch (not just first install)
- **MCP bridge path auto-update** — `ensure_mcp_configs()` runs on every launch, detects stale bridge paths in agent configs (from reinstalls, updates, or moves) and updates them automatically
- **MCP session output ANSI stripping** — MCP session output now strips ANSI codes by default (pass `format=raw` to preserve)

### Fixed
- **False question notification on user-typed input** — User-submitted lines echoed by PTY no longer trigger question detector
- **Voice dictation TOCTOU race on rapid start** — `compare_exchange` prevents duplicate recording sessions
- **Voice dictation final transcription accuracy** — Final transcription uses full captured audio instead of tail-only for improved accuracy
- **DictationToast lifecycle** — Removed duplicate event subscription causing stale toast state
- **macOS TCC permission prompts** — App was triggering "would like to access Desktop/Documents" dialogs due to filesystem probing in Claude Usage slug resolver, terminal path canonicalization, and file dialogs without defaultPath. All four code paths now guard against TCC-protected directories
- **Tab/sidebar animations not playing** — `pulse-opacity` keyframes defined in `global.css` were silently ignored by CSS Modules (scoped name mismatch). Moved keyframes into each module file; activity dots, busy indicators, and awaiting-input pulses now animate correctly
- **Rate-limit false positives** — Rate-limit pattern matches are now suppressed when the terminal is actively producing output (busy state), eliminating noise from agents reading code that contains rate-limit strings
- **Prompt Library focus loss** — Terminal now regains focus after prompt injection from the Prompt Library drawer
- **False positive API error** — Removed overly generic "request failed unexpectedly" from copilot-auth-error pattern, was triggering on normal Claude Code output

## [0.6.0] - 2026-02-28

### Added
- **Plugin filesystem write/rename** — New `fs:write` and `fs:rename` capabilities allow plugins to write and rename files within `$HOME` with path-traversal validation
- **Plugin panel message bridge** — `openPanel()` accepts `onMessage` callback for structured iframe→host messaging; `PanelHandle.send()` delivers host→iframe messages. Replaces fragile global `window.addEventListener("message")` pattern
- **Plugin panel CSS theme injection** — CSS custom properties (`--bg-*`, `--fg-*`, `--border*`, etc.) are automatically injected into plugin panel iframes, so plugins inherit the app theme without manual color copying
- **Auto-delete branch on PR close** — Per-repo setting (off/ask/auto) to automatically delete local branches when their GitHub PR is merged or closed. Handles worktree cleanup, dirty-state escalation, and main-branch protection
- **Worktree system overhaul** — Configurable storage strategies (sibling, app dir, inside-repo), three creation flows (dialog with base ref, instant, right-click quick-clone), hybrid branch naming (`{source}--{random}`), merge & archive workflow, external worktree detection via `.git/worktrees/` monitoring, per-repo worktree settings with global defaults
- **Centralized error log panel** — Ring-buffer logger captures all errors, warnings, and info from app, plugins, git, network, and terminal subsystems. Filterable overlay panel with level tabs, source dropdown, and text search. Status bar badge shows unseen error count. Keyboard shortcut: `Cmd+Shift+E` ([solution doc](docs/solutions/integration-issues/centralized-error-logging.md))
- **Plugin log forwarding** — Plugin `host.log()` calls now appear in the centralized error log panel alongside app-wide logs
- **Agent-scoped plugins** — `agentTypes` manifest field restricts plugin output watchers and structured event handlers to terminals running specific agents (e.g. `["claude"]`). Universal plugins (empty array) continue to receive all events
- **File browser → Markdown viewer routing** — `.md`/`.mdx` files opened from the file browser now open in the Markdown panel instead of the code editor
- **Plugin CLI execution** — `exec:cli` capability allows plugins to run whitelisted CLI binaries (sandboxed: allowlist, timeout, stdout limit, trusted-directory validation)
- **Session prompt tracking** — Built-in `sessionPromptPlugin` reconstructs user-typed input from PTY keystrokes and displays in Activity Center
- **Input line buffer** — Rust-side virtual line editor (`input_line_buffer.rs`) reconstructs typed input from raw PTY keystroke data, supporting cursor movement, word operations, and Kitty protocol sequences
- **mdkb Dashboard plugin** — External installable plugin for viewing mdkb knowledge base status, memories, and configuration
- **API error detection** — Output parser detects API errors (5xx, auth failures) from agents (Claude Code, Aider, Codex CLI, Gemini CLI, Copilot) and provider-level JSON error formats (OpenAI, Anthropic, Google, OpenRouter, MiniMax). Triggers error notification sound and logs to centralized error panel
- **Rust-backed log ring buffer** — Warn/error entries survive webview reloads via `push_log`/`get_logs` Tauri commands
- **Switch Branch submenu** — Main worktree context menu with dirty-tree stash prompt and running-process guard
- **Merged badge** — Branches merged into main show a "Merged" badge in the sidebar
- **Info notification sound type** — Added "info" to per-event notification sounds
- **Tab bar overflow menu** — Right-click scroll arrows to see clipped tabs; `+` button always stays visible
- **Focus-aware dictation** — Transcribed text inserts into focused input element instead of always targeting terminal PTY
- **Auto-fetch interval** — Per-repo setting to periodically `git fetch --all` in the background (5/15/30/60 min), keeping branch stats and ahead/behind counts fresh without manual intervention
- **LLM intent declaration** — Agents emit `[[intent: <action>]]` tokens that the output parser captures and displays in the Activity Dashboard, showing real-time work intent alongside user prompts
- **Mobile Companion UI** — Phone-optimized PWA at `/mobile` for monitoring AI agents remotely. Session list with status cards, live output with quick-reply chips, question overlay banner, activity feed, notification sounds. Installable via Add to Home Screen on iOS Safari and Android Chrome
- **Streaming dictation with VAD** — Real-time partial transcription during push-to-talk via adaptive sliding windows (1.5s→3s). Voice Activity Detection energy gate skips silence to prevent hallucinations. Floating toast shows partial text above status bar. No new dependencies — built entirely on whisper-rs

### Changed
- **`get_repo_summary` single-IPC** — New Rust command collapses worktree paths + merged branches + per-path diff stats into one round-trip, replacing N+2 separate IPC calls in `refreshAllBranchStats`
- **RPC deduplication** — Concurrent identical idempotent (GET) RPC calls are coalesced into a single in-flight request
- **StatusBar shared timer** — Merged two separate 1-second intervals (rate-limit countdown + PR grace period) into one
- **Terminal resize cleanup** — Removed redundant Tauri window resize listener (ResizeObserver already handles this)
- Agent session restore now shows a clickable banner instead of auto-injecting the resume command
- Migrated ~200 `console.error`/`console.warn` calls to centralized `appLogger` across terminal, hooks, stores, UI components, plugins, and utilities (waves 1-4)
- Activity Dashboard shows last user prompt (>= 10 words) as sub-row with tooltip, now native Rust implementation
- OSC 8 hyperlinks in terminal now open in system browser correctly

### Fixed
- Worktree removal now respects the `deleteBranchOnRemove` setting instead of always deleting the local branch
- File path link underline no longer flickers on mouse hover (cached link provider)
- Rate limit and usage limit badges no longer trigger redundantly on terminal resize
- Terminal focus no longer silently switches to a terminal from another repo
- Rapid branch switching no longer creates duplicate terminals (serialization lock)
- HEAD-changed events during branch rename no longer lose terminal state
- Push-to-talk race condition — fast key release no longer drops transcription
- Claude usage timeline gaps — flush orphan tokens from active sessions
- Merged branch detection hardened with file I/O probing and 5s TTL cache
- **Activity Dashboard state inconsistencies** — `setActive()` no longer resets `shellState` to null; busy flag reconciliation on every PTY chunk prevents "—" status for working terminals; agent polling now covers all terminals (not just the active one)
- **Rate-limit false positives** — Added `line_is_source_code()` guard so agents reading `output_parser.rs` no longer trigger their own rate-limit patterns
- **False "awaiting input" indicator** — Silence-based question detector threshold raised from 5s to 10s; added `line_is_likely_not_a_prompt()` guard to filter code, markdown, and long lines
- **Output parser false positives** — Status line detection now skips diff output, code listings, and block comments; intent parsing requires line-start/whitespace anchor; rate limit and API error detection uses ANSI-stripped text to prevent escape-code bridging (e.g. "story 429" no longer triggers HTTP 429 detection)

### Removed
- `showAllBranches` toggle (replaced by Switch Branch submenu)
- `sessionPromptPlugin` built-in (replaced by native Rust last-prompt tracking)

### Documentation
- FEATURES.md: documented tab pinning, branch sorting, Kitty keyboard protocol, PTY pause/resume, MCP registration with Claude CLI

### Security
- **Plugin exec binary resolution hardened** — Removed `which`/`where` PATH lookup; binary resolution now uses only hardcoded trusted directories with symlink canonicalization to prevent symlink attacks
- **Plugin exec stderr truncated** — Error messages from failed CLI commands now truncate stderr to 256 bytes to prevent leaking secrets

### Housekeeping
- **Removed dead wizStoriesPlugin built-in** — Extracted to external plugin; orphaned source and tests cleaned up
- **Replaced wiz-specific example plugins** — `wiz-stories` and `wiz-reviews` examples replaced with generic `report-watcher` and `claude-status` (demonstrates agentTypes)
- **Ideas audit** — Reclassified 4 ideas: PR Merge Readiness → done, Worktree Status Refresh → done (implemented via revision-based reactivity), Structured Agent Output → rejected (requires upstream adoption), Analytics/Editor Settings clarified (editors done, analytics deferred)
- **Plugins submodule updated** — registry.json and README cleaned up, mdkb-dashboard added

### Planned
- **Tab scoping per worktree** — Each worktree/branch will have its own isolated set of tabs instead of sharing a global tab list

### Infrastructure
- **Nightly workflow: move tip tag** — Cleanup job now force-moves the `tip` git tag to the current commit before building, so the release always points to HEAD
- **Makefile: unified CI targets** — Replace `build-github-release` / `publish-github-release` / old `github-release` with two clean targets: `make nightly` (push + tip tag) and `make github-release BUMP=patch` (version bump + tag + CI + publish)
- **Makefile: github-release fixes** — `cargo check` stderr no longer suppressed; run ID lookup matches by commit SHA to avoid race conditions

---

## [0.5.4] - 2026-02-24

### Terminal

- **Ghostty terminal identity** — Switch from kitty to ghostty for Claude Code's terminal detection allow-list (CC v2.1.52 compatibility)
- **Shift+Enter multi-line input** — Sends `\x1b\r` (ESC+CR) for multi-line newlines in Claude Code and other CLI apps
- **Shift+Tab focus fix** — Prevents browser focus navigation while letting xterm send CSI Z to PTY
- **Kitty flags initial sync** — Race condition fix: query kitty flags on listener attach to avoid missed push events
- **Tab close focus transfer** — Closing the active tab now properly focuses the next tab via `handleTerminalSelect` (includes `ref.focus()`)

### Infrastructure

- **Transport layer compliance** — `get_kitty_flags` routed through `usePty`/`transport.ts` with HTTP handler for browser mode
- **Linux CLI resolution** — Added `/usr/bin` to `extra_bin_dirs` for minimal desktop environments
- **Nested session guard** — `env_remove("CLAUDECODE")` prevents "cannot launch inside another CC session" error

### Fixed

- **Windows clippy errors** — Unused variables and collapsible ifs
- **rAF close-all guard** — Prevent crash when concurrent tab closes race with deferred focus callback

---

## [0.5.0] - Unreleased

### Plugin System

- **External plugin loading** — Plugins live in `~/.config/tui-commander/plugins/{id}/` and are loaded at runtime via the `plugin://` URI scheme; hot reload when files change on disk
- **Plugin Settings tab** — Install plugins from a ZIP file or URL, enable/disable, uninstall, view per-plugin logs
- **Community registry / Browse tab** — Discover and install plugins from `sstraus/tuicommander-plugins`; 1-hour TTL cache with manual refresh
- **`tuic://` deep link scheme** — `tuic://install-plugin?url=…`, `tuic://open-repo?path=…`, `tuic://settings?tab=…`
- **Per-plugin error logging** — 500-entry ring-buffer logger per plugin; errors from lifecycle hooks and watchers captured automatically
- **Capability-gated PluginHost API** — Tier 1 (activity/watchers), Tier 2 (read-only state), Tier 3 (PTY write, markdown panel, sound), Tier 4 (whitelisted Tauri invoke)
- **Built-in plugin toggle** — Plan and Stories plugins can be disabled from Settings → Plugins
- **Activity Center bell** — Toolbar bell replaces the plan button; plugins contribute sections and items; supports per-item dismiss and "Dismiss All"
- **4 sample plugins** in `examples/plugins/` demonstrating all capability tiers
- **Plugin filesystem API** — `fs:read`, `fs:list`, `fs:watch` capabilities for sandboxed file access within `$HOME` (10 MB limit, glob filtering, debounced watching via `notify`)
- **Plugin data HTTP endpoint** — `GET /api/plugins/{id}/data/{path}` exposes plugin data to external HTTP clients

### Terminal

- **Detachable terminal tabs** — Float any terminal tab into an independent OS window; re-attach on close
- **Find in Terminal** (`Cmd+F`) — In-terminal search overlay with match count and navigation
- **Configurable keybindings** — Remap any shortcut in Settings → Keyboard Shortcuts; persisted to `~/.config/tui-commander/keybindings.json`
- **iTerm2-style Option key split** — macOS: left Option sends Meta (for Emacs/readline), right Option sends special chars; configurable per repo
- **Per-repo terminal meta hotkeys** — Override Option key behavior per repository in Settings

### Settings Panel

- **Split-view layout** — Vertical nav sidebar + content pane replaces the old dialog
- **Repos in Settings nav** — Each repo appears as a nav item with deep-link open support
- **Keyboard Shortcuts tab** — Browse and rebind all app actions
- **About tab** — App version, links, acknowledgements
- **Appearance tab** — Absorbs former Groups tab; theme, color, font settings in one place
- **Global repo defaults** — Set base branch, color, and other defaults; per-repo settings override only what differs

### File Browser & Editor

- **File browser panel** (`Cmd+E`) — Tree view of the active repository with git status indicators, copy/cut/paste, context menu
- **CodeMirror 6 code editor** — Full editor panel with tab system, syntax highlighting, and file browser integration
- **Markdown edit button** — Pencil icon in MarkdownTab header opens the file in the code editor
- **Clickable file paths** — File references in diff and code panels open in the editor or focused in the IDE
- **Panel search** — Search within code and diff panels
- **Mutually exclusive panels** — File browser, Markdown, and Diff panels are now mutually exclusive to save screen space
- **Drag-resize** — Panel dividers are draggable

### Git & GitHub

- **Diff panel commit dropdown** — Select any recent commit to diff against; Working / Last Commit scope toggle
- **PR notification rich popover** — Click the bell to see PR title, CI status, review state, and open in browser
- **Plan file detection** — Toolbar button lights up when an agent creates a plan file in the active repo
- **GitHub API rate limit handling** — Graceful backoff and UI indicator when GitHub API rate limit is hit

### Agent Support

- **New agents** — Amp, Jules, Cursor, Warp, Ona; brand SVG logos for all supported agents
- **Silence-based question detection** — Recognizes interactive prompts for unrecognized agents via output silence heuristic
- **MCP tools consolidation** — 21 individual MCP tools replaced by 5 meta-commands

### Cross-Platform

- **Windows compatibility** — Platform-aware shell escaping (cmd.exe vs POSIX), foreground process detection via `CreateToolhelp32Snapshot`, Windows paths in `resolve_cli`, IDE detection/launch, `if exist` syntax for lazygit config detection

### Other Added

- **Command Palette** (`Cmd+P`) — Fuzzy search across all app actions with recent-first ordering
- **Activity Dashboard** (`Cmd+Shift+A`) — Real-time view of all terminal sessions and agent status
- **Park Repos** — Right-click any repo to park it; sidebar footer button shows parked repos with badge count
- **Repository groups context menu** — Right-click any repo to "Move to Group" with "New Group..." option
- **Lazy terminal restore** — Terminal sessions materialize only when clicking a branch, not on startup
- **Check for Updates menu** — In both app menu and Help menu
- **Repo watcher** — Shared file watcher for automatic panel refresh on `.git/` changes
- **Context menu submenus** — ContextMenu supports nested children
- **Remote access QR code** — Shows actual local IP address; HTTPS-only install links; firewall reachability check
- **Auto-hide closed/merged PRs** — PR notifications for closed or merged PRs are automatically dismissed

### Changed

- **Display name** — "TUI Commander" renamed to "TUICommander" across the codebase
- **UX density** — Tighter status bar (22px), toolbar (35px macOS), and sidebar row spacing to match VS Code density
- **Browser/remote mode** — Full compatibility with MCP session events, CORS for any origin, IPv4 binding
- **Status bar icons** — All text labels replaced with monochrome SVG icons; buttons reordered
- **HelpPanel** — Simplified to app info and resource links; keyboard shortcuts moved to Settings
- **Sidebar design** — Flat layout; harmonized git actions and footer; SVG branch/asterisk icons
- **Tab creation UX** — `+` button creates new tab; split options on right-click only
- **CLI resolution** — All `git` and `gh` invocations route through `resolve_cli()` for reliable PATH in release builds
- **Diff panel shortcut** — Remapped from `Cmd+D` to `Cmd+Shift+D`
- **Data persistence guard** — `save()` blocks until `hydrate()` completes to prevent wiping `repositories.json`

### Fixed

- **Lazygit pane ghost terminal** on close
- **xterm fit() minimum dimensions** — Guard prevents crash on zero-size terminal
- **Terminal reattach fit** after floating window closes
- **Splash screen timing** — Deferred removal until stores are fully hydrated
- **Markdown viewer refresh** — Viewer now refreshes after saving a file in the code editor
- **Window-state corruption** — Guard against zero-dimension or off-screen persisted state causing PTY garbage
- **IDE detection in release builds** — `resolve_cli` probes well-known directories
- **Multi-byte UTF-8 panic** — Fixed in rate-limit debug output
- **International keyboard support** — Correct handling of intl input; fewer rate-limit false positives
- **Tab drag-and-drop** — Fixed by working around Tauri's internal drag handler
- **Left Option key state leak** — Reset on `altKey=false` to prevent stuck Meta state
- **PromptDialog hidden on mount** — Dialog now shows correctly when first rendered
- **Browser-mode init freeze** — Fixed hang when session cookie expires
- **Silent failures and memory leak** — P1 issues resolved (floating promises, missing cleanup)
- **Drag-over visual feedback** — Group sections show drop indicator during drag
- **Tab store mutual exclusivity** — Fixed markdown wheel scroll by enforcing only one tab store active at a time
- **Browser mode PTY creation** — Fixed ConnectInfo extraction and keybinding conflicts in remote mode

---

## [0.3.0] - 2026-02-19

### Added
- **Auto-update** - Check for updates on startup via tauri-plugin-updater, download progress badge in status bar, one-click install and relaunch
- **Prevent system sleep** - keepawake integration prevents sleep while agents are working (configurable in Settings)
- **Usage limit badge** - Detects Claude Code "You've used X% of your weekly/session limit" messages and displays a color-coded badge in status bar (blue < 70%, yellow 70-89%, red pulsing >= 90%)
- **Ideas panel** - Renamed Notes to Ideas with lightbulb icon, send-to-terminal and delete actions
- **Terminal session persistence** - Terminal sessions survive app restarts, with activeRepoPath live-sync
- **GitHub GraphQL API** - Replaced `gh pr list` CLI with direct GraphQL for PR statuses, CI checks, and token resolution
- **HEAD file watcher** - Watches `.git/HEAD` for branch changes instead of polling
- **Build & release targets** - Makefile targets for `build-github-release` and `publish-github-release`

### Changed
- **Git status via file reads** - Read branch and remote URL from `.git` files instead of subprocess for better performance
- **Status bar overflow** - Handles long content gracefully
- **Color picker** - Added to settings for theme customization
- **Default theme** - Changed to VS Code Dark, reordered theme lists

### Fixed
- **Empty GitHub token** - Filter empty strings from `gh_token` crate, fall back to `gh auth token` CLI
- **Agent resume commands** - Updated resume commands for OpenCode and Aider
- **Download progress bar** - Fixed layout in Dictation Settings
- **PTY environment** - Set `TERM=xterm-256color`, `COLORTERM`, and `LANG` for proper color and UTF-8 support
- **Branch name overflow** - Text ellipsis on long branch names in sidebar
- **Branch name styling** - Font size and color consistency
- **Worktree button** - Disabled during creation to prevent double-clicks
- **CI builds** - Linux `libasound2-dev` dependency, macOS notarization, Windows process group guard

---

## [0.2.0] - 2026-02-18

### Added
- **Terminal context menu** - Split right/left/down/up, reset terminal, change title
- **PR state badges** - Replace CI ring with merge/review state badges in sidebar
- **PR clickable links** - PR number opens GitHub in browser
- **Rate limit warning** - Badge in status bar when AI agents hit rate limits
- **Question detection** - Recognizes interactive prompts in terminal, shows ? icon in sidebar
- **Dock badge count** - macOS dock badge for attention-requiring notifications
- **Auto-show PR popover** - Optional setting to auto-display PR details
- **Splash screen** - Branded loading screen on app start
- **Repo header context menu** - Right-click on repo header in sidebar
- **Smart branch terminal spawn** - Auto-spawns terminal only on first branch select; respects user intent when all tabs are closed

### Changed
- **Design system tokens** - Migrated all hardcoded CSS values to CSS custom properties
- **WCAG AA compliance** - Theme-aware text-on-color system, contrast fixes across all UI elements
- **Standardized sizing** - Consistent button, badge, and input dimensions
- **Sci-fi worktree names** - More creative auto-generated worktree names
- **OSC title cleaning** - Filters shell script noise and extracts useful command names
- **Lazygit tab naming** - Explicitly sets tab name to avoid polluted OSC titles
- **Tauri webview build targets** - Optimized build configuration

### Fixed
- **macOS "Restored session" message** - Suppressed by setting TERM_PROGRAM in PTY
- **PR popover layout** - Improved readability and positioning
- **Hotkey macOS symbols** - Correct modifier symbol translation
- **Sidebar badge layout** - Proper alignment and spacing
- **MCP HTTP port conflict** - Server falls back to localhost on port conflict
- **WebGL canvas fallback** - Graceful degradation when WebGL addon fails
- **ErrorBoundary crash screen** - Shows recovery UI instead of blank screen

---

## [0.1.0] - 2026-02-04

### Added
- Initial TUICommander implementation
- **Multi-terminal support** - Up to 50 concurrent PTY sessions
- **Repository sidebar** - Hierarchical view of repositories with branches and worktrees
- **Tab bar** - Terminal tabs with keyboard shortcuts (Cmd+1-9)
- **Git worktree integration** - Create and manage git worktrees from the UI
- **Per-pane zoom** - Independent font size control per terminal (Cmd+Plus/Minus)
- **VS Code / Claude Code launchers** - Quick access buttons in status bar
- **Markdown preview panel** - Toggle with MD button
- **Diff preview panel** - Toggle with Diff button
- **Session persistence** - Terminals maintain state across tab switches
- **Branch-terminal association** - Terminals are tracked per branch

### Architecture
- **Frontend**: SolidJS + TypeScript + Vite
- **Backend**: Tauri (Rust) for native performance
- **Terminal**: xterm.js with WebGL renderer
- **State Management**: Custom stores (terminals, repositories)

### Known Issues
- Tabs from all worktrees visible when switching branches (fix planned)
