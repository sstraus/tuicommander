# Changelog

All notable changes to TUICommander will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- **Git Panel** — Tabbed side panel (`Cmd+Shift+G`) replacing the Git Operations Panel floating overlay. Five tabs: Changes (staging/unstaging, commit with amend, discard), Log (virtual scroll + Canvas commit graph with lane assignment and Bezier connections), Stashes (apply/pop/drop), History (per-file commit log following renames), Blame (per-line blame with age heatmap). Keyboard navigation: Escape to close, Ctrl/Cmd+1–5 to switch tabs
- **Canvas-based commit graph** — Visual commit graph in the Log tab rendered on Canvas with lane assignment, 8-color palette, ref badges, and Bezier curve connections between parent/child commits
- **Ideas panel image paste** — `Ctrl+V` / `Cmd+V` pastes clipboard images into notes. Images saved to disk, displayed as thumbnails, and sent as absolute paths when forwarding to terminal (so AI agents can read them). Supports PNG, JPEG, WebP, GIF up to 10 MB. Image-only notes (no text) are allowed. Cleanup on delete.
- **Ideas panel in-place edit** — Edit now preserves note identity (no ID change). `Escape` cancels edit mode.
- **Archive script** — Per-repo lifecycle hook that runs before a worktree is archived or deleted; non-zero exit blocks the operation. Configurable via Settings → Repository → Scripts or `.tuic.json`
- **Repo-local config (`.tuic.json`)** — Team-shareable configuration file in the repository root. Three-tier precedence: `.tuic.json` > per-repo app settings > global defaults. Covers base branch, scripts, worktree storage, merge strategy, and more
- **PR Review button** — Review button in the PR Detail Popover spawns a terminal running the agent's "review" run config with interpolated PR variables (`{pr_number}`, `{branch}`, `{base_branch}`, `{repo}`, `{pr_url}`). Shown only when an active agent has a run config named "review"

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
- **Plan Panel** (`Cmd+P`) — New right-side panel showing plan files for the active repository. Plans are detected from agent output via structured events, filtered by active repo, and auto-open as background tabs on first detection. Frontmatter is stripped from rendered content. Panel visibility and width persist across restarts
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

- **Command Palette** (`Cmd+Shift+P`) — Fuzzy search across all app actions with recent-first ordering
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
