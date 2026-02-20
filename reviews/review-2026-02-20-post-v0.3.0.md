# Code Review: Post v0.3.0 (e393997..HEAD)
**Date:** 2026-02-20
**Reviewers:** Multi-Agent (security, performance, architecture, simplicity, silent-failure, test-quality, rust, typescript)
**Target:** 33 commits since v0.3.0 (code-only: ~2930 additions, ~306 deletions across 40 files)
**Confidence Threshold:** 70

## Summary
- **P1 Critical Issues:** 5
- **P2 Important Issues:** 19
- **P3 Nice-to-Have:** 16
- **Filtered Out (below threshold):** 3

---

## P1 - Critical (Must Fix)

- [ ] **[SECURITY]** Shell injection in Sidebar git quick-action buttons (Confidence: 95)
  - Location: `src/components/Sidebar/Sidebar.tsx:795-826`
  - Issue: `repo.path` interpolated into shell commands without `escapeShellArg()`. Adjacent `GitOperationsPanel` correctly escapes — this is inconsistent.
  - Fix: `cd ${escapeShellArg(repo.path)} && git pull` (and push/fetch/stash)
  - Agent: security-reviewer

- [ ] **[SECURITY]** `binary_path` in spawn_agent accepts arbitrary executables without validation (Confidence: 90)
  - Location: `src-tauri/src/mcp_http/agent_routes.rs:66-68`, `src-tauri/src/agent.rs:442-451`
  - Issue: Caller-supplied `binary_path` passed directly to `CommandBuilder::new()` — no path validation or allow-listing
  - Fix: Validate path is absolute and exists; consider allow-listing known agent binaries
  - Agent: security-reviewer

- [ ] **[SIMPLICITY]** Production `eprintln!` debug statement in hot path (Confidence: 99)
  - Location: `src-tauri/src/output_parser.rs:105`
  - Issue: `[RateLimit DEBUG]` prints to stderr on every rate-limit match in the PTY reader thread
  - Fix: Remove the `eprintln!` — events are already emitted as `ParsedEvent::RateLimit`
  - Agent: simplicity-reviewer

- [ ] **[TEST]** `resolve_cli()` and `extra_bin_dirs()` have zero unit tests (Confidence: 95)
  - Location: `src-tauri/src/agent.rs:17-77`
  - Issue: Critical platform function (release-build PATH resolution) with no tests. AGENTS.md explicitly flags this scenario.
  - Fix: Add tests for fallback behavior, non-empty dirs, duplicate prevention
  - Agent: test-quality-reviewer

- [ ] **[RUST]** Missing `// SAFETY:` comments on `unsafe` blocks (Confidence: 97)
  - Location: `src-tauri/src/pty.rs:582, 631`
  - Issue: Two `unsafe` blocks for Windows process enumeration lack mandatory safety comments
  - Fix: Add `// SAFETY:` comments documenting the API contract invariants
  - Agent: rust-reviewer

---

## P2 - Important (Should Fix)

- [ ] **[PERF]** `resolve_cli()` does uncached filesystem probes on every git/gh invocation (Confidence: 97)
  - Location: `src-tauri/src/agent.rs:69-77`
  - Issue: 1-4 `stat()` syscalls per git command via `Path::exists()`. Result is stable for app lifetime.
  - Fix: Cache per binary name in `OnceLock<HashMap>` or `DashMap` in `AppState`
  - Agents: performance-reviewer, rust-reviewer

- [ ] **[ARCH]** `resolve_cli` is in `agent.rs` but used by git.rs, github.rs, worktree.rs, lib.rs (Confidence: 92)
  - Location: `src-tauri/src/agent.rs:69` (25+ cross-module call sites)
  - Issue: Feature-envy — git.rs should not depend on the agent module
  - Fix: Extract to `src-tauri/src/cli.rs` or `platform.rs`
  - Agent: architecture-reviewer

- [ ] **[ARCH]** `window.prompt()` used in Sidebar despite `PromptDialog` existing (Confidence: 95)
  - Location: `src/components/Sidebar/Sidebar.tsx:144`
  - Issue: `PromptDialog` was created specifically to replace `window.prompt()` which doesn't work in Tauri webview
  - Fix: Replace with `PromptDialog` component
  - Agent: architecture-reviewer

- [ ] **[RUST]** `is_relevant_git_path` uses `/refs/` with forward slash — broken on Windows (Confidence: 82)
  - Location: `src-tauri/src/repo_watcher.rs:27`
  - Issue: `path_str.contains("/refs/")` won't match Windows backslash paths — suppresses all branch events on Windows
  - Fix: Use `path.components().any(|c| c.as_os_str() == "refs")`
  - Agent: rust-reviewer

- [ ] **[TS]** Broken drag path: ungrouped-to-group move silently fails via `?? ""` (Confidence: 85)
  - Location: `src/components/Sidebar/Sidebar.tsx:547`
  - Issue: `sourceGroupId ?? ""` passes empty string to `moveRepoBetweenGroups`, which guards against empty IDs and returns early
  - Fix: Use `addRepoToGroup` when source is ungrouped, `moveRepoBetweenGroups` only when both groups are valid
  - Agent: typescript-reviewer

- [ ] **[PERF]** Windows process tree walk is O(n*depth) with full system snapshot every 3s (Confidence: 92)
  - Location: `src-tauri/src/pty.rs:624-668`
  - Issue: `CreateToolhelp32Snapshot` of all processes, then linear scan per depth level
  - Fix: Build `HashMap<pid, Vec<child>>` once, walk in O(depth)
  - Agent: performance-reviewer

- [ ] **[PERF]** `getGroupForRepo()` is O(n*m) linear scan on every render (Confidence: 90)
  - Location: `src/stores/repositories.ts:605-607`
  - Issue: `Object.values(groups).find(g => g.repoOrder.includes(repoPath))` — called 3-4 times per repo per render
  - Fix: Maintain inverted index `repoPath -> groupId` for O(1) lookups
  - Agent: performance-reviewer

- [ ] **[PERF]** `repo_watcher` watches .git/ recursively including `objects/` (Confidence: 82)
  - Location: `src-tauri/src/repo_watcher.rs:85-88`
  - Issue: Git pack/fetch operations generate hundreds of events in `objects/` that are all filtered out
  - Fix: Watch `.git/` non-recursively + `.git/refs/` recursively
  - Agent: performance-reviewer

- [ ] **[SIMPLICITY]** Two dead stub handlers in Sidebar (`handleGroupRename`, `handleGroupColorChange`) (Confidence: 97)
  - Location: `src/components/Sidebar/Sidebar.tsx:690-696`
  - Issue: Context menu items "Rename Group" and "Change Color" are silently inert
  - Fix: Wire to existing `GroupsTab` functionality or remove menu items
  - Agents: simplicity-reviewer, typescript-reviewer

- [ ] **[ARCH]** `repo_watcher` and `head_watcher` both fire on .git/HEAD changes (Confidence: 83)
  - Location: `src-tauri/src/repo_watcher.rs:33-36`
  - Issue: Redundant IPC round-trips on branch switch — both watchers fire for HEAD
  - Fix: Remove `HEAD` from `repo_watcher`'s relevance filter
  - Agent: architecture-reviewer

- [ ] **[SILENT]** `git branch -d` non-zero exit is fully invisible (Confidence: 95)
  - Location: `src-tauri/src/worktree.rs:260-266`
  - Issue: `if let Err(e)` only catches spawn failures, not git exit code. Most common failure (unmerged branch) produces zero log.
  - Fix: Check `output.status.success()` and log stderr
  - Agent: silent-failure-hunter

- [ ] **[SILENT]** `ensure_window_visible` recovery ops silently ignored (Confidence: 90)
  - Location: `src-tauri/src/lib.rs:63-65`
  - Issue: `let _ =` on `set_size`, `set_position`, `center` — guard may do nothing
  - Fix: Log failures with `if let Err(e)`
  - Agent: silent-failure-hunter

- [ ] **[SILENT]** Repo save failures logged only at `console.debug` (Confidence: 88)
  - Location: `src/stores/repositories.ts:96`
  - Issue: Every group mutation saves via fire-and-forget with `console.debug` catch — invisible data loss
  - Fix: Escalate to `console.error`; consider toast for repeated failures
  - Agent: silent-failure-hunter

- [ ] **[SILENT]** Hydration failure logged at `console.debug` only (Confidence: 85)
  - Location: `src/stores/repositories.ts:199-202`
  - Issue: App starts with empty sidebar, no user feedback
  - Fix: Escalate to `console.error`
  - Agent: silent-failure-hunter

- [ ] **[SILENT]** `repo_watcher` filesystem errors silently dropped (Confidence: 85)
  - Location: `src-tauri/src/repo_watcher.rs:63`
  - Issue: `let Ok(events) = events else { return }` — no log for watcher errors
  - Fix: Add `eprintln!` for watcher errors
  - Agent: silent-failure-hunter

- [ ] **[RUST]** Arithmetic overflow in `ensure_window_visible` on corrupted dimensions (Confidence: 85)
  - Location: `src-tauri/src/lib.rs:43-44`
  - Issue: `size.width as i32` silently wraps for large u32 values — the exact scenario this guard handles
  - Fix: Use `i32::try_from()` with `saturating_add`
  - Agent: rust-reviewer

- [ ] **[RUST]** `szExeFile` i8→u8 cast + `String::from_utf8` silently drops non-ASCII process names (Confidence: 88)
  - Location: `src-tauri/src/pty.rs:597-602`
  - Issue: Windows process with non-ASCII name → `from_utf8` fails → returns `None` → agent not detected
  - Fix: Use `String::from_utf8_lossy`
  - Agent: rust-reviewer

- [ ] **[TS]** CSS variable sync outside reactive context (Confidence: 90)
  - Location: `src/components/Sidebar/Sidebar.tsx:641`
  - Issue: `document.documentElement.style.setProperty(...)` runs once at mount, not tracked
  - Fix: Wrap in `createEffect`
  - Agent: typescript-reviewer

---

## P3 - Nice-to-Have

- [ ] **[PERF]** `extra_bin_dirs()` allocates Vec on every call — `src-tauri/src/agent.rs:17-65` (95)
- [ ] **[PERF]** `repoMenuItems()` not wrapped in `createMemo` — `src/components/Sidebar/Sidebar.tsx:120` (85)
- [ ] **[PERF]** Redundant `repos` memo used only for empty check — `src/components/Sidebar/Sidebar.tsx:444-445` (80)
- [ ] **[SIMPLICITY]** Dead code: `remaining = String::new()` reassignment — `src-tauri/src/pty.rs:123-125` (96)
- [ ] **[SIMPLICITY]** `spawn_reader_thread` and `spawn_headless_reader_thread` share ~70 lines duplicated — `src-tauri/src/pty.rs:63-208` (90)
- [ ] **[SIMPLICITY]** `dominated` misleading variable name — `src-tauri/src/repo_watcher.rs:66` (95)
- [ ] **[SIMPLICITY]** Drag-and-drop index adjustment logic duplicated 3x — `src/components/Sidebar/Sidebar.tsx:519-541` (80)
- [ ] **[SECURITY]** `detect_agent_binary` passes unvalidated binary name to `which`/`where` — `src-tauri/src/agent.rs:370` (85)
- [ ] **[SECURITY]** Group color injected into `style` without validation — `src/components/Sidebar/Sidebar.tsx:418` (88)
- [ ] **[RUST]** `detect_installed_ides` uses `Vec::contains` for dedup — `src-tauri/src/agent.rs:245` (90)
- [ ] **[RUST]** `crate::agent::resolve_cli("git")` repeated at 25+ call sites — `src-tauri/src/git.rs` (87)
- [ ] **[RUST]** `Cow::to_string()` instead of `.into_owned()` — `src-tauri/src/agent.rs:328-331` (80)
- [ ] **[TS]** `children: any` on GroupSection — `src/components/Sidebar/Sidebar.tsx:391` (92)
- [ ] **[TS]** Non-null assertion in useAppInit — `src/hooks/useAppInit.ts:60` (95)
- [ ] **[TS]** `substr` deprecated, use `slice` — `src/stores/repositories.ts:118` (95)
- [ ] **[TS]** Unchecked `as` cast for `agentType` — `src/stores/repositories.ts:472` (88)

---

## Cross-Cutting Analysis

### Root Causes Identified

| Root Cause | Findings Affected | Suggested Fix |
|------------|-------------------|---------------|
| `resolve_cli` in wrong module, uncached, repeated | 5 findings (perf cache, arch coupling, DRY, Vec alloc, git helper) | Extract to `cli.rs`, add OnceLock cache, create `git()` helper |
| Missing input validation at system boundaries | 3 findings (shell injection, binary_path, detect_agent_binary) | Add `escapeShellArg`, validate binary_path, validate binary name |
| `head_watcher` / `repo_watcher` overlap | 3 findings (dual watcher, HEAD duplication, redundant IPC) | Unify into single watcher or remove HEAD from repo_watcher |
| Insufficient error visibility | 4 findings (console.debug saves, git branch -d, window guard, watcher errors) | Escalate console.debug→error, check exit codes, log recovery failures |
| Stub/dead code shipped as user-facing features | 2 findings (rename/color handlers, debug eprintln) | Implement or remove menu items; remove debug print |

### Single-Fix Opportunities

1. **Extract `cli.rs` + add OnceLock cache** — Fixes 5 findings (~50 lines)
2. **Add `escapeShellArg` to Sidebar git buttons** — Fixes P1 injection (~4 line changes)
3. **Remove debug `eprintln!`** — Fixes P1 (~1 line delete)
4. **Use `path.components()` in `is_relevant_git_path`** — Fixes Windows repo_watcher (~3 lines)
5. **Fix ungrouped-to-group drag path** — Fixes silent broken drag (~10 lines)

### Context Files (Read Before Fixing)

| File | Reason | Referenced By |
|------|--------|---------------|
| `src/components/GitOperationsPanel/GitOperationsPanel.tsx` | Shows correct `escapeShellArg` usage | security |
| `src/components/PromptDialog/PromptDialog.tsx` | Existing `window.prompt` replacement | architecture, typescript |
| `src-tauri/src/head_watcher.rs` | Overlap assessment with repo_watcher | architecture, simplicity |
| `src-tauri/src/state.rs` | AppState cache infrastructure | performance, rust |
| `src-tauri/src/mcp_http/auth.rs` | Localhost auth bypass context | security |

---

## Dependency Changes Detected

`Cargo.lock` changed (`windows-sys 0.59.0` added). Run vulnerability scan:
```
cargo audit
```

---

## Recommended Actions

1. **Immediate:** Fix P1 items (shell injection, debug eprintln, binary_path validation, safety comments, resolve_cli tests)
2. **This sprint:** Address P2 items (resolve_cli caching, Windows refs path, broken drag, error visibility)
3. **Follow-up:** Create stories for P3 items and structural improvements (watcher unification, reader thread dedup)

---

## Agent Highlights

- **Security:** Shell injection in Sidebar git buttons (P1); unvalidated binary_path in agent spawn (P1)
- **Performance:** Uncached `resolve_cli` filesystem probes on every git command (P2); Windows process snapshot every 3s (P2)
- **Architecture:** `resolve_cli` in wrong module (P2); `window.prompt` despite PromptDialog existing (P2)
- **Simplicity:** Production debug `eprintln!` in hot path (P1); dead stub handlers (P2); watcher duplication (P2)
- **Silent Failures:** `console.debug` for save errors (P2); git branch -d exit code ignored (P2)
- **Test Quality:** `resolve_cli` untested (P1); 7 public store methods untested (P2); Windows process detection untested
- **Rust Idioms:** Missing SAFETY comments on unsafe (P1); Windows path separator bug (P2); uncached allocs (P2)
- **TypeScript Idioms:** Broken ungrouped-to-group drag (P2); CSS var not reactive (P2); stub handlers (P3)
