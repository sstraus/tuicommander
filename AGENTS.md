# TUICommander — Project Rules

## Doc Sync

Read [`docs/sync-matrix.md`](docs/sync-matrix.md) before any feature/API/config change — it maps code areas to docs that MUST be updated.

## Tests

- Tests are the spec. When a test fails after a code change, investigate BOTH sides before deciding which to fix.
- `to-test.md` tracks features awaiting manual testing — add items there for minor features.
- **`[HUMAN]` is a last resort.** Before marking a to-test item `[HUMAN]`, you MUST attempt verification through this escalation ladder:
  1. **Code inspection** — read the source, confirm the logic exists at file:line
  2. **Test execution** — `cargo test`, `vitest run` with relevant filter
  3. **CLI probing** — `curl` HTTP endpoints, `grep` for patterns
  4. **MCP maccontrol** — take screenshots, click UI elements, verify visual state
  5. **MCP invoke/JS** — call Tauri commands, inspect store state, trigger actions programmatically
  Only use `[HUMAN]` when the item genuinely requires real hardware (audio, IME, touch), multi-app interaction (drag to Finder, global hotkey from another app), or timing-sensitive observation that none of the above can capture. When code-verifying, change `[HUMAN]` to `[x]` with a `_(verified: file:line explanation)_` annotation. When code reveals the description is wrong, change to `[ ]` with a `_(NOTE: ...)_` correction.

## Test instance vs orchestrator instance — READ BEFORE TESTING

There are TWO running TUICommander instances; do not confuse them:

- **Orchestrator instance** — the one this agent is embedded in. The `tuicommander` MCP tools and `debug invoke_js` target THIS instance (Mission Control on `:14319`, app logs on `:9876`). It does **NOT** run your worktree build, so testing it proves nothing about your changes.
- **Test instance** — the worktree dev build you start with `make dev`. Test your changes against it **only via its HTTP API on `http://127.0.0.1:9877`**. MCP/`invoke_js` cannot reach it.

9877 endpoints (see `src-tauri/src/mcp_http/mod.rs`): `GET/POST /sessions`, `DELETE /sessions/{id}`, `POST /sessions/{id}/write`, `GET /sessions/{id}/output`, and terminal grid ops (all session-scoped) `POST /sessions/{id}/terminal/scroll {delta}`, `POST /sessions/{id}/terminal/scroll-to {line}` (absolute; `line`=top row, 0=oldest), `POST /sessions/{id}/terminal/scroll-to-offset {offset}` (coalesced display-offset jump; powers wheel + scrollbar-drag in browser mode), `GET /sessions/{id}/terminal/scroll-info`, `GET /sessions/{id}/terminal/lines?start&end`, `GET /sessions/{id}/terminal/row-text?row`, `POST /sessions/{id}/terminal/search-buffer {query}`. Create a throwaway session, exercise it, then `DELETE` it — never test against Boss's live sessions.

Canvas rendering (selection highlight, smooth-scroll visuals, cursor) is **not observable over HTTP** — those still need a visual check with Boss.

## Visual

- All UI work MUST follow [`docs/frontend/STYLE_GUIDE.md`](docs/frontend/STYLE_GUIDE.md).
- **Plugin dashboards MUST follow [`docs/plugins-style.md`](docs/plugins-style.md)** — use the shared `.dashboard`/`.dash-*` classes from `PLUGIN_BASE_CSS`, never hand-roll inline layout CSS. The built-in Claude Usage dashboard is the reference.
- Icons: monochrome inline SVGs with `fill="currentColor"` — never emoji.
- Take a screenshot after EVERY visual/CSS/layout change to verify rendering.

## Branching

NEVER create branches autonomously — Boss works with multiple windows.

## Commits

When a commit resolves a **GitHub issue**, use a closing keyword so GitHub auto-closes it: `Fixes #N` / `Closes #N` / `Resolves #N` (anywhere in the message — `fix(scope): desc (closes #N)` in the subject is fine). A bare `(#N)` only *links* the issue, it does NOT close it. This repo pushes directly to `main` (the default branch), where closing keywords take effect on push — no PR merge required.

- Use the GitHub-issue keyword only for the commit that actually fixes it; reference-only commits keep `(#N)`.
- This is distinct from **mdkb story ids** (7-char hex like `#abc1234`): those follow the wiz convention — `(#abc1234)` for traceability, `(closes #abc1234)` on story completion — and are unrelated to GitHub issue auto-close.

## Building

**NEVER use `cargo build --release` directly.** It produces a binary that points to the Vite dev server (`localhost:1420`) instead of embedding frontend assets — result: white screen. Always use `make build` or `pnpm tauri build`, which runs `beforeBuildCommand` (frontend build + sidecar) and embeds the dist/ into the binary.

To debug the WebView in a release build, temporarily add `"devtools"` to the tauri features in `Cargo.toml`, add `w.open_devtools()` in the `setup` closure (after getting the main webview window), and rebuild with `make build`. Remove both before committing.

## Cross-Platform

Targets macOS, Windows, Linux. Use Cmd/Ctrl abstractions, Tauri cross-platform primitives. Test in release mode (`cargo tauri build`) — release builds lack shell PATH and env vars.

## Panel Refresh

Panels with repo-dependent data MUST use `repositoriesStore.getRevision(repoPath)` in `createEffect` — not file watchers or polling. `repo_watcher` emits `"repo-changed"` → `bumpRevision()`.

## Architecture

All business logic in Rust. Frontend only renders and handles interaction — no data reshaping, computation, or process orchestration.

## PTY Command Injection

NEVER write text + `\r` directly to a PTY. Always use `sendCommand()` from `src/utils/sendCommand.ts` — it handles agent-specific Enter semantics (Ink raw mode needs split writes). This applies to dictation, command palette, suggested actions, and any other feature that sends input to a terminal.

## Agent Session Management

TUIC tracks each agent's session ID for resume-after-restart. Two strategies coexist:

**Discovery-based (Claude, Gemini, Codex, Grok).** TUIC does NOT inject `--session-id` at launch — the agent creates its own UUID. TUIC discovers the active session by scanning the agent's session directory for the newest file, re-checking every 30s poll. This survives `/clear` (all three agents have it: Claude `/clear`, Gemini `/clear`+`/new`, Codex `/clear`+`/new`+`/fork`) because re-discovery picks up the new session file. Resume uses `agentSessionId` (disk-discovered), not `tuicSession`. Grok stores each session as a UUIDv7-named directory under `~/.grok/sessions/<percent-encoded-cwd>/`; the newest dir is the active session (`grok --resume <id>`).

**Forced injection (Goose).** Shell wrapper injects `--name $TUIC_SESSION` into `goose session/run` commands. The TUIC tab UUID IS the goose session name. Discovery returns `None` (SQLite storage, no filesystem scan). Resume uses `tuicSession`.

**No session tracking (Aider, Amp, Cursor, Droid, OpenCode).** Either no local session files, cloud-only, or no UUID-based resume. `TUIC_SESSION` env var is available but unused.

When adding a new agent: choose discovery-based if the agent writes session files to disk (add `sessionDiscovery` to `agents.ts` and a Rust `discover_*_session` to `agent_session.rs`). Choose forced injection only when discovery is impossible (e.g., SQLite-only storage).

## Logging

Use `appLogger` from `src/stores/appLogger.ts` — never `console.log/warn/error`. Check app logs via `GET http://localhost:9876/logs` (supports `?level=`, `?source=`, `?limit=` filters) before asking Boss for logs.

## Diagnostics

Runtime diagnostics for debugging performance issues. Code: `src-tauri/src/cpu_watchdog.rs`.

**Always on (zero overhead when idle):**
- CPU spike detection via `getrusage(RUSAGE_SELF)` — only the TUIC process, not PTY children
- Logs `CPU SPIKE` warning when >80% for 10+ consecutive seconds with full snapshot
- Sleep/wake detection — skips stale ticks after lid close/open

**Diagnostic mode (toggle at runtime):**

```bash
# Enable diagnostic mode
curl -X POST http://localhost:9876/diagnostics -d '{"enabled":true}' -H 'Content-Type: application/json'

# Check status
curl http://localhost:9876/diagnostics

# Read diagnostic logs
curl 'http://localhost:9876/logs?source=diagnostics'
```

When enabled, emits health snapshots every 30s and alerts on FD/thread growth trends. Each snapshot includes: CPU% (TUIC-self only, via `RUSAGE_SELF`), `children_cpu` (aggregate %cpu of PTY children + hottest child — the spike trigger deliberately ignores children, so this is the only place a hot `cargo`/agent surfaces when TUIC itself is calm), thread count, FD count, PTY session count, content index build state, semaphore permits, `grid_frame_in_flight` stuck sessions, event bus subscriber count, `head_emits_suppressed` (repo-watcher `head-changed` emits skipped by the resolved-HEAD-target guard — a high/climbing value signals a filesystem-event storm, issue #82).

**When to enable:** Boss reports sluggishness, CPU spikes, or UI freezes. Enable it, reproduce the issue, then check the logs. The snapshot at the time of the spike tells you what subsystem is overloaded.

**Known past failure patterns this catches:**
- IPC flush loop (ack_terminal_frame sending frames in ack path → 240+ IPC/sec)
- Content index build saturating CPU on large repos
- `grid_frame_in_flight` stuck (WebView JS thread blocked)
- FD/thread leak (progressive growth without cleanup)
- Sleep/wake false idle cascades (tokio timers firing stale)

## Releases

See [`docs/release-checklist.md`](docs/release-checklist.md) for version bump, tag, and GitHub release steps. After creating any release or nightly tag, **verify CI completes successfully** — check `gh run list`, inspect failures, and confirm all platform assets (macOS .dmg, Linux .deb/.rpm/.AppImage, Windows .exe) are uploaded before reporting done.

## Implementation Memory

After non-trivial implementations, write an mdkb `memory_write` entry. Content: **Goal**, **Approach**, **Outcome**, **Gotchas**, **Rejected alternatives**. Skip file lists (mdkb indexes code). Focus on non-obvious insights a future session can't derive from reading the code. Search existing memories first to avoid duplicates.

## Accepted Security Decisions

Do NOT flag these as security issues in reviews — they are intentional design choices.

- **CSP is intentionally wide open.** TUIC is a local dev tool, not a SaaS. The user IS the trust boundary. The CSP uses a single permissive `default-src` that allows `https:`, `http:`, `data:`, `blob:`, `unsafe-inline`, etc. **NEVER tighten the CSP.** Every time we've had per-directive restrictions, some iframe content (reveal.js slides, plugin panels, dashboards) broke. The only specific directive kept is `frame-src` (for localhost wildcard ports). If you feel the urge to add CSP restrictions, don't — read this bullet point again.
- **`dangerousDisableAssetCspModification: ["style-src", "script-src"]`** in `tauri.conf.json` — **DO NOT REMOVE.** Tauri auto-injects sha256 hashes for inline `<script>` tags. Per CSP3, hashes silently disable `'unsafe-inline'`. This kills all JS in srcdoc iframes (plugins, HTML previews). The override prevents Tauri from injecting those hashes.
- **`lazy_static` in `output_parser.rs`, `pty.rs`, etc.** — transitive deps (`portable-pty`, `symphonia`) also use it; removing the direct dep saves nothing. Modules outside `ai_agent/` will migrate opportunistically.
- **`opener:allow-open-path` scope `"**"`** — FileBrowser must open any file the user can see. Narrower globs break external drives and network mounts.
- **Iframe sandbox = `allow-scripts allow-same-origin`** — ALL iframes MUST use this. NEVER use bare `sandbox=""` — it kills JavaScript.

## Ideas

See CLAUDE.md for ideas folder rules (gitignored).
