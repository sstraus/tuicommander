# Performance Review: TUICommander v0.9.7

**Created:** 2026-03-28
**Status:** Complete
**Estimated Effort:** XL (audit + remediation)

## Summary

Comprehensive performance audit of TUICommander covering the Rust backend (Tauri commands, PTY I/O, git operations, state management) and the SolidJS frontend (rendering, polling, memory, bundle). The review identified **~30 concrete issues** across 6 categories, prioritized by impact. The most critical findings are blocking sync commands on the Tokio runtime, excessive polling with subprocess forks, monolithic bundle, and unbatched PTY event emission.

## Audit Categories

| # | Category | Scope | Issues Found |
|---|----------|-------|-------------|
| 1 | Rust: Blocking Commands | `src-tauri/src/git.rs`, `lib.rs`, `git_graph.rs` | P1×2, P2×15 |
| 2 | Frontend: Polling & Timers | `useAgentPolling`, `github.ts`, `StatusBar`, `ActivityDashboard` | P2×4, P3×2 |
| 3 | PTY Data Pipeline | `pty.rs`, `Terminal.tsx` | P2×1, P3×3 |
| 4 | Bundle & Startup | `vite.config.ts`, `App.tsx` | P2×2 |
| 5 | Memory & Reactivity | `terminals.ts`, `Terminal.tsx`, `ActivityDashboard` | P3×4 |
| 6 | Lock Contention & Serialization | `registry.rs`, `config.rs`, `pty.rs` | P2×1, P3×3 |

---

## Category 1: Rust — Blocking Sync Commands

**Root cause:** Many `#[tauri::command]` handlers are sync functions that call `std::process::Command` (git subprocess) or CPU-intensive functions directly, blocking a Tokio worker thread.

### P1-01: `update_from_base` blocks Tokio on network I/O
- **File:** `src-tauri/src/git.rs:298`
- **Problem:** Sync command calls `git fetch` (network) + `git rebase/merge`. Can block for seconds to minutes. No `spawn_blocking`.
- **Fix:** Convert to `async fn` + `spawn_blocking`.
- **Impact:** Can freeze all IPC while fetch runs.

### P1-02: `hash_password` blocks Tokio for ~200-500ms
- **File:** `src-tauri/src/lib.rs:140`
- **Problem:** `bcrypt::hash(cost=12)` is intentionally slow CPU work, runs synchronously on IPC thread.
- **Fix:** Convert to `async fn` + `spawn_blocking`.

### P2-03: ~20 sync git commands lack `spawn_blocking`
- **Files:** `src-tauri/src/git.rs` (multiple), `git_graph.rs:218`
- **Commands:** `get_git_diff`, `get_diff_stats`, `get_changed_files`, `get_git_branches`, `get_recent_commits`, `get_git_panel_context` (cache miss), `get_commit_graph`, `git_stage_files`, `git_unstage_files`, `git_discard_files`, `git_commit`, `git_stash_*`, `get_stash_list`, `resolve_context_variables`
- **Fix:** Convert each to `async fn` + `spawn_blocking`. Batch approach: create a macro or helper that wraps the body.
- **Note:** Some commands like `get_branches_detail`, `get_merged_branches`, `get_repo_summary` already use `spawn_blocking` correctly — follow that pattern.

### P2-04: `get_changed_files` runs 2 sequential subprocesses
- **File:** `src-tauri/src/git.rs:510`
- **Problem:** Calls `git diff --name-status` then `git diff --numstat` sequentially.
- **Fix:** Combine into a single `git diff --name-status --numstat` call and parse both outputs.

### P2-05: `get_git_panel_context_impl` runs ~5 serial git commands
- **File:** `src-tauri/src/git.rs:1311`
- **Problem:** Sequential subprocess calls for status, ahead/behind, stash count, last commit, detached HEAD.
- **Fix:** Replace with `git status --porcelain=v2 --branch` which provides branch + ahead/behind + file status in one call. Parallelize remaining calls.

---

## Category 2: Frontend — Polling & Timers

**Root cause:** Multiple independent `setInterval` timers run continuously, scaling linearly with terminal count and firing even when their data hasn't changed.

### P2-06: `useAgentPolling` forks `ps` per terminal every 3s
- **File:** `src/hooks/useAgentPolling.ts` (3000ms interval)
- **Rust side:** `src-tauri/src/pty.rs:1847` — `process_name_from_pid` calls `std::process::Command::new("ps")`
- **Problem:** N terminals = N `ps` forks every 3s = ~100 forks/min with 5 terminals. The Tauri command `get_session_foreground_process` is sync (not `async fn`), blocking a thread pool slot per fork.
- **Fix (Rust):** Replace `ps` subprocess with `libc::proc_pidinfo()` + `PROC_PIDTBSDINFO` (macOS) or `/proc/{pid}/comm` (Linux). Zero fork overhead.
- **Fix (Architecture):** Push process-change detection from the PTY reader thread via Tauri event (periodic `tcgetpgrp` check in the reader loop, emit only on change). Eliminates the polling entirely.

### P2-07: StatusBar 1s timer fires unconditionally
- **File:** `src/components/StatusBar/StatusBar.tsx:46`
- **Problem:** `setInterval` at 1000ms increments signals even when no merged PR and no rate limit active. `activePrData` createMemo re-evaluates every second.
- **Fix:** Only start timer when conditions are true (merged PR detected or rate limit active). Use `createEffect` to start/stop the timer reactively.

### P3-08: ActivityDashboard `{ equals: false }` forces re-render
- **File:** `src/components/ActivityDashboard/ActivityDashboard.tsx:152`
- **Problem:** Snapshot signal bypasses SolidJS equality check, forcing `<For>` to diff every 10s even when data unchanged.
- **Fix:** Remove `{ equals: false }` or implement shallow array comparison.

### P3-09: Multiple independent global timers
- **Active timers at any time:** `useAgentPolling` (3s), `githubStore` (30s/120s/300s), `statusBarTicker` (1s + 5s), `StatusBar` (1s), `prNotificationsStore` (1s), `ActivityDashboard` (1s + 10s), `useAutoFetch` (60s), `useAppInit.snapshotTimer` (30s)
- **Opportunity:** Consolidate into a unified tick scheduler that distributes callbacks. Lower priority — each timer is individually cheap, but the accumulation adds scheduling noise.

---

## Category 3: PTY Data Pipeline

### P2-10: No write batching from Rust to frontend
- **File:** `src-tauri/src/pty.rs:1171`
- **Problem:** Every 4096-byte read chunk is immediately emitted as a Tauri event with JSON serialization + `session_id.clone()`. During high-throughput agent output, this means hundreds of IPC events/second.
- **Fix:** Accumulate chunks over a 5-10ms window, emit one batched event. The PTY reader thread could use a timer or byte-count threshold.

### P3-11: Double VT100 parse in diff-render mode
- **File:** `src-tauri/src/pty.rs:1153-1166`
- **Problem:** When `DiffRenderer` is active, data is parsed by both `VtLogBuffer` and `renderer.lock().process()` — two VT100 state machines per chunk.
- **Fix:** Share parsed state between them, or skip `VtLogBuffer` when diff renderer handles parsing.

### P3-12: PTY read buffer too small
- **File:** `src-tauri/src/pty.rs:1124` — 4096 bytes
- **Problem:** Small buffer = many iterations, many lock acquisitions, many events per second of output.
- **Fix:** Increase to 32KB or 64KB.

### P3-13: `session_id.clone()` per chunk in hot path
- **File:** `src-tauri/src/pty.rs:1171`
- **Problem:** Heap allocation on every PTY read for session ID string.
- **Fix:** Use `Arc<str>` for session_id — clone is a ref-count increment.

---

## Category 4: Bundle & Startup

### P2-14: No code splitting — monolithic bundle
- **File:** `vite.config.ts`, `src/App.tsx`
- **Problem:** All panels statically imported: `SettingsPanel`, `ActivityDashboard`, `CommandPalette`, `GitPanel`, `FileBrowserPanel`, `DiffTab`, `MarkdownPanel`, `CodeEditorPanel`, `HtmlPreviewTab`. Heavy deps (`@codemirror/*`, `@git-diff-view/solid`, `marked`, `dompurify`, `qrcode`, `xterm`) in one chunk.
- **Fix (quick):** Add `manualChunks` to vite config:
  ```js
  { xterm: ['@xterm/xterm', '@xterm/addon-*'],
    codemirror: ['@codemirror/*', '@lezer/*'],
    diff: ['@git-diff-view/core', '@git-diff-view/solid'],
    markdown: ['marked', 'dompurify'] }
  ```
- **Fix (proper):** Wrap infrequent panels in SolidJS `lazy()` + `<Suspense>`. Pattern already used for `FloatingTerminal`.

### P2-15: Heavy panels eager-loaded
- **File:** `src/App.tsx:11-69`
- **Candidates for lazy():** `SettingsPanel`, `ActivityDashboard`, `GitPanel` family (pulls in CodeMirror), `DiffTab`, `MarkdownPanel`, `CodeEditorPanel`
- **Impact:** Faster initial parse/eval, lower time-to-interactive.

---

## Category 5: Memory & Reactivity

### P3-16: xterm scrollback 10,000 lines per terminal
- **File:** `src/components/Terminal/Terminal.tsx:621`
- **Problem:** ~1.6MB heap per terminal at 80 cols. With 10+ terminals open, ~16MB+ of scrollback buffers.
- **Fix:** Consider reducing to 5,000 or making configurable. Monitor actual usage patterns.

### P3-17: `debouncedBusy` updates outside `batch()`
- **File:** `src/stores/terminals.ts`
- **Problem:** `setState("debouncedBusy", id, true)` fires separately from `setState("terminals", id, data)`, causing two reactive notification passes for one logical change. StatusBar, SmartButtonStrip, TabBar all subscribe to `debouncedBusy`.
- **Fix:** Wrap both state mutations in a single `batch()` call.

### P3-18: xterm atlas rebuild every 5 minutes per terminal
- **File:** `src/components/Terminal/Terminal.tsx:1120`
- **Problem:** `setInterval` that forces WebGL atlas rebuild by re-assigning `terminal.options.fontSize`. Runs per visible terminal.
- **Investigation:** Profile whether this causes frame drops. May be necessary for Unicode glyph correctness with AI agent output — needs measurement before removing.

### P3-19: `pluginRegistry.processRawOutput` on every PTY chunk
- **File:** `src/components/Terminal/Terminal.tsx:247`
- **Problem:** Called for every data chunk regardless of whether plugins are loaded. If no plugins, this should be a no-op check, but verify there's no string processing overhead.
- **Fix:** Early-return when no plugins are registered.

---

## Category 6: Lock Contention & Serialization

### P2-20: MCP upstream Mutex serializes all concurrent tool calls
- **File:** `src-tauri/src/mcp_proxy/registry.rs:597,624,752`
- **Problem:** `tokio::sync::Mutex` on `UpstreamClient::Http` held across full async HTTP call. Only one concurrent MCP tool call per upstream. Multiple agent sessions queue up.
- **Fix:** Use `tokio::sync::Semaphore(8)` for concurrent calls, keep Mutex only for reconnect operations.

### P3-21: Config disk reads on every command (14 commands)
- **File:** `src-tauri/src/config.rs:864-993`
- **Problem:** `load_json_config()` reads disk + deserializes JSON on every call. No in-memory cache for `NotificationConfig`, `UIPrefsConfig`, `RepoSettingsMap`, etc.
- **Fix:** Cache in `AppState` with invalidation on save, same pattern as `AppConfig`.

### P3-22: PTY parsed events serialized twice
- **File:** `src-tauri/src/pty.rs:910-923`
- **Problem:** `serde_json::to_value(emit_event)` for broadcast bus, then `app.emit()` serializes again for Tauri IPC.
- **Fix:** Pre-serialize once, pass raw JSON to both consumers.

### P3-23: `AppEvent::PtyParsed` cloned per broadcast subscriber
- **File:** `src-tauri/src/pty.rs`
- **Problem:** `AppEvent` carrying `serde_json::Value` is cloned per subscriber. For large parsed events (file diffs, long outputs), `Value::clone()` is expensive.
- **Fix:** Wrap payload in `Arc<serde_json::Value>` — clone becomes ref-count increment.

---

## Implementation Order

Grouped by wave — items within a wave can be parallelized.

### Wave 1: Critical Blocking (P1) — Do First
1. `update_from_base` → async + spawn_blocking
2. `hash_password` → async + spawn_blocking

### Wave 2: High-Impact Backend (P2)
3. Batch-convert remaining ~20 sync git commands to async + spawn_blocking
4. Replace `ps` subprocess with `proc_pidinfo` syscall (macOS) / `/proc/pid/comm` (Linux)
5. MCP upstream: Semaphore instead of Mutex for concurrent calls
6. `get_changed_files`: merge two subprocess calls into one

### Wave 3: Frontend Quick Wins (P2)
7. Add `manualChunks` to vite config
8. Lazy-load heavy panels with `lazy()` + `<Suspense>`
9. Guard StatusBar 1s timer — only active when conditions met
10. PTY output batching (Rust side: 5-10ms window)

### Wave 4: Medium Optimizations (P3)
11. Remove `{ equals: false }` in ActivityDashboard
12. `debouncedBusy` inside `batch()`
13. Cache config disk reads in AppState
14. Pre-serialize PTY parsed events once
15. Increase PTY read buffer to 32KB
16. `Arc<str>` for session_id in hot paths

### Wave 5: Investigation Required (profile first)
17. xterm atlas rebuild frequency — profile frame drop impact
18. `pluginRegistry.processRawOutput` overhead — measure with/without plugins
19. Terminal store granularity — SolidJS DevTools signal tracking
20. `get_all_pr_statuses` GraphQL payload size with many PRs
21. `useAppInit` snapshot serialization cost at 30s interval

---

## Profiling Toolkit

| Layer | Tool | Purpose |
|-------|------|---------|
| Rust CPU | `samply record ./target/release/app` | Flamegraph, identify CPU hotspots |
| Tokio runtime | `tokio-console` (add `console-subscriber`) | Task scheduling, blocked futures |
| Tauri IPC | CrabNebula DevTools | IPC call frequency, payload sizes |
| JS rendering | Chrome DevTools > Performance | Long tasks, layout thrashing |
| SolidJS reactivity | Solid DevTools extension | Signal graph, memo re-evaluation count |
| Memory | Chrome DevTools > Memory > Heap Snapshot | Leak detection, growth patterns |
| WebGL | Chrome `about:gpu` + Performance tab | xterm renderer performance |

---

## Acceptance Criteria

- [ ] Zero P1 blocking commands (all subprocess/CPU-heavy commands use spawn_blocking)
- [ ] Agent polling uses syscall instead of subprocess fork
- [ ] StatusBar timer idle when no active conditions
- [ ] Bundle split into ≥4 chunks (main, xterm, codemirror, markdown)
- [ ] Heavy panels lazy-loaded
- [ ] PTY output batched (measurable: <50 IPC events/sec during continuous agent output)
- [ ] MCP upstream supports concurrent tool calls
- [ ] Profiling baseline established for Waves 4-5

## Wave 5 Investigation Results (2026-03-28)

### IPC Baseline (20 iterations, Unix socket)

| Endpoint | p50 | p95 | Verdict |
|----------|-----|-----|---------|
| panel_context | 0.2ms | 2.8ms | Healthy (cache hit) |
| remote_url | 0.2ms | 0.3ms | Healthy (file read) |
| stash_list | 5.4ms | 7.6ms | Healthy |
| branches | 6.7ms | 13.9ms | Healthy |
| recent_commits | 6.7ms | 13.3ms | Healthy |
| diff_stats | 18.1ms | 27.8ms | Acceptable |
| repo_info | 27.2ms | 57.8ms | Acceptable (git status) |
| changed_files | 30.8ms | 49.7ms | Acceptable (git diff) |

### Investigation Item Decisions

| Item | Decision | Rationale |
|------|----------|-----------|
| xterm atlas rebuild (5min) | **Keep** | Workaround for xterm.js WebGL atlas corruption with diverse Unicode. Only runs on visible terminals. Removing causes visual glitches. |
| pluginRegistry.processRawOutput | **No action** | Fast path already exists: `if (outputWatchers.length === 0) return;` — zero overhead when no plugins loaded. |
| Terminal store granularity | **No action** | SolidJS path-level reactivity is already granular. `get(id)` returns a store proxy, not a clone. Only fields actually read create subscriptions. |
| GraphQL PR payload size | **No action** | Payload is proportional to open PRs. Typical repos: <5KB. Not a bottleneck. |
| useAppInit snapshot cost | **No action** | Iterates repos × branches × terminals every 30s. Typical: ~50-75 objects. Cost: microseconds. |
| debouncedBusy batch() | **Already done** | `handleShellStateChange` is called inside the `batch()` in `update()`. The setTimeout callback for idle→busy runs outside batch, but that's intentional (delayed). |

### Profiling Infrastructure

Scripts in `scripts/perf/`:
- `setup.sh` — install tools
- `bench-ipc.sh` — IPC latency via Unix socket
- `bench-pty.sh` — PTY throughput guide
- `record-cpu.sh` — CPU flamegraph via samply
- `record-tokio.sh` — live Tokio task inspector
- `snapshot-memory.sh` — heap snapshot scenarios
- `run-all.sh` — automated benchmark suite

Cargo feature `tokio-console` available for async task inspection.
