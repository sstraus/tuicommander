# Code Review: Terminal Resize Debounce + Debug Log Cleanup
**Date:** 2026-02-23
**Reviewers:** Multi-Agent (security, performance, architecture, simplicity, silent-failure, test-quality, typescript, rust)
**Target:** Uncommitted changes on `main` (src-tauri/src/pty.rs, src/components/Terminal/Terminal.tsx)

## Summary
- **P1 Critical Issues:** 2
- **P2 Important Issues:** 6
- **P3 Nice-to-Have:** 5
- **Confidence Threshold:** 70
- **Filtered Out (below threshold):** 2

## P1 - Critical (Block Merge)

- [ ] **[ASYNC-LIFECYCLE]** Corrective-fit timer handle not stored — cannot be cancelled on unmount `Terminal.tsx:755` (Confidence: 92)
  - Issue: The `setTimeout(150ms)` for the corrective fit is anonymous — its handle is never stored. The `onCleanup` at line 829 clears `resizeTimer` and `resizeObserverTimer` but has no way to cancel this timer. If the component unmounts within 150ms, the callback fires on a disposed component, calling `pty.resize()` with a stale `sessionId`.
  - Fix: Store the handle in a `correctiveFitTimer` variable, clear it in `onCleanup`.
  - Agents: typescript-reviewer, architecture-reviewer

- [ ] **[ASYNC-LIFECYCLE]** `requestAnimationFrame` in `createEffect` not cancellable — leaks ResizeObserver observation on rapid toggle `Terminal.tsx:741` (Confidence: 83)
  - Issue: `requestAnimationFrame` returns an ID but it is not stored. If `isVisible()` toggles false before the rAF fires, `onCleanup` runs `resizeObserver?.disconnect()`, then the stale rAF fires and calls `resizeObserver.observe(containerRef)`, restarting observation on a logically-inactive terminal. Store the rAF handle and call `cancelAnimationFrame` in cleanup.
  - Fix: `let rafHandle = requestAnimationFrame(...)` + `cancelAnimationFrame(rafHandle)` in `onCleanup`.
  - Agents: typescript-reviewer, architecture-reviewer

## P2 - Important (Fix Before/After Merge)

- [ ] **[PERF/DUPLICATION]** Corrective fit is redundant with ResizeObserver debounce — causes 2-3 `pty.resize` IPC calls per tab switch `Terminal.tsx:754-764` (Confidence: 92)
  - Issue: On tab switch to an existing session, `resizeObserver.observe()` fires at T+100ms and the corrective `setTimeout` fires at T+150ms. Both call `doFit()` + `pty.resize()`. The ResizeObserver already solves the "flex layout oscillation" problem described in the corrective timer's comment. Up to 3 resize IPC calls fire within 250ms, causing SIGWINCH storms in vim/htop.
  - Fix: Remove the corrective fit block entirely (lines 754-764). The ResizeObserver with 100ms debounce is the authoritative path for layout-driven resize. OR: if you've observed cases where the ResizeObserver alone doesn't suffice, guard with `sessionInitialized` to only fire on reconnects and extend to 200ms.
  - Agents: performance-reviewer, simplicity-reviewer, architecture-reviewer

- [ ] **[LOGIC]** `clearTimeout(resizeTimer)` guarded by `sessionId` — misses cancellation during init `Terminal.tsx:665-667` (Confidence: 85)
  - Issue: `doFit()` at line 662 triggers `terminal.onResize` → sets `resizeTimer`. The cancellation at line 666 is only reached when `sessionId` is truthy. During session init (`sessionId` is null), `resizeTimer` fires uncancelled 150ms later and races with `initSession()`'s own `pty.resize`.
  - Fix: Move `clearTimeout(resizeTimer)` before the `if (sessionId)` guard — always cancel the stale onResize debounce after doFit.
  - Agents: performance-reviewer

- [ ] **[CONSISTENCY]** Missing `offsetHeight` guard in corrective fit `Terminal.tsx:757` (Confidence: 90)
  - Issue: The corrective fit checks `offsetWidth <= 0` but not `offsetHeight <= 0`. The ResizeObserver path at line 661 correctly checks both. A zero-height container passes the corrective guard and calls `doFit()` + `pty.resize()`.
  - Fix: `if (!containerRef || containerRef.offsetWidth <= 0 || containerRef.offsetHeight <= 0) return;`
  - Agents: typescript-reviewer, architecture-reviewer, simplicity-reviewer

- [ ] **[SILENT-FAILURE]** `.catch(() => {})` silently swallows PTY resize errors `Terminal.tsx:667,760` (Confidence: 92)
  - Issue: Both new code paths use `.catch(() => {})` while the existing `terminal.onResize` at line 694 uses `console.error`. Resize failures (e.g., stale session after app restart) leave the terminal in a desync state with no diagnostic output. The `initSession` reconnect path (line 389-397) even uses resize failure as a signal to reset `sessionId`; neither new path has this recovery.
  - Fix: `pty.resize(...).catch((err) => console.error("[Terminal] resize failed:", err))` — or extract a `syncPtyDimensions()` helper with consistent error handling.
  - Agents: silent-failure-hunter

- [ ] **[LISTENER-LEAK]** `unlistenKitty` not cleaned up in reconnect catch block `Terminal.tsx:389-397` (Confidence: 88)
  - Issue: When `pty.resize()` fails during reconnect (session gone), `initSession` cleans up `unsubscribePty` and `unlistenParsed` but NOT `unlistenKitty`. The stale `kitty-keyboard-{oldSessionId}` listener remains attached.
  - Fix: Add `unlistenKitty?.(); unlistenKitty = undefined;` in the catch block.
  - Agents: silent-failure-hunter
  - Note: Pre-existing bug exposed by reviewing the surrounding code.

## P3 - Nice-to-Have

- [ ] **[RUST]** Unnecessary `session_id.clone()` in hot read loop `pty.rs:184` (Confidence: 92)
  - Issue: `DashMap::entry()` requires an owned key, causing a heap allocation on every chunk with kitty sequences. Use `get()` first (returns reference without clone), fall back to `entry()` only on first insertion.
  - Agents: rust-reviewer

- [ ] **[RUST]** `active_sessions` counter underflow risk `pty.rs:286,684` (Confidence: 80)
  - Issue: Both `close_pty` and the reader thread EOF path call `fetch_sub(1)`. If both fire (user closes + process exits simultaneously), the counter wraps to `usize::MAX`. Gate the reader thread's decrement on `sessions.remove()` returning `Some`.
  - Agents: rust-reviewer

- [ ] **[RUST]** Dead `if remaining.is_empty()` reassignment `pty.rs:260-262` (Confidence: 88)
  - Issue: `if remaining.is_empty() { remaining = String::new(); }` — a no-op. Remove it.
  - Agents: rust-reviewer

- [ ] **[DEBUG]** Removed `eprintln!` / `console.debug` reduces kitty protocol debuggability `pty.rs:182, Terminal.tsx:362,474` (Confidence: 85)
  - Issue: Zero observability for kitty keyboard protocol state changes. Consider `tracing::debug!` or `#[cfg(debug_assertions)]` instead of full removal.
  - Agents: silent-failure-hunter, rust-reviewer

- [ ] **[TEST-GAP]** No tests for ResizeObserver debounce, corrective fit, or `doFit` MIN_FIT guards (Confidence: 92)
  - Issue: The two timing behaviors introduced by this diff have zero test coverage. `classify_agent` is also missing tests for 5 of 10 agent mappings (`amp`, `jules`, `cursor-agent`, `oz`, `gitpod`).
  - Agents: test-quality-reviewer

---

## Cross-Cutting Analysis

### Root Causes Identified

| Root Cause | Findings Affected | Suggested Fix |
|------------|-------------------|---------------|
| No shared resize helper | P2 silent-failure, P2 consistency, P2 duplication | Extract `syncPtyDimensions()` with consistent guards and error handling |
| Unmanaged async handles in SolidJS effects | P1 timer leak, P1 rAF leak | Store all timer/rAF handles, cancel in `onCleanup` |
| Corrective fit duplicating ResizeObserver | P2 duplication, P2 consistency, P1 timer leak | Remove corrective fit entirely — ResizeObserver handles it |

### Single-Fix Opportunities

1. **Remove corrective fit block (lines 754-764)** — Fixes P1 timer leak, P2 duplication, P2 consistency (offsetHeight). ~10 lines removed.
2. **Extract `syncPtyDimensions()` helper** — Fixes P2 silent-failure, P2 consistency, P3 pattern drift. ~15 lines added, ~20 removed.
3. **Store rAF handle + cancel in cleanup** — Fixes P1 rAF leak. ~5 lines changed.

### Context Files (Read Before Fixing)

| File | Reason | Referenced By |
|------|--------|---------------|
| `src/hooks/usePty.ts` | `pty.resize()` contract — async RPC, no client-side dedup | performance, silent-failure, architecture |
| `src/stores/terminals.ts` | `terminalsStore.state.activeId`, `layout.panes` — signals driving `isVisible()` | typescript, architecture |
| `src-tauri/src/state.rs` | `AppState`, `KittyKeyboardState`, `DashMap` types | rust-reviewer |

---

## Review Complete

### Findings Summary

| Severity | Count | Status |
|----------|-------|--------|
| P1 Critical | 2 | Must fix — async handle leaks |
| P2 Important | 6 | Recommended — redundancy, consistency, silent failures |
| P3 Nice-to-Have | 5 | Optional — Rust hot-path, debug logging, test gaps |

### Agent Highlights

- **Security:** N/A — no security-sensitive changes
- **Performance:** Redundant corrective fit causes 2-3 `pty.resize` IPC calls per tab switch (SIGWINCH storms)
- **Architecture:** PTY resize now in 3 code paths with diverging guards — extract helper
- **Simplicity:** Corrective fit duplicates ResizeObserver logic; remove it
- **Silent Failures:** `.catch(() => {})` on new resize paths inconsistent with existing error handling; `unlistenKitty` leak in reconnect
- **Test Quality:** Zero coverage for new timing logic; `classify_agent` missing 5 test cases
- **TypeScript:** rAF and setTimeout handles not stored — cleanup cannot cancel them
- **Rust:** Clean one-line change; surrounding code has minor hot-loop clone and counter underflow issues
- **Go/C#/Python/Java/Terraform/Helm/Data Safety:** N/A — no files in scope

### Recommended Priority

The highest-impact fix is **removing the corrective fit block entirely** (lines 754-764). This single action:
- Eliminates the P1 timer leak (no timer = no leak)
- Eliminates the P2 duplicate resize IPC calls
- Eliminates the P2 inconsistent height guard
- Removes ~10 lines of code

The ResizeObserver with 100ms debounce was already designed to solve the layout oscillation problem. The corrective fit was belt-and-suspenders that introduced more bugs than it prevented.
