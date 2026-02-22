# Code Review: Full Codebase — Pre-Major-Upgrade Master Review
**Date:** 2026-02-22
**Reviewers:** Multi-Agent (security, rust, typescript, architecture, simplicity, silent-failure, performance, css, test-quality, documentation)
**Target:** Full codebase (main branch)
**Confidence Threshold:** 80

---

## Summary

| Severity | Count |
|----------|-------|
| P1 Critical | 21 |
| P2 Important | 47 |
| P3 Nice-to-Have | 35 |
| **Total** | **103** |

---

## P1 — Critical (Must Fix)

### Security

- [ ] **[SEC]** MCP SSE `config/save` allows remote authenticated users to overwrite `shell` binary path → arbitrary code execution on next PTY spawn
  `src-tauri/src/mcp_http/mcp_transport.rs:463` (Confidence: 95)
  Fix: Add `if !addr.ip().is_loopback() { return error }` guard at top of `"save"` arm

- [ ] **[SEC]** `POST /sessions/agent` accepts caller-supplied `binary_path` + raw `args` → one-shot arbitrary binary execution for any authenticated remote user
  `src-tauri/src/mcp_http/agent_routes.rs:66` (Confidence: 90)
  Fix: Add localhost-only guard, or remove `binary_path`/`args` fields from the API

### Rust Backend

- [ ] **[RUST]** `get_mcp_status` performs 800ms blocking TCP connect on the Tauri event thread
  `src-tauri/src/lib.rs:353` (Confidence: 90)
  Fix: Make command `async` and wrap TCP connect in `tokio::task::spawn_blocking`

### Silent Failures (Data Destruction)

- [ ] **[SILENT]** `refreshAllBranchStats` deletes all tracked branches when `getWorktreePaths` returns `{}` on backend error
  `src/hooks/useGitOperations.ts:47-55` (Confidence: 95)
  Fix: Guard against empty result being a network/backend error before clearing branches

- [ ] **[SILENT]** `load_json_config` silently resets all settings (including `repositories.json`) when config file is corrupt — wipes all tracked repos
  `src-tauri/src/config.rs:92-102` (Confidence: 92)
  Fix: Propagate parse errors instead of `.ok()` chaining; show error dialog

- [ ] **[SILENT]** `head_watcher.rs` watcher errors silently swallowed with no log (inconsistent with `repo_watcher.rs` which logs same error)
  `src-tauri/src/head_watcher.rs:55` (Confidence: 88)
  Fix: Add `log::error!` matching the repo_watcher pattern

### CSS — Invisible UI Elements

- [ ] **[CSS]** `--danger` CSS variable used in 5 places in PluginsTab but never defined in `:root` → error badges and uninstall buttons render with no color
  `src/components/SettingsPanel/tabs/PluginsTab.module.css:197,221,296,331-332` (Confidence: 99)
  Fix: Replace all `var(--danger)` with `var(--error)`

- [ ] **[CSS]** `--fg-accent` used in Toolbar notification repo names and PrDetailPopover repo label but never defined → text is invisible
  `src/components/Toolbar/Toolbar.module.css:252`, `src/components/PrDetailPopover/PrDetailPopover.module.css:33` (Confidence: 99)
  Fix: Replace with `var(--accent)`

- [ ] **[CSS]** `--fg-tertiary` used for Settings nav group label but never defined → labels render in wrong color (white instead of muted)
  `src/components/SettingsPanel/Settings.module.css:116` (Confidence: 99)
  Fix: Replace with `var(--fg-muted)`

- [ ] **[CSS]** Toolbar background hardcodes `#1e1e1e` instead of `var(--bg-primary)`
  `src/components/Toolbar/Toolbar.module.css:5` (Confidence: 98)
  Fix: `background: var(--bg-primary)`

### Documentation — Wrong Information

- [ ] **[DOCS]** SPEC.md says `Cmd+D` for diff panel toggle — wrong, actual binding is `Cmd+Shift+D`
  `SPEC.md:257` (Confidence: 100)
  Fix: Update to `Cmd+Shift+D`

- [ ] **[DOCS]** Command Palette (`Cmd+Shift+P`) fully shipped but undocumented in SPEC.md, FEATURES.md, CHANGELOG, IDEAS.md
  (Confidence: 100)
  Fix: Add to all four documents

- [ ] **[DOCS]** Activity Dashboard (`Cmd+Shift+A`) fully shipped but undocumented everywhere
  (Confidence: 100)

- [ ] **[DOCS]** Park Repos feature fully shipped but undocumented in all docs
  `src/components/Sidebar/ParkedReposPopover.tsx` (Confidence: 100)

- [ ] **[DOCS]** Configurable keybindings system fully shipped but undocumented everywhere
  `src/keybindingDefaults.ts`, `src/stores/keybindings.ts`, `src/components/SettingsPanel/tabs/KeyboardShortcutsTab.tsx` (Confidence: 100)

### Test Quality

- [ ] **[TEST]** `repositories.test.ts` spies on `console.debug` but source uses `console.error` → error output leaks to test output, invariant not verified
  `src/__tests__/stores/repositories.test.ts:363` (Confidence: 98)
  Fix: Change spy to `console.error`, add assertion on call

- [ ] **[TEST]** `hydrate()` error path test doesn't assert that saves remain blocked (the primary data-safety invariant)
  `src/__tests__/stores/repositories.test.ts:361` (Confidence: 90)

- [ ] **[TEST]** `pluginLoader.ts` — `setPluginEnabled`, `syncDisabledList`, `isPluginDisabled`, `handlePluginChanged` have zero test coverage
  `src/__tests__/plugins/pluginLoader.test.ts` (Confidence: 95)

---

## P2 — Important (Fix Soon)

### Security

- [ ] **[SEC]** `remove_worktree_http` skips `validate_repo_path` on `repo_path` → authenticated remote user can trigger worktree removal on arbitrary git repos
  `src-tauri/src/mcp_http/worktree_routes.rs:71` (Confidence: 88)

- [ ] **[SEC]** Debug static file server concatenates URL path without traversal check → path traversal in dev builds
  `src-tauri/src/mcp_http/static_files.rs:30` (Confidence: 78)

- [ ] **[SEC]** CORS `allow_headers(Any)` — replace with explicit header allowlist
  `src-tauri/src/mcp_http/mod.rs:80` (Confidence: 82)

### Rust

- [ ] **[RUST]** `.unwrap()` on `std::sync::RwLock` throughout lib.rs and mcp_http — can panic on lock poisoning. Switch `AppState::config` to `parking_lot::RwLock`
  `src-tauri/src/lib.rs:84,91,99,354` (Confidence: 90)

- [ ] **[RUST]** `OutputRingBuffer::write` loops byte-by-byte with modulo per byte — hot PTY output path
  `src-tauri/src/state.rs:281-287` (Confidence: 92)
  Fix: Use `copy_from_slice` with wrap-around handling (2 memcpy ops instead of N modulos)

- [ ] **[RUST]** `OutputParser::new()` rebuilds 10 regex objects per PTY session — should use `lazy_static!`
  `src-tauri/src/output_parser.rs:63-67` (Confidence: 88)

- [ ] **[RUST]** `save_config` double-clones `AppConfig` unnecessarily
  `src-tauri/src/lib.rs:98-99` (Confidence: 88)

### TypeScript / SolidJS

- [ ] **[TS]** `setup()` async shortcut registration is a floating promise inside `createEffect` — race between registration and `onCleanup` causes shortcut leak
  `src/App.tsx:636` (Confidence: 95)

- [ ] **[TS]** `onCleanup` called after `await` inside `onMount` — may not register in all SolidJS versions
  `src/App.tsx:223` (Confidence: 90)

- [ ] **[TS]** `transport.ts` `rpc()` returns `data as T` without null guard → null body from server produces silent runtime error at call site
  `src/transport.ts:324` (Confidence: 88)

### Architecture

- [ ] **[ARCH]** `App.tsx` is a 927-line God Component — orchestrates PTY, shortcuts, menu events, tab reattach, quit dialog, dictation, lazygit float, quick-switcher
  `src/App.tsx:80` (Confidence: 98)

- [ ] **[ARCH]** `AppState` in Rust bundles PTY sessions, MCP SSE, GitHub caches, watchers, HTTP client in one struct
  `src-tauri/src/state.rs:361` (Confidence: 95)

- [ ] **[ARCH]** `isMainBranch()` hardcodes branch name list as domain knowledge in TypeScript store (violates Rust-owns-logic rule + will diverge from Rust's implementation)
  `src/stores/repositories.ts:59` (Confidence: 92)

- [ ] **[ARCH]** `handleOpenUrl` duplicated identically in `HelpPanel.tsx` and `AboutTab.tsx`
  (Confidence: 98) — Fix: Extract to `src/utils/openUrl.ts`

- [ ] **[ARCH]** `formatRelativeTime`/`formatDate` duplicated across `ActivityDashboard.tsx` and `NotesPanel.tsx`
  (Confidence: 90) — Fix: Consolidate into `src/utils/time.ts`

- [ ] **[ARCH]** Path manipulation (split on `/`, navigate up, resolve relative paths) in UI components — POSIX-only, breaks on Windows
  `src/components/FileBrowserPanel/FileBrowserPanel.tsx:111`, `MarkdownTab.tsx:83`, `SettingsPanel.tsx:64` (Confidence: 85)

- [ ] **[ARCH]** Plugin line buffers never freed on session close — memory leak with many terminal sessions
  `src/plugins/pluginRegistry.ts` (Confidence: 88)

- [ ] **[ARCH]** `PluginsTab` directly calls `invoke()` for install/uninstall instead of using `pluginStore`
  `src/components/SettingsPanel/tabs/PluginsTab.tsx:68` (Confidence: 80)

- [ ] **[ARCH]** `actionRegistry.ts` imports `comboToDisplay` from a Settings component — wrong dependency direction
  `src/actions/actionRegistry.ts:9` (Confidence: 92) — Fix: Move to `src/utils/keybindings.ts`

- [ ] **[ARCH]** `DiffTab` doesn't subscribe to `repositoriesStore.getRevision()` — stale diff content after file changes (violates panel refresh rule)
  `src/components/DiffTab/DiffTab.tsx:21` (Confidence: 85)

### Dead Code / Simplicity

- [ ] **[DEAD]** `src/utils/prStateMapping.ts` has zero production imports — pure dead code superseded by Rust pre-classification
  (Confidence: 97) — Delete file and test

- [ ] **[DEAD]** Duplicate `stripAnsi` in `MarkdownRenderer.tsx` vs canonical `src/utils/stripAnsi.ts` (weaker regex — misses OSC sequences)
  `src/components/ui/MarkdownRenderer.tsx:13` (Confidence: 95)

- [ ] **[DEAD]** `wizStoriesPlugin` singleton exported but never registered in production (already moved to external plugin)
  `src/plugins/wizStoriesPlugin.ts:173` (Confidence: 87)

- [ ] **[DEAD]** `classify_error_message` and `calculate_backoff_delay_cmd` registered as Tauri commands but never called from frontend
  `src-tauri/src/error_classification.rs:49-81`, `src-tauri/src/lib.rs:587-588` (Confidence: 85)

### Silent Failures

- [ ] **[SILENT]** PTY reader thread breaks on errors silently (no log, no user notification)
  `src-tauri/src/pty.rs:206` (Confidence: 88)

- [ ] **[SILENT]** All WebSocket PTY writes are fire-and-forget with no error handling
  `src-tauri/src/mcp_http/session.rs:467-475` (Confidence: 85)

- [ ] **[SILENT]** GitHub GraphQL errors converted to empty `Ok([])` responses — callers can't distinguish error from "no data"
  `src-tauri/src/github.rs:766,946` (Confidence: 82)

- [ ] **[SILENT]** `get_diff_stats` returns zeros on git failure with no log
  `src-tauri/src/git.rs:312` (Confidence: 80)

- [ ] **[SILENT]** GitHub polling outer catch has no logging — failures are invisible
  `src/stores/github.ts:185` (Confidence: 82)

### Performance

- [ ] **[PERF]** Duplicate GitHub polling: `useGitHub` hook AND `githubStore` both run 30-second intervals for the active repo → 2 background subprocess spawns per poll cycle per active repo
  `src/hooks/useGitHub.ts:91-94`, `src/components/StatusBar/StatusBar.tsx:83` (Confidence: 95)

- [ ] **[PERF]** Sleep prevention `createEffect` subscribes to entire terminals store object → fires IPC calls on every terminal field mutation (name, lastDataAt, etc.)
  `src/App.tsx:231-246` (Confidence: 93)

- [ ] **[PERF]** `CommandPalette` calls `filteredActions()` memo 3× per render + sort uses `indexOf` O(m) in comparator
  `src/components/CommandPalette/CommandPalette.tsx:66,119,123` (Confidence: 90)

- [ ] **[PERF]** `updateSavedTerminals` iterates all repos × all branches on every terminal add/remove event
  `src/stores/repositories.ts:472-498` (Confidence: 90)

- [ ] **[PERF]** `repoPathForTerminal` is O(repos × branches × terminals) called per terminal during every `TerminalArea` render
  `src/components/TerminalArea.tsx:32-46` (Confidence: 85)

- [ ] **[PERF]** `output_parser::parse()` calls `strip_ansi()` 3× per PTY chunk (3 allocations + 3 full passes in hot path)
  `src-tauri/src/output_parser.rs:225-337` (Confidence: 92)

### CSS

- [ ] **[CSS]** ActivityDashboard status colors use wrong RGB fallback values — `--success-rgb` undefined, fallback is wrong green tint vs actual teal `#4ec9b0`
  `src/components/ActivityDashboard/ActivityDashboard.module.css:109-121` (Confidence: 95)

- [ ] **[CSS]** `prOpen` and `prReady` are identical CSS classes — merge with comma selector
  `src/components/Sidebar/Sidebar.module.css:455-464` (Confidence: 97)

- [ ] **[CSS]** Four side panels repeat ~15 lines of identical `.panel` structure CSS each — use `src/components/shared/panel.module.css`
  `DiffPanel`, `MarkdownPanel`, `FileBrowserPanel`, `NotesPanel` modules (Confidence: 95)

- [ ] **[CSS]** `MarkdownPanel .fileItem:hover` sets same background as panel → hover is invisible
  `src/components/MarkdownPanel/MarkdownPanel.module.css:49-51` (Confidence: 93)
  Fix: `var(--bg-highlight)`

- [ ] **[CSS]** Modal shadow values diverge from `--shadow-popup` design token (0.3 vs 0.4 alpha)
  `CommandPalette.module.css:20`, `ActivityDashboard.module.css:20`, `PromptDrawer.module.css:20` (Confidence: 92)

- [ ] **[CSS]** ContextMenu/Sidebar popover use custom shadow values instead of `--shadow-dropdown`
  `ContextMenu.module.css:7,83`, `Sidebar.module.css:640` (Confidence: 90)

- [ ] **[CSS]** Off-scale `border-radius` values (`10px`, `7px`) bypass design system scale
  `Settings.module.css:351`, `GitOperationsPanel.module.css:79`, `Sidebar.module.css:714`, `Toolbar.module.css:168` (Confidence: 88)

- [ ] **[CSS]** Markdown content links use `#3b82f6` (Tailwind blue) instead of `var(--accent)`
  `src/styles.css:506` (Confidence: 85)

### Documentation

- [ ] **[DOCS]** AGENTS.md instructs developers to update `HelpPanel.tsx` for new shortcuts — HelpPanel no longer contains shortcuts (refactored in `982b31b`)
  `AGENTS.md` checklist (Confidence: 100)
  Fix: Update rule to "Add to `ACTION_META` in `actionRegistry.ts`"

- [ ] **[DOCS]** SPEC.md shortcuts table is missing 12+ shortcuts added since initial writing (Cmd+F, Cmd+E, Cmd+[, Cmd+Shift+D, Cmd+G, etc.)
  `SPEC.md:249-276` (Confidence: 100)

- [ ] **[DOCS]** CHANGELOG Unreleased section missing: Park Repos, Command Palette, Activity Dashboard, Copy Path in Markdown
  `CHANGELOG.md` (Confidence: 100)

- [ ] **[DOCS]** IDEAS.md status stale: Command Palette, Activity Dashboard, Live Dictation, Clickable File Paths, Code Editor, File Browser all implemented but marked `concept`/`designed`
  `IDEAS.md` (Confidence: 100)

- [ ] **[DOCS]** SPEC.md Feature Status missing all features added after 0.3.0 (File Browser, Editor, Find, Keybindings, Command Palette, Activity Dashboard, Park Repos, Plugins, Remote Access)
  `SPEC.md:286-347` (Confidence: 100)

- [ ] **[DOCS]** Plugin system entirely absent from SPEC.md Feature Status and Architecture diagram
  `SPEC.md` (Confidence: 100)

### Test Quality

- [ ] **[TEST]** `bumpRevision()` and `getRevision()` — the panel refresh mechanism — have no tests
  `src/__tests__/stores/repositories.test.ts` (Confidence: 92)

- [ ] **[TEST]** `transport.ts` Tauri branch (`invoke()`) never exercised — only browser/HTTP fetch branch is tested
  `src/__tests__/transport.test.ts` (Confidence: 88)

- [ ] **[TEST]** `actionRegistry` — `open-lazygit` conditional guard (when `lazygitAvailable()` is false) not tested
  `src/__tests__/actions/actionRegistry.test.ts` (Confidence: 85)

- [ ] **[TEST]** `pluginLoader` — disabled plugin registration branch has no test
  `src/__tests__/plugins/pluginLoader.test.ts` (Confidence: 83)

---

## P3 — Nice-to-Have

### Security
- [ ] **[SEC]** Session cookie uses `SameSite=Lax` instead of `SameSite=Strict` — `auth.rs:100`
- [ ] **[SEC]** No security response headers (CSP, X-Frame-Options, X-Content-Type-Options) — `mcp_http/mod.rs`
- [ ] **[SEC]** No per-IP rate limiting on session creation — `session.rs`
- [ ] **[SEC]** `detect_agent_binary_http` allows probing for arbitrary binaries — `agent_routes.rs:32`

### Rust
- [ ] **[RUST]** `Vec<String>` clone per entry in `list_directory` gitignore check — `fs.rs:257`
- [ ] **[RUST]** Second heap allocation in `Utf8ReadBuffer::push` — `state.rs:70`
- [ ] **[RUST]** `parse_rate_limit` runs regex engine twice (find + captures) — `output_parser.rs:111`

### TypeScript
- [ ] **[TS]** `agentType` cast from `string` to `AgentType` without type guard — `repositories.ts:489`
- [ ] **[TS]** `DiffTabData.status` typed as `string` instead of `"M" | "A" | "D" | "R"` union
- [ ] **[TS]** `RepoInfo.status` union type misaligned with `useGitOperations.ts` local union (missing `"merge"`, has `"not-git"`)
- [ ] **[TS]** `listWorktrees` returns `Promise<unknown[]>` — define `WorktreeInfo` interface
- [ ] **[TS]** `setInterval` in `StatusBar` created at component body scope, not `onMount`
- [ ] **[TS]** `pluginRegistry.ts` non-null assertion (`!`) after `Map.set` — use `?? []` pattern

### Architecture
- [ ] **[ARCH]** `close-tab` logic duplicated between `App.tsx` menu handler and `useKeyboardShortcuts.ts`
- [ ] **[ARCH]** `TaskQueuePanel` duration calculation in component — consolidate into `utils/time.ts`

### Dead Code
- [ ] **[DEAD]** `registerLocale` i18n infrastructure unused (only one language exists)
- [ ] **[DEAD]** `NotificationManager` 4 one-line wrapper methods (`playQuestion`, `playError`, etc.) — call `play()` directly from store
- [ ] **[DEAD]** `BUILTIN_PLUGINS` export has no consumer outside its own file
- [ ] **[DEAD]** `markdownProviderRegistry` provider-stacking logic (previous provider restore) never exercised
- [ ] **[DEAD]** `NotificationManager.setEnabled/setVolume` unused by store — store calls `updateConfig` directly
- [ ] **[DEAD]** `BranchPrState` interface is a one-field `{ state?: string }` wrapper — use inline type

### Silent Failures
- [ ] **[SILENT]** Three `getDiffStats` catch blocks use comment-only suppression with no logging
- [ ] **[SILENT]** Notification config save failure logged at `console.debug` — invisible in production
- [ ] **[SILENT]** MCP `prs` tool serializes a `Result` then calls `unwrap_or_default()` — errors silently become empty arrays

### Performance
- [ ] **[PERF]** `StatusBar` rate-limit interval fires every second unconditionally even when no rate limits are active
- [ ] **[PERF]** `repoMenuItems` recomputes `getGroupedLayout()` on every context menu open — already memoized in Sidebar
- [ ] **[PERF]** `ActivityDashboard terminals()` rebuilds full terminal list every 1s tick — use `createMemo` for the structural data
- [ ] **[PERF]** `actionEntries` memo iterates repositories twice (two for-loops over same array)

### CSS
- [ ] **[CSS]** `transition: all` appears 30+ times across modules — STYLE_GUIDE violation (specify properties explicitly)
- [ ] **[CSS]** `DictationSettings` uses bare `monospace` instead of `var(--font-mono)`
- [ ] **[CSS]** Notification icon colors use close-but-wrong hex values instead of `var(--error)` / `var(--success)`
- [ ] **[CSS]** `--border-subtle` used with fallback but never defined — simplify to `var(--border)`
- [ ] **[CSS]** `dragOverTarget` has only a placeholder comment — verify referenced or delete
- [ ] **[CSS]** Near-duplicate `mic-pulse` / `mic-loading-pulse` keyframes

### Documentation
- [ ] **[DOCS]** `Cmd+,` (toggle-settings) missing from SPEC.md shortcuts table
- [ ] **[DOCS]** SPEC.md documents removed `agentFallbackStore`/`rateLimitStore` (removed in `def23778`)
- [ ] **[DOCS]** SPEC.md State Management missing ~10 stores added since initial writing
- [ ] **[DOCS]** IDEAS.md has duplicate "Command Palette" entry (both `concept` and `designed` status)

### Test Quality
- [ ] **[TEST]** `testInScopeAsync` helper doesn't use `finally` for dispose — scope may leak on test failure
- [ ] **[TEST]** `useGitHub` loading signal transition not tested (stuck-loading regression undetectable)

---

## Cross-Cutting Analysis

### Root Causes

| Root Cause | Findings Affected |
|-----------|-----------------|
| CSS variables defined in component but not in `:root` | --danger, --fg-accent, --fg-tertiary (3 P1s) |
| Business logic leaking into TypeScript UI layer | isMainBranch, path manipulation, time formatting, duration calc (4 P2s) |
| Architectural God Objects | App.tsx + AppState.rs (2 P1s — structural) |
| Missing revision signal subscription | DiffTab stale content (1 P2) |
| Silent error swallowing pattern | 9 findings across Rust and TS |
| Dead code from Rust pre-empting TS fallbacks | prStateMapping.ts, Tauri commands, wizStoriesPlugin |
| Documentation not updated when features shipped | 5 P1 docs findings, 6 P2 docs findings |
| AGENTS.md maintenance checklist pointing at wrong file | Every developer follows wrong update path |

### Single-Fix Opportunities

1. **Switch `AppState::config` to `parking_lot::RwLock`** — fixes all 5 `.unwrap()` lock poisoning sites in one type change
2. **Add `validate_repo_path` to `remove_worktree_http`** — one-line fix to match adjacent code pattern
3. **Strip ANSI once in `OutputParser::parse()`** — eliminates 2 redundant allocations + 2 full passes per PTY chunk
4. **Extract `handleOpenUrl` to `src/utils/openUrl.ts`** — fixes DRY violation and cross-platform consistency
5. **Replace `var(--danger)` → `var(--error)` everywhere** — mechanical find-replace across PluginsTab.module.css

### Context Files (Read Before Fixing)

| File | Reason |
|------|--------|
| `src-tauri/src/state.rs` | Core `AppState` struct — affects Rust God Object decomposition and RwLock fixes |
| `src-tauri/src/mcp_http/mod.rs` | MCP router setup — affects security fixes (localhost guards, CORS) |
| `src/components/SettingsPanel/tabs/KeyboardShortcutsTab.tsx` | Actual location of shortcut docs (HelpPanel misconception) |
| `src/actions/actionRegistry.ts` | `ACTION_META` map — the self-maintaining shortcut registry |
| `docs/frontend/STYLE_GUIDE.md` | CSS design system tokens — required reading before any CSS fixes |

---

## Recommended Actions

### Immediate (block release)
1. Fix the 3 CSS undefined variables (`--danger`, `--fg-accent`, `--fg-tertiary`) — invisible UI elements in production
2. Fix the 2 P1 security vulnerabilities in MCP HTTP server
3. Fix the data-destruction bug in `refreshAllBranchStats`
4. Fix the corrupt-config silent reset in `load_json_config`
5. Fix the wrong `Cmd+D` in SPEC.md

### Before Major Upgrade
6. Address all P2 silent failure findings (PTY reader, GitHub GraphQL, etc.)
7. Fix performance hotspots: duplicate GitHub polling, sleep prevention effect, PTY strip_ansi
8. Update AGENTS.md HelpPanel rule → actionRegistry rule
9. Update CHANGELOG, SPEC.md shortcuts table, IDEAS.md statuses

### Follow-up Stories
- Story: CSS variable audit and transition:all cleanup (30+ locations)
- Story: App.tsx decomposition (God Component)
- Story: Plugin line buffer cleanup on session close
- Story: Test coverage for pluginLoader enable/disable, transport Tauri branch, bumpRevision
