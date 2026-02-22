# Code Review: Daily Refactoring — 27 Commits (2026-02-21)
**Date:** 2026-02-21
**Reviewers:** Multi-Agent (security, performance, architecture, simplicity, silent-failure, test-quality, typescript, rust)
**Target:** All commits from 2026-02-21 (`db7b79b..82e4f86`) — 93 files, +8701/-2331

## Summary
- **P1 Critical Issues:** 2
- **P2 Important Issues:** 12
- **P3 Nice-to-Have:** 8
- **Confidence Threshold:** 70
- **Filtered Out (below threshold):** 2 findings

---

## P1 - Critical (Must Fix)

- [ ] **#1 [SILENT-FAIL / TS]** `plugin.onload()` has no try/catch — throwing plugin crashes app startup `src/plugins/pluginRegistry.ts:108` (Confidence: 92)
  - Issue: `onload(host)` is the only unguarded call in the registry. A plugin that throws during `onload` propagates to `initPlugins()` → `onMount()` in App.tsx, crashing the entire app. Dispose and dispatchLine are both guarded — onload is not.
  - Fix: Wrap in try/catch, clean up partial disposables, log error, return without registering
  - Agents: typescript-reviewer, silent-failure-hunter

- [ ] **#2 [MEMORY-LEAK]** `lineBuffers` Map grows unboundedly — never cleaned on session close `src/plugins/pluginRegistry.ts:30` (Confidence: 95)
  - Issue: `processRawOutput` creates a `LineBuffer` per `sessionId` and stores it in a Map, but nothing ever removes entries when PTY sessions exit. Accumulates stale entries over the app's lifetime.
  - Fix: Add `removeSession(sessionId)` to pluginRegistry API; call from Terminal.tsx `onCleanup`
  - Agents: architecture-reviewer, performance-reviewer

---

## P2 - Important (Should Fix)

- [ ] **#3 [SILENT-FAIL]** `dispatchStructuredEvent` has no per-handler try/catch — one bad handler kills all subsequent handlers `src/plugins/pluginRegistry.ts:178-184` (Confidence: 88)
  - Issue: `dispatchLine` correctly wraps each watcher in try/catch with console.error. `dispatchStructuredEvent` does not — asymmetric and dangerous. A throw in planPlugin's handler aborts all remaining handlers.
  - Fix: Add per-handler try/catch matching `dispatchLine`'s pattern
  - Agents: silent-failure-hunter, test-quality-reviewer

- [ ] **#4 [TS-TYPE]** Double `as` cast in Toolbar bypasses discriminated union narrowing `src/components/Toolbar/Toolbar.tsx:178-183` (Confidence: 95)
  - Issue: Inside `<Show when={src().kind === "activity"}>`, `src()` is cast with `as` to access `.item`. Should use keyed `<Show>` with pre-narrowed accessor for proper type narrowing.
  - Fix: Extract narrowed accessors (`activitySrc`, `prSrc`) and use `<Show when={...} keyed>`
  - Agent: typescript-reviewer

- [ ] **#5 [TS-TYPE]** Unsafe `payload as { path: string }` cast without validation `src/plugins/planPlugin.ts:83` (Confidence: 88)
  - Issue: Structured event handler receives `payload: unknown` and casts without guard. If payload is malformed, `path.split("/")` throws.
  - Fix: Add runtime type guard before cast
  - Agents: typescript-reviewer, test-quality-reviewer

- [ ] **#6 [REACTIVITY]** `buildNavItems()` is not reactive — stale nav when repos change while Settings panel is open `src/components/SettingsPanel/SettingsPanel.tsx:119` (Confidence: 90)
  - Issue: Called as plain expression in JSX (`tabs={buildNavItems()}`), evaluated once at mount. Adding a repo while Settings is open won't update the nav.
  - Fix: Wrap in signal getter: `const navItems = () => buildNavItems();` then `tabs={navItems()}`
  - Agent: typescript-reviewer

- [ ] **#7 [PERF]** `setSettingsNavWidth` saves to disk on every mousemove pixel during drag `src/components/SettingsPanel/SettingsShell.tsx:42` (Confidence: 95)
  - Issue: `onMove` handler calls `props.onNavWidthChange()` which triggers `uiStore.setSettingsNavWidth()` → `saveUIPrefs()` IPC on every pixel. At 30-60Hz during drag.
  - Fix: Separate "update reactive state" (every pixel) from "persist" (on mouseup only)
  - Agent: performance-reviewer

- [ ] **#8 [RUST]** `UIPrefsConfig` derives Default but serde defaults diverge — new panel widths default to 0 `src-tauri/src/config.rs:301` (Confidence: 92)
  - Issue: `#[derive(Default)]` gives `sidebar_visible: false`, panel widths: 0. But `#[serde(default = "...")]` gives non-zero values. Any code calling `UIPrefsConfig::default()` directly gets wrong defaults.
  - Fix: Replace `#[derive(Default)]` with manual `impl Default` matching serde defaults
  - Agent: rust-reviewer

- [ ] **#9 [RUST]** Cursor rate-limit pattern uses plain English phrase, violating stated design rule `src-tauri/src/output_parser.rs:145` (Confidence: 85)
  - Issue: Comment block says "NEVER match plain English phrases." The new `cursor-rate-limit` pattern matches `"User Provided API Key Rate Limit Exceeded"` — a sentence.
  - Fix: Verify this comes from structured error output; document source; or anchor to JSON context
  - Agent: rust-reviewer

- [ ] **#10 [SILENT-FAIL]** `repoDefaultsStore.save()` uses `console.debug` for write failures — invisible in production `src/stores/repoDefaults.ts:33` (Confidence: 85)
  - Issue: Save failures are logged at `console.debug` level — hidden by default in DevTools and absent in production. User's changes appear to take effect but are not persisted.
  - Fix: Change to `console.error` for save failures (hydration can stay as debug)
  - Agent: silent-failure-hunter

- [ ] **#11 [SECURITY]** `innerHTML` with plugin-supplied SVG strings has no sanitization `src/components/Toolbar/Toolbar.tsx:178, 265` (Confidence: 82)
  - Issue: Plugin `icon` strings rendered via `innerHTML`. Currently safe (hardcoded SVGs) but no type or runtime enforcement preventing future misuse.
  - Fix: Add branded `TrustedSvgString` type, or document trust constraint in `types.ts`
  - Agents: security-reviewer, typescript-reviewer

- [ ] **#12 [CROSS-PLATFORM]** `LineBuffer` does not strip `\r` — plugin regex breaks on Windows PTY output `src/utils/lineBuffer.ts:18` (Confidence: 82)
  - Issue: Splits on `\n` only. Windows PTY emits `\r\n`. Trailing `\r` breaks `$`-anchored regex patterns in plugins.
  - Fix: `return parts.map(line => line.endsWith("\r") ? line.slice(0, -1) : line);`
  - Agent: typescript-reviewer

- [ ] **#13 [SIMPLICITY]** `dispatchLine` exposed publicly but only used by tests — leaks internal bypassing line buffering `src/plugins/pluginRegistry.ts:193` (Confidence: 90)
  - Issue: Production code only calls `processRawOutput`. `dispatchLine` exposes a path that bypasses line-buffering. Tests should go through `processRawOutput`.
  - Fix: Remove from public API surface; test through `processRawOutput`
  - Agent: simplicity-reviewer

- [ ] **#14 [SIMPLICITY]** Provider stacking in markdownProviderRegistry is YAGNI `src/plugins/markdownProviderRegistry.ts:15` (Confidence: 88)
  - Issue: `Map<string, MarkdownProvider[]>` stack-per-scheme. Only 2 providers exist, one per scheme. A plain `Map<string, MarkdownProvider>` suffices.
  - Fix: Simplify to `Map<string, MarkdownProvider>`
  - Agent: simplicity-reviewer

---

## P3 - Nice-to-Have

- [ ] **#15** `pluginId` on `ActivityItem` stored but never read anywhere `src/plugins/types.ts:44` (Conf: 85)
- [ ] **#16** `updateItem` on PluginHost/activityStore has no production caller `src/plugins/types.ts:150` (Conf: 82)
- [ ] **#17** Double-dispose in plugin classes — `onunload()` redundant with registry dispose `planPlugin.ts, wizStoriesPlugin.ts` (Conf: 80)
- [ ] **#18** Settings tab-key encoding uses magic strings instead of discriminated union `SettingsShell.tsx` (Conf: 75)
- [ ] **#19** Floating promise: `updaterStore.checkForUpdate()` in GeneralTab `GeneralTab.tsx:118` (Conf: 90)
- [ ] **#20** Default export alongside named export in SettingsPanel `SettingsPanel.tsx:160` (Conf: 95)
- [ ] **#21** Error messages in MCP HTTP routes leak raw filesystem details `config_routes.rs:39-42` (Conf: 80)
- [ ] **#22** `save_repositories` accepts opaque `serde_json::Value` with no schema validation `config_routes.rs:120-131` (Conf: 80)

---

## Cross-Cutting Analysis

### Root Causes Identified

| Root Cause | Findings Affected | Suggested Fix |
|------------|-------------------|---------------|
| Missing error isolation in plugin dispatch | #1, #3, #5 | Wrap all plugin-called code in try/catch consistently |
| No session lifecycle cleanup in pluginRegistry | #2 | Add `removeSession()` API, call from Terminal.tsx |
| innerHTML without trust boundary | #4, #11 | Branded TrustedSvgString type or DOMParser sanitization |
| Inconsistent save-failure logging | #10 | Standardize: `console.error` for writes, `console.debug` for reads |
| Cross-platform line endings | #12 | Strip `\r` in LineBuffer.push() |

### Single-Fix Opportunities

1. **Plugin error isolation** — Wrapping `onload` + `dispatchStructuredEvent` + payload validation fixes findings #1, #3, #5 (~20 lines)
2. **Session cleanup** — `removeSession()` + Terminal.tsx call fixes #2 (~5 lines)
3. **LineBuffer `\r` strip** — One `.map()` call fixes #12 (~1 line)
4. **Settings nav reactivity** — Signal wrapper fixes #6 (~1 line)
5. **Drag persist on mouseup** — Move saveUIPrefs to onUp handler fixes #7 (~3 lines)

### Context Files (Read Before Fixing)

| File | Reason | Referenced By |
|------|--------|---------------|
| `src/plugins/pluginRegistry.ts` | Core of findings #1-3, #13 | all reviewers |
| `src/components/Toolbar/Toolbar.tsx` | innerHTML + type casts #4, #11 | typescript, security |
| `src/plugins/types.ts` | Type contracts for icon, pluginId | typescript, simplicity |
| `src-tauri/src/config.rs` | UIPrefsConfig defaults #8 | rust |
| `src/components/Terminal/Terminal.tsx` | Session lifecycle for #2 | architecture, performance |

---

## Recommended Actions

1. **Immediate:** Fix P1 items (#1 plugin onload crash, #2 lineBuffer leak) — both are ~10 lines
2. **This session:** Address P2 items, especially #3, #6, #7, #8, #12 — all quick fixes
3. **Follow-up:** P3 items can be tracked as stories

## Agent Highlights

- **Security:** Latent XSS in innerHTML icon rendering; path traversal defended by Rust canonicalize
- **Performance:** IPC storm on settings nav drag; lineBuffer leak; activity store queries could be createMemo
- **Architecture:** Plugin system is well-designed; lineBuffer leak is the main structural issue; wizStoriesPlugin bypasses PluginHost by importing stores directly
- **Simplicity:** Plugin system is appropriate complexity; ~50 lines of dead/unused API surface
- **Silent Failures:** dispatchStructuredEvent unguarded; save failures at console.debug; catch-null patterns need logging
- **Test Quality:** Strong new test coverage; key gap is dispatchStructuredEvent error isolation test
- **TypeScript:** Non-reactive buildNavItems; unsafe payload casts; LineBuffer \r on Windows
- **Rust:** UIPrefsConfig Default/serde divergence; cursor rate-limit plain-English violation

---

**Dependency changes detected.** Consider running vulnerability scan:
- TypeScript: `npm audit` / `pnpm audit`
- Rust: `cargo audit`
