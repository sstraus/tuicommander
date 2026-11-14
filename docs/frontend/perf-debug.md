# Frontend Performance Instrumentation (perfDebug)

These rules moved here from `AGENTS.md` to keep that file inside the
32,768-byte project-instruction budget. They stay binding for every agent;
`AGENTS.md` keeps a summary and a link to this page.

## Frontend performance instrumentation (`perfDebug`)

The frontend counterpart to backend Diagnostics. **One master flag gates ALL frontend perf/debug instrumentation** — `isPerfDebug()` from `src/utils/perfDebug.ts`.

- **Default = `import.meta.env.DEV`** → active in dev, **dormant in release**. We ship a quiet binary; we never distribute hyper-logging.
- **Runtime-toggleable, NOT tree-shaken** — a release build can be woken up to diagnose a field issue:
  ```js
  window.__TUIC__.setPerfDebug(true)   // persists to localStorage; starts the freeze detector
  window.__TUIC__.perfDebug()          // read current state
  ```
  (Run via the WebView devtools / MCP `debug invoke_js`. After toggling on in a build that started dormant, the freeze detector is (re)started automatically.)
- **Dormant cost:** a single boolean read at each entry point — negligible even per-frame.

**What it gates** (all in `src/utils/`): `markPerf`, `timeSync`, `timeBatch` (`perfTrace.ts`), `noteFrameRequest` (frame-burst detector), and `startFreezeDetector` (`freezeDetector.ts`). `frameTiming.ts` is a **heavy opt-in sub-harness subordinate to this master gate** — it has its own local enable (`__terminalFrameTiming.enable(true)`), but cannot record unless `perfDebug` is also on.

**RULE for all future perf/debug instrumentation:** gate it on `isPerfDebug()` (or route it through a `perfTrace` helper, which already does). **Never ship always-on perf logging or per-frame timing.** Do not invent a second on/off flag — extend this one.

**Reading the output** (only present when active): `appLogger.warn` lines like `SLOW <label>: <n>ms` (and `SLOW git.refreshBatch:<repo>: <n>ms (body Xms + flush Yms)` — body = our setState loop, flush = dependent effects/memos waking), plus `UI freeze: <n>ms main-thread block` carrying a `perfTrace` breadcrumb that names the culprit. Read via `GET http://localhost:9876/logs` (or `:9877` for a worktree build).
