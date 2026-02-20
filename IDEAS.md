# IDEAS - Feature Concepts for TUI Commander

**Purpose:** Shared memory between Boss and Bot for features we want to evaluate before implementing. Each idea captures the reasoning, trade-offs, and open questions so we don't lose context between sessions.

**Status legend:**
- `concept` - Just an idea, needs discussion
- `validated` - We agree it's worth doing, needs design
- `designed` - Approach decided, ready to implement
- `rejected` - Evaluated and discarded (kept for memory)
- `moved` - Promoted to SPEC.md for implementation

---

## Replace xterm.js with libghostty-vt (ghostty-web)

**Status:** `concept`

**What:** Replace xterm.js 6.x with ghostty-web (WASM build of Ghostty's VT parser) for terminal rendering. ghostty-web provides an xterm.js-compatible API, so the migration surface is small.

**Why it matters:**
- Better VT parsing accuracy — Ghostty handles edge cases (complex scripts, grapheme clusters, XTPUSHSGR/XTPOPSGR) that xterm.js doesn't
- SIMD-optimized parsing in the WASM build for faster rendering of heavy terminal output
- Battle-tested core — Ghostty is a full terminal emulator used daily by many developers, not just a web widget
- API-compatible drop-in: `ghostty-web` mirrors `@xterm/xterm` imports, so our Terminal.tsx integration requires minimal changes

**Current state of the ecosystem (Feb 2026):**
- `ghostty-web` v0.4.0 shipped Dec 2025 (npm, 1.8k GitHub stars). Actively maintained by Coder.
- It's an interim bridge: compiles Ghostty's Zig parser to WASM and wraps it in an xterm.js-compatible API. Will switch to official Ghostty WASM distribution once available.
- `libghostty-vt` (the official C library by Mitchell Hashimoto) is still pre-release. C API being designed, tagged release expected within ~6 months of the Jan 2025 announcement. Zig API usable now.
- WASM bundle size: ~400KB (vs xterm.js core ~200KB + addons). Not a concern for Tauri desktop app.

**Our xterm.js surface area (small):**
- 4 npm packages: `@xterm/xterm`, `@xterm/addon-fit`, `@xterm/addon-web-links`, `@xterm/addon-webgl`
- 4 source files import xterm: `Terminal.tsx`, `types/index.ts`, `themes.ts`, `index.tsx` (CSS)
- Addons: FitAddon (resize), WebLinksAddon (clickable URLs), WebglAddon (GPU rendering). Need to verify ghostty-web equivalents exist.

**Risks:**
- ghostty-web addon ecosystem is immature — FitAddon and WebglAddon equivalents may not exist yet (Ghostty handles GPU rendering natively, but in WASM mode it renders to Canvas2D)
- WebGL rendering is a major perf feature for us (Story 158 CanvasAddon is our fallback). If ghostty-web doesn't support WebGL, we'd lose GPU acceleration in the browser
- 400KB WASM bundle needs to load before first terminal paint — cold start latency
- ghostty-web is a community project (Coder), not official Ghostty. API stability not guaranteed until official WASM ships

**Decision criteria — migrate when:**
1. ghostty-web (or official Ghostty WASM) supports WebGL/GPU rendering OR our CanvasAddon fallback proves sufficient
2. FitAddon equivalent exists (terminal resize on container change)
3. Web links (clickable URLs) work out of the box or via addon
4. At least one production app has shipped with ghostty-web

**Open questions:**
- Does ghostty-web handle our Terminal.tsx lifecycle correctly? (dispose, attach/detach to DOM)
- Performance comparison: WASM parse + Canvas2D render vs xterm.js + WebGL — is parsing speed worth losing GPU rendering?
- Should we wait for official libghostty WASM or use Coder's bridge now?

## Command Palette

**Status:** `concept`


**What:** A Cmd+P / Cmd+Shift+P overlay that lets you search and execute any action by name. Fuzzy matching, recency-weighted ranking, keyboard-first navigation.

**Why it matters:**
- TUI Commander already has many keyboard shortcuts (Cmd+T, Cmd+W, Cmd+K, Cmd+D, etc.) - a command palette is the natural discoverability layer on top of those
- With 50+ terminals and multiple repos, quick action access becomes critical
- Can surface worktree actions (create, switch, archive) and PR actions (view, create, merge) without dedicated UI for each

**Open questions:**
- What's the action registry? Do we build a centralized command system or just wire existing shortcuts?
- How does it interact with the existing prompt library (Cmd+K)? Are they separate or merged?
- Should it support parameter input (e.g., "new terminal in repo X")?

**Trade-offs:**
- Adds UI complexity - but replaces the need for users to memorize shortcuts
- Needs a command registry architecture that doesn't exist yet

---

## Archived Worktrees View

**Status:** `concept`


**What:** A dedicated section showing worktrees that are no longer active but haven't been deleted. Collapsible sections, PR metadata display, multi-select with batch delete, keyboard delete support.

**Why it matters:**
- Git worktrees accumulate fast when running parallel agents - cleanup is a real pain point
- Seeing PR metadata (merged? closed? open?) on archived worktrees helps decide what to keep vs delete
- Batch operations prevent tedious one-by-one cleanup

**Open questions:**
- What defines "archived"? Is it user-initiated or automatic (e.g., PR merged = archived)?
- Where does this live in the sidebar? Separate collapsible section below active worktrees?
- Do we need this before we even have solid worktree management in the sidebar?

**Dependencies:**
- Requires worktree management to be more mature first
- Needs GitHub PR status integration per worktree

---

## PR Merge Readiness Status

**Status:** `partially implemented`


**What:** Surface PR merge readiness in the toolbar: conflict status, line changes summary, CI status tags, review approval state.

**Why it matters:**
- We already have PR detection and basic status in the StatusBar
- Merge readiness is the natural next step - "can I merge this?" at a glance
- Conflict highlighting saves context-switching to GitHub

**Open questions:**
- How much `gh` CLI data can we get without hammering the API? Need to check rate limits
- Should this be per-terminal (current branch) or a global view of all open PRs?
- Do we need CI status or just merge conflicts + approvals?

**What we already have:**
- PR detection from terminal output (071-cc1f)
- PR badge in sidebar and StatusBar
- `useGitHub.getPRStatus()` hook
- `classify_merge_state` / `classify_review_state` in Rust backend (github.rs)
- `prStateMapping.ts` frontend mapping
- `PrDetailPopover` component with merge/review state display
- `StatusBadge` component for CI status

**What's missing:**
- Conflict status (line-level)
- Line changes summary (+/- counts)
- CI status tags in toolbar (currently only in popover)

---

## Notification System (Popover + Toasts)

**Status:** `partially implemented`


**What:** Two notification surfaces:
1. Hover popover on a notification icon showing recent events
2. Toolbar toasts for real-time updates (PR merged, agent finished, rate limit hit)

**Why it matters:**
- With 50+ agents, you need to know when things happen without watching every terminal
- We already have rate limit detection and awaiting-input detection - these are natural notification sources
- SPEC.md already has "Audio notification when agent awaits input" as pending P2

**Open questions:**
- What events trigger notifications? Rate limits, agent waiting for input, task completion, PR updates?
- Toast vs persistent notification log - do toasts disappear or accumulate?
- How do notifications interact with terminal focus? Clicking a notification should probably switch to that terminal

**What we already have:**
- `notificationsStore` with full notification lifecycle
- `NotificationsTab` in Settings
- Rate limit detection, agent waiting detection as event sources
- Audio notification support

**What's missing:**
- Toast UI component (visual popups)
- Notification popover (history view)
- Click-to-focus-terminal on notification

**Trade-offs:**
- Notification fatigue with 50 agents is a real risk - needs good filtering/grouping
- Adds always-visible UI chrome (notification icon) - sidebar or toolbar?

---

## Advanced Analytics & Editor Settings

**Status:** editors `done`, analytics `concept`


**What:** Two things bundled:
1. Analytics settings for tracking agent usage, task completion rates, time spent
2. New editor options: Windsurf and Neovim alongside existing IDE launchers

**Why it matters for editors:**
- Windsurf and Neovim are popular in the AI coding agent space
- Adding them to the IDE launcher dropdown is trivial

**Editor status:**
- Windsurf: already implemented
- Neovim: added (Feb 2026) - detection via `nvim` CLI, opens with `nvim <path>`

**Why analytics is interesting but risky:**
- Usage stats could help optimize workflows (which agents are fastest? which repos get most work?)
- But YAGNI applies hard here - we're not at the scale where analytics drives decisions yet

**Recommendation:**
- Editor options: done
- Analytics: park it. Not worth the complexity until we have stable daily usage

---

## Drag-and-Drop Repository Reordering

**Status:** `concept`
**What:** Reorder repositories in the sidebar via drag-and-drop with a visual drag preview.

**Why it matters:**
- When managing multiple repos, custom ordering helps organize by project/priority
- Visual drag preview gives feedback during the interaction

**Open questions:**
- Do we even need this? Alphabetical or most-recently-used ordering might be sufficient
- Drag-and-drop in a Tauri webview can be tricky - is the effort worth it?

---

## Periodic Worktree Status Refresh

**Status:** `concept`
**What:** Worktree status auto-refreshes on a timer to keep the sidebar current without manual refresh.

**Why it matters:**
- Agents modify files constantly - stale status is misleading
- Users shouldn't have to click "refresh" to see if a worktree has changes

**Open questions:**
- What's the polling interval? Too fast = performance hit, too slow = stale data
- Can we use filesystem watchers instead of polling? Tauri has `notify` crate access
- What "status" exactly? Dirty/clean? Number of changed files? Branch ahead/behind?

**Trade-offs:**
- Polling N worktrees every M seconds adds background load
- Filesystem watchers are more efficient but more complex to implement

---

## Live Dictation with Local Speech-to-Text

**Status:** `partially implemented`
**Source:** Boss request (Feb 2026)

**What:** Add a microphone icon in the status bar (bottom bar) with a hotkey toggle for live dictation using local speech-to-text models like MacWhisper. Voice input gets transcribed and injected into the active terminal or prompt field.

**Why it matters:**
- Hands-free input for agent prompts and terminal commands
- Local models (MacWhisper, whisper.cpp) keep data private — no cloud dependency
- Speeds up interaction with agents when typing complex prompts

**Open questions:**
- Which local STT engine? MacWhisper (macOS app), whisper.cpp (library), or system Dictation API?
- Does the mic icon start/stop recording, or is it push-to-talk via hotkey?
- Where does transcribed text go? Active terminal stdin? A dedicated prompt input?
- How to handle multi-platform? macOS has better local STT options than Linux/Windows
- Audio feedback (visual waveform, recording indicator)?

**UI concept:**
- Mic icon in the status bar, next to existing controls
- Hotkey (e.g., Cmd+Shift+M) to toggle recording
- Visual indicator (pulsing red dot or waveform) when recording
- Transcribed text appears at cursor position in active terminal/prompt

**What we already have:**
- `dictationStore` with whisper model download/delete, recording state
- `DictationSettings` component in SettingsPanel
- `useDictation` hook
- whisper-rs-sys in Cargo dependencies (whisper.cpp integration)
- Mic icon in StatusBar
- Model management (download, select, delete)

**What's missing:**
- Actual audio capture and transcription pipeline (needs verification)
- Text injection into active terminal
- Push-to-talk hotkey

**Trade-offs:**
- Adds audio permission requirements (macOS microphone access)
- Local models need disk space (Whisper base ~150MB, large ~3GB)
- Latency depends on model size — need to balance accuracy vs speed

---

## Agent Teams Integration (tmux Shim)

**Status:** `designed`
**Source:** Claude Code Agent Teams feature (experimental, Feb 2026)

**What:** TUI Commander replaces tmux as the display layer for Claude Code Agent Teams. When a lead Claude Code instance spawns teammates via `tmux split-window`, a scoped shim intercepts the call and creates TUI tabs instead.

**Why it matters:**
- Agent Teams currently requires tmux or iTerm2 for split-pane mode — TUI Commander IS already a multiplexer
- Our tab system (up to 50 PTY sessions, per-branch grouping, activity indicators, parsed events) is purpose-built for this
- Claude Code's tmux integration has known bugs (pane indexing, command corruption at scale) — we can do better

**Design decisions (brainstormed 2026-02-15):**

### Architecture

```
Claude Code (lead in TUI tab)
  → calls "tmux split-window" to spawn teammate
  → calls "tmux send-keys -t {pane}" to initialize teammate

~/.tui-commander/bin/tmux  (shell script shim)
  ├── split-window / new-window → JSON over unix socket → TUI creates new tab
  ├── send-keys -t {pane}       → JSON over unix socket → TUI writes to tab PTY
  ├── list-panes                → JSON over unix socket → TUI returns fake pane list
  └── * (anything else)         → passthrough to real /usr/bin/tmux
```

### Scoped PATH injection (no global tmux breakage)
- Shim lives at `~/.tui-commander/bin/tmux`
- TUI Commander prepends `~/.tui-commander/bin` to `PATH` **only** for PTY sessions it spawns
- Outside TUI Commander, tmux works normally
- Inside TUI Commander, non-intercepted tmux commands pass through to real binary

### IPC: Unix socket
- TUI Commander (Rust backend) listens on `~/.tui-commander/ipc.sock`
- Shim connects, sends JSON commands, receives JSON responses
- Bidirectional: shim can receive pane IDs back from TUI

### IPC Protocol
```json
→ {"cmd": "split-window", "args": ["-h"], "env": {"CLAUDE_CODE_TEAM_NAME": "review"}}
← {"ok": true, "pane_id": "tuic-3"}

→ {"cmd": "send-keys", "target": "tuic-3", "keys": "claude --resume abc123\n"}
← {"ok": true}

→ {"cmd": "list-panes", "format": "..."}
← {"ok": true, "panes": [{"id": "tuic-1"}, {"id": "tuic-3"}]}
```

### Configurable via Settings
- **Enable Agent Teams shim** (toggle, off by default) — Controls PATH injection + socket listener
- **Team tab grouping** (toggle) — Visual distinction for teammate tabs
- **Auto-focus new teammates** (toggle) — Switch to new tab when teammate spawns

### Components needed
| Component | Location | Purpose |
|-----------|----------|---------|
| Shim script | `~/.tui-commander/bin/tmux` | Intercepts tmux commands, routes to IPC |
| IPC socket server | `src-tauri/src/ipc_server.rs` | Listens on unix socket, translates to tab operations |
| Pane registry | `src-tauri/src/pane_registry.rs` | Maps fake tmux pane IDs to TUI terminal IDs |
| Settings config | Settings store | Persists agent-teams toggle + sub-options |
| Settings UI | SettingsPanel | Toggle switches |
| PATH injection | `create_pty` in lib.rs | Conditionally prepends shim dir to PATH |
| CI tests | `tests/tmux-shim/` | Verify shim handles known CC tmux commands |

### Fragility mitigation
- CI tests verify shim handles the exact tmux commands CC currently uses
- If CC changes their tmux integration, tests fail and we update the shim
- No stable API contract exists — this is reverse engineering, managed with tests

### NOT in scope (YAGNI)
- Task list UI visualization (CC manages via `~/.claude/tasks/`)
- Inter-teammate message UI (filesystem-based, works without us)
- Custom team creation UI (user types naturally in Claude Code)
- WezTerm/iTerm2 shim (only tmux backend for now)
- Contributing upstream to CC (separate effort if we pursue it)

**Dependencies:**
- Claude Code Agent Teams feature stability (currently experimental with `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`)
- `socat` or equivalent for unix socket communication from shell script

**Risks:**
- CC's tmux commands are internal implementation details — can change without notice
- Agent Teams feature might be redesigned or deprecated
- `socat` dependency on the shim script (could use `/dev/tcp` or a tiny binary instead)

---

## Remote Access Mode (Tablet/Browser)

**Status:** `moved` → Stories 145-149
**Source:** Boss request (Feb 2026)

**What:** An activatable setting that starts an embedded HTTP server inside the Tauri process, serving the React frontend and bridging commands/terminals over HTTP+WebSocket. Allows accessing TUI Commander from a tablet browser on the same LAN.

**Why it matters:**
- Work from tablet without being at the desk
- No extra infrastructure — runs inside the existing Tauri process
- Reuses the PTY layer being implemented for native terminal support

**Primary use case:** Boss accessing TUI Commander from a tablet on the same local network.

**Architecture:**

```
Mac (Tauri running)
├── Webview locale (IPC, come oggi)
└── axum server :9876 (attivabile da Settings)
    ├── GET /           → serve React bundle statico
    ├── POST /rpc       → proxy ai comandi Tauri
    ├── WS /terminal    → bridge PTY ↔ WebSocket
    └── Auth: Bearer token
```

### Key design decisions:

1. **PTY è solo locale (IPC)** — il PTY nativo espone i terminali solo al webview Tauri via IPC, mai sulla rete. Il WebSocket bridge è un layer separato sopra, attivato esplicitamente dall'utente.

2. **Transport layer abstraction nel frontend** — il frontend ha bisogno di un'astrazione che scelga tra `invoke()` (Tauri IPC) e `fetch()`/`WebSocket` (browser remoto):
   ```typescript
   // rpc() usa invoke() in Tauri, fetch() in browser
   const result = await rpc('git_status', { path });
   ```

3. **Attivabile da Settings** — tutto il remote access è off by default. Il toggle nei Settings:
   - Avvia/ferma il server axum
   - Genera e mostra il bearer token
   - Mostra l'URL di connessione (es. `http://192.168.1.x:9876`)
   - Vive nella stessa sezione Settings del PTY nativo (sono feature correlate)

4. **Auth** — Bearer token generato al momento dell'attivazione, mostrato in UI. Sufficiente per LAN domestica/ufficio. No TLS necessario in LAN.

5. **Single user** — nessuna gestione multi-utente, è solo Boss da un altro device.

### Reuso dal PTY nativo:
- La gestione PTY in Rust è la stessa — il WebSocket bridge legge/scrive sugli stessi PTY
- Il resize del terminale passa via WebSocket message invece che IPC event
- xterm.js nel browser remoto è identico a quello locale

### Lavoro specifico per remote access:
- Server axum embedded con lifecycle (start/stop da Settings)
- RPC endpoint che traduce HTTP POST → chiamate alla stessa logica dei comandi Tauri
- WebSocket endpoint per PTY streaming
- Auth middleware (bearer token)
- Transport abstraction nel frontend (`invoke()` vs `fetch()`)
- Touch-friendly extras per tablet (bottoni Ctrl/Tab/Esc) — nice-to-have, non bloccante

### Open questions:
- Porta configurabile o fissa con fallback?
- Il token viene rigenerato a ogni attivazione o persiste?
- Serve un QR code nell'UI per connessione rapida dal tablet?

**Dependencies:**
- PTY nativo (in corso) — il WebSocket bridge si appoggia sulla stessa infrastruttura

**Trade-offs:**
- Aggiunge axum come dipendenza Rust (ma è leggero)
- Superficie di attacco aumentata quando attivo — mitigata da auth + off by default
- Touch input su xterm.js è limitato — usabile ma non perfetto

---

## Move Business Logic from Frontend Stores to Rust Backend

**Status:** `concept`
**Source:** Code review (Feb 2026, story 187-f906)

**What:** Migrate data transformation, parsing, and business logic from TypeScript stores to Rust backend. Frontend would only handle rendering and user interaction — never data reshaping or computation.

**Why it might matter:**
- Aligns with the project's "Logic in Rust" CLAUDE.md rule
- Rust-side logic is faster, type-safer, and testable without a DOM environment
- Reduces frontend complexity

**Why we're deferring:**
- Current architecture works — no user-facing bugs from frontend logic
- Moving logic piecemeal risks inconsistency (some in Rust, some in TS)
- Big bang migration is risky and high-effort with unclear payoff
- Should be done incrementally as stores are touched for other reasons

**If we revisit:** Do it one store at a time, starting with whichever store gets modified next for a feature. Don't do a dedicated migration effort.

---

## Multi-Instance Support

**Status:** `rejected`
**Source:** Boss request (Feb 2026)

**What:** Allow running multiple TUI Commander instances simultaneously.

**Why rejected:** TUI Commander is already a multi-repo, multi-tab (50+) multiplexer. There's no use case that requires a second instance — everything can be managed within the single window. The complexity of shared state (mcp-port, config files, file watchers) far outweighs any benefit.

---

## Notes

### Deferred improvements
Features that depend on underlying infrastructure not yet built:
- Toolbar PR layout glitches (PR popover exists now, could revisit)
- Terminal/notification focus handling (notification store exists, needs toast UI)
