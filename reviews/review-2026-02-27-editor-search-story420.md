# Code Review: CodeEditor Search + Story 420 (autoDeleteOnPrClose)
**Date:** 2026-02-27
**Reviewers:** Multi-Agent (typescript, rust, performance, architecture, silent-failure, simplicity)
**Target:** Uncommitted changes on main

## Summary
- **P1 Critical Issues:** 2
- **P2 Important Issues:** 6
- **P3 Nice-to-Have:** 4
- **Confidence Threshold:** 70

## P1 - Critical

- [ ] **[PERF-1] 2s polling interval causes terminal typing jank** (Confidence: 95)
  - Location: `src/components/CodeEditorPanel/CodeEditorTab.tsx:114-119`
  - Issue: `setInterval(checkDiskContent, 2000)` fires IPC round-trip reading full file content every 2s. Tauri IPC serializes entire file to JSON, crosses IPC boundary, deserializes in webview — all on the main thread. For files approaching 500KB cap, this causes the half-second freezes Boss reports while typing in terminal (same JS event loop).
  - Fix: Replace polling with Rust-side file watcher (`notify` crate) emitting events. Short-term: increase to 5s + skip while `document.visibilityState === "hidden"`.
  - Agent: performance-reviewer

- [ ] **[SILENT-1] `eprintln!` in `load_json_config` invisible in production Tauri builds** (Confidence: 90)
  - Location: `src-tauri/src/config.rs:111,118`
  - Issue: Config load failures (corrupt JSON, permissions) silently reset all settings to defaults with zero trace in release builds. `eprintln!` goes nowhere in Tauri WebView apps.
  - Fix: Replace with `log::warn!` / `log::error!` to route through ring-buffer log.
  - Agent: silent-failure-hunter
  - Note: Pre-existing, but every new config type added inherits the problem.

## P2 - Important

- [ ] **[PERF-2] Per-keystroke `setCode` → `createEditorControlledValue` cascade** (Confidence: 90)
  - Location: `src/components/CodeEditorPanel/CodeEditorTab.tsx:33,139,145`
  - Issue: `onValueChange` → `setCode(value)` → controlled-value effect → CodeMirror "replace all" transaction → string equality check. For 200KB file, every keypress does a 200KB string comparison.
  - Fix: Use a mutable ref (`let currentCode = ""`) in `onValueChange` instead of writing to the `code` signal. Only push external changes (disk reload) through the signal.
  - Agent: performance-reviewer

- [ ] **[PERF-3] Dirty-state effect fires on every keystroke** (Confidence: 88)
  - Location: `src/components/CodeEditorPanel/CodeEditorTab.tsx:56-59`
  - Issue: `isDirty()` reads `code()` signal, so the effect re-evaluates and calls `editorTabsStore.setDirty()` on every character, triggering tab bar re-renders.
  - Fix: Guard with `if (dirty === lastDirty) return;` or fix PERF-2 first (eliminates the root cause).
  - Agent: performance-reviewer

- [ ] **[SILENT-2] Bare `catch {}` in `checkDiskContent` swallows all errors** (Confidence: 92)
  - Location: `src/components/CodeEditorPanel/CodeEditorTab.tsx:100-102`
  - Issue: Only file-deleted is documented as intentional, but IPC failures, permissions errors are also silently swallowed. Polling becomes a permanent no-op with no trace.
  - Fix: `appLogger.debug("app", "checkDiskContent failed", { filePath, err })`
  - Agent: silent-failure-hunter

- [ ] **[SILENT-3] `saveSettings` logs failures at `debug` level** (Confidence: 88)
  - Location: `src/stores/repoSettings.ts:102-104`
  - Issue: Sister function `save()` in `repoDefaults.ts` uses `appLogger.error`. Asymmetry means repo settings save failures are invisible in production.
  - Fix: Change `appLogger.debug` → `appLogger.error`
  - Agent: silent-failure-hunter

- [ ] **[RUST-1] Missing `Debug` derive on `AutoDeleteOnPrClose`** (Confidence: 95)
  - Location: `src-tauri/src/config.rs:223`
  - Issue: All sibling enums (`WorktreeAfterMerge`, `OrphanCleanup`, `MergeStrategy`) derive `Debug`. This one doesn't. Will block `Debug` on any future wrapper type.
  - Fix: `#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]`
  - Agent: rust-reviewer

- [ ] **[YAGNI-1] Global default for `autoDeleteOnPrClose` wired but no UI to set it** (Confidence: 90)
  - Location: `src/stores/repoDefaults.ts:35,52` + `src-tauri/src/config.rs:575`
  - Issue: `RepoDefaultsConfig` has the field, `save()`/`load()` serialize it, but `GeneralTab.tsx` has no dropdown for it. "Use global default (off)" is always "off" with no way to change it.
  - Fix: Either add to GeneralTab or remove from global defaults (per-repo only).
  - Agent: simplicity-reviewer

## P3 - Nice-to-Have

- [ ] **[RUST-2] No serde round-trip tests for `AutoDeleteOnPrClose`** (Confidence: 85)
  - Location: `src-tauri/src/config.rs:223`
  - Issue: All sibling enums have serialize/deserialize tests. This one has none.
  - Agent: rust-reviewer

- [ ] **[TS-1] Floating promises in `checkDiskContent` call sites** (Confidence: 85)
  - Location: `src/components/CodeEditorPanel/CodeEditorTab.tsx:111,117`
  - Issue: Async function called without `.catch()`. Internal try/catch masks it today but fragile.
  - Agent: typescript-reviewer + silent-failure-hunter

- [ ] **[TS-2] Unchecked type assertion in select onChange** (Confidence: 80)
  - Location: `src/components/SettingsPanel/tabs/RepoWorktreeTab.tsx:284`
  - Issue: `as AutoDeleteOnPrClose` with no runtime check. Matches existing codebase pattern for all dropdowns.
  - Agent: typescript-reviewer

- [ ] **[DOCS-1] Documentation not updated for new features** (Confidence: 95)
  - Missing updates per AGENTS.md sync matrix:
    - `docs/FEATURES.md` section 3.5: No mention of search/find (Cmd+F, Cmd+H)
    - `docs/FEATURES.md` keyboard shortcuts: Code Editor table only has Cmd+S
    - `docs/FEATURES.md` section 8/settings: No mention of autoDeleteOnPrClose
    - `docs/user-guide/keyboard-shortcuts.md`: No Cmd+F/Cmd+G/Cmd+H for editor
    - `docs/backend/config.md`: No mention of new config field
  - Agent: architecture-reviewer

---

## Cross-Cutting Analysis

### Root Causes Identified

| Root Cause | Findings | Fix |
|------------|----------|-----|
| Frontend file polling instead of Rust event-driven | PERF-1, SILENT-2 | Rust file watcher + event emission |
| Per-keystroke signal cascade in CodeMirror | PERF-2, PERF-3 | Mutable ref for in-editor value |
| `eprintln!` in Rust config loader | SILENT-1 | Replace with `log::warn!`/`log::error!` |
| Incomplete story 420 wiring | YAGNI-1, RUST-1, RUST-2, DOCS-1 | Finish or trim scope |

### Single-Fix Opportunities

1. **Mutable ref for editor value** — Fixes PERF-2 + PERF-3 (~10 lines changed)
2. **Rust file watcher** — Fixes PERF-1 + SILENT-2 (eliminates polling entirely)
3. **`Debug` derive + test additions** — Fixes RUST-1 + RUST-2 (~15 lines)

### Context Files (Read Before Fixing)

| File | Reason | Referenced By |
|------|--------|---------------|
| `src/hooks/useFileBrowser.ts` | `readFile` is a straight IPC call, no caching | performance, silent-failure |
| `src/invoke.ts` | Tauri IPC bridge, no batching | performance |
| `src/stores/appLogger.ts` | Severity levels and production visibility | silent-failure |
| `src-tauri/src/lib.rs` | Logging backend wiring | silent-failure |

---

## Recommended Actions

**Immediate priority (jank fix):**
1. PERF-1: Replace 2s polling with longer interval + visibility guard (short-term) or Rust file watcher (proper fix)
2. PERF-2: Mutable ref instead of signal for editor keystrokes
3. RUST-1: Add `Debug` derive (one-line fix)

**Before merge (story 420):**
4. YAGNI-1: Decide — add GeneralTab UI or remove global default
5. DOCS-1: Update FEATURES.md and keyboard shortcuts docs

**Debt items:**
6. SILENT-1: Replace `eprintln!` with `log::warn!` in config.rs
7. SILENT-3: `debug` → `error` in repoSettings.ts saveSettings
