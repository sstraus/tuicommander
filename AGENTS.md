# TUICommander — Project Rules

## Doc Sync

Read [`docs/sync-matrix.md`](docs/sync-matrix.md) before any feature/API/config change — it maps code areas to docs that MUST be updated.

## Tests

- Tests are the spec. When a test fails after a code change, investigate BOTH sides before deciding which to fix.
- `to-test.md` tracks features awaiting manual testing — add items there for minor features.

## Visual

- All UI work MUST follow [`docs/frontend/STYLE_GUIDE.md`](docs/frontend/STYLE_GUIDE.md).
- **Plugin dashboards MUST follow [`docs/plugins-style.md`](docs/plugins-style.md)** — use the shared `.dashboard`/`.dash-*` classes from `PLUGIN_BASE_CSS`, never hand-roll inline layout CSS. The built-in Claude Usage dashboard is the reference.
- Icons: monochrome inline SVGs with `fill="currentColor"` — never emoji.
- Take a screenshot after EVERY visual/CSS/layout change to verify rendering.

## Branching

NEVER create branches autonomously — Boss works with multiple windows.

## Building

**NEVER use `cargo build --release` directly.** It produces a binary that points to the Vite dev server (`localhost:1420`) instead of embedding frontend assets — result: white screen. Always use `make build` or `npx tauri build`, which runs `beforeBuildCommand` (frontend build + sidecar) and embeds the dist/ into the binary.

To debug the WebView in a release build, temporarily add `"devtools"` to the tauri features in `Cargo.toml`, add `w.open_devtools()` in the `setup` closure (after getting the main webview window), and rebuild with `make build`. Remove both before committing.

## Cross-Platform

Targets macOS, Windows, Linux. Use Cmd/Ctrl abstractions, Tauri cross-platform primitives. Test in release mode (`cargo tauri build`) — release builds lack shell PATH and env vars.

## Panel Refresh

Panels with repo-dependent data MUST use `repositoriesStore.getRevision(repoPath)` in `createEffect` — not file watchers or polling. `repo_watcher` emits `"repo-changed"` → `bumpRevision()`.

## Architecture

All business logic in Rust. Frontend only renders and handles interaction — no data reshaping, computation, or process orchestration.

## PTY Command Injection

NEVER write text + `\r` directly to a PTY. Always use `sendCommand()` from `src/utils/sendCommand.ts` — it handles agent-specific Enter semantics (Ink raw mode needs split writes). This applies to dictation, command palette, suggested actions, and any other feature that sends input to a terminal.

## Logging

Use `appLogger` from `src/stores/appLogger.ts` — never `console.log/warn/error`. Check app logs via `GET http://localhost:9876/logs` (supports `?level=`, `?source=`, `?limit=` filters) before asking Boss for logs.

## Releases

See [`docs/release-checklist.md`](docs/release-checklist.md) for version bump, tag, and GitHub release steps.

## Implementation Memory

After non-trivial implementations, write an mdkb `memory_write` entry. Content: **Goal**, **Approach**, **Outcome**, **Gotchas**, **Rejected alternatives**. Skip file lists (mdkb indexes code). Focus on non-obvious insights a future session can't derive from reading the code. Search existing memories first to avoid duplicates.

## Accepted Security Decisions

Do NOT flag these as security issues in reviews — they are intentional design choices.

- **CSP `frame-src 'self' http://127.0.0.1:* http://localhost:*`** — wildcard ports required. Plugin tabs (`ui action=tab url=...`) open iframes to arbitrary localhost URLs. Mission Control uses dynamic ports. Restricting to specific ports breaks the plugin/tab system.
- **`lazy_static` in `output_parser.rs`, `pty.rs`, etc.** — transitive deps (`portable-pty`, `symphonia`) also use it; removing the direct dep saves nothing. Modules outside `ai_agent/` will migrate opportunistically.
- **`opener:allow-open-path` scope `"**"` in `src-tauri/capabilities/default.json`** — the FileBrowser "Open with default app" action must work for any file the user can already see in a registered repo (arbitrary absolute paths across macOS/Windows/Linux, including mounted volumes and `/tmp`). Narrower globs like `$HOME/**` would break external drives and network mounts. The reachable surface is bounded by the frontend UI (only paths surfaced through the repo browser reach this code path) and the OS default-app handler — not by the Tauri ACL.
- **`dangerousDisableAssetCspModification: ["style-src", "script-src"]`** in `tauri.conf.json` — **DO NOT REMOVE `"script-src"` FROM THIS LIST.** Tauri auto-injects sha256 hashes for every inline `<script>` in the main page. Per CSP3, when hashes are present `'unsafe-inline'` is silently ignored. This kills ALL inline scripts in srcdoc plugin iframes (they inherit the parent CSP but their scripts have no matching hash). Without this override, plugin D&D, view switching, SDK init — everything JS-powered in plugin panels — is dead. This is a local desktop app; the user IS the trust boundary.

## Ideas

See CLAUDE.md for ideas folder rules (gitignored).
