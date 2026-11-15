# Runtime Diagnostics

These rules moved here from `AGENTS.md` to keep that file inside the
32,768-byte project-instruction budget. They stay binding for every agent;
`AGENTS.md` keeps a summary and a link to this page.

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

When enabled, emits health snapshots every 30s and alerts on FD/thread growth trends. Each snapshot includes: CPU% (TUIC-self only, via `RUSAGE_SELF`), `children_cpu` (aggregate %cpu of PTY children + hottest child — the spike trigger deliberately ignores children, so this is the only place a hot `cargo`/agent surfaces when TUIC itself is calm), thread count, FD count, PTY session count, content index build state, semaphore permits, sessions with grid frames outstanding (`GridGate`), event bus subscriber count, `head_emits_suppressed` (repo-watcher `head-changed` emits skipped by the resolved-HEAD-target guard — a high/climbing value signals a filesystem-event storm, issue #82).

**Frontend liveness (always on, desktop only):** the WebView beats every 5s from its main thread (`frontendHeartbeat.ts` → `frontend_heartbeat` → `frontend_liveness.rs`); after six missed beats the diagnostics thread logs **once**:

```
Frontend unresponsive: no heartbeat for 30s — the WebView main thread is blocked or gone.
```

**Whole-process silence is not proof of a frontend-only block.** On 2026-10-20 an occluded agent-test instance without a power assertion showed a 41.6 s callback gap, 40 s backend-log silence, `Sleep-wake detected` lines, ~100 threads at priority 4, and a sync IPC command sampled in `rename(2)` over the gap. Historical attribution remains open. A controlled replay with normal-priority CPU contention reproduced a 36.753 s callback gap; adding the `app_nap.rs` process-lifetime activity alone reduced the maximum to 2.561 s. Independent backend requests still completed in that RED, so it does not reproduce the original backend outage. See `scripts/stall-workload/OCCLUDED-RESULTS.md` (story 1638-c94c).

**Persistent stall evidence:** selected numeric slow-event metadata survives ring overflow and graceful restart in a bounded journal. See `docs/backend/stall-journal.md` for retention, replay inputs and limits; this is not proof of historical stall attribution.

**Lost document (always on, desktop only):** a *second*, different white screen. The `webview-recovery` thread reads the main frame's URL every 15s (`webview_recovery.rs`); anything on the `about:` scheme means the app is no longer in the DOM, and it navigates back to the last healthy URL by itself, logging once. This is not the heartbeat's job and the heartbeat cannot see it: the app is gone rather than blocked, so it never beats from the blank document, and "never beat" is deliberately silent (`tuic-remote` has no WebView).

Observed twice on 2026-09-08, both times after the Mac went to standby with the display off: the main frame came back on **`about:srcdoc`** holding `<html><body></body></html>`, while the WebContent process was alive throughout (so it is *not* the `about:blank` WebContent-crash case) — under the memory-pressure sweep macOS runs while asleep.

Recover manually without losing PTY sessions — they live in the backend, not the WebView:

```bash
curl -X POST http://localhost:9876/debug/reload_webview   # navigates back to the app
```

**That endpoint navigates; it must never go back to `reload()`.** There is no URL behind `about:srcdoc` to reload, so the reload version answered `{"ok":true}` and left the window white for an hour.

**Memory: `GET /diagnostics/memory`** names which structure is holding the process's footprint — entry counts for every map that grows with sessions, clients or repos, measured bytes for the four that hold payloads, sorted biggest first, plus `phys_footprint_bytes` (resident *plus compressed*; `ps` RSS read 0.52 GB while the process held 40 GB). Exists because on 2026-09-08 the backend reached **40.7 GB** and could not be asked what it was holding: `leaks` found only 47 MB unreferenced, so it is live, reachable state — but a 40 GB process is not debuggable and every candidate had to be excluded by reading code. `accounted_bytes` far below the footprint means the growth is outside `AppState`.

Two traps this exists to close. **`grid frame gate stuck` is not a frontend-liveness signal** — a hidden terminal deliberately never acks (`CanvasTerminal.onFrame`), so it fires constantly in normal operation; reading it as "the WebView is wedged" is a false positive on every backgrounded tab. And **`freezeDetector.ts` cannot report a block that never ends**, because its `setInterval` runs on the thread it watches — which is why a five-hour white screen on 2026-09-08 left no frontend log at all and had to be diagnosed from the *absence* of lines. `/debug/invoke_js` is useless in that state for the same reason: it needs the stuck thread to run the script.

**When to enable:** Boss reports sluggishness, CPU spikes, or UI freezes. Enable it, reproduce the issue, then check the logs. The snapshot at the time of the spike tells you what subsystem is overloaded.

**Known past failure patterns this catches:**
- IPC flush loop (ack_terminal_frame sending frames in ack path → 240+ IPC/sec)
- Content index build saturating CPU on large repos
- grid frames outstanding on a session (WebView JS thread blocked)
- FD/thread leak (progressive growth without cleanup)
- Sleep/wake false idle cascades (tokio timers firing stale)
