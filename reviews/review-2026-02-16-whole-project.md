# Code Review: TUICommander - Whole Project
**Date:** 2026-02-16
**Reviewers:** Multi-Agent (security, performance, architecture, simplicity, silent-failure, test-quality, typescript, rust, data-safety)
**Target:** Entire codebase (~39K lines, 172 files)
**Branch:** develop

## Summary
- **P1 Critical Issues:** 14
- **P2 Important Issues:** 20
- **P3 Nice-to-Have:** 8
- **Confidence Threshold:** 70
- **Filtered Out (below threshold):** 3

---

## P1 - Critical (Must Fix)

### Security

- [ ] **[SEC-01] Command injection via git branch names** (Confidence: 95)
  - Location: `src-tauri/src/lib.rs:1269`
  - Issue: Branch names interpolated into git commands without validation. A branch named `main; rm -rf /` executes arbitrary commands.
  - Fix: Use `Command::arg()` instead of string interpolation for all git operations
  - Agent: security-reviewer

- [ ] **[SEC-02] Command injection on Windows terminal path** (Confidence: 95)
  - Location: `src-tauri/src/lib.rs:1417`
  - Issue: Path concatenated into shell command string. `C:\temp & calc.exe` executes arbitrary code.
  - Fix: Split command arguments instead of string concatenation
  - Agent: security-reviewer

- [ ] **[SEC-03] XSS in Markdown renderer** (Confidence: 95)
  - Location: `src/components/ui/MarkdownRenderer.tsx:41`
  - Issue: `innerHTML` used with `marked` output without sanitization. `marked` does not sanitize HTML by default.
  - Fix: Add DOMPurify sanitization before `innerHTML` assignment
  - Agent: security-reviewer

- [ ] **[SEC-04] MCP auth bypass via localhost** (Confidence: 90)
  - Location: `src-tauri/src/mcp_http.rs:1267-1274`
  - Issue: Localhost connections bypass authentication entirely. SSH tunneling or DNS rebinding can exploit this.
  - Fix: Remove localhost exception when remote access is enabled
  - Agent: security-reviewer

### Architecture

- [ ] **[ARCH-01] App.tsx is a 1771-line God Object** (Confidence: 95)
  - Location: `src/App.tsx:1-1771`
  - Issue: Manages terminals, git ops, keyboard shortcuts, split panes, settings, dialogs, and 13 stores. Violates SRP. Untestable.
  - Fix: Extract TerminalOrchestrator, GitOrchestrator, KeyboardManager as separate modules
  - Agent: architecture-reviewer

- [ ] **[ARCH-02] lib.rs is a 2992-line monolith** (Confidence: 98)
  - Location: `src-tauri/src/lib.rs:1-2992`
  - Issue: Combines PTY management, git operations, GitHub API, worktree lifecycle, file I/O, agent detection, and 60+ Tauri commands.
  - Fix: Split into pty.rs, git.rs, worktree.rs, agents.rs modules. lib.rs becomes thin glue.
  - Agent: architecture-reviewer

- [ ] **[ARCH-03] Business logic in frontend violates hexagonal architecture rule** (Confidence: 90)
  - Location: `src/stores/github.ts:90-93`, `src/stores/repositories.ts:36`
  - Issue: Exponential backoff calculation, string transformation in stores. CLAUDE.md states "All business logic MUST be in Rust."
  - Fix: Move business logic to Rust backend, frontend becomes pure rendering layer
  - Agent: architecture-reviewer

### Rust

- [ ] **[RS-01] Blocking PTY operations in async handlers** (Confidence: 92)
  - Location: `src-tauri/src/mcp_http.rs:843`
  - Issue: Synchronous PTY syscalls (openpty, spawn) block the tokio runtime in async handlers.
  - Fix: Wrap in `tokio::task::spawn_blocking()`
  - Agent: rust-reviewer

- [ ] **[RS-02] Panic on serialization failure in MCP bridge** (Confidence: 91)
  - Location: `src-tauri/src/bin/tui_mcp_bridge.rs:571`
  - Issue: `.expect()` panics if JSON serialization fails. Crashes the bridge process.
  - Fix: Return JSON-RPC error response instead of panicking
  - Agent: rust-reviewer

### Data Safety

- [ ] **[DATA-01] Config file race condition (read-modify-write)** (Confidence: 95)
  - Location: `src-tauri/src/config.rs:26-38`, `src/stores/settings.ts:268-278`
  - Issue: Non-atomic file writes + concurrent store saves = last-write-wins data loss. Two settings changed simultaneously → one overwritten.
  - Fix: Atomic write (temp file + rename) in Rust; debounce/serialize concurrent saves in frontend
  - Agent: data-safety-reviewer

- [ ] **[DATA-02] PII: Password hash in world-readable config file** (Confidence: 90)
  - Location: `src-tauri/src/config.rs:71`
  - Issue: Bcrypt hash stored in `~/.tui-commander/config.json` with default 0644 permissions. Readable by any local user.
  - Fix: Set 0600 permissions on config files; ideally use OS keychain
  - Agent: data-safety-reviewer

### Silent Failures

- [ ] **[SF-01] Promise.all swallows store hydration failures** (Confidence: 90)
  - Location: `src/App.tsx:212-221`
  - Issue: If any store fails to hydrate, app proceeds with defaults silently. User's settings/repos disappear without warning.
  - Fix: Use `Promise.allSettled()`, warn user on partial failures
  - Agent: silent-failure-hunter

- [ ] **[SF-02] MCP HTTP server panics on port conflict** (Confidence: 95)
  - Location: `src-tauri/src/mcp_http.rs:1507`
  - Issue: `.expect()` on address binding panics the entire app if port is in use.
  - Fix: Return error gracefully, allow user to configure different port
  - Agent: silent-failure-hunter

---

## P2 - Important (Should Fix)

### Performance

- [ ] **[PERF-01] N+1 persistence: every state mutation saves to disk** (Confidence: 95)
  - Location: `src/stores/repositories.ts:50-62` (11 call sites)
  - Issue: Every branch stat update, terminal reorder, collapse toggle triggers full serialization + IPC. 10 repos × 5 branches = 50+ disk writes/sec during refresh.
  - Fix: Debounce `saveRepos()` with 500ms delay; stats are ephemeral, don't persist them
  - Agent: performance-reviewer

- [ ] **[PERF-02] Sequential git operations during branch refresh** (Confidence: 90)
  - Location: `src/App.tsx:132-167`
  - Issue: Iterates repos then branches sequentially. 5 repos × 4 branches = 20 serial git calls (~800ms).
  - Fix: Use `Promise.all()` for both loops → ~80ms
  - Agent: performance-reviewer

- [ ] **[PERF-03] Terminal activity update per PTY byte chunk** (Confidence: 88)
  - Location: `src/components/Terminal/Terminal.tsx:138-140`
  - Issue: Every PTY data chunk updates store → triggers SolidJS reactivity. 100+ chunks/sec during builds.
  - Fix: Throttle activity flag to max 10Hz
  - Agent: performance-reviewer

- [ ] **[PERF-04] Missing memoization on terminalIds()** (Confidence: 85)
  - Location: `src/App.tsx:1356-1364`
  - Issue: O(n²) filter called on every TabBar render. Not wrapped in `createMemo()`.
  - Fix: Wrap in `createMemo()`
  - Agent: performance-reviewer

### Rust Idioms

- [ ] **[RS-03] std::thread::sleep in async context** (Confidence: 90)
  - Location: `src-tauri/src/mcp_http.rs:214`
  - Issue: Blocks tokio executor. Should use `tokio::time::sleep`.
  - Agent: rust-reviewer

- [ ] **[RS-04] Lossy string conversion on paths (to_string_lossy)** (Confidence: 88)
  - Location: `src-tauri/src/lib.rs:643` (10+ occurrences)
  - Issue: Silently replaces invalid UTF-8 with `�`. Should fail early with clear error.
  - Agent: rust-reviewer

- [ ] **[RS-05] Result<T, String> instead of proper error types** (Confidence: 82)
  - Location: `src-tauri/src/lib.rs:254` (most functions)
  - Issue: String errors lose context and can't be programmatically handled.
  - Fix: Use `thiserror` for structured errors
  - Agent: rust-reviewer

- [ ] **[RS-06] &PathBuf instead of &Path in function signatures** (Confidence: 87)
  - Location: `src-tauri/src/lib.rs:251` (many occurrences)
  - Issue: Non-idiomatic. Always prefer `&Path` over `&PathBuf`.
  - Agent: rust-reviewer

### Silent Failures

- [ ] **[SF-03] WebSocket close doesn't distinguish normal vs error** (Confidence: 95)
  - Location: `src/transport.ts:207-209`
  - Issue: `onclose` handler calls `onExit()` regardless of close code. Network failures invisible.
  - Fix: Check `event.code !== 1000` for abnormal closure, log details
  - Agent: silent-failure-hunter

- [ ] **[SF-04] 38 catch blocks only console.log without user feedback** (Confidence: 85)
  - Location: Multiple files (hooks, stores, components)
  - Issue: Users never see errors unless DevTools is open.
  - Fix: Surface critical failures via toast/status bar notifications
  - Agent: silent-failure-hunter

- [ ] **[SF-05] Git operations return empty defaults on failure** (Confidence: 85)
  - Location: `src/hooks/useRepository.ts:71-83`
  - Issue: Callers can't distinguish "no results" from "operation failed".
  - Fix: Throw errors instead of returning empty arrays
  - Agent: silent-failure-hunter

- [ ] **[SF-06] Branch stat refresh failures silently ignored** (Confidence: 90)
  - Location: `src/App.tsx:159-165`
  - Issue: Empty catch block. Users see stale/missing diff stats without knowing why.
  - Fix: Log failure count, display warning for persistent failures
  - Agent: silent-failure-hunter

### Simplicity

- [ ] **[SIMP-01] Config explosion: 7 config files with duplicate load/save patterns** (Confidence: 88)
  - Location: `src-tauri/src/config.rs:275-359`
  - Issue: 7 nearly identical command pairs. Should be generic.
  - Fix: Single generic config loader parameterized by type + filename
  - Agent: simplicity-reviewer

- [ ] **[SIMP-02] Store hydration pattern duplicated in 8 stores** (Confidence: 95)
  - Location: All stores with `hydrate()` methods
  - Issue: Identical localStorage migration + Tauri load + fallback pattern repeated 8 times.
  - Fix: Extract to shared `hydrateStore<T>()` utility
  - Agent: simplicity-reviewer

- [ ] **[SIMP-03] Unused tasksStore (244 lines of dead code)** (Confidence: 98)
  - Location: `src/stores/tasks.ts`
  - Issue: No references outside tests and barrel export. Never used in app.
  - Fix: Remove store and its tests (verify with Boss first)
  - Agent: simplicity-reviewer

### TypeScript

- [ ] **[TS-01] Array.sort() mutation in promptLibrary** (Confidence: 100)
  - Location: `src/stores/promptLibrary.ts:244`
  - Issue: `prompts.sort()` mutates in-place. Should copy first: `[...prompts].sort()`.
  - Agent: typescript-reviewer

### Data Safety

- [ ] **[DATA-03] Config corruption silently returns defaults** (Confidence: 88)
  - Location: `src-tauri/src/config.rs:13-23`
  - Issue: Malformed UTF-8 or JSON parse errors silently fall back to `Default`. User loses all config without notification.
  - Fix: Return Result with error details, notify user
  - Agent: data-safety-reviewer

### Test Quality

- [ ] **[TEST-01] Component tests mock everything, test mock behavior** (Confidence: 92)
  - Location: `src/__tests__/components/Sidebar.test.tsx` and others
  - Issue: Violates project rule "NEVER write tests that test mocked behavior". Tests pass even when integration is broken.
  - Fix: Use real stores like terminals.test.ts does
  - Agent: test-quality-reviewer

---

## P3 - Nice-to-Have

- [ ] **[SIMP-04] 23 barrel index.ts files for single-export modules** (Confidence: 98)
  - Agent: simplicity-reviewer

- [ ] **[SIMP-05] Wrapper stores duplicate class APIs (errorHandling, notifications)** (Confidence: 95)
  - Agent: simplicity-reviewer

- [ ] **[RS-07] Ignored git cleanup errors with `let _`** (Confidence: 85)
  - Location: `src-tauri/src/lib.rs:343`
  - Agent: rust-reviewer

- [ ] **[RS-08] Arithmetic overflow in random generation** (Confidence: 82)
  - Location: `src-tauri/src/lib.rs:2202`
  - Agent: rust-reviewer

- [ ] **[TS-02] Catch clauses not explicitly typed as `unknown` (372 occurrences)** (Confidence: 95)
  - Fix: Enable `useUnknownInCatchVariables` in tsconfig
  - Agent: typescript-reviewer

- [ ] **[TS-03] Test files use `as any` instead of proper typing** (Confidence: 92)
  - Location: Multiple test files
  - Agent: typescript-reviewer

- [ ] **[TEST-02] Missing tests for diffTabsStore and mdTabsStore** (Confidence: 98)
  - Location: src/stores/diffTabs.ts (121L), src/stores/mdTabs.ts (120L)
  - Agent: test-quality-reviewer

- [ ] **[TEST-03] App.tsx has zero test coverage (1771 LOC)** (Confidence: 95)
  - Agent: test-quality-reviewer

---

## Cross-Cutting Analysis

### Root Causes Identified

| Root Cause | Findings Affected | Suggested Fix |
|------------|-------------------|---------------|
| **Monolithic files** | ARCH-01, ARCH-02, SIMP-01, SIMP-02 | Split App.tsx and lib.rs into focused modules; extract shared patterns |
| **Non-atomic config I/O** | DATA-01, DATA-02, DATA-03 | Implement atomic write (temp+rename) with proper permissions in config.rs |
| **Console-only error reporting** | SF-01, SF-04, SF-05, SF-06 | Add global error notification system (toast/status bar) |
| **Missing input validation** | SEC-01, SEC-02, SEC-03 | Add validation layer at Tauri command boundary |
| **Blocking in async contexts** | RS-01, RS-03 | Audit all async handlers for blocking calls, wrap in spawn_blocking |

### Single-Fix Opportunities

1. **Atomic config writer** - Fixes DATA-01, DATA-02, DATA-03 (~30 lines in config.rs)
2. **Input sanitization middleware** - Fixes SEC-01, SEC-02 (~50 lines)
3. **Store hydration utility** - Fixes SIMP-02, SF-01 (~40 lines)
4. **Global error toast system** - Fixes SF-04, SF-05, SF-06 (~60 lines)
5. **DOMPurify integration** - Fixes SEC-03 (~5 lines)

### Context Files (Read Before Fixing)

| File | Reason | Referenced By |
|------|--------|---------------|
| `src-tauri/src/config.rs` | Config I/O patterns, race conditions, permissions | data-safety, simplicity, security |
| `src/stores/repositories.ts` | N+1 persistence pattern, save frequency | performance, data-safety, simplicity |
| `src/transport.ts` | Transport abstraction (underused) | architecture, silent-failure |
| `src/__tests__/stores/terminals.test.ts` | Exemplary test pattern to follow | test-quality |
| `src/components/ui/MarkdownRenderer.tsx` | XSS vulnerability | security |

---

## Agent Highlights

- **Security:** 4 critical vulns - command injection (2), XSS, auth bypass. All exploitable.
- **Performance:** N+1 persistence + sequential git ops = visible UI jank at scale. Surgical fixes available.
- **Architecture:** Two God Objects (App.tsx 1771L, lib.rs 2992L) are the #1 maintainability risk. Business logic leaks into frontend.
- **Simplicity:** ~500 lines removable. Tasks store is dead code. Config system over-engineered (7 files, identical patterns).
- **Silent Failures:** 4 P1s. Store hydration, WebSocket errors, port conflicts all fail silently. Users lose data without knowing.
- **Test Quality:** Component tests mock too much. Two stores untested. App.tsx (1771L) has zero coverage.
- **TypeScript:** Clean codebase overall. One mutation bug (sort), catch clauses need `unknown` typing.
- **Rust:** Blocking in async handlers is the top concern. String-based errors, lossy path conversion throughout.
- **Data Safety:** Race conditions in config writes will cause data loss. Password hash exposed in world-readable file.

---

## Recommended Actions

1. **Immediate:** Fix SEC-01, SEC-02, SEC-03 (command injection + XSS) - these are exploitable
2. **This week:** Fix DATA-01 (atomic config writes), SF-02 (panic on port conflict), RS-01 (async blocking)
3. **Next sprint:** Split App.tsx and lib.rs (ARCH-01, ARCH-02) - prevents further sprawl
4. **Backlog:** Address PERF-01 (debounce saves), TEST-01 (fix mock-heavy tests), SIMP-02 (extract shared patterns)

---

## Next Steps

**What would you like to do?**

| Option | When to Use |
|--------|-------------|
| **Triage** (`/wiz:triage`) | Create stories, prioritize, track progress |
| **Work directly** | Fix the highest-priority issues now |
| **Explain** any finding in detail | |
| **Generate fix** for a specific issue | |
