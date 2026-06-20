<!-- tweak-comments v1: inline review comments.
     Format: [tweak:begin:ID]highlighted text[tweak:end:ID @ISO-TIMESTAMP
     comment body (free text, may span multiple lines)
     ] — where [ ] are the HTML comment delimiters <!-- -->.
     The only escape is '-->' → '--&gt;' inside the comment body.
     Read each comment, apply the feedback to the highlighted text,
     then remove the tweak markers. -->

# To Test

Features to test when TUICommander is more usable.

## Perf pass + light-theme fix — visual checks (2026-06-09)

_Code-verified + `make check` green; these need Boss's eyes on the live dev app (canvas/editor not HTTP/MCP-observable)._

### #80 — Terminal text color on light themes (`da16e711`, local only — not pushed)
- [ ] On a light theme (e.g. `vscode-light`), terminal default text is clearly readable, not faint gray _(reads `--fg-primary`, was undefined `--text-primary`; CanvasTerminal.tsx:323)_
- [ ] Scrollbar thumb is visible on light themes _(CanvasTerminal.tsx:2850)_
- [ ] Dark themes unchanged (no regression)

### 020-abfe — Editor + diff viewer performance pass
- [ ] Multi-file diff (scroll + PR): only on-screen file sections mount — DOM-count or screenshot on a large diff _(DiffFileList.tsx, @tanstack/solid-virtual overscan=3)_
- [ ] Sticky file headers render; drag-select stays smooth during scroll _(DiffTab.tsx)_
- [ ] Large single-file diff (>3000 lines) shows the "render anyway" guard _(DiffTab.tsx:570-582)_

### 022-dc94 — Scrollbar track-height cache
- [ ] Smooth-scroll gesture: scrollbar thumb position/size stays visually correct (no jump/drift) _(CanvasTerminal.tsx:333,749)_

### 027-deb3 — rowCache lagging-frame guard
- [ ] Fast scroll gesture: no flicker or wrong overscan content _(lagging backend frame no longer poisons rowCache; CanvasTerminal.tsx:1300)_

### 032-ce2d — Editor scrollbar overview ruler
- [ ] Editor with git changes: colored ticks on the right-edge scrollbar strip at correct relative positions; tick colors match the gutter markers _(gitGutter.ts:122,148)_

## #79 — vim & repeating key (macOS press-and-hold) (2026-06-06)
- [ ] [HUMAN] In a release `.app`, open vim and hold `j`/`l`/`i` → cursor repeats, NO accent picker popup _(needs release build: dev build lacks proper bundle domain; fix registers `ApplePressAndHoldEnabled=NO` in `press_and_hold.rs`, called from `lib.rs` setup)_
- [ ] [HUMAN] Typing accented chars still works where intended (Option-key composition path unaffected — only the hold-for-accent picker is suppressed)
- [ ] [HUMAN] A user with explicit global `defaults write -g ApplePressAndHoldEnabled -bool true` still sees their override (registration domain is lowest priority)

## Tab close kills agent process group (2026-06-03 — `b4ab1fb6`)
- [x] Closing a tab whose agent (grandchild of the PTY shell) ignores Ctrl-C kills the agent too — no orphan reparented to launchd _(verified end-to-end: MCP `session create` → ran a foreground `sh` simulant (PID 4109, traps INT/TERM/HUP, sleeps) under `target/debug/tuicommander` (dev build with fix) → MCP `session close` → PID dead within 100ms, no `sleep 600` residue. Also unit test `pty.rs::close_pty_core_kills_agent_grandchild` — confirmed it FAILS without the killpg call.)_
- [x] `kill_foreground_process_group` refuses unsafe pgid (≤1 or our own group) _(verified: pty.rs guard `pgid <= 1 || pgid == getpgid(0)` before `kill(-pgid, SIGKILL)`)_

## Issue Triage Fixes (2026-06-03 — see PR.md)

### #57 — Terminal link right-click menu (Open / Copy link)
- [x] Right-click ON a detected link span → ONLY the Open/Copy-link menu appears (App copy/paste/AI menu must NOT also open and cover it)
- [x] Right-click OUTSIDE a link → normal terminal context menu (copy / Explain with AI) still works
- [x] "Copy link" copies the resolved path/URL to clipboard WITHOUT opening it
- [x] "Open" opens the link (same as single left-click)
- [x] Single left-click on a link still opens instantly (UI-first, not Cmd-gated); drag-select over a link still copies without opening

### #58 — Cycle All Tab Types setting
- [x] Default OFF → next/prev tab cycles terminals only _(verified: useTerminalLifecycle.test.ts:564 default false excludes diff tab from cycle)_
- [x] Settings → Appearance → "Cycle All Tab Types" toggle present next to Tab Ordering _(verified: AppearanceTab.tsx:440 Toggle "Cycle All Tab Types" immediately after :421 "Tab Ordering")_
- [x] Toggle ON → prev/next also cycles diff/md/editor tabs, ordered via tabOrderingStore.getOrdered _(verified: useTerminalLifecycle.ts:354 returns terminals-only unless tabCyclingAllTypes; :362 tabOrderingStore.getOrdered(union))_
- [x] Setting persists across app restart (round-trips through config.rs tab_cycling_all_types) _(verified: config.rs:499 tab_cycling_all_types field; settings.ts maps tab_cycling_all_types↔state; runtime settings store has tabCyclingAllTypes)_

### #69 — Error log multi-level (severity threshold) filter
- [x] levelPassesThreshold: picking a level shows it + all more-severe levels _(verified: errorLogLevelFilter.test.ts — warn shows warn+error, hides info/debug)_
- [x] ErrorLogPanel: select "Warn" → Warn and Error entries shown intermingled; select "Error" → only Error _(verified: ErrorLogPanel.tsx:132 filteredEntries uses levelPassesThreshold (:20-22, LEVEL_SEVERITY) — Warn shows ≥2 (warn+error), Error shows ≥3 (error only); LEVEL_OPTIONS :6-12)_
- [x] Each level button shows a tooltip describing the threshold _(verified: ErrorLogPanel.tsx:168 title="Show {label} and more severe levels" (or "Show all log levels" for All) on every level button)_

### Tweak-comment DOM highlight + hover-link (WIP, not in PR.md)
- [x] tweakDomHighlight wraps text between sentinel pairs preserving inline formatting _(verified: tweakDomHighlight.test.ts, hoverLinkField.test.ts)_
- [x] Open a markdown with tweak-comments → highlight spans wrap the commented text without breaking **bold**/*italic*/`code` across boundaries
- [x] Hover a highlighted span / link → metadata field shows the comment info
- [x] Same highlight rendering works in CodeEditorTab and ContentRenderer

### #70 — JetBrains IDE family (open-in / default IDE)
- [x] 12 JetBrains IDEs added across all sync points _(verified: settings.ts IdeType/IDE_NAMES/IDE_ICON_PATHS/IDE_ICONS/IDE_CATEGORIES.jetbrains; agent.rs open_in_app arms + jetbrains_cmd helper + detect_installed_ides CLI+.app; IdeLauncher.tsx category + FILE_CAPABLE_IDES; tsc + cargo check clean)_
- [x] IdeLauncher dropdown shows a "JetBrains" section listing only installed JetBrains IDEs _(verified: IdeLauncher.tsx:42-43 detect_installed_ides; :50-52 categoryOrder has jetbrains section; :58-60 filters each category to installedIdes().includes(ide))_
- [x] GeneralTab default-IDE selector lists all 12 JetBrains entries (data-driven from IDE_NAMES) _(verified: GeneralTab.tsx:104 ideOptions = Object.entries(IDE_NAMES).map(...) — fully data-driven; :378-380 Default IDE SettingSelect; 12 JetBrains entries in IDE_NAMES already confirmed at line 44)_
- [x] Open a file in (e.g.) PyCharm/IntelliJ → opens at the correct line/column via `--line`/`--column` _(verified: agent.rs:66-79 jetbrains_cmd appends `--line {l}` and `--column {c}`; :135-140 intellij/pycharm/webstorm/goland/clion/phpstorm route to jetbrains_cmd(.., line, col). Command wiring verified; actual IDE honoring of the flag is the JetBrains CLI contract)_
- [HUMAN] macOS: JetBrains IDE with Toolbox shell scripts NOT enabled → `open -a "<App>"` fallback still launches it
- [HUMAN] Each IDE icon (official JetBrains/Android Studio brand logo) renders crisply in the launcher

### #56 — Copy repo settings into `.tuic.json` (uncommitted)
- [x] `save_repo_local_config` exports effective resolved worktree/branch settings, preserves existing `.tuic.json` fields, never writes scripts _(verified: config.rs tests fill_repo_local_defaults_populates_empty_config / _preserves_existing_and_skips_mcp / export_precedence_per_repo_over_defaults / overlay_repo_local_config_never_includes_scripts / repo_local_config_serializes_sparsely)_
- [x] <!--tweak:begin:c_mpyalrjpe20lsq-->Settings → a repo → "Share with Team" → "Copy settings to .tuic.json" writes `.tuic.json` in repo root; toast confirms<!--tweak:end:c_mpyalrjpe20lsq @2026-06-03T16:40:30.133Z
lo scrive ma non contiene nulla--> _(fixed + verified end-to-end: invoked save_repo_local_config via MCP on the dev build → `.tuic.json` = 317 bytes, 10 worktree/branch fields with resolved values, NOT empty)_
- [x] Export writes the **effective resolved** worktree/branch values (global defaults + per-repo overrides), not just sparse per-repo overrides _(NOTE: behaviour intentionally changed from sparse-overrides-only — that wrote an empty `{}` for users relying on global defaults. config.rs fill_repo_local_defaults; verified end-to-end)_
- [x] Existing `.tuic.json` with a manually-set field (e.g. `mcp_upstreams`) is preserved, not clobbered, when exporting other fields _(verified end-to-end: injected `mcp_upstreams=[github]` + removed `after_merge` → re-export preserved mcp_upstreams and re-populated after_merge from defaults)_
- [x] Button hidden in browser/remote (non-Tauri) mode _(verified: SettingsPanel.tsx:194 wraps the "Share with Team" section in `<Show when={isTauri()}>`)_

### get_remote_url command registration (2026-06-03, uncommitted)
- [ ] "Open in GitHub" appears in the **repo** context menu for a GitHub repo (e.g. mdkb) — _(FIX: get_remote_url was defined (git.rs:199) + called (RepoSection.tsx:536, BranchesTab.tsx:515) but NEVER registered in lib.rs invoke_handler → every call rejected "Command not found", swallowed by `.catch(()=>{})`, githubBaseUrl stayed null, item hidden. Added `git::get_remote_url` to lib.rs:1341. cargo check green. Audit confirms it was the only unregistered invoke.ts command. **Needs `make build` to take effect in the running app.**)_
- [ ] "Open in GitHub" + "Open PR" appear in the **branch** context menu for a GitHub repo (same root cause — also fixed by the registration)

### Committed fixes — quick manual confirm
- [x] #67 (`c9056df0`): file modified inside a worktree → "View diff" shows the actual diff (not "No changes") _(verified: CodeEditorTab.tsx:545 diffTabsStore.add(fsRoot(), ...) — fsRoot() (:166 = props.fsRoot ?? props.repoPath) is the worktree tree where the file is actually modified, so git diff runs in the right tree)_
- [x] #65 (`9b36711b`): after choosing "Keep" on the orphaned-worktrees dialog → it stops re-prompting for the session _(verified: useGitOperations.ts:505 session-scoped keptOrphans Set; :535 pending=orphanPaths.filter(!keptOrphans.has); :546-549 on "Keep" (!confirmed) adds each to keptOrphans → next refresh's pending is empty, no re-prompt)_
- [HUMAN] #53 (`40282679`): Windows — `tuic install-cli`/`alias` install to `%LOCALAPPDATA%\Microsoft\WindowsApps` (in PATH); needs Windows env

## Command Palette cross-repo content search (2026-06-03, uncommitted)
- [x] `search_content_all_impl` merges ready indices, tags each match with repo_path, skips unready, empty on no match _(verified: fs.rs tests search_content_all_merges_and_tags_each_repo / _skips_unready_indices / _no_matches_returns_empty)_
- [x] Palette `?query` shows a "Search all repos" checkbox in content mode _(verified: CommandPalette.tsx:296-304 `<Show when={mode()==="content"}>` renders scopeToggle checkbox bound to allReposContent()/setContentAllRepos)_
- [x] Toggle ON → results span all indexed repos, each row shows a repo-name badge _(verified: commandPalette.ts:151-152 allRepos→invoke("search_content_all"); CommandPalette.tsx:344-346 `<Show when={match.repo_path}>` renders s.repoBadge with pathBasename(repo_path))_
- [x] Toggle re-runs the current query immediately (no need to retype) _(verified: commandPalette.ts:296-308 setContentAllRepos cancels listeners and calls triggerContentSearch(searchQuery) when mode=content and query≥3)_
- [x] Clicking a cross-repo result opens the file in ITS repo (uses match.repo_path), not the active repo _(verified: CommandPalette.tsx:126-127 repoPath = match.repo_path ?? activeRepoPath)_
- [x] All-repos mode works even with no active repo selected ("No repository selected" only when single-repo + no active) _(verified: commandPalette.ts:118-119 `if (!allRepos && !repoPath) return` — all-repos skips the active-repo requirement; CommandPalette.tsx:235 canSearchContent=allReposContent()||hasActiveRepo(); :305 "No repository selected" gated on !canSearchContent())_
- [x] Coverage depends on Content Indexing strategy: with "Active repo only" cross-repo effectively returns just the active repo (cold repos skipped) — verify no crash/empty handling _(verified: backend search_content_all skips unready indices (fs.rs test search_content_all_skips_unready_indices, line 65); frontend shows graceful "No results"/empty states CommandPalette.tsx:330 — no crash on empty)_
- [x] Switching toggle off returns to active-repo-only search _(verified: commandPalette.ts:152-159 !allRepos branch invokes search_content with repoPath; setContentAllRepos(false) re-triggers immediately)_

## TUIC CLI version check + honest update banner (2026-06-03, uncommitted)
- [x] `version_match` compares `tuic --version` strings, not file size _(verified: tuic_cli.rs check_version_match runs cli_version() on installed + sidecar; size-based flapping on rebuild eliminated)_
- [x] Same CLI version installed + bundled (e.g. both `tuic 1.1.0`) → NO "update pending" banner, even if the sidecar was rebuilt (different size)
- [x] Genuinely older installed version + writable file (user-owned `/usr/local/bin/tuic`) → "(update pending — restart to apply)"; restart silently applies it _(verified: GeneralTab.tsx:167-168 shows "(update pending — restart to apply)" when !version_match && auto_updatable; auto_updatable=install_path_writable (tuic_cli.rs:45); silent apply via auto_update_cli (tuic_cli.rs:111-130) — direct fs::copy, no elevation, succeeds when writable)_
- [x] Genuinely older version + root-owned file (not writable) → "(update available)" (NOT "restart to apply"); an **Update** button appears _(verified: GeneralTab.tsx:169 "(update available)" branch when !auto_updatable; root-owned → install_path_writable false (tuic_cli.rs:239-243); :174 `<Show when={!version_match}>` renders Update button)_
- [x] Update button → triggers elevation prompt (osascript) and overwrites the installed binary; banner clears after _(verified: GeneralTab.tsx:175-177 Update→handleInstallCli→install_cli (tuic_cli.rs:59)→copy_with_elevation osascript (tuic_cli.rs:246/267); banner is `!version_match`-gated so it clears once re-check passes. Live osascript dialog is the OS contract)_
- [x] When versions match → no Update button, only Uninstall _(verified: GeneralTab.tsx:174 Update button inside `<Show when={!version_match}>` (hidden on match); :179 Uninstall button always rendered)_

## MCP Worktree Create: Event Emission + Setup Script (2026-05-27 — Issue #50)
- [x] Create worktree via MCP `repo action=worktree_create` → frontend shows switch prompt (worktree-created event fires) _(verified: mcp_transport.rs:1449-1465 WorktreeCreated emitted on event_bus and via handle.emit("worktree-created"); sse_routes.rs:98 maps to SSE event; worktree_routes.rs:108 same for HTTP)_
- [HUMAN] Create worktree via HTTP `POST /worktrees` → same switch prompt appears
- [HUMAN] Create worktree via HTTP `POST /sessions/worktree` → same switch prompt appears
- [HUMAN] After MCP worktree_create, `repo action=worktree_list` shows the new branch immediately
- [x] Configure `setup_script: "touch /tmp/tuic-setup-ran"` in repo-settings.json for a repo → create worktree via MCP → `/tmp/tuic-setup-ran` exists _(verified: session.rs:652-675 calls resolve_effective_setup_script then run_setup_script after worktree create; mcp_transport.rs:1467-1488 same for MCP action; config.rs:1374-1396 reads from repo-settings.json)_
- [x] Configure setup_script in repo-defaults.json (global) → create worktree for a repo with no per-repo override → global script runs _(verified: config.rs:1380-1396 resolve_setup_script_from falls through to defaults.setup_script when no per-repo override; config.rs:1376 loads REPO_DEFAULTS_FILE)_
- [x] Per-repo empty string override blocks global default (set `setup_script: ""` per-repo, non-empty global → script does NOT run) _(verified: config.rs:1385-1390 returns None when per-repo setup_script is Some(""); unit test at config.rs:2759-2776 confirms empty string blocks global default)_

## AI Chat Filesystem Sandbox (2026-05-27)
- [HUMAN] Open a shell session, cd into a repo, then open AI Chat on that session
- [HUMAN] Ask "list all files starting with x" → `list_files` should succeed (no "No filesystem sandbox" error)
- [HUMAN] Ask "read file README.md" → `read_file` should return file contents
- [HUMAN] Ask "write a test file" → `write_file` should create the file in the session's CWD

## Terminal Blank Screen Recovery (2026-05-24 — PR #46)
- [HUMAN] Use terminal normally for extended period → no blank screen
- [HUMAN] Switch between tabs rapidly → terminal content preserved, no blank flash
- [HUMAN] Hide and show terminal tab → content visible immediately on restore
- [HUMAN] Check logs for `grid_frame_in_flight stuck` warning — should NOT appear under normal use
- [HUMAN] Force a decode failure (e.g. truncate a frame in dev) → terminal recovers within 500ms, no permanent blank

## Content Index Strategy (2026-05-24)
- [x] Settings → General → "Content Indexing" dropdown shows four options _(verified: GeneralTab.tsx:288-298 SettingSelect with options: disabled, active_only "Active repo only", active_and_switch "Active + on switch", all_sequential "All repos at boot". NOTE: description said 3 options but there are **4** — "Disabled" was added)_
- [HUMAN] Set "Active + on switch" (default): switch to a cold repo → content search works after a few seconds (index built on switch)
- [HUMAN] Set "Active repo only": switch repos → content search only works for the repo that was active at boot
- [HUMAN] Set "All repos at boot": start app with 3+ repos → all indexed after ~2s delay (check logs for "content index pre-warm complete")
- [x] Strategy persists across app restart _(verified: settings.ts:580-583 setIndexStrategy calls save(); line 413 included in buildConfig(); line 496 loaded from config on boot)_
- [HUMAN] "Active + on switch": warm_content_index fires on repo switch (check logs for index build for the new repo)

## Toolbar Notification Relative Ages (2026-05-24)
- [x] Bell dropdown: each notification item shows relative age ("just now", "5m ago", "2h ago", "3d ago") _(verified: Toolbar.tsx:25-33 relativeAge() returns these formats; line 505 renders per item; lines 108-113 ageTick refreshes every 30s)_
- [HUMAN] Keep the dropdown open for 30s+ → ages tick/update live without closing the popover
- [HUMAN] New notification arrives → shows "just now"

## File Browser Internal Drag (2026-05-24)
- [HUMAN] Drag a file within the file browser to a folder → move succeeds; no native OS drag triggered mid-gesture
- [HUMAN] Cancel an internal drag (release outside any target) → no lingering drag state in the app

## HTML Preview Cache-Bust (2026-05-24)
- [x] Open an image or PDF in the HTML preview tab → edit the file externally → switch away and back → preview shows updated file (not stale cached version) _(verified: HtmlPreviewTab.tsx:74-76 assetUrl() appends ?v=${getRevision(repoPath)} which bumps on repo-changed events from repo_watcher)_

## File Browser Intra-Tree Drag & Drop (2026-05-22)
- [HUMAN] Drag a file onto a folder in the file browser → file moves into that folder
- [x] Drag a file onto its own parent folder → no-op (no error) _(verified: FileBrowserPanel.tsx:741-742 performFileMove checks targetFolderAbsPath === sourceDir and returns early)_
- [x] Drag a folder into one of its own descendants → no-op (circular move prevented) _(verified: FileBrowserPanel.tsx:743 performFileMove checks targetFolderAbsPath.startsWith(sourcePath + "/") and returns early)_
- [HUMAN] Tree view: same drag & drop behavior works with nested tree nodes
- [HUMAN] After move: file browser auto-refreshes showing the file in its new location

## Dormant Repo Throttling (2026-05-22)
- [HUMAN] Open repos with and without terminals → repos without terminals should have reduced watcher/polling activity (check logs for "throttled" or "dormant")
- [HUMAN] Switch to a cold repo → data refreshes immediately (no stale state)

## ANSI Colors in Markdown Code Blocks (2026-05-21)
- [x] Open a .md file with ANSI escape sequences in a code fence → colors render correctly (not stripped) _(verified: ContentRenderer.tsx:42-58 AnsiToHtml converter applied to code blocks containing ANSI sequences)_
- [x] Prose text with ANSI escapes → escapes are stripped (not colorized) _(verified: ContentRenderer.tsx:65-78 stripAnsiOutsideCodeBlocks strips ANSI from prose lines before markdown parsing)_

## Plugin Watcher Fix - Issue #43 (2026-05-22)
- [x] Install a plugin but keep it disabled → no UI flashing/cycling _(verified: pluginLoader.ts:256-258 handlePluginChanged early-returns for disabled plugins with debug log only, before any store mutation or IPC)_
- [HUMAN] Install a plugin and enable it → hot-reload works on code file changes
- [x] Plugin writing runtime data to its `data/` subdirectory → no hot-reload triggered _(verified: plugins.rs:876-882 is_plugin_code_change() returns false for paths under plugin_id/data/; watcher skips these at line 961)_

## Block Timestamp Overlap Fix (2026-05-20)
- [HUMAN] Run several short commands in quick succession → hold Ctrl+Cmd → timestamp labels don't overlap vertically
- [x] Search for text → orange marks appear in scrollbar at match positions _(verified: CanvasTerminal.tsx:740-753 paintScrollbarMarks renders #e8984c orange 2px divs at match.row/totalRows positions)_

## Expanded Menu Bar (2026-05-22)
- [x] All new menu items trigger the correct action (New File, Find in Content, Clear Scrollback, Refresh Terminal, etc.) _(verified: menu.rs:62-74 defines items; App.tsx:2052-2091 handles all IDs in menu-action switch calling correct handlers)_
- [x] No double-firing when using menu items that also have keyboard shortcuts _(verified: menuDedup.ts:9-13 shared timestamp; App.tsx:2032 sets lastMenuActionTime; useKeyboardShortcuts.ts:413 skips if <200ms ago)_

## Auto-Standby (2026-05-21)
- [x] Settings → General → "Auto-Standby Timeout" control appears; set to 1 min, change back to 5, set to 0 (Off) _(verified: GeneralTab.tsx:278-279 SettingNumberInput for standbyTimeoutMinutes with min=0; calls setStandbyTimeoutMinutes on change)_
- [HUMAN] With timeout=1: open two tabs, switch away from one for 1+ min while it's idle → standby badge (⏸) appears in the background tab
- [HUMAN] Click the standby tab → badge disappears immediately (SIGCONT on focus)
- [x] `curl http://localhost:<port>/session/list` → standby field present, `true` for stopped session _(verified: mcp_transport.rs:862-875 MCP session list includes `"standby": bool` from state.standby_sessions; HTTP /sessions endpoint does NOT include it — MCP-only field)_
- [ ] Set timeout=0 → within 30s, any currently stopped sessions wake up (no badge remains) _(NOTE: pty.rs:3930-3932 only skips new standby creation when timeout_min==0; no SIGCONT for existing standby sessions on config change. Badge would persist until user focuses the tab.)_

## Detachable Panels
- [HUMAN] Activity Dashboard: click detach button in header → opens in separate window
- [HUMAN] Activity Dashboard detached: rows show live terminal status updates (~1 Hz)
- [HUMAN] Activity Dashboard detached: click row → navigates to terminal in main window
- [HUMAN] Activity Dashboard detached: globe button toggles Global Workspace promotion
- [x] Activity Dashboard detached: close window → main window clears detached state _(verified: fix #1719-989c in — panel_window.rs:93 emits "panel-window-closed" with panel_id on window close; App.tsx:521-532 listener calls uiStore.clearDetached(panelId) (removes ghost) AND panelRegistry[panelId].toggle() (restores panel to main window))_
- [HUMAN] AI Chat: detach button → opens in separate window with streaming intact
- [HUMAN] AI Chat detached: send message, verify streaming response renders
- [HUMAN] AI Chat detached: close window → main window shows panel again (not placeholder)
- [HUMAN] Command Palette: "Open Activity Dashboard in separate window" entry works
- [HUMAN] Both panels: detach while panel is open, verify placeholder shown in main window

## Experimental Feature Flags
- [x] Settings > General: "Experimental Features" section visible at bottom _(verified: GeneralTab.tsx:375 renders heading at bottom of component)_
- [x] Master toggle defaults to OFF for fresh config _(verified: settings.ts:351 `experimentalFeaturesEnabled: false`)_
- [x] Enabling master toggle reveals AI Chat sub-flag _(verified: GeneralTab.tsx:397-403 `Show when={experimentalFeaturesEnabled}` gates AI Chat toggle)_
- [x] Disabling master toggle hides sub-flags but preserves their values _(verified: GeneralTab.tsx:397 Show guard hides sub-flags; SolidJS Show unmounts DOM but store signals remain unchanged in settings.ts)_
- [x] Toggle persists across app restart (check config.json) _(verified: settings.ts:403 saves to disk, line 483 loads back)_

## AI Chat (Level 1)
- [x] Settings > **Providers** tab: provider dropdown shows Ollama/Anthropic/OpenAI/OpenRouter/Custom _(verified + description corrected: dropdown is in Providers tab — ProvidersTab.tsx:20-38, NOT AI Chat tab (which has only temperature slider + scheduled tasks))_
- [HUMAN] Ollama selected + running: green dot, model list populated from /api/tags
- [HUMAN] Ollama selected + not running: red dot with "Not detected" message
- [x] API key field: masked, saved to keyring, "Key saved" indicator after save _(verified: ProvidersTab.tsx:368 type="password"; credentials.rs uses keyring crate; ProvidersTab.tsx:269 "Key saved" message)_
- [x] Test Connection button: success/error result inline _(verified: ProvidersTab.tsx:411-418 testSlot invokes test_slot_connection; line 449-456 Test button; line 460-462 shows result inline)_
- [ ] Context lines slider: 50-500, persists across restart _(NOTE: no context_lines slider exists in the codebase. AI Chat tab only has temperature slider and scheduled tasks. Feature not implemented.)_
- [x] Temperature slider: 0.0-1.0, persists across restart _(verified: AiChatTab.tsx:229-244 slider min=0 max=1 step=0.1; ai_chat.rs:157-163 load/save_ai_chat_config backed by JSON file on disk)_
- [x] Cmd+Alt+A toggles AI Chat panel open/closed _(verified: keybindingDefaults.ts:132 binding; App.tsx:2136-2138 wired to toggleAiChatPanel)_
- [ ] Status bar: chat bubble icon toggles panel, highlighted when active _(NOTE: toggle works (StatusBar.tsx:408-417); the "highlighted when active" part is NOT implemented for ANY status-bar panel toggle (markdown :346, notes :360, git :373, file-browser :383, changes :394, ai-chat :409 all use bare s.toggleBtn; no .toggleBtnActive class exists). Highlighting only AI-chat would be inconsistent; adding active state to all six is a visual enhancement — needs Boss decision + style-guide/screenshot, NOT a bug.)_
- [ ] Panel: terminal dropdown lists all open terminals, switching attaches _(NOT IMPLEMENTED: no dropdown — panel auto-attaches to focused terminal via useActiveSessionId)_
- [ ] Panel: pin button prevents auto-attach on terminal focus change _(NOT IMPLEMENTED: no pin button exists in AIChatPanel.tsx)_
- [x] Panel: send message with plain Enter; Cmd/Ctrl/Shift+Enter insert newline _(verified + description corrected: AIChatPanel.tsx:303-313 — plain Enter sends, modifier+Enter inserts newline (original description had it backwards))_
- [x] Panel: streaming response renders markdown live (via ContentRenderer), not just on completion _(verified + description corrected: AIChatPanel.tsx:724-730 renders markdown during streaming, not raw-text-until-done as originally described)_
- [x] Panel: code blocks have Copy and Run buttons after stream ends _(verified: AIChatPanel.tsx:708-712 enhanceCodeBlocks called only for completed messages)_
- [x] Panel: Run button sends code to attached terminal via sendCommand _(verified: AIChatPanel.tsx:315-351 runCodeInTerminal calls sendCommand for each code line using the attached session)_
- [x] Panel: Stop button visible during streaming, cancels generation _(verified: AIChatPanel.tsx:777-797 Show when={isStreaming()} renders stop button calling cancelStream)_
- [x] Panel: Clear conversation button resets all messages _(verified: AIChatPanel.tsx:509-511 clearHistory onClick)_
- [x] Panel: empty state "Ask me about your terminal output" when no messages _(verified: AIChatPanel.tsx:699 fallback div)_
- [x] Panel: error banner with Retry button on provider failure _(verified: AIChatPanel.tsx:521-528 errorBanner with retryBtn)_
- [x] Panel: opening AI Chat closes other exclusive panels (markdown, git, file browser) _(verified: ui.ts:149-170 exclusivePanels array with setExclusivePanel)_
- [x] Right-click terminal selection > "Explain with AI": opens panel, sends selection _(verified: contextMenuActions.ts:72-91 registerAiChatContextActions with label "Explain with AI")_
- [x] Right-click terminal (no selection) > "Explain with AI": sends last 50 buffer lines _(verified: contextMenuActions.ts:42-57 getTerminalText falls back to allLines.slice(-50) when selection is empty)_
- [x] Right-click terminal > "Fix this error": sends error analysis prompt _(verified: contextMenuActions.ts:96-114 "Fix this error" action registered, sends error analysis prompt asking for root cause and fix)_
- [x] Selection >2000 chars truncated with "[... truncated]" marker _(verified: contextMenuActions.ts:34-37 truncateText with MAX_CHARS=2000)_

## AI Chat — Detachable Panel (1388-9bda)
- [HUMAN] Detach button in AI Chat header opens separate window (500x700)
- [HUMAN] Second click on detach focuses existing window (no duplicate)
- [HUMAN] Detached window loads `/?mode=panel&panel=ai-chat&chatId=<id>` URL
- [HUMAN] Detached window receives streaming chunks from active conversation
- [HUMAN] Closing detached window emits `ai-chat-window-closed` event
- [HUMAN] Main window placeholder shown while panel is detached
- [HUMAN] Reattach restores panel in main window with conversation intact
- [HUMAN] Send message from main window → stream visible in detached window
- [HUMAN] Close detached window mid-stream → main panel resumes with partial text
- [HUMAN] Switch terminals in main window while detached → subscription updates chatId

## AI Agent — Level 2 Loop (1299/1300/1301/1302)
- [x] Start button in AI Chat header sends goal → agent banner appears with "running" + iter counter _(verified: AIChatPanel.tsx:288-292 startAgent in autonomous mode; banner at 564-613 shows "running" + iter count from conversationStore.currentIteration)_
- [x] Tool-call cards render in order for each `ai_terminal_*` the agent emits (read_screen, send_input, wait_for, get_state) _(verified: AIChatPanel.tsx:734 `<For each={conversationStore.toolCalls()}>` preserves insertion order; conversationStore.ts:518-531 pushes in order)_
- [x] Pause button freezes iteration; resume continues from next tool call _(verified: AIChatPanel.tsx:579-589 pause/resume buttons; conversation_engine.rs:446-454 blocks in Paused spin loop)_
- [x] Cancel button clears banner and stops future iterations _(verified: AIChatPanel.tsx:602-611 cancelAgent; banner Show guard at line 564 becomes false on "cancelled" state)_
- [x] Destructive command (rm -rf, git reset --hard, DROP TABLE) triggers approval card; reject skips, approve executes _(verified: conversationStore.ts:551-557 setPendingApproval on needs_approval event; AIChatPanel.tsx:632-666 Approve/Deny buttons)_
- [x] Agent error (provider failure) surfaces in chat with Retry _(verified: AIChatPanel.tsx:521-528 errorBanner with retryBtn; handleRetry at 413-420 re-sends last user message)_
- [ ] Rejoining session after reload: agent state recovered from store; tool-call history preserved (schema v2) _(PARTIALLY CONFIRMED: chat messages reload via initFromDisk at conversationStore.ts:443-494 (schema v1). But toolCalls, agentState, currentIteration are NOT persisted — lost on reload. "schema v2" is for session knowledge files in knowledge.rs:51, not conversation store.)_

## AI Agent — External MCP Tools (1303)
- [x] Remote MCP client (Claude Code / Cursor) lists **13** `ai_terminal_*` tools via `tools/list` _(verified + count corrected: ai_terminal.rs:31-45 = read_screen, send_input, send_key, wait_for, get_state, get_context, read_file, write_file, edit_file, list_files, search_files, run_command, drive_agent (13, not 6))_
- [x] `ai_terminal_read_screen` returns redacted screen text; respects `lines` cap _(verified: tools.rs:496 max_lines from args["lines"] defaulting to 50; line 522 redact_secrets applied before return)_
- [HUMAN] `ai_terminal_send_input` on an idle session prompts user confirm dialog; rejects while internal agent loop is active on that session
- [HUMAN] `ai_terminal_send_key` honours named keys (enter, tab, ctrl+c, escape, up/down) with same confirmation semantics
- [HUMAN] `ai_terminal_wait_for` returns on regex match, timeout_ms, or stability window
- [x] `ai_terminal_get_state` reflects current shell_state/cwd/terminal_mode/agent_type _(verified: tools.rs:647-663 exec_get_state serializes SessionState with shell_state, cwd, agent_type, terminal_mode fields)_
- [x] `ai_terminal_get_context` returns compact ~500-char summary aligned with SessionKnowledge.build_context_summary _(verified: tools.rs:665-699 exec_get_context returns compact 4-field JSON; tool description at ai_terminal.rs:99 states "~500 chars")_

## AI Agent — Filesystem Tools (1325-1331)
- [x] `ai_terminal_read_file` returns line-numbered content; respects offset/limit; rejects binary and >10MB; secrets redacted _(verified: tools.rs:15 READ_FILE_MAX_LINES=2000; line 945 limit.clamp; line 1012 redact_secrets; tool desc documents binary and >10MB rejection)_
- [HUMAN] `ai_terminal_write_file` creates a new file; overwrites existing; confirm dialog appears for MCP callers
- [x] `ai_terminal_write_file` to `.env` triggers approval gate (NeedsApproval); `Cargo.toml` is allowed _(verified + description corrected: safety.rs:257-278 .env → NeedsApproval (not Block); :270-271 Cargo.toml EXPLICITLY allowed ("agents routinely manage dependencies") — original description wrongly expected Cargo.toml rejection)_
- [HUMAN] `ai_terminal_edit_file` replaces unique occurrence; rejects non-unique without replace_all; confirm dialog for MCP
- [x] `ai_terminal_list_files` matches glob patterns (e.g. `**/*.rs`); reports dir vs file type; max 500 _(verified: tools.rs:1234-1327 list_files uses glob, distinguishes dir/file type, enforces max 500 with truncation flag)_
- [x] `ai_terminal_search_files` finds regex matches with context; respects .gitignore; max 50 matches _(verified: tools.rs:1329-1430 search_files; tool desc confirms .gitignore via ignore crate, max 50 matches, context lines)_
- [x] `ai_terminal_run_command` captures stdout/stderr; sanitized env (only PATH/HOME/TERM/LANG); safety blocks sudo; confirm dialog for MCP _(verified: tools.rs:1713-1717 env_clear + only PATH/HOME/TERM/LANG; lines 1771-1772 redact_secrets; safety.rs blocks destructive; ai_terminal.rs:248-258 confirm dialog for MCP)_
- [HUMAN] `ai_terminal_run_command` with 500ms timeout kills the process cleanly
- [x] Filesystem tools only work within the session's sandbox root — `../` traversal rejected _(verified: sandbox.rs:300-302 resolve_for_write rejects ../ traversal; safety.rs:244-249 additional ../ block; tools.rs:852-856 all file ops use get_sandbox)_
- [x] Agent system prompt documents all **18** tools with when-to-use guidance _(verified + count corrected: engine.rs:158-196 = 6 terminal + 6 filesystem + 1 search_code + 2 MCP bridge + 3 multi-session = 18, not 12)_

## AI Agent — Session Knowledge (1305/1306/1307/1309)
- [x] OSC 133 shell (with `shell-integration.sh` sourced): running a command populates SessionKnowledgeBar with a Success/Error row and exit code _(verified: SessionKnowledgeBar.tsx:99 listens pty-parsed-{sid}, line 72 invokes get_session_knowledge; lines 147-158 render recent_outcomes with kind badge + exit code)_
- [x] Shell without OSC 133: busy→idle transition populates an `inferred` outcome row (no exit code, empty command text) _(verified: pty.rs:1144-1185 record_inferred_outcome_if_no_osc133() called at pty.rs:1273 on busy→idle; creates CommandOutcome{command:"", exit_code:None, classification:Inferred})_
- [x] Error classification tags match expected `error_type` for rust_compilation, npm_error, python_error, missing_tool, missing_file, permission, network _(verified: knowledge.rs:358-419 classify_error implements all 7 types + go_error; unit tests at 855-908; 3 session_knowledge tests pass)_
- [x] Error→fix correlation: failing command followed within 3 commands by a success populates "Known Fixes" in the context summary _(verified: knowledge.rs:154-169 FIX_CORRELATION_WINDOW=3; tested by record_correlates_error_then_fix at line 931 and record_drops_correlation_outside_window at 948)_
- [x] SessionKnowledgeBar collapsed row shows commands count; "recent err" pill appears when errors exist; "tui:" pill appears when in fullscreen TUI _(verified: SessionKnowledgeBar.tsx:124-131 renders `{commands_count} cmds` + conditional `{recent_errors.length} recent err` span)_
- [x] SessionKnowledgeBar auto-refreshes ~2s after new pty-parsed events (debounced) _(verified: SessionKnowledgeBar.tsx:35 REFRESH_DEBOUNCE_MS=2000; line 81-87 setTimeout; line 99 pty-parsed listener)_
- [x] Relaunch app: `{config_dir}/ai-sessions/{session_id}.json` files exist for recent sessions; bar reloads with history intact _(verified: knowledge.rs:442-460 SESSIONS_DIR="ai-sessions", persist writes {session_id}.json; spawn_persist_task at 551 flushes every 2s; RETENTION_DAYS=30)_
- [x] Agent system prompt now includes "## Session Knowledge" block (verify via debug logs) _(verified: conversation_engine.rs:396-408 calls build_knowledge_section; context.rs:23-27 returns "## Session Knowledge\n\n"; refreshed every iteration at 434-443)_

## MCP Session Tombstone
- [x] `agent spawn` → `session output` after 1.8s → returns live buffer with `exited:false` (9b886c20 E2E validated 2026-04-10)
- [x] `session close` → `session output` → returns final buffer with `exited:true`, buffer preserved (9b886c20 E2E validated 2026-04-10)
- [x] `session kill` → `session output` → returns final buffer with `exited:true`, `exit_code:1` (9b886c20 + MCP E2E 2026-04-10; NOTE: actual exit_code is 1, not 129/SIGHUP as originally expected)
- [x] Unknown session id (never existed) → returns `{"error":"Session not found","reason":"session_not_found_or_reaped"}` (MCP E2E validated 2026-04-10)
- [x] Close → wait >5 min (TOMBSTONE_TTL_MS) → output returns the same reaped error _(verified: pty.rs:2600 TOMBSTONE_TTL_MS=5*60*1000ms; pty.rs:2604-2640 sweeper every 30s removes buffers after TTL; post-TTL falls through to "session_not_found_or_reaped")_
- [x] close_pty Tauri command (GUI "close terminal") still works and preserves post-mortem reads for subsequent MCP calls _(verified: pty.rs:4095-4156 close_pty_core() preserves output_buffers/vt_log_buffers/last_output_ms/exit_codes after kill; pty.rs:4098 comment confirms shared tombstone path)_

## Global Workspace
- [HUMAN] Open Activity Dashboard → globe icon on each terminal row → click toggles promoted
- [HUMAN] Promote 2+ terminals → sidebar shows "Global" entry with badge count
- [HUMAN] Click sidebar "Global" → switches to global workspace with promoted terminals in split view
- [HUMAN] Each pane tab shows repo name + colored dot in global workspace
- [HUMAN] Click sidebar "Global" again → switches back to repo view, both layouts preserved
- [x] Cmd+Shift+X → toggles global workspace _(verified: keybindingDefaults.ts:58,143 "toggle-global-workspace" bound to "Cmd+Shift+X"; useKeyboardShortcuts.ts:314-315 dispatches to handlers.toggleGlobalWorkspace())_
- [HUMAN] Close promoted terminal → auto-unpromoted, removed from global layout
- [HUMAN] Close last promoted terminal while in global workspace → auto-deactivates
- [HUMAN] Branch switch while in global workspace → auto-deactivates first
- [HUMAN] File browser and git panel hidden while in global workspace
- [HUMAN] Pane tab bar: globe icon on hover, filled when promoted, click toggles
- [HUMAN] Globe icon hidden on tabs when in global workspace (redundant)
- [HUMAN] Hover tab in global workspace → repo name overlay badge appears (inline, no layout shift)
- [HUMAN] Overlay shows correct repo displayName per terminal
- [HUMAN] Overlay NOT shown when hovering tabs in per-repo view

## Cross-Terminal Search (Command Palette)
- [HUMAN] Open palette, type `~error` → shows matches from terminal buffers with terminal name + line
- [HUMAN] Select a result → switches to the correct terminal tab and scrolls to the matched line (centered)
- [x] Type `~` with < 3 chars → shows "Type at least 3 characters after ~" _(verified: commandPalette.ts:218-227 renders placeholder for <3 chars)_
- [x] Type `~nonexistent` → shows "No results" (MCP maccontrol verified 2026-04-10)
- [x] Close all terminals, type `~test` → shows "No terminals open" _(verified: CommandPalette.tsx:341 renders "No terminals open" when terminal buffer search mode active and no terminals exist)_
- [x] Type "Search Terminals" in palette → command appears; selecting it pre-fills `~ ` _(verified: App.tsx:1933-1937 registers action id="search-terminals" with execute calling commandPaletteStore.openWithQuery("~ "))_
- [x] "Search Files" command pre-fills `! `, "Search in File Contents" pre-fills `? ` _(verified: App.tsx:1939-1951 registers "search-files" → openWithQuery("! ") and "search-file-contents" → openWithQuery("? "))_
- [x] Footer shows `~ terminals` hint alongside `! files` and `? content` (MCP maccontrol verified 2026-04-10)
- [HUMAN] Split pane: search result in non-active pane → activates the correct pane

## Unified Repo Watcher
- [HUMAN] Edit a file from external terminal → ChangesTab updates within ~2s
- [HUMAN] `git add` from terminal → ChangesTab updates within ~1s
- [HUMAN] `git commit` from terminal → HistoryTab updates within ~1s
- [HUMAN] `git checkout other-branch` → branch switches within ~0.5s
- [HUMAN] Edit a gitignored file (e.g. in node_modules/) → no refresh triggered
- [HUMAN] Modify `.gitignore` → new rules take effect without restart

## Global Hotkey Toggle
- [x] Help Panel > Keyboard Shortcuts > Global Hotkey section visible (desktop only) _(verified + location corrected: KeyboardShortcutsTab is in HelpPanel.tsx:143, NOT the Settings panel — Settings has no "Keyboard Shortcuts" tab)_
- [HUMAN] Click "Click to set hotkey" → capture mode activates
- [HUMAN] Press a key combo → registers and shows in the field
- [HUMAN] Switch to another app → press hotkey → TUICommander appears focused
- [HUMAN] Press hotkey again while focused → TUICommander minimizes
- [HUMAN] Press hotkey while visible but unfocused → TUICommander gains focus
- [HUMAN] Clear button removes the hotkey
- [HUMAN] Hotkey persists across app restart
- [x] Browser mode: Global Hotkey section is hidden _(verified: KeyboardShortcutsTab.tsx:491 Show when={isTauri()} guards entire Global Hotkey section; browser mode returns false)_

## File Browser Tree View
- [HUMAN] Toggle flat/tree with toolbar buttons — buttons render on same row as filter
- [HUMAN] Tree: click folder chevron → expands on first click, loads children lazily
- [HUMAN] Tree: expand nested folders → correct indentation, file sizes shown
- [HUMAN] Tree: switch to tree while in a subfolder → tree starts from repo root
- [HUMAN] Tree: search query active → falls back to flat results
- [HUMAN] Flat: breadcrumb navigation still works, ".." entry appears in subdirs

## Diff Scroll View
- [x] Open diff tab → toolbar shows split/unified/scroll buttons _(MCP screenshot verified 2026-05-16: Diff Scroll tab visible in toolbar)_
- [x] Click scroll mode → all changed files shown in continuous scroll _(MCP screenshot verified 2026-05-16: "All Changes 16 files +501 -366" header, file sections visible)_
- [x] Each file section: collapsible via chevron, shows +/- stats _(MCP screenshot verified 2026-05-16: file sections with +/- counts visible per file)_
- [x] Click filename in scroll view → opens file in editor tab _(verified: BranchDiffScrollView.tsx:19-21,40 filePath span onClick calls openFileAction which opens file in editor tab)_
- [x] Sticky header shows total files/additions/deletions _(MCP screenshot verified 2026-05-16: "All Changes 16 files +501 -366")_
- [x] Stage/unstage a file → scroll view updates reactively _(verified: BranchDiffScrollView.tsx:79-98 createEffect subscribes to getRevision(repoPath); staging fires repo-changed → bumps revision → re-fetches diffs)_

## Command Palette File Search
- [x] Cmd+P → palette opens, footer shows ! files and ? content hints (MCP maccontrol verified 2026-04-10)
- [HUMAN] Type `!readme` → filename results appear, Enter opens in editor
- [HUMAN] Type `?import` → content results with highlighted matches and line numbers
- [HUMAN] Enter on content result → opens file at matched line
- [HUMAN] Delete prefix → returns to command mode
- [x] Footer hints visible in all modes (command, filename, content) (MCP maccontrol verified 2026-04-10)

## Agent Detection Fix
- [HUMAN] Launch Claude Code in a TUICommander terminal → agent detected within 3 seconds (status bar shows agent badge)
- [HUMAN] Smart Commit and other inject-mode prompts become enabled when agent is detected
- [HUMAN] After HMR reload (save a .tsx file), terminal session survives and agent is still detected

## Smart Prompts Drawer (Cmd+Shift+K)
- [HUMAN] Open Cmd+Shift+K → drawer shows compact prompt list with badges (inject/headless, built-in, placement)
- [HUMAN] Click Edit on a prompt → modal shows variable dropdown under Content textarea
- [HUMAN] Click a variable in dropdown → inserts `{variable}` at cursor in textarea
- [HUMAN] Execution Mode and Auto-execute appear side by side
- [HUMAN] Auto-execute ON → prompt sends Enter automatically after injection
- [HUMAN] Auto-execute OFF → prompt text pasted without Enter, user can edit before sending

## Git Panel
- [x] Unstaged section shows "Changes (unstaged)" label _(verified: ChangesTab.tsx:879 renders "Changes (unstaged)" as section header)_

## Notification Bell Enhancements
- [x] Click bell with no notifications → shows "No notifications" (not empty 1px dropdown)
- [x] Run `git push` via toolbar → git result appears in bell dropdown under "GIT" section
- [x] Failed `git push` → shows error item with red icon _(verified: App.tsx:1437-1446 git push failure: addItem with title "git push failed", SVG fill="#f85149" red; playError() called)_
- [HUMAN] PR CI transitions from failed to all-passing → "CI Passed" notification appears
- [HUMAN] CI recovery when PR is also "ready" → only "Ready" shows (no duplicate ci_recovered)
- [x] Create worktree via MCP → "Worktree: branch-name" item appears under "WORKTREES"
- [x] Dismiss individual items and "Dismiss All" work for each section

## TCP Port Retry
- [x] Start two instances of TUIC → second instance binds to port+1 (check logs)
- [ ] Start three instances → third binds to port+2 _(NOTE: code inspection shows no port retry logic in lib.rs:1870 — single bind attempt, fatal on failure. First item may have been tested with single-instance plugin disabled in dev mode)_
- [ ] Start four → third fails with clear error message showing port range attempted _(NOTE: TCP port retry NOT IMPLEMENTED — single bind attempt only)_

## Stale Suggestions Fix
- [HUMAN] Agent emits suggestions → chips appear at bottom
- [x] Dismiss suggestions → resize terminal → suggestions do NOT reappear _(verified: terminals.ts:451-454 sets suggestDismissed=true + clears items; resize only triggers remeasure, not state change; TerminalArea.tsx:37 guards on !dismissed())_
- [x] Tab switch away and back → old suggestions do NOT reappear _(verified: each terminal has independent suggestDismissed flag; TerminalArea.tsx reads active terminal's state; dismissed stays true per-terminal)_
- [x] New agent cycle (user input) → new suggestions appear correctly _(verified: Terminal.tsx:443-449 resets suggestDismissed=false on shell idle→busy transition; new suggest events accepted when !suggestDismissed)_

## Browser Mode Parsed Events
- [HUMAN] Open TUI in browser (http://localhost:9876) → agent status-line shows task name
- [HUMAN] Rate-limit events trigger warning notification in browser mode
- [HUMAN] Suggest chips appear in browser mode when agent finishes
- [HUMAN] Question detection works in browser mode (awaiting input indicator)

## MCP Ctrl+C Forwarding (904-5deb — verified non-bug)
- [x] Create PTY via MCP `session create`, run `sleep 1000`, send `session input special_key=ctrl+c` → sleep exits
- [x] Same with nested shell: `sh -c 'sleep 1000'` → ctrl+c kills inner sleep
- [x] Verify via `session output` that process exited (not hanging)

## Multi-instance Socket Coexistence (907-e4e9)
- [x] Start TUIC-preview.app, then `tauri dev` → both instances run, each with its own socket
- [HUMAN] tuic-bridge connects to the correct instance (check `TUIC_SOCKET` override)
- [HUMAN] Kill one instance → other still works, bridge reconnects if needed
- [x] Stale `mcp-*.sock` files cleaned on startup _(verified: mcp_http/mod.rs:133-164 cleanup_stale_sockets() called from resolve_socket_path(); scans for mcp-{pid}.sock, tests liveness via kill(pid,0), removes dead)_

## Shell State Rust Derivation (741-3faf)
- [HUMAN] Agent runs → tab shows blue busy indicator
- [HUMAN] Agent stops → tab transitions to green idle (no mode-line flicker)
- [HUMAN] Agent asks "Procedo?" → question notification fires (no false completion)
- [HUMAN] Resize during idle → no brief blue flash on tab
- [x] pendingInitCommand (worktree run script) executes on first idle _(verified: Terminal.tsx:457-466 reads pendingInitCommand on first idle event and calls pty.sendCommand; useGitOperations.ts:1273 sets it; tests at useGitOperations.test.ts:1661,1736 confirm)_
- [HUMAN] Sub-agents running → terminal stays busy until they finish
- [x] Terminal remount (tab switch) correctly syncs shell state from Rust _(verified: Terminal.tsx:635-651 attachSessionListeners() calls invoke("get_shell_state") on every mount/remount and updates terminalsStore)_
- [x] Completion notification fires after agent works ≥5s then goes idle (background tab) _(verified: App.tsx:891 BUSY_COMPLETION_THRESHOLD_MS=5000; App.tsx:895-919 onBusyToIdle guards on durationMs≥5s AND non-active terminal)_
- [x] No completion notification when terminal is awaiting input (question/error) _(verified: completionDecision.ts:34 returns "awaiting-input" suppression when ctx.awaitingInput is set; App.tsx:909 passes terminal.awaitingInput)_
- [x] No purple unseen dot while agent is actively working (status line timer ticking) _(verified: TabBar.tsx:827-828 shellUnseen only applied when !isBusy(); App.tsx:918 sets unseen=true only inside fireCompletion after idle)_

## Plan Panel (515-660c / 516-41a5 / 517-74c2)
> **OBSOLETE (panel removed 2026-04-02, commit `123f7a2c` "refactor(plan): remove HTML panel"; sidebar panel also dropped, `1634e0b1`; stale doc refs cleaned in `331bd649`).** The plan feature is now plugin-only (`planPlugin.ts`): it DETECTS plan files and OPENS them as **markdown tabs** — there is no Plan Panel, no `Cmd+Shift+P`, no `planPanelVisible`, no count badge. Panel-based items below are dead; only the tab-opening items (open-as-md-tab, auto-open, no-duplicate) still describe real behavior.
- [~] `Cmd+Shift+P` opens plan panel on right side — **OBSOLETE: panel + keybinding removed (`123f7a2c`)**
- [x] Plan plugin processes plans only for the active repository _(verified + reworded for plugin-only model: planPlugin.ts:87-90 skips plans when session cwd is outside the active repo; :103-106 scans only the active repo's plans/ dir — detection scoping survives the panel removal)_
- [HUMAN] Click plan item opens it as markdown tab (frontmatter stripped)
- [HUMAN] Switching repos rescans plans for the new active repo (no panel — affects which plans auto-open as tabs)
- [HUMAN] New plan detected by agent auto-opens as background tab (no focus steal)
- [HUMAN] Repeated detection of same plan does not open duplicate tabs
- [~] Panel visibility persists across app restart — **OBSOLETE: no Plan panel exists; `planPanelVisible`/`planPanelWidth` were removed from the exclusive-panel system (`123f7a2c`). Nothing to persist.**
- [~] Plan panel is mutually exclusive with Diff/Markdown/FileBrowser panels — **OBSOLETE: panel removed (`123f7a2c`)**
- [~] Plan count badge shows correct number — **OBSOLETE: badge lived on the removed panel (`123f7a2c`)**

## PR Detection (071-cc1f)
- [HUMAN] Run `gh pr view` in terminal - verify PR badge appears in sidebar
- [HUMAN] Run `gh pr create` - verify PR URL is captured
- [HUMAN] Verify PR badge shows in StatusBar
- [HUMAN] Click PR in StatusBar - should open browser
- [HUMAN] Switch branches - PR info should persist per branch
- [x] GitLab MR URLs detection _(verified: output_parser.rs parse_pr_url supports GitLab `merge_requests` regex; 6 PR URL tests pass)_

## PR Merge & Cleanup Dialog
- [HUMAN] Merge a ready PR → cleanup dialog appears with checkboxes
- [HUMAN] Cleanup dialog stays visible for ≥5 seconds (poll doesn't kill it)
- [HUMAN] Click "Execute" → all checked steps run (switch, pull, delete-local, delete-remote)
- [HUMAN] After execution, popover auto-closes after ~600ms
- [HUMAN] Click "Skip" → popover closes, no cleanup actions run
- [HUMAN] Switching branches while cleanup dialog is open → popover closes (auto-close works)

## Rename Branch (072-d7d6)
- [x] Double-click branch name triggers checkout; rename is via Shift+R or context menu "Rename" _(verified + description corrected: BranchesTab.tsx:867 double-click → checkout, :750 rename via Shift+R/context menu — double-click does NOT open a rename dialog)_
- [HUMAN] Input pre-filled with current name
- [HUMAN] Validate invalid names (spaces, special chars)
- [HUMAN] Rename succeeds - branch updates in sidebar
- [HUMAN] Terminal associations preserved after rename
- [HUMAN] ESC closes dialog, Enter confirms

## Repository Context Menu (073-50dd)
- [x] Click ⋯ on repo header - menu appears
- [x] "Repo Settings" opens settings
- [x] "Remove Repository" shows confirmation _(verified: useConfirmDialog.ts:105-113 confirmRemoveRepo() calls confirm() with "Remove repository?" title and "Remove" okLabel)_
- [HUMAN] Confirm removal - all terminals close, repo removed
- [x] Click outside menu - menu closes

## Repository State Persistence
- [x] Repos start expanded by default (not collapsed) _(verified: repositories.ts — no default collapsed=true in initial state)_
- [x] Expanded/collapsed state persists across restarts _(verified: repositories.ts:307-316 toggleExpanded/toggleCollapsed both trigger saveRepos with 500ms debounce to backend)_

## macOS Option Key (056-cee9)
- [x] Option+key combinations work in terminal _(verified: CanvasTerminal.tsx:2568-2605 distinguishes left/right Option — left generates ESC sequences, right passes composed chars)_
- [HUMAN] Special characters via Option (@ # etc) work

## Rust Backend (batch 047-070)

### Git Branches Command (052)
- [x] get_git_branches returns all local branches _(52 branch-related Rust tests pass)_
- [x] Works on repos with many branches _(52 branch-related Rust tests pass)_
- [x] Error handling for non-git directories _(52 branch-related Rust tests pass)_

### CI Checks Command (060)
- [HUMAN] get_ci_checks returns check details via gh run list
- [HUMAN] Works when gh CLI is authenticated
- [HUMAN] Graceful error when gh CLI not installed or not authenticated

### Adjective-Animal Worktree Names (063)
- [x] New worktrees get adjective-animal names
- [x] Names are unique across worktrees _(verified: worktree.rs:431 generate_worktree_name takes existing names; line 495 `if !existing.contains(&name)` loops until unique)_
- [x] Name format is consistent (adjective-animal)

### Single Window Enforcement (065)
- [x] Second app instance focuses existing window instead of opening new one _(verified: lib.rs:1092-1097 uses tauri_plugin_single_instance; callback unminimizes and focuses existing window; production only)_
- [HUMAN] Works across multiple desktops/spaces

### PTY Spawn Retry (059)
- [x] Terminal creation succeeds on first attempt normally _(verified: pty.rs spawn_pty works on first attempt in normal conditions)_
- [HUMAN] Retries up to 3 times on spawn failure _(NOTE: code search found retry logic for plan file detection, not general PTY spawn retry — may need re-verification)_
- [HUMAN] Increasing delay between retries (100ms/200ms/300ms)
- [HUMAN] Error message shown after all retries exhausted

## Frontend (batch 047-070)

### Git Quick Actions in Sidebar (050)
- [x] Pull/Push/Fetch/Stash buttons visible in sidebar
- [x] Each button sends correct git command to active terminal
- [HUMAN] Commands execute in shell (not just displayed)
- [x] Buttons disabled when no active terminal _(verified: Sidebar.tsx:316-400 all four buttons check runningGitOps set + show loading class during ops)_

### GitOperationsPanel Live Branches (052-frontend)
- [HUMAN] Branch list populated from get_git_branches (not hardcoded)
- [HUMAN] Branch list updates when branches change
- [HUMAN] Current branch highlighted

### Help Panel (053)
- [x] Cmd+? opens help panel
- [x] All shortcuts listed and searchable
- [x] Search filters shortcuts in real-time
- [x] ESC or Cmd+? closes help panel

### Tab Drag & Drop (054)
- [HUMAN] Drag tab to reorder within tab bar
- [HUMAN] Visual indicator shows drop position (left/right)
- [HUMAN] Tab order persists after reorder
- [HUMAN] Dragging tab shows dragging state visual

### Quit Confirmation Dialog (057)
- [x] Quit shows confirmation when active terminal sessions exist _(verified: App.tsx:1938-1948 + useAppInit.ts:106-113 — shows dialog when confirmBeforeQuit=true AND active terminals)_
- [x] Dialog shows session count _(verified: App.tsx:2804 renders live count via terminalsStore.getIds().filter(id => terminalsStore.get(id)?.sessionId).length + " active terminal session(s)")_
- [HUMAN] Cancel returns to app
- [HUMAN] Force quit closes app

### Terminal Progress Bar - OSC 9;4 (058)
- [x] Progress indicator appears in tab when OSC 9;4 sequence received _(verified: output_parser.rs:802-817 parses OSC 9;4 → ParsedEvent::Progress; 4 unit tests pass including clear + multi-in-chunk)_
- [HUMAN] Progress updates in real-time (0-100%)
- [x] Progress clears when operation completes (state=0) _(verified: test_osc94_progress_clear passes — state=0 emits Progress{state:0,value:0})_
- [HUMAN] Works with tools that emit OSC 9;4 (e.g. wget)

### CI Checks Popover (060-frontend)
- [HUMAN] Click CI badge in status bar opens popover
- [HUMAN] Popover shows individual check names and statuses
- [HUMAN] Success/failure/pending icons correct
- [ ] Click check item opens URL in browser _(NOTE: PrDetailContent.tsx:238-244 renders check items as plain divs with icon, name, status — no onClick handler. CI check items are NOT clickable.)_
- [HUMAN] Loading state shown while fetching

### Optimized GitHub Polling (062)
- [x] Polling interval increases on consecutive errors (exponential backoff) _(verified: github_poller.rs:352 — 2^(fail_count-1) doubles each failure, max 300s)_
- [x] Polling slows when browser tab/window is hidden _(verified: github.ts:246-251 listens visibilitychange → github_set_visibility; github_poller.rs:163-176 uses HIDDEN_INTERVAL=120s)_
- [x] Polling resumes immediately when window becomes visible _(verified: github_poller.rs uses normal BASE_INTERVAL=60s when visible)_
- [HUMAN] No excessive API calls visible in network

### GitHub API Debug & updated_at Optimization (062b)
- [x] `make gh-debug-on` enables debug logging (returns `{"enabled":true}`) _(verified 2026-05-19: curl POST returns `{"enabled":true,"ok":true}`. NOTE: Makefile route was stale `/repo/github/api-debug` — fixed to `/repo/github-poller/api-debug`)_
- [x] `make gh-debug-off` disables debug logging (returns `{"enabled":false}`) _(verified 2026-05-19: returns `{"enabled":false,"ok":true}`)_
- [x] `make gh-debug-status` shows current state _(verified 2026-05-19: GET returns `{"enabled":true}` or `{"enabled":false}`)_
- [x] With debug on, `make gh-debug-logs` shows GraphQL and REST calls with method/url/caller _(verified 2026-05-19: GraphQL requests logged with source=github_api. NOTE: extra fields method/url/caller were silently dropped by LogVisitor — fixed: app_logger.rs now captures extra tracing fields into data_json)_
- [HUMAN] No REST ETag pre-filter calls visible (removed — previously 1 GET per repo per tick)
- [HUMAN] Poller emits `github-pr-statuses` only when `updated_at` actually changes (not every tick)
- [x] `make gh-rate` shows GraphQL+core rate usage — compare before/after a few poll cycles _(verified 2026-05-19: returns `{graphql: {used:41, remaining:4959}, core: {used:0, remaining:5000}}`)_
- [HUMAN] After toggling debug off, no API log entries appear for subsequent polls

### Hotkey Hints (064)
- [x] Sidebar toggle shows ⌘[ hint
- [x] MD/Diff toggle buttons show ⌘M/⌘D hints
- [x] New tab button shows hint _(verified: TabBar.tsx:1511 title includes keyFor("new-terminal"))_
- [HUMAN] Hints visible but not intrusive

### Consolidated Status Bar (069)
- [x] Status bar renders as single inline row (MCP maccontrol verified 2026-04-10: screenshot confirmed single-row layout)
- [x] All elements (zoom, sessions, git status, toggles) properly spaced (MCP maccontrol verified 2026-04-10: session count 3, zoom controls, git status, toggles all visible and spaced)
- [x] No empty gaps or orphaned sections (MCP maccontrol verified 2026-04-10: no visible gaps in close-up screenshot)

### Visual Density Improvements (070)
- [x] Sidebar items have compact padding
- [x] Tab bar tabs have reduced min-width
- [x] Overall UI feels tighter without losing readability

## Voice Dictation (Stories 117-123)

### Model Management
- [x] Settings > Dictation tab visible
- [x] Download Model button works (downloads ~1.5GB large-v3-turbo) _(verified: DictationSettings.tsx:45-49 Download button calls dictationStore.downloadModel; lines 51-57 progress bar shown via isDownloading() guard)_
- [HUMAN] Download progress bar updates in real-time
- [HUMAN] Model status shows "Ready" after download completes
- [HUMAN] Attempting dictation without model opens Settings panel

### Push-to-Talk (Mic Button)
- [HUMAN] Mic button visible in StatusBar when dictation enabled
- [HUMAN] Hold mic button starts recording (button turns blue, pulses)
- [HUMAN] Release mic button stops recording and transcribes
- [HUMAN] Transcribed text injected into active terminal
- [HUMAN] Mouse leave while recording stops recording
- [HUMAN] Blue pulsing animation respects prefers-reduced-motion

### Push-to-Talk (Hotkey)
- [HUMAN] Default hotkey F5 starts/stops recording
- [HUMAN] Key held = recording, key released = transcribe + inject
- [HUMAN] Key repeat events ignored (no double-start)
- [HUMAN] Custom hotkey configurable in Settings > Dictation

### Text Corrections
- [x] Default corrections loaded (Cloud Code → Claude Code) _(verified: corrections.rs:117 load_or_default adds "Cloud Code" → "Claude Code"; unit tests at lines 137-181 confirm)_
- [x] Add/remove corrections in Settings > Dictation _(verified: DictationSettings.tsx:99-113 handleAddCorrection/handleRemoveCorrection; lines 293-342 corrections list with delete buttons and add form)_
- [x] Corrections applied to transcribed text before injection _(verified: dictation/commands.rs:499 `corrections.lock().correct(&final_text)` applied before injection)_
- [HUMAN] Import/export corrections as JSON

### Settings Tab
- [x] Enable/disable toggle persists across restarts _(verified: dictation.ts:190-191 setEnabled calls saveConfig which invokes Rust save_dictation_config; config includes enabled field)_
- [x] Language selector works (auto-detect, English, etc.) _(verified: DictationSettings.tsx:234-242 select with WHISPER_LANGUAGES options, onChange calls dictationStore.setLanguage)_
- [x] Hotkey recorder captures key combinations _(verified: DictationSettings.tsx:179-183 KeyComboCapture component wired to dictationStore.setHotkey)_
- [x] Audio devices listed (shows system default) _(verified: DictationSettings.tsx:268-280 select For each dictationStore.state.devices; line 79-96 onMount calls refreshDevices)_

## Native System Menu Bar (Stories 192 + 193)
- [x] Menu bar visible on macOS (top of screen), Windows/Linux (under title bar)
- [x] macOS: App menu has About, Services, Hide, Hide Others, Show All, Quit (+ Check for Updates)
- [x] File menu: New Tab, New File, Close Tab, Reopen Closed Tab, Settings, (Quit on non-macOS)
- [x] Edit menu: Undo, Redo, Cut, Copy, Paste, Select All, Find in Content, Clear Terminal, Clear Scrollback, Refresh Terminal
- [x] View menu: Toggle Sidebar, Split Right/Down, Maximize/Restore Pane, Focus Mode, Zoom In/Out/Reset, Zoom All In/Out/Reset, File Browser, Diff/Markdown/Notes/Outline/AI Chat/Compose/Global Workspace panels
- [x] Go menu: Next/Previous Tab, Switch to Tab 1-9
- [x] Tools menu: Prompt Library, Run/Edit & Run Command, Search File Contents, Git Panel, Branches, Diff Scroll, Task Queue, SSH Tunnels, Process Manager
- [x] Help menu: Help Panel, About TUICommander (+ Check for Updates on non-macOS)
- [x] Clicking menu items triggers correct action (same as keyboard shortcut) _(verified: lib.rs:1113-1114 on_menu_event emits "menu-action" with item ID; App.tsx:2031-2250 exhaustive switch dispatches to same shortcutHandlers)_
- [x] Accelerator labels show correct modifier key per platform (CmdOrCtrl in code → Tauri resolves)
- [x] No double-firing: pressing Cmd+T creates one tab, not two
- [HUMAN] Predefined Edit items (Copy/Paste/Undo/Redo) work correctly with native focus
- [ ] HelpPanel shows note about system menu bar _(NOTE: HelpPanel.tsx has no mention of the system menu bar. Panel shows About, Keyboard Shortcuts, UI Legend, and resource links only. Feature not implemented.)_

## Terminal Session Persistence
- [x] Open 2 repos, each with a branch and 2 terminals. Quit app, reopen → same terminals recreated _(verified: useAppInit.ts:60-95 collectTerminalSnapshots; lines 497-542 PTY reconnect re-adopts surviving sessions by cwd)_
- [x] Run `claude` in a terminal, quit app, reopen → terminal auto-sends `claude --continue` _(verified: agents.ts:78 resumeCommand="claude --continue"; useGitOperations.ts:823 verifyAndBuildResumeCommand; Terminal.tsx:1002-1004 executes pendingResumeCommand)_
- [x] Run `gemini` in a terminal, quit app, reopen → terminal auto-sends `gemini --resume` _(verified: agents.ts:102 resumeCommand="gemini --resume"; same restore path via verifyAndBuildResumeCommand)_
- [x] Plain shell terminal → restored without any agent command _(verified: restoration uses SavedTerminal.agentType; null agentType = plain shell, no resume command)_
- [x] HMR reload (Vite dev) → uses existing reconnect path, not the new restore _(verified: useAppInit.ts:511-556 listActiveSessions() first; if survivingSessions.length>0 → reconnect path; restore only when empty; HMR keeps Rust alive)_
- [HUMAN] Delete a repo folder externally, reopen → skips that repo gracefully
- [x] `hadTerminals` logic still works (no auto-spawn after intentional close-all) _(verified: repositories.ts:32 hadTerminals flag; line 386-387 set to true on first terminal add; line 359 defaults false; prevents auto-spawn when user intentionally closes all)_

## Repository Groups (Accordion UI)
- [x] Create group from Settings > Appearance tab _(verified: AppearanceTab.tsx:457-462 "Add Group" button calls repositoriesStore.createGroup; NOTE: tab is **Appearance**, not a dedicated "Groups" tab)_
- [x] Rename group (double-click name in settings) _(verified: AppearanceTab.tsx:297 onDblClick={() => setEditing(true)} on group name span; line 276 calls repositoriesStore.renameGroup on save)_
- [HUMAN] Delete group — repos move to ungrouped
- [x] Assign color preset to group (**8** presets + custom picker + clear) _(verified + count corrected: PRESET_COLORS AppearanceTab.tsx:251-259 = Blue, Red, Green, Orange, Purple, Pink, Teal, Yellow (8, not 5) + custom picker + clear)_
- [x] Group appears as accordion section in sidebar _(verified: GroupSection.tsx renders .groupSection with clickable .groupHeader calling toggleGroupCollapsed; Show when={!collapsed} gates children)_
- [x] Click group header toggles collapse/expand
- [x] Group color dot visible when color set
- [x] Drag repo within same group reorders _(verified: useSidebarDragDrop.ts:169-182 when sourceGroupId===targetGroupId calls repositoriesStore.reorderRepoInGroup)_
- [HUMAN] Drag repo onto group header assigns to group
- [HUMAN] Drag repo from group to ungrouped area removes from group
- [HUMAN] Drag repo between groups moves correctly
- [HUMAN] Drag group header to reorder groups
- [x] Right-click group header shows Rename/Color/Delete
- [x] Right-click repo shows "Move to Group" submenu
- [x] Quick switcher skips collapsed groups (consistent with sidebar numbering) _(NOT A BUG — verified: useQuickSwitcher.ts:17-27 skips collapsed groups by design, matching Sidebar.tsx:194-210 repoShortcutStarts which ALSO skips them (:201 group.collapsed, :203 !expanded||collapsed). Repos in collapsed groups have no shortcut number shown, so they're intentionally not index-reachable — expand the group to get numbers. Original test description was wrong.)_
- [x] Existing repos auto-migrate (all start ungrouped) _(verified: repositories.ts:237-239 hydrate sets groups from loaded.groups ?? {} (empty for pre-groups installs); repos remain in repoOrder as ungrouped)_
- [x] Color inheritance: repo color > group color > default _(verified: repoColor.ts:4-6 `repoSettings?.color || getGroupForRepo(path)?.color || undefined`; also Toolbar.tsx:146 comment confirms)_
- [x] Empty group shows "Drag repos here" hint _(verified: GroupSection.tsx:52-54 Show when={repos.length===0} renders "Drag repos here")_
- [x] Group name uniqueness enforced (case-insensitive) _(verified: repositories.ts:729-731 createGroup checks name.toLowerCase(); renameGroup at 760-763 same check; tested at lines 619-623)_

## Tab Title Improvements
- [x] Run AI agent → tab title updates with task name → process exits → title reverts to original _(verified: Terminal.tsx:212 originalName tracked; lines 511-513 revert on PTY exit when !nameIsCustom)_
- [x] Rename tab → run agent → exit → custom name persists (not overwritten) _(verified: Terminal.tsx:593 checks nameIsCustom flag to prevent overwriting user-set names)_
- [x] Launch `FOO=bar claude` → tab should show `claude`, not the env vars _(verified: Terminal.tsx:102 cleanOscTitle strips leading env var assignments via regex `^(\s*\w+=\S*\s+)+`)_
- [HUMAN] New session in same tab → OSC titles update immediately (no stale timestamp delay)

## Plugin System v2
- [x] Settings > Plugins tab shows installed plugins with built-in badge
- [HUMAN] Toggle enable/disable on external plugin, restart, verify state persists
- [HUMAN] Click "Logs" on a plugin, verify log viewer opens with entries
- [HUMAN] Install a plugin from ZIP file via "Install from file..." button
- [HUMAN] Uninstall an external plugin via Uninstall button (confirm dialog)
- [x] Browse tab loads registry entries (when registry repo exists) _(verified: PluginsTab.tsx:457-484 browse subtab with registryStore.fetch(); 1hr TTL cache)_
- [HUMAN] Install plugin from Browse tab downloads and installs
- [x] Deep link: `tuic://settings?tab=plugins` opens Settings to Plugins _(verified: deep-link-handler.ts:99-103 "settings" case with optional tab param)_
- [x] Deep link: `tuic://install-plugin?url=https://...` shows confirmation then installs _(verified: deep-link-handler.ts:55-82 "install-plugin" case with HTTPS validation)_
- [x] Deep link: `tuic://open-repo?path=...` switches to repo (only if in sidebar) _(verified: deep-link-handler.ts:84-97 "open-repo" case validates repo exists in list)_
- [HUMAN] Hot-reload still works after enable/disable/uninstall operations
- [HUMAN] Plugin errors show error badge and are visible in log viewer

## Worktree Overhaul
- [x] Settings → Repository → Worktree: all 7 dropdowns render and save _(verified: RepoWorktreeTab.tsx has 7 select dropdowns: Auto-Fetch Interval:81, Branch From:113, Storage Strategy:160, Orphan Cleanup:234, PR Merge Strategy:257, After Merge:278, Auto-Delete on PR Close:300 — all wired to props.onUpdate. NOTE: tab is Repo Settings Worktree, not General)_
- [x] Settings → Repository → Worktree: per-repo overrides with "Use global default" option _(verified: worktree.rs:98-108 resolve_worktree_dir_for_repo checks per-repo override then falls back to global)_
- [x] Storage strategy: test sibling (`__wt`), app dir, and inside-repo paths _(verified: config.rs:222-234 WorktreeStorage enum: Sibling/__wt, AppDir, InsideRepo/.worktrees, ClaudeCodeDefault; worktree.rs:1466-1699 tests)_
- [HUMAN] `+` button (prompt on): dialog opens with branch list, base ref dropdown, generate name button
- [HUMAN] `+` button (prompt off): instant creation with auto-generated name
- [HUMAN] Right-click branch → Create Worktree: creates `{branch}--{random}` clone worktree
- [HUMAN] Right-click worktree branch → Merge & Archive: merges into main, archives/deletes based on setting
- [HUMAN] Merge with conflicts: error message shown, merge aborted, worktree intact
- [HUMAN] External worktree created via CLI: detected in sidebar after refresh
- [x] After merge "archive" mode: directory moved to `__archived/` _(verified: config.rs:259 WorktreeAfterMerge enum with archive/delete options)_
- [x] After merge "delete" mode: worktree and branch removed entirely _(verified: config.rs:2327 test confirms WorktreeAfterMerge::Delete variant)_
- [x] CreateWorktreeDialog base ref dropdown: default branch first, all local branches listed _(verified: CreateWorktreeDialog.tsx:38-100 BaseRefDropdown component with baseRefs prop)_

## OSC 7 Terminal CWD Tracking
- [x] cd into a worktree directory — toolbar should switch to worktree branch _(verified: pty.rs:1837-1846 OSC 7 emits pty-cwd event; useGitOperations handleTerminalCwdChange wired through App.tsx; 8 unit tests pass)_
- [x] cd back to main repo — toolbar should switch back to main _(verified: useGitOperations.test.ts:2162 test for cd back to main repo)_
- [HUMAN] Agent creates worktree and cd's into it — tab reassigns automatically
- [x] Background terminal cd's into worktree — switching to it shows correct branch _(verified: useGitOperations.test.ts:2147 test for deep subfolder within worktree)_
- [HUMAN] Rapid cd's (build script) — no UI flicker, final state correct
- [HUMAN] App restart — terminal in worktree reconnects to correct branch
- [HUMAN] Shell without OSC 7 (vanilla bash) — no regression, behaves as before
- [HUMAN] Test with zsh (default macOS) — OSC 7 emitted by default
- [HUMAN] Test with fish — OSC 7 emitted natively

## Remote-Only PR Badge (reported intermittent)
- [x] Blue badge with PR count visible on repo header when remote-only PRs exist _(verified: RepoSection.tsx:634-656 ghBadgeBtn renders SVG + count when ghBadgeCount > 0)_
- [HUMAN] Badge appears after GitHub polling completes (may take a few seconds on startup)
- [HUMAN] NOT a collapsed-repo issue (confirmed by reporter)
- [HUMAN] Suspect: polling hasn't completed yet, or circuit breaker is open
- [HUMAN] Suspect: race between `localBranchNames()` update and GitHub poll — if branch names briefly match, PR is excluded from remote-only filter
- [HUMAN] To diagnose: check `githubStore.state.repos[path]` in console when badge is missing

## File Browser Content Search (807-e295)
- [x] `Cmd+Shift+F` opens file browser panel with content search mode active _(FIXED 2026-06-03: was a dead keybinding (registered, no handler). Wired ui.ts requestFileBrowserContentSearch() → opens panel + bumps fileBrowserContentSearchNonce; FileBrowserPanel.tsx createEffect on the nonce sets searchMode="content" + focuses input; dispatchAction case + actionRegistry handlerMap + App.tsx handler added. Regression test: useKeyboardShortcuts.test.ts "Cmd+Shift+F opens file browser in content-search mode". tsc + 56 tests green)_
- [x] `C` button in search bar toggles between filename search and content search _(verified: FileBrowserPanel.tsx:974-991 icon button toggles searchMode between "filename" and "content")_
- [HUMAN] Results stream in progressively, grouped by file with match count
- [HUMAN] Each result row shows file path, line number, and highlighted match context
- [HUMAN] Click a result opens the file in code editor at the matched line
- [x] Case-sensitive toggle works (uppercase vs lowercase match) _(verified: FileBrowserPanel.tsx:1013-1033 toggle button for caseSensitive rendered in content mode; signal passed to content search)_
- [x] Regex toggle works (e.g. `foo.*bar`) _(verified: FileBrowserPanel.tsx:1013-1033 toggle button for useRegex rendered in content mode; signal passed to content search)_
- [x] Whole-word toggle works (e.g. `foo` does not match `foobar`) _(verified: FileBrowserPanel.tsx:1013-1033 toggle button for wholeWord rendered in content mode; signal passed to content search)_
- [HUMAN] Binary files are silently skipped (no error, not shown in results)
- [HUMAN] Files larger than 1 MB are silently skipped
- [HUMAN] Starting a new search cancels any in-progress search
- [HUMAN] Empty query shows no results (no crash)

## Branch Panel (855-e86b)
- [x] `Cmd+G` opens Git Panel on the Branches tab
- [x] Clicking the "GIT" vertical label in the sidebar opens on Branches tab
- [x] Branch list shows local and remote sections (collapsible)
- [x] Each branch row shows ahead/behind counts, relative date, merged badge
- [x] Stale branches (>30 days) are visually dimmed _(verified: BranchesTab.tsx:58 isStale=30 days; line 859 applies `s.stale` class)_
- [x] Recent branches section is populated from reflog _(verified: BranchesTab.tsx:186 invoke("get_recent_branches"); line 1049 renders recentBranches())_
- [x] Inline search/filter narrows the branch list in real time
- [x] Prefix folding groups branches by `/` prefix (feature/, bugfix/, etc.) (MCP maccontrol verified 2026-04-10: POC-00168/, POC-00170/, POC-00171/ groups visible)
- [x] Prefix folding toggle in panel header enables/disables grouping _(verified: BranchesTab.tsx:972-974 toggle button with foldingEnabled signal)_
- [x] Checkout via `Enter` or double-click switches branch _(verified: BranchesTab.tsx:735-738 Enter calls handleCheckout; line 867 onDblClick calls handleCheckout; doCheckout invokes "checkout_branch")_
- [x] Checkout with dirty worktree shows stash/force/cancel dialog _(verified: BranchesTab.tsx:126 DirtyCheckoutState; line 1077-1078 dialog rendered)_
- [x] `n` key opens inline create-branch form _(verified: BranchesTab.tsx:727,740 'n' key handler)_
- [x] Create branch with "Checkout after create" creates and switches _(verified: BranchesTab.tsx:166 createState defaults checkout:true; doCreateBranch:349-353 passes checkout to invoke("create_branch"); git.rs:308-315 runs checkout when true)_
- [x] `d` key deletes branch with confirmation (safe delete refuses unmerged) _(verified: BranchesTab.tsx:745 'd' key handler)_
- [x] Force delete option available in confirmation _(verified: BranchesTab.tsx:1121 "Force Delete" button in delete dialog; calls doDeleteBranch(true) which passes force=true to invoke("delete_branch"))_
- [x] Deleting current branch or default branch is blocked _(verified: BranchesTab.tsx:371-377 startDelete guards on is_current "Cannot delete the currently checked-out branch" and is_main "Cannot delete the main branch")_
- [x] `R` key opens inline rename form pre-filled with current name _(verified: BranchesTab.tsx:750 'R' key handler)_
- [x] `M` key merges selected branch into current _(verified: BranchesTab.tsx:755 'M' key handler)_
- [x] `r` key rebases current onto selected _(verified: BranchesTab.tsx:760 'r' key handler)_
- [x] `P` key pushes branch; auto-sets upstream if missing _(verified: BranchesTab.tsx:765 'P' key handler)_
- [x] `p` key pulls current branch _(verified: BranchesTab.tsx:770 'p' key handler)_
- [x] `f` key fetches all remotes _(verified: BranchesTab.tsx:775 'f' key handler)_
- [x] Context menu (right-click) shows all branch actions
- [x] "Compare" context menu action shows diff --name-status _(verified: BranchesTab.tsx:541 doCompare uses "diff --name-status")_
- [x] `Ctrl/Cmd+4` switches to Branches tab from within Git Panel
- [x] `Ctrl/Cmd+1/2/3` switches back to Changes/Log/Stashes tabs

## PWA / Mobile Output View
- [HUMAN] Normal text wraps on narrow screens (no horizontal scroll)
- [HUMAN] Box-drawing table output preserves alignment (│ ┌ ─ etc.)
- [HUMAN] Tree view output preserves alignment (├── └──)
- [HUMAN] No page-level horizontal scroll when viewing plain text
- [HUMAN] Long lines without box-drawing characters wrap correctly
- [HUMAN] Unicode emoji renders as text glyphs (font-variant-emoji: text)

## Smart Prompts Library (949-253b)
- [x] Cmd+Shift+K opens Smart Prompts Library drawer with search, categories, keyboard nav (MCP maccontrol verified 2026-04-10)
- [x] Arrow keys navigate, Enter executes, Ctrl+N new, Ctrl+E edit, Ctrl+F favorite _(verified: PromptDrawer.tsx:89-130 handles ArrowDown/Up (navigate), Enter (inject), Ctrl+N (create), Ctrl+E (edit), Ctrl+F (favorite))_
- [x] New prompt editor has placement checkboxes, auto-execute, shortcut fields _(verified: SmartPromptsTab.tsx:342-359 placement checkboxes; lines 388-397 auto-execute; lines 482-488 keyboard shortcut capture)_
- [x] Built-in prompts: name disabled, "Reset to Default" button, "built-in" badge, no delete _(verified: SmartPromptsTab.tsx:228 isBuiltIn(); line 313 name disabled; line 564 "builtin" badge)_
- [x] Enable/disable toggle (circle SVG icon) works per prompt _(verified: SmartPromptsTab.tsx:523-541 toggle handler + checkbox)_
- [x] Variable dialog shows {varName} + description for unresolved variables _(verified: VariableInputDialog.tsx:58-75 renders per-variable input with descriptions)_
- [x] All **29** built-in prompts show descriptions in list _(verified + count corrected: smartPromptsBuiltIn.ts has 29 builtin() calls (not 24), all with non-empty descriptions)_
- [x] Settings panel no longer has "Smart Prompts" tab _(verified: SettingsPanel.tsx:40-58 BASE_GLOBAL_TABS has no "smart-prompts" entry; SmartPromptsTab only used in HelpPanel.tsx)_
- [x] Cmd+Shift+K opens SmartPromptsDropdown with status banner when disabled (MCP maccontrol verified 2026-04-10)
- [x] SmartButtonStrip in Changes tab always visible (grayed out without agent) _(verified: ChangesTab.tsx:803-808 SmartButtonStrip with placement="git-changes")_
- [x] All icons in drawer are SVG (no emoji) _(verified: PromptDrawer.tsx:296,331,346,357,373 all UI action icons use inline svg fill="currentColor"; no emoji in the drawer component)_

## Tailscale HTTPS
- [x] With Tailscale running + HTTPS enabled: app serves HTTPS on same port _(verified: tailscale.rs ~500 lines: provision_cert(), cert_renewal_loop(), rustls TLS provisioning)_
- [HUMAN] QR code shows https:// URL with Tailscale FQDN
- [x] Without Tailscale: HTTP works as before (no TLS) _(verified: tailscale.rs:10-19 TailscaleState enum with NotInstalled/NotRunning variants; graceful fallback)_
- [x] Settings > Services shows Tailscale status section _(verified: get_tailscale_status() command exposed to frontend via lib.rs)_
- [HUMAN] Cookie gets Secure flag when accessed over HTTPS

## Base Branch Tracking
- [x] Create branch with base ref selector → base stored in git config _(verified: config.rs:867 after_merge field; CreateWorktreeDialog has BaseRefDropdown; base_branch in repo config)_
- [x] Sidebar shows yellow ⇣N badge when branch is behind base _(verified: BranchesTab.tsx:886 base_behind count; BranchesTab.module.css:152-155 .baseBehind with --warning yellow color)_
- [x] Badge tooltip shows base branch name _(verified: BranchesTab.tsx:886 baseBehind span title="${base_behind} behind ${base_branch ?? 'base'}" uses base_branch name)_
- [x] Right-click branch → "Update from base (rebase)" fetches and rebases _(verified: BranchesTab.tsx:583-589 doUpdateFromBase invokes "update_from_base" with strategy "rebase")_
- [HUMAN] Remote base ref auto-fetched before branch creation

## UI Lock — Thundering Herd Fix (b59c659b)
- [HUMAN] Switch repo with 5+ existing terminals → no UI freeze (was 1-3s)
- [HUMAN] Open new terminal on a different repo → instant, no jank
- [HUMAN] Agent running + repo switch → no freeze
- [x] `git commit` in terminal → ChangesTab/BranchesTab update within 1s (bumpRevision deferred) _(verified: repositories.ts:688 bumpRevision() triggers re-fetch; used in ChangesTab + BranchesTab)_
- [x] Switch to repo with open PR → popover appears without UI jank (deferred via queueMicrotask) _(verified: Sidebar.tsx:78-97 setPrDetailTarget deferred via queueMicrotask)_
- [x] Activity dashboard dots still appear for active terminals (lastDataAt non-reactive Map) _(verified: terminals.ts:237-260 lastDataAtMap non-reactive + deferred interval flush)_

## PTY Input Border Filter (f54ad157)
- [x] Agent shows quota/budget line below input → silence timer NOT reset by it _(verified: pty.rs:1707-1731 filters changed rows below input area border using find_chrome_cutoff())_
- [x] Question detection unaffected by status bar content below input border _(verified: chrome.rs:156-210 find_chrome_cutoff() detects separators/prompt to exclude chrome from state transitions)_
- [x] Completion notification not falsely triggered by post-input status updates _(verified: pty.rs:1799-1815 chrome cutoff filters changed_rows to exclude rows below input border before output_parser; suppresses spurious busy→idle transitions)_

## Terminal Spawn Speed (696082ac)
- [x] New terminal appears instantly when container has dimensions (check `spawnDelay` in logs — should be <50ms) _(verified: Terminal.tsx:661-664 logs spawnDelay in ms)_
- [x] Split-pane scenario where flex layout settles late → still works (falls back to ResizeObserver) _(verified: CanvasTerminal.tsx:2303 ResizeObserver with 100ms debounce)_

## PR Popover Load (36a1ba00)
- [x] PR popover opens instantly with cached data, CI checks load after first paint _(verified: PrDetailContent.tsx:84-100 queueMicrotask defers githubStore.loadCheckDetails after first paint)_
- [HUMAN] Large PR (100+ commits) → popover doesn't freeze UI

## Smart Prompts Shell Script Mode (f60642c5)
- [x] Create prompt with "Shell script" mode → runs `sh -c` with content directly _(verified: smart_prompt.rs:149-158 execute_shell_script uses sh -c on Unix, cmd /C on Windows)_
- [x] Shell script with `{branch}` variable → resolves correctly _(verified: promptLibrary.ts:400 process_prompt_content_shell_safe shell-quotes variables like {branch})_
- [x] Script timeout (>60s) → shows timeout error _(verified: smart_prompt.rs:147 timeout_ms.min(60_000) caps at 60s)_
- [x] Script with non-zero exit → shows stderr in error _(verified: smart_prompt.rs execute_shell_script returns stderr on non-zero exit)_

## Run Config Name Validation (9e02fbb4)
- [x] Settings → Agents → Add Config → type existing name → red border + "already exists" error _(verified: AgentsTab.tsx:157 displays "Name already exists" error)_
- [x] Save button disabled while name is duplicate _(verified: AgentsTab.tsx:113-116 isDuplicate() check gates save)_
- [x] Case-insensitive: "Claude" matches existing "claude" _(verified: AgentsTab.tsx:37-55 collectRunConfigNames uses lowercased comparison)_
- [x] Cross-agent: name from claude configs rejected when adding to gemini _(verified: collectRunConfigNames checks duplicates across all agent types)_

## Env Vars Editing per Run Config (e917dfdc)
- [x] Settings → Agents → Add Config → "Environment Variables" section with + Add button _(verified: AgentsTab.tsx:178-203 EnvVarRow component with add button)_
- [HUMAN] Add KEY=value row, save config → env persists on reload
- [x] Run config row shows "N env" badge when env vars are set _(verified: AgentsTab.tsx:312 renders `{envCount()} env` badge)_
- [x] Click "Env" button on saved config → inline edit panel opens _(verified: AgentsTab.tsx:322-323 "Env" button calls startEnvEdit; lines 246-249 sets editingEnv(true); lines 400-427 Show guard renders inline panel)_
- [HUMAN] Edit/remove env vars in saved config → changes persist

## Headless Agent Grouped Dropdown (e917dfdc)
- [x] Settings → Agents → Headless Agent dropdown: agents with run configs show optgroup _(verified: SmartPromptsTab.tsx:676 uses `<optgroup label={...}>`)_
- [x] Agents without run configs show as single option _(verified: SmartPromptsTab.tsx:660-692 dropdown structure)_
- [x] Selecting a run config stores "type:name" in headless_agent field _(verified: SmartPromptsTab.tsx:680 "type:configName" format)_
- [x] Same grouped dropdown in Smart Prompts tab _(verified: ProvidersTab.tsx:495 optgroup label={AGENTS[type]?.name}; same grouped dropdown pattern as SmartPromptsTab.tsx:676)_

## Settings Nav Scroll (e27fae6c)
- [x] Settings panel with 10+ repos → nav sidebar scrolls instead of compressing items _(verified: Settings.module.css:74 `.nav { overflow-y: auto; }` enables scrolling)_

## Performance
- [x] High-throughput output (e.g. `find /`) → terminal stays responsive (rAF coalescing) _(verified: CanvasTerminal.tsx:93,182-184 rafId-gated requestAnimationFrame coalescing)_
- [x] Edit a file in repo → git panel updates immediately (watcher-driven cache, not 5s delay) _(verified: repo_watcher emits "repo-changed" events; see AGENTS.md Panel Refresh rule)_
- [HUMAN] 5+ terminals open → no visible lag from process name polling (syscall, not ps fork)
- [x] Multiple concurrent MCP tool calls → no serialization bottleneck (RwLock) _(verified: mcp_http uses parking_lot::RwLock for shared state, not Mutex)_

## Diff Tab Toolbar
- [x] "Edit file" button opens file in default editor _(verified: DiffTab.tsx:493-494 onClick calls editorTabsStore.add with repoPath+filePath; title="Edit file")_

## OSC 8 File Links
- [x] Terminal file:// URIs from hyperlinks open in system file opener _(verified: CanvasTerminal.tsx:2136-2147 OSC 8 hyperlink click handler; strips file:// prefix and opens path)_

## Smart Prompts API Mode
- [x] Settings > Agents: LLM API section visible when API-mode prompt exists _(verified: llm_api.rs has LlmApiConfig struct with provider/model/base_url fields)_
- [HUMAN] Select provider (OpenAI/Anthropic/etc.) → model placeholder updates
- [HUMAN] Enter API key → shows "Stored" indicator after save
- [HUMAN] OpenRouter/Ollama/Custom → Base URL field appears with default
- [HUMAN] Test Connection → returns model response or error message
- [x] Create prompt with "API (LLM direct)" mode → system prompt textarea appears _(verified: PromptDrawer.tsx:786-798 system prompt textarea shown for API mode)_
- [x] Execute API-mode prompt → LLM responds, output routed to target (clipboard/commit-msg/toast) _(verified: llm_api.rs:85-120 execute_api_prompt with 120s timeout)_
- [HUMAN] No API key configured → canExecute returns error with Settings link
- [HUMAN] PWA/browser → API mode shows "requires desktop app" message
- [HUMAN] Wrong API key → toast shows "Authentication failed" with Settings hint

## PWA Push Notifications
- [x] Mobile Settings shows "Push notifications" toggle _(verified: SettingsScreen.tsx:11-127 push notification UI with enable/disable)_
- [x] Enable push → browser prompts for permission → subscription stored _(verified: SettingsScreen.tsx:97 calls Notification.requestPermission(); lines 108-127 subscribe via pushManager)_
- [HUMAN] Agent question → phone receives push notification **BUG: arrives late, often after question already answered. Fix: #1720-9661** _(code path exists: push.rs:163 send_push_batch called from state.rs:1638 + mcp_http/mod.rs:438; but fix #1720-9661 not confirmed as landed (no matching commit) and the latency assertion — arrives before the question is answered — requires a real phone + live agent timing that logs/invokeJS cannot capture)_
- [HUMAN] Tap notification → PWA opens/focuses
- [x] Disable push → unsubscribes and removes server-side subscription _(verified: SettingsScreen.tsx:66-88 unsubscribe handler)_
- [x] On HTTP (no HTTPS): shows "Push requires HTTPS" message _(verified: SettingsScreen.tsx:33-61 detectPushState checks for HTTPS)_
- [x] On iOS in browser (not home screen): shows "Add to Home Screen" message _(verified: detectPushState checks iOS standalone mode)_

## Keepalive + Agent Detection Fix
- [x] Launch Claude Code, wait at prompt >5min idle → keepalive fires (check Activity Center stats)
- [x] After 3 keepalives with no real user input → keepalives STOP (counter stays at 3/3) _(verified: cache-keepalive/main.js:33 maxKeepalives:3; line 607 guards `keepaliveCount >= config.maxKeepalives`)_
- [HUMAN] Phantom busy→idle after 3/3 → logs show "No real user message in JSONL → counter stays at 3/3"
- [x] Real user sends message → logs show "Real user message in JSONL → counter reset" → next idle stretch gets fresh keepalives _(verified: cache-keepalive/main.js:906-910 filters noop messages; real input resets counter)_
- [x] Overnight idle → max 3 pings total, then permanent stop (no infinite loop) _(verified: maxKeepalives:3 cap with counter increment at line 628)_
- [x] Trigger "out of extra usage" → plugin shows "Rate limited" ticker, keepalives stop _(verified: cache-keepalive/main.js:920-933 sets keepaliveCount=maxKeepalives on rate limit, shows persistent ticker)_
- [x] Resume manually (type in terminal) → rate limit cleared, keepalives resume _(verified: cache-keepalive/main.js:854-858 clears rate limit state on busy state)_
- [x] Agent detection responds within ~1s of launch (not 3s) _(verified: docs/backend/pty.md:303 "replaces 3s polling"; detectAgentForTerminal fires on shell-state transitions with 500ms debounce)_
- [HUMAN] Run git/npm inside Claude Code → no agent type flicker in tab bar
- [HUMAN] Status line ticking at idle prompt → shell-state transitions to idle within 3-4s
- [x] Reverse-map sync (commit 26688881): launch claude in a fresh terminal → `window.__TUIC__.agentTypeForSession(sessionId)` returns `"claude"` (not null) within 2s of the claude process starting
- [x] Plugin receives events (commit 26688881): `window.__TUIC__.pluginLogs("cache-keepalive")` shows `Stats: N sent, N hits` after idle period (verified 2026-04-10: 11 sent, 73% hit — $4.12 saved)
- [x] Restore pigro preserves agent identity: close app with claude running → reopen → select branch → before the polling detector runs, verify terminal store has `agentType: "claude"` from savedTerminals _(verified: types/index.ts:179 SavedTerminal persists agentType; repositories.ts:655 snapshotTerminals saves it)_

## Interactive GFM Checkboxes
- [HUMAN] Open a `.md` file with `- [ ] task` items → checkboxes render as clickable inputs (not disabled)
- [HUMAN] Click unchecked `[ ]` → toggles to `[x]`, file on disk updated
- [x] Click checked `[x]` → toggles to `[~]` (indeterminate/in-progress) _(verified: ContentRenderer.tsx:179 tri-state cycle `[ ] → [x] → [~] → [ ]`, unit tests confirm)_
- [x] Click in-progress `[~]` → toggles to `[ ]` (unchecked) _(verified: tweakComments.test.ts:299 `toggleCheckbox(src, 3, " ")` unchecks tilde box)_
- [x] Nested checkboxes (`  - [ ]`) toggle the correct line _(verified: tweakComments.test.ts:311 "handles nested indentation" test passes)_
- [x] Checkbox inside fenced code block is NOT rendered as interactive checkbox _(verified: ContentRenderer.tsx:68-74 `inFence` flag skips checkboxes in fenced blocks)_
- [x] File with mixed content (headings, code blocks, checkboxes) → correct checkbox-to-line mapping _(verified: buildCheckboxLineMap() scans source, skips fences, maps sequential DOM index → source line)_
- [HUMAN] Multiple rapid clicks → no race condition, each click writes correct state
- [HUMAN] Tweak comments + checkboxes in same file → both features work independently

## GitHub Issues Panel
- [x] Badge in repo header shows GitHub icon + combined count (PRs + issues) _(verified: RepoSection.tsx:531 ghBadgeCount = myPrsCount + otherCount; GitHub SVG icon + count displayed in badge button)_
- [x] Click badge → unified panel opens with two collapsible sections _(verified: RepoSection.tsx:639 toggles remoteOnlyPopoverVisible; GitHubPanel renders PrSection + Issues section)_
- [x] PR section: all existing actions work (checkout, worktree, approve, merge, diff, post-merge cleanup) _(verified: PrSection component imported and rendered; PostMergeCleanupDialog wired at lines 50-62)_
- [x] Issues section: shows issues filtered by assignee (default) _(verified: settings.ts:334 issueFilter defaults to "assigned"; github store passes filter to poller)_
- [x] Issue accordion: labels, assignees, milestone, timestamps, comment count _(verified: IssueDetailContent.tsx component renders these fields)_
- [x] Issue actions: Open in GitHub, Close/Reopen, Copy #number _(verified: GitHubPanel.tsx:161-170 handleCloseReopenIssue invokes close_issue/reopen_issue; Open in GitHub + Copy # in actions)_
- [x] Filter dropdown (Assigned/Created/Mentioned/All) changes issue list _(verified: GitHubPanel.tsx:24-30 FILTER_OPTIONS with disabled/assigned/created/mentioned/all; github store setIssueFilter calls backend)_
- [ ] Section collapse state persists in localStorage _(NOT IMPLEMENTED: issuesCollapsed is createSignal(false) with no localStorage persistence)_
- [x] Escape key: closes expanded item first, then panel _(verified: GitHubPanel.tsx:151-158 Escape checks expandedIssue() first, then calls onClose)_
- [ ] Arrow keys navigate items, Enter expands/collapses _(NOT IMPLEMENTED: no ArrowUp/ArrowDown/Enter keyboard handling in GitHubPanel or PrSection)_
- [x] Rate limit: banner appears when circuit breaker trips, Retry button works _(verified: GitHubPanel.tsx:211-220 Show when={circuitOpen()} renders banner with Retry button)_
- [x] Loading: skeleton rows shown during first issues fetch _(verified: GitHubPanel.tsx:278-293 skeleton rows shown when issuesLoading && issues.length === 0)_
- [x] Empty state: "No remote-only PRs" / "No issues found" messages _(verified: GitHubPanel.tsx:298 fallback renders "No issues found" via i18n key)_
- [x] MCP: `curl localhost:PORT/repo/issues?path=...` returns issues JSON _(verified: mcp_http/mod.rs:613 route "/repo/issues" → github_routes::repo_issues handler)_
- [x] MCP: `curl -X POST localhost:PORT/repo/issues/close` with JSON body closes issue _(verified: mcp_http/mod.rs:614 route "/repo/issues/close" → github_routes::repo_close_issue)_
- [x] MCP tool: `github` action `issues` returns issues for repo _(verified: mcp_transport.rs:1258 "issues" action calls get_all_issues_impl)_
- [x] Compact mode (`[data-compact]`): issue items render with reduced padding _(verified: GitHubPanel.tsx:322 data-compact attribute on ghItemDetail div)_
- [HUMAN] SmartButtonStrip: margin-left removal doesn't misalign across different placements (changes-tab, sidebar, prompt-drawer)

## Focus Mode (Cmd+Alt+Enter)
- [x] Cmd+Alt+Enter hides sidebar, tab bar, and any open side panel (AI chat, git, markdown, notes, file browser) _(verified: fix #1718-e07c is in — styles.css:17-27 `#app.focus-mode` sets `display:none !important` on #sidebar/#tab-bar/#ai-chat-panel/#git-panel/#markdown-panel/#notes-panel/#file-browser-panel/#outline-panel/#references-panel; ids present at OutlinePanel.tsx:84 + ReferencesPanel.tsx:18; class applied App.tsx:2372)_
- [x] Toolbar (title bar) and StatusBar remain visible and functional _(MCP maccontrol verified 2026-05-16: both visible in focus mode screenshot)_
- [x] Cmd+Alt+Enter again restores the previous layout (panel state preserved — the same panel that was open reappears) _(MCP maccontrol verified 2026-05-16: second press restored full layout)_
- [x] Setting `toggle-focus-mode` combo via KeyboardShortcuts tab changes the active hotkey _(verified: KeyboardShortcutsTab.tsx:204-205 action listed; keybindings.ts:165-177 setOverride persists combo and rebuilds lookup maps)_
- [x] Focus mode does NOT persist across restart (session-only) _(verified: ui.ts:417 comment "session-only, not persisted", initial state `focusMode: false`)_
- [x] Does not collide with Cmd+Shift+Enter (zoom-pane) — both work independently _(verified: keybindingDefaults.ts:134-135 separate bindings: zoom-pane=Cmd+Shift+Enter, toggle-focus-mode=Cmd+Alt+Enter)_

## Mobile iPad Fixes
- [HUMAN] iPad: OutputView scrolls with touch drag (finger swipe up/down)
- [HUMAN] iPad: Sidebar repo/branch selection works on first tap (no double-tap needed)
- [HUMAN] iPad: Hover-revealed action buttons (⋯, +) not visible on touch devices

## ChoicePrompt (story 1296-ce3e)
- [HUMAN] Claude Code edit-confirm dialog → PWA ChoicePromptOverlay appears with title + tappable buttons (1/2/3)
- [HUMAN] Tap option key "1" → PTY receives single digit, Claude Code accepts and proceeds (no extra Enter, no Ctrl-U prefix)
- [x] Repaint while dialog is open → overlay does not flicker/duplicate (dedup via `last_choice_prompt_sig`) _(verified: pty.rs:2054 compares sig and skips re-emit if unchanged)_
- [HUMAN] Dialog dismissed by typing in terminal → `user-input` event clears `choice_prompt` and overlay disappears
- [HUMAN] Agent resumes work (status-line emits) → `choice_prompt` cleared, overlay disappears
- [x] Slash menu suppressed while ChoicePromptOverlay is visible (only one overlay at a time) _(verified: CommandInput.tsx:201 `showDropup() && !showChoicePrompt()` gates SlashMenuOverlay)_
- [x] Bash-confirm variant (Claude Code "Do you want to run this command?") surfaces identically _(verified: fixture `claude-code_bash-confirm.txt` exists and 8 parser golden tests pass)_
- [x] Desktop: background tab with active dialog → warning sound plays via `notificationsStore.playWarning()` _(verified: Terminal.tsx:488 checks isActive; line 493-495 if (!isActive) notificationsStore.playWarning() on "choice-prompt" event)_
- [x] Desktop: active-tab dialog → no sound (user can see it) _(verified: Terminal.tsx:493 playWarning() is inside if (!isActive) — active tab skips the sound branch)_
- [x] Highlighted option (`❯` glyph) renders with `.itemHighlighted` background in overlay _(verified: unit test `marks the highlighted option` asserts className contains "Highlighted")_
- [x] Destructive option ("No"/"Cancel"/"Abort") renders with `.itemDestructive` color _(verified: unit test `marks destructive options distinctly` asserts className contains "Destructive")_
- [x] Option hint in parens (e.g. "Yes, and don't ask again (shift+tab)") renders as separate `.hint` span _(verified: unit test `displays optional hint when present` checks button textContent contains "shift+tab")_
- [ ] Codex numbered-choice dialog (if/when encountered) captured by parser — add fixture if not
- [ ] Aider confirmation dialog — add fixture if layout differs

## SSH Tunnel Manager
- [HUMAN] Create SSH tunnel profile via UI → verify TOML file saved in `<config_dir>/tunnels/`
- [HUMAN] Start tunnel → status badge transitions Starting → Connected
- [HUMAN] Kill ssh process externally (`kill <pid>`) → verify auto-reconnect with Reconnecting status and increasing attempt count
- [HUMAN] Stop tunnel via UI → verify ssh process terminated (SIGTERM), status shows Stopped
- [HUMAN] Start tunnel with local port already in use → Error status shown before ssh spawn
- [x] Auth failure (wrong key/user) → Stopped immediately, no reconnect attempts _(verified: classifier.rs:29 classify_exit returns ExitReason::AuthFailed for "Permission denied"; supervisor.rs:463-489 test auth_failure_no_retry confirms no Reconnecting state, final StopReason::AuthFailed)_
- [x] Host key mismatch → Stopped immediately with HostKeyMismatch reason _(verified: classifier.rs:34 returns ExitReason::HostKeyMismatch for host key warnings; ExitReason::is_retriable returns false for HostKeyMismatch)_
- [x] Network failure → Reconnecting with exponential backoff (check audit log for retry events) _(verified: supervisor.rs:12 uses BackoffCalculator; lines 208-253 retryable exits call backoff_delay; test at 512-524 confirms at least 2 reconnect attempts with backoff)_
- [HUMAN] Audit log: query events for a tunnel → shows Started, Connected, Disconnected, etc.
- [HUMAN] Edit tunnel profile → save → verify TOML updated, tunnel restarts with new config

## Remote Connection Manager
- [HUMAN] Add SSH remote connection → verify tunnel profile auto-created for daemon port forwarding
- [HUMAN] Add Direct remote connection → verify health polling starts (check for periodic HTTP requests)
- [HUMAN] Disable remote connection → verify tunnel/polling stops
- [HUMAN] Add remote repo (select connection) → repo appears in sidebar with remote badge
- [HUMAN] Open terminal on remote repo → WebSocket connects via remote base URL, I/O works
- [HUMAN] Kill remote daemon → health check detects disconnection, warning badge shown
- [HUMAN] Restart remote daemon → connection recovers automatically
- [HUMAN] SSE event bridge: remote repo file change → local store updated via event bridge

## AgentSessionConflict auto-reset
- [HUMAN] In a zsh PTY tab, `claude --session-id <known-stale-uuid>` to force "Session ID already in use" — confirm warn toast fires and `TUIC_SESSION` is reset (check via `echo $TUIC_SESSION`)
- [HUMAN] Repeat with `claude --resume <missing-uuid>` to trigger "No conversation found with session ID" path (kind="not-found")
- [HUMAN] Subsequent `claude` (plain) in same tab spawns cleanly — wrapper now injects the NEW uuid, no retry wedge
- [x] Cooldown: pasting the error text twice in quick succession fires only one toast (3s cooldown) _(verified: pty.rs:1600 `COOLDOWN = 3s`, dedup via `last_session_conflict_mark` instant comparison)_
- [HUMAN] Fish shell (`chsh` or spawn explicitly): conflict triggers flag-file mechanism (no-session-inject.{uuid}), shell wrapper stops injecting `--session-id`
- [x] False-positive guard: an agent pasting the source of `output_parser.rs` (regex line) does NOT trigger a reset — indented string literals rejected by `line_is_code_or_diff` _(verified: 7 unit tests pass including `ignores_source_code`, `ignores_diff_hunk`, `ignores_markdown_fence`, `ignores_not_found_in_code`)_

## CanvasTerminal lastFg Color Cache Fix (2026-05-11)
- [HUMAN] Powerline prompt (Starship/P10k/Agnoster): wrapped lines preserve correct foreground colors — no color bleeding from powerline arrow glyphs into subsequent text
- [HUMAN] Box drawing characters (borders, frames) followed by regular text → text has correct fg, not the box drawing color
- [HUMAN] Braille patterns (e.g. from `spark` or progress indicators) followed by text → text fg correct
- [HUMAN] Block elements (▄▀█ etc.) followed by text → text fg correct
- [HUMAN] Theme switch → powerline prompt re-renders with correct colors on all segments
- [HUMAN] Bold text after a powerline separator → bold + correct fg (not the arrow's fg)

## Reflow: WRAPLINE Stale Flag Clearing (2026-05-11)
- [x] Type a long command that wraps, press Enter, widen the terminal → wrapped line unwraps into a single line (correct merge) **FIXED: #1721-8895 — grow_columns cursor-at-Line(0) clamping bypass**
- [x] Type a long command that wraps, press Enter, new prompt appears below, widen terminal → old wrapped line unwraps correctly, new prompt stays on its own line (no merge corruption) **FIXED: same root cause**
- [HUMAN] Type a long command that wraps, press Enter, command produces output, widen terminal → wrapped command unwraps, output lines remain independent
- [HUMAN] Shell sends `\r` to redraw current line (e.g. bash prompt redraw) → stale WRAPLINE cleared, no phantom merges on resize
- [x] Alternate screen app (vim, less) → `ReflowMode::None` applies, no reflow on alt screen _(verified: alacritty_terminal/src/term/mod.rs:732-735 resize_reflow sets primary_mode=ReflowMode::None when ALT_SCREEN active)_
- [HUMAN] `clear` command (CSI 2J) → previously wrapped lines in scrollback unwrap correctly
- [HUMAN] Rapid resize (drag terminal edge) → no corruption, final state correct
- [HUMAN] History-only reflow (experimental flag off) → only scrollback rows reflow, screen rows padded/truncated

## File Browser: Native Drag to External Apps (2026-05-13)
- [HUMAN] Flat view: drag a file from file browser to Finder → file is copied to the Finder location
- [HUMAN] Flat view: drag a file from file browser to Mail.app compose → file is attached
- [HUMAN] Tree view: drag a file from tree node to an external app → same behavior as flat view
- [HUMAN] Drag a directory to Finder → directory is copied
- [HUMAN] Internal drag still works: drag a file to a folder within the file browser → file moves/copies
- [HUMAN] Internal drag still works: drag a file to a terminal → path is pasted
- [HUMAN] Drag icon shows the app's 32x32 icon during native drag

## File Browser: Finder → FileBrowser Drop Coordinate Fix (2026-05-29)
- [x] Retina display: drag a file from Finder onto a folder row in the file browser → file transfers into that folder (was: path written to terminal) _(verified live by Boss; root cause: Tauri drop position is logical px on macOS, tauriPhysicalToCss no longer divides by DPR on mac)_
- [x] Retina display: drop a file on the file browser empty area → transfers into the current directory _(panel root carries data-drop-target="folder" + data-abs-path=current dir)_
- [HUMAN] External non-Retina monitor (DPR=1): same drops still work (divide-by-1 no-op)
- [HUMAN] Drop a file onto a terminal pane → path still pasted (regression check, terminal hit-test now precise)
- [HUMAN] Drop a file onto the tab bar → opens as a viewer tab (regression check)
- [HUMAN] Internal tab/pane reorder still works (uses mouse events, unaffected by coordinate fix)

## Compose Panel: Text Persistence (2026-05-13)
- [x] Open compose (Cmd+I) → type text → close (Esc) → reopen (Cmd+I) → text is preserved (MCP maccontrol verified 2026-05-16)
- [x] Cmd+I when compose is open → closes it (proper toggle) (MCP maccontrol verified 2026-05-16)
- [x] Send a command (Ctrl+Enter) → compose closes → reopen → editor is empty (reset after send) _(verified: Terminal.tsx:1132 setPendingComposeText("") + setComposeOpen(false) on send; ComposePanel reinitialises from empty initialText on reopen)_
- [HUMAN] Open compose with no prior text → cursor line is pre-populated (first open behavior)
- [x] Open compose, type something, close, open again → typed text preserved, NOT overwritten by cursor line (MCP maccontrol verified 2026-05-16)

## Status Bar: Pulse on Status Change (2026-05-13)
- [x] Copy text in terminal (Cmd+C on selection) → "Copied to clipboard" flashes accent color then fades _(verified: fix #1717-44d9 in — copy sets statusInfo "Copied to clipboard" (CanvasTerminal.tsx:3499, useTerminalLifecycle.ts:428); StatusBar.tsx:133-142 pulses infoPulse for 600ms on each statusInfo≠"Ready"; :214 toggles s.infoPulse; CSS .info :54 transition + .infoPulse :57-59 accent — single transition-based pulse, no @keyframes)_
- [x] Git operation → status message pulses once in accent color _(verified: same mechanism — git ops call deps.setStatusInfo throughout useGitOperations.ts → StatusBar pulse at StatusBar.tsx:133-142/214)_
- [x] "Ready" status does not pulse (only non-Ready messages trigger it) _(verified: StatusBar.tsx:138 `if (text && text !== "Ready")` guards setInfoPulse(true))_
- [x] Pulse is a single flash (accent → normal), not repeating _(verified: StatusBar.tsx:139-140 setInfoPulse(true) then setTimeout 600ms → setInfoPulse(false); single shot, not animation loop)_

## Group Park/Unpark (2026-05-13)
- [x] Right-click group header → "Park Group" option visible _(verified: GroupSection.tsx context menu includes park/unpark label)_
- [x] Park Group → all repos in group disappear from sidebar _(verified: command palette shows "Park Group: Progetti", "Park Group: IOS")_
- [x] Command palette → "Unpark Group: <name>" → all repos reappear _(verified: command palette integration confirmed)_
- [x] Right-click on fully-parked group header → shows "Unpark Group" instead _(verified: GroupSection.tsx:28 uses `allParked ? "Unpark Group" : "Park Group"`)_
- [HUMAN] Partially parked group (some repos parked individually) → "Park Group" parks the rest _(requires manually parking individual repos then testing group park)_
- [HUMAN] Park Group stops file watchers for all repos in the group _(requires monitoring watcher state during park — not observable via screenshot)_

## Markdown Tab: Mermaid Diagram Rendering (2026-05-13)
- [x] Open a .md file with a ```mermaid code block → diagram renders as SVG (not raw text) _(verified: screenshot shows SVG diagrams rendered)_
- [x] Flowchart (graph TD/LR) renders correctly with dark theme _(verified: screenshot confirms dark-themed flowchart)_
- [x] Sequence diagram renders correctly _(verified: screenshot shows sequence diagram rendered)_
- [x] Multiple mermaid blocks in one file all render _(verified: test file with 2 mermaid blocks both rendered)_
- [x] Non-mermaid code blocks still render as syntax-highlighted code (no regression) _(verified: typescript block rendered as code, not as diagram)_
- [x] Invalid mermaid syntax → shows the raw code block (graceful fallback) _(verified: ContentRenderer.tsx catches render errors and leaves original code block)_
- [x] Mermaid library loads lazily (only when a mermaid block is present) _(verified: dynamic import in ContentRenderer.tsx onMount, only when `.language-mermaid` elements exist)_

## Sidebar: External API Hidden from Agent Menu (2026-05-13)
- [x] Right-click branch → Add Agent submenu → "External API" is NOT listed _(verified: buildSidebarAgentMenuItems filters `a.type !== "api"`)_
- [x] Tab bar Add Agent menu → "External API" is NOT listed (was already filtered) _(verified: buildAgentMenuItems filters `a.type !== "api"`)_

## AST Navigation via mdkb (2026-05-15)
- [x] Outline panel: open a file in editor → toggle outline → symbols listed with kind badges and line numbers _(verified: screenshot shows Outline panel with kind badges fn/c/etc and line numbers for App.tsx)_
- [HUMAN] Outline panel: click a symbol → editor scrolls to that line _(requires precise click targeting on outline row)_
- [x] Outline panel: empty file or non-indexed language → shows "No symbols found" _(verified: OutlinePanel.tsx:101 fallback renders "No symbols found" when symbols empty + file active)_
- [x] Go-to-definition: Cmd+Click on a symbol name in editor → opens definition file at correct line _(verified: CodeEditorTab.tsx:336-356 Cmd+Click handler calls mdkb_goto_definition)_
- [x] Go-to-definition: Cmd+Click on unknown symbol → nothing happens (no error) _(verified: CodeEditorTab.tsx:348 returns null for unknown symbols, catch logs debug)_
- [x] Find references: Shift+F12 on a symbol → References panel opens with callers list _(verified: CodeEditorTab.tsx:362-366 Shift-F12 keybinding triggers mdkb_references)_
- [x] Find references: right-click → "Find References" context menu item works _(verified: CodeEditorTab.tsx:589 context menu "Find References (Shift+F12)" entry)_
- [HUMAN] mdkb_status: DevTools `await __TAURI__.core.invoke("mdkb_status")` → `{available: true, connected: true}` _(requires DevTools console access)_

## Command Block System (2026-05-20)
- [HUMAN] Run a few commands → scrollbar shows color-coded marks at block boundaries
- [HUMAN] Hold Ctrl+Cmd → timestamp overlay appears showing relative time for each block
- [HUMAN] Click in the gutter area → selects the entire block output
- [x] Cmd+Shift+. → toggles fold on the current block (collapses/expands) _(verified: keybindingDefaults.ts:157 "block-fold-toggle": "Cmd+Shift+."; App.tsx:2166-2167 dispatches to blockFoldToggle())_
- [x] Cmd+Shift+Up/Down → jumps between block boundaries _(verified: keybindingDefaults.ts:158-159 "block-prev": "Cmd+Shift+ArrowUp", "block-next": "Cmd+Shift+ArrowDown")_
- [x] Cmd+Shift+B → toggles block-scoped search (search restricted to current block) _(verified: keybindingDefaults.ts:160 "block-search-toggle": "Cmd+Shift+B"; App.tsx:2169 dispatches handler)_
- [HUMAN] Cmd+F with block-scoped toggle ON → only matches within current block shown
- [HUMAN] Settings > Terminal > Blocks → toggle timestamps and folding on/off
- [HUMAN] Run 500+ commands → oldest blocks evicted, no crash or memory growth
- [HUMAN] Claude Code session: tool calls show as blocks without OSC 7770 (heuristic detection)

## Generators Modal (2026-05-20)
- [x] Command palette → type "generator" → "Open generators" action appears in Generators category _(verified: actionRegistry.ts:93 "open-generators" registered with label "Open generators" and category "Generators")_
- [x] Modal opens — left sidebar shows 10 generators: Password, UUID v4, UUID v7, ULID, CUID2, JWT Secret, TOTP Secret, Nano ID, Slug, Ed25519 Key _(verified: GeneratorsModal.tsx:26-38 GENERATORS array with exactly these 10 entries in this order)_
- [x] Selecting each generator auto-generates a value immediately (no manual click needed) _(verified: GeneratorsModal.tsx:122-126 createEffect tracks active() signal; calls generate() on every change including initial mount)_
- [x] Password: length slider (4–128) and charset checkboxes (A–Z, a–z, 0–9, !@#…) work _(verified: GeneratorsModal.tsx:57-65 pwLen signal with slider; charset checkboxes for upper/lower/digits/symbols; hasOptions:true on password)_
- [x] Nano ID: length number input (4–64) works; changing length regenerates _(verified: GeneratorsModal.tsx:64-65 nanoLen signal default 21; lines 204-214 number input with min=4 max=64)_
- [x] Ed25519 Key: shows two textareas (Private key PKCS#8 / Public key SPKI) + "Copy Private" + "Copy Public" buttons _(verified: GeneratorsModal.tsx:228-236 isKeypair() guard renders two outputGroup divs with textareas for value() and extra())_
- [x] Copy button → clipboard contains the generated value; button shows "Copied!" for 2s then resets _(verified: GeneratorsModal.tsx:104-109 copy() sets copied=true then setTimeout 2000ms resets; button label renders copied() ? "Copied!" : "Copy")_
- [x] Regenerate button produces a new value each click _(verified: GeneratorsModal.tsx:268-270 Regenerate button calls generate() which clears value/extra, invokes "generate_value" with fresh request)_
- [x] Escape key closes modal; clicking overlay closes modal _(verified: GeneratorsModal.tsx:112-120 keydown handler on document captures Escape, calls props.onClose(); onCleanup removes listener)_
- [x] No generated value appears in app logs (`curl http://localhost:9876/logs | grep -i "password\|secret\|key"` returns nothing) _(verified: GeneratorsModal.tsx:95 and generators.rs:37 explicit SECURITY comments; no appLogger/tracing calls on generated values)_

## Process Monitor
- [x] HTTP endpoint: `curl http://localhost:<port>/process/stats` returns JSON array of `{session_id, name, pid, rss_kb, cpu_pct}` _(verified via curl 2026-05-28: with 1 terminal open returns 2 entries — TUIC main process + child session process, both with correct fields)_
- [x] HTTP panel: `curl http://localhost:<port>/process/monitor` returns HTML dashboard _(verified via curl 2026-05-28: returns HTML with `<title>Process Monitor</title>`)_
- [x] MCP tool: `session action=process_stats` returns `{processes: [...]}` with TUIC + child stats _(verified via MCP session list earlier this session; process_stats endpoint confirmed via HTTP)_
- [HUMAN] Panel: open `/process/monitor` in TUIC tab — shows summary stats + process tree table
- [x] Panel: auto-refresh at **3s** interval shows live CPU/memory updates _(verified + interval corrected: process_monitor.html:47 setInterval(refresh, 3000) — 3s, not 5s)_
- [HUMAN] Panel: changing refresh interval to Manual stops auto-polling
- [HUMAN] Panel: Refresh button triggers immediate data fetch

## IME Candidate Window Positioning — Issue #42 Bug 1 (2026-05-25)
- [HUMAN] Windows + Chinese Pinyin IME: type in a plain shell terminal → IME candidate window appears near the cursor (not at top-left corner of screen)
- [HUMAN] Windows + Chinese Pinyin IME: type in Claude Code terminal → same behavior, candidate window near cursor
- [HUMAN] macOS + Japanese IME: type in terminal → candidate window appears near cursor
- [HUMAN] macOS + dead-key composition (accents: `, ´, ^, ¨) → still works correctly (no regression from IME repositioning)
- [HUMAN] Scroll terminal while IME is not composing → cursor position updates (input element follows cursor on next paint)
- [HUMAN] Resize terminal window during typing → IME input position adjusts to new cursor coordinates
- [HUMAN] Split pane: IME candidate window appears in the correct pane (not in the other pane)
- [HUMAN] macOS Option+key sequences (Alt+B word back, Alt+F word forward) → still work correctly (no regression)

## Worktree Fixes — PR #47 (2026-05-25)
- [HUMAN] Create a worktree, delete its directory externally (rm -rf), then view it in sidebar → branch name shows the actual HEAD (or error), not the originally requested branch
- [HUMAN] Click "Remove" on a worktree → button shows "…" and is disabled during removal → re-enables after completion
- [HUMAN] Double-click "Remove" on a worktree rapidly → only one removal operation runs (no concurrent remove errors)
- [HUMAN] Remove a worktree that has already been partially cleaned up → error message is clear, no crash
- [HUMAN] Remove a worktree while another removal is in flight (different branch) → both complete independently

## Git Diff: Deleted File Fix (2026-05-25, uncommitted)
- [HUMAN] Delete a tracked file (`git rm foo.txt`), open its diff → diff shows deletion (red lines), no crash
- [HUMAN] Untracked new file → diff shows addition (green lines) as before (no regression)
- [HUMAN] Modified file → diff shows changes as before (no regression)

## PTY Debug Logging Cleanup (2026-05-25, uncommitted)
- No user-visible behavior change — removed shell-spike debug logging from BUSY↔IDLE transitions

## Remote Settings Context (2026-05-27)
- [HUMAN] Open Settings → Agents tab with a local repo selected → verify agent list loads normally (no banner)
- [HUMAN] Switch to a remote repo in Settings nav → Agents tab shows "Configuring remote: <name>" banner
- [HUMAN] With remote repo active, add/edit/delete a run config → verify changes persist on remote daemon (not local)
- [HUMAN] Disconnect remote daemon, open Agents tab with remote repo → verify error state "Remote config unavailable"
- [HUMAN] Switch from remote repo back to local (or global) → verify banner disappears, local config loads correctly
- [VISUAL] Remote banner styling: accent border, icon color, spacing

## PWA log-mode reconnect dedup + shell typing (2026-05-31)
- [x] WS log reconnect resumes from tracked cursor, not mount offset _(verified: transport.test.ts "log mode reconnect resumes from the tracked cursor"; server total_lines in session.rs catch-up + poll frames)_
- [x] computeInputDelta sends minimal end-anchored delta (no full-line storm) _(verified: CommandInput.test.ts "REGRESSION: mid-line fix sends a minimal delta")_
- [HUMAN] Mobile PWA: background/foreground or lock/unlock the phone during an active session → scrollback is NOT duplicated on reconnect (the original bug)
- [HUMAN] Mobile PWA shell: type a command, tap mid-line and fix a typo (no arrows) → terminal shows correct line, no flicker/garbage burst
- [HUMAN] Mobile PWA shell: use the new ← / → keybar keys to move the readline cursor mid-line and insert/delete → readline edits at the right position (textarea may show a stale tail; screen is authoritative)
- [HUMAN] Mobile PWA agent (Claude/Codex): typing + slash menu + tab completion still work (no regression from the delta change)

## File browser per-repo directory memory (#72, 2026-06-03)
- [x] FileBrowser remembers current subdir per root: switch away to another repo and back restores the subdir, not the root _(verified: FileBrowserPanel.tsx rootToSubdir map saved on root-change in the load effect, restored via setCurrentSubdir(rootToSubdir.get(fsRoot) ?? "."))_
- [HUMAN] Browse into a subdir in repo A, switch to repo B, switch back to A → file browser is still in that subdir
- [HUMAN] Browse into a subdir in repo A, switch to B, delete that subdir externally, switch back to A → falls back to root without an error screen

## Terminal link context menu (#57, 2026-06-03)
- [HUMAN] In a shell, `ls` a dir → right-click on an underlined filename/URL → context menu shows "Open" and "Copy link"
- [HUMAN] Right-click NOT on a link → no link menu appears (default behavior preserved)
- [HUMAN] Context menu "Open" → opens the file (md preview / editor) or URL, same as left-click
- [HUMAN] Context menu "Copy link" → clipboard contains the resolved path / URL, file is NOT opened
- [HUMAN] Left single-click on a link still opens immediately (UI-first: open is primary, not Cmd-gated)
- [HUMAN] Drag-select across a filename still selects text without opening (copyOnSelect copies)

## FileBrowser copy & paste: cross-repo + focus fix (2026-06-04)
- [x] Same-repo `copy_path` works at backend level _(verified live via debug invoke_js: copy_path(tuic,"PR.md","src/...") → OK)_
- [x] Cross-repo paste fails on old code path _(verified live: copy_path(MCPublisher,"PR.md",...) → "Failed to resolve path: No such file or directory")_
- [x] `copy_path_abs`/`move_path_abs` copy/move a file across two different repos by absolute path _(verified: fs.rs test_copy_path_abs_cross_repo + test_move_path_abs_cross_repo)_
- [x] `copy_path_abs` rejects directories; same-path is a no-op _(verified: fs.rs test_copy_path_abs_rejects_directory + test_copy_path_abs_same_path_is_noop)_
- [HUMAN] Copy a file in repo A, switch FileBrowser to repo B, navigate into a dir, paste → file appears in B (the cross-repo bug)
- [HUMAN] Cut a file in repo A, switch to repo B, paste → file moves to B and is gone from A
- [HUMAN] Same-repo: copy a file, double-click into a subdir, Cmd+V → file appears in the subdir (focus fix: keyboard paste works after entering a folder)
- [HUMAN] Paste a file that no longer exists (delete source externally first) → an error toast "Copy failed"/"Move failed" appears instead of silent nothing

## Watcher PR-pushed worktree provisioning (2026-06-04)
- [x] Reuse existing worktree session for (repo, branch) _(verified: watcher.rs find_session_for_branch_reuse_and_miss unit test + find_existing_pr_session adapter)_
- [HUMAN] New commit on another author's open PR → watcher provisions a worktree (appears in TUIC) + spawns a live PTY session (real GitHub PR + push required)
- [HUMAN] Second push to the same PR branch → reuses the same worktree session, no duplicate worktree

## Watcher-fire frontend handoff (2026-06-04)
- [x] handleWatcherFire dispatch logic _(verified: src/__tests__/watcherFire.test.ts — 5 cases: prompt→execute, instructions→startAgent, PR→assisted, PR-instructions→assisted, missing-prompt→warn)_
- [HUMAN] End-to-end: a real watcher-fire event triggers the smart prompt in the target terminal session
- [HUMAN] PR-review fire: agent's `gh pr review/approve/comment` surfaces a pendingApproval (ui:confirm) dialog before hitting GitHub

## WatcherManager: PR-pushed trigger + repo + prompt picker (2026-06-04)
- [x] pr_pushed in trigger dropdown; repo selector + authored-by-others toggle gated by isGitTrigger _(verified: WatcherManager.tsx <Show when={isGitTrigger(formTrigger())}> + watcherManager.test.ts)_
- [x] Smart-prompt picker sets prompt_id; instructions is advanced fallback _(verified: formPromptId signal + showAdvanced toggle; watcherFormReady test covers prompt-or-instructions)_
- [VISUAL] Open Watchers popover, select "PR pushed": repo dropdown + "authored by others" checkbox appear, terminal-only instruction textarea collapses to advanced; layout matches STYLE_GUIDE — screenshot after rebuild

## WatcherManager: PR-opened trigger + Advanced removal + tooltip (2026-06-05)
- [x] `pr_opened` trigger end-to-end: WatcherTrigger::PrOpened, PrTransition::Opened, on_pr_opened, pr_rule_matches PrEventKind gating _(verified: watcher.rs tests pr_opened_rule_matches_only_opened_kind / pr_pushed_rule_inert_for_opened_kind; 104 cargo tests pass)_
- [x] PrOpened fires at most once per PR (first-poll seed suppressed) _(verified: github_poller.rs process_repo_update `first_poll_for_repo` guard)_
- [x] Advanced toggle removed from form; instructions textarea shows when no smart prompt _(verified: WatcherManager.tsx <Show when={!formPromptId() && !isGitTrigger(...)}>; tsc+biome+vitest green)_
- [HUMAN] Real PR-opened e2e: open a new PR (by another author) on a repo with an active PrOpened watcher → worktree session provisioned + smart prompt fires (needs live GitHub + poll cycle)
- [VISUAL] Watchers popover: "PR opened" option in trigger dropdown; selecting it shows repo + authored-by-others fields (same as PR pushed) — screenshot after rebuild
- [VISUAL] Cooldown `?` shows a CSS tooltip on hover (native `title` is a no-op in WKWebView); bubble not clipped at the panel's right edge — screenshot after rebuild

## PrDetailPopover: footer buttons aligned to PR panel (2026-06-05)
- [x] View Diff / GitHub / Merge buttons restyled to `.ghActionBtn` language: compact (font-xs, 4px/12px, radius-sm), transparent bg, wrap layout, accent border-glow on hover _(verified: PrDetailPopover.module.css; PrDetailPopover.test.tsx 39 tests pass)_
- [VISUAL] Open PR detail popover: footer buttons match the GitHub PR panel buttons in size/border/hover; no always-on filled background — screenshot after rebuild

## CanvasTerminal: right-click link menu under mouse-reporting (2026-06-05)
- [x] mousedown no longer preventDefaults a right-click that lands on a detected link while an app has mouse reporting on (mouseMode>0) — lets the contextmenu (Open/Copy link) fire _(verified: CanvasTerminal.tsx mousedown `e.button === 2 && detectedLinks.get(pos.row)?.some(...)` early-return before forward/preventDefault; tsc+biome green)_
- [HUMAN] Rebuild, run Claude Code (or any TUI with mouse reporting) in a pane, right-click a URL/file link in its output → Open / Copy link menu appears. Root cause was WKWebView suppressing `contextmenu` after a preventDefault'd right-button mousedown; needs a live macOS WKWebView run to confirm (debug server was off, no static repro).
- [HUMAN] Regression check: right-click OUTSIDE a link (still under mouse reporting) keeps prior behavior; left-click/drag forwarding to the app unchanged.

## Global Workspace: terminal hidden after repo round-trip (2026-06-05)
- [x] activate() restores focus to a promoted terminal after restoring the layout — TerminalArea gates pane visibility on terminalsStore.activeId, which a repo switch reset to a repo terminal _(verified: globalWorkspace.ts activate() setActive(target); regression test "activate restores focus to a promoted terminal" fails without fix, passes with it; 40/40 globalWorkspace tests)_
- [HUMAN] Live: activate Global Workspace, configure (promote terminals), switch to another repo, switch back, click Global Workspace → the workspace terminal/split appears immediately (no need to click a tab first).

## BranchesTab: focus-steal fix + mouse-first (remove letter hotkeys) (2026-06-05)
- [x] Auto-focus now fires once (`didAutoFocus` guard) instead of on every branch reload — the reactive `createEffect` tracked `loading()/branches()`, so each `repo-changed` refresh re-stole focus from the terminal, hijacking keystrokes (`n`/`d`/…) and image paste while the Git panel was open _(verified: BranchesTab.tsx createEffect guard; tsc+biome green)_
- [x] Letter action shortcuts removed (`n` `d` `r` `R` `M` `p` `P` `f` + `Enter`-checkout); navigation kept (`↑`/`↓`, `/`, `Esc`) _(verified: handleKeyDown now only handles ArrowUp/Down/Escape/"/"; no actionRegistry duplicates)_
- [x] `+` New-branch button added to the search bar header to preserve mouse-create (was the only `startCreate()` entry point) _(verified: PlusIcon button onClick=startCreate, reuses .foldToggle style)_
- [VISUAL] Open Git panel → Branches: a `+` button sits left of the folder-fold toggle in the search bar; clicking it opens the inline create form; layout matches STYLE_GUIDE — screenshot after rebuild
- [HUMAN] With Git panel open, click into a terminal and type (including after the letter "n") + paste an image → all input reaches the terminal; trigger a git refresh (commit/fetch) while typing → focus stays in the terminal (no re-steal)

## useMouseDrag: stuck drag fixed via pointer capture (2026-06-05)
- [x] initMouseDrag converted from mouse-events to pointer-events; setPointerCapture(pointerId) fires only after the movement threshold is crossed (real drag, not a click) so child onClick semantics are unchanged; listeners stay on document (captured events bubble); pointercancel cleanup added _(verified: src/hooks/useMouseDrag.ts; tsc+biome green)_
- [x] Call sites onMouseDown→onPointerDown + param MouseEvent→PointerEvent: RepoSection, GroupSection, TabBar (8 bindings), PaneTree _(verified: grep initMouseDrag callers all converted; tsc green)_
- [HUMAN] Live macOS WKWebView: drag a repo in the Sidebar, a tab in the TabBar, and a pane in a split → the ghost releases on mouseup and the drop-line clears (previously the ghost stayed glued to the cursor). Root cause was native NSDragging swallowing mouseup; needs a real pointer gesture to confirm.
- [HUMAN] Regression: plain click on a repo header (toggle expand), on inner repo buttons (GitHub badge, menu, collapse initials), and on a tab (activate) still works — capture only engages past the drag threshold.

## genai extended thinking (Opus 4.7+) + worker renderer freeze fix (2026-06-06)
- [x] Backend: `ConversationConfig.reasoning` (ReasoningLevel Auto/Off/Low/Medium/High), `supports_extended_thinking` gates Opus 4.7+, `resolve_reasoning` maps to genai `ReasoningEffort`; ChatOptions rebuilt per-iteration with `with_reasoning_effort`+`with_capture_reasoning_content` _(verified: conversation_engine.rs; 26 module tests pass incl resolve_reasoning/supports_extended_thinking)_
- [x] Backend: streaming handles `ReasoningChunk` → `ConversationEvent::ReasoningChunk`; assistant turn appended from full `captured_content` to preserve thinking block + ThoughtSignature for the tool-use continuation (Anthropic requirement) _(verified: conversation_engine.rs append path; reasoning_chunk serialization test)_
- [x] Backend: `commands.rs` threads `reasoning_effort` param, falls back to persisted `AiChatConfig.reasoning_effort`; 50ms bridge batches ReasoningChunk like TextChunk _(verified: cargo build green; ai_chat 56 tests pass)_
- [x] Frontend: `conversationStore` accumulates `reasoning_chunk` into `reasoningChunks()`, reset on new turn + reset() _(verified: aiAgentStore.test.ts reasoning_chunk accumulation + reset)_
- [VISUAL] AIChatPanel shows a collapsible "Thinking" disclosure above the answer; auto-open while thinking; muted styling per STYLE_GUIDE — screenshot after rebuild
- [VISUAL] SettingsPanel → AI Chat: "Extended thinking" dropdown (Auto/Off/Low/Medium/High) below Temperature — screenshot
- [HUMAN] Live with Claude Opus 4.7+: send a chat message → reasoning streams into the Thinking disclosure; a multi-tool autonomous run continues without an Anthropic thinking/signature rejection; setting Off disables it
- [x] Worker renderer freeze: `createRepaintScheduler` now races rAF with a 100ms setTimeout fallback (WebKit suspends worker rAF under CPU pressure → glyphs froze while input worked) _(verified: workerGridState.test.ts 17 tests incl timer-fallback/race/stop)_
- [HUMAN] Live: under heavy CPU load (e.g. a Rust release build at 100%), the terminal keeps painting Ink animations and typed glyphs appear within ~100ms (previously froze "completely immobile" with cursor moving but characters delayed 1s+)

## worktree switch prompt auto-cancel countdown (2026-06-06)
- [x] `ConfirmDialog` gains opt-in `autoCancelMs`: a per-second countdown signal appended to the cancel-button label that auto-clicks cancel (onClose) at 0; interval cleaned up on hide/interaction _(verified: ConfirmDialog.tsx:23-42; tsc green)_
- [x] `autoCancelMs` threaded through `ConfirmOptions`→`ConfirmDialogState`→App.tsx ConfirmDialog binding; absent on all other confirm dialogs (remove worktree, folder-drop) _(verified: useConfirmDialog.test.ts 11 pass incl threads/undefined cases)_
- [x] Worktree-created switch prompt sets `autoCancelMs: 10_000` so "Stay" shows a 10s countdown and is auto-clicked if untouched _(verified: useWorktreeSwitchPrompt.ts:64)_
- [VISUAL] Create a worktree via TUIC/MCP → switch prompt shows "Stay (10)" counting down; left untouched for 10s it auto-dismisses (stays put); clicking Switch/Stay or pressing Enter/Esc before 0 still works

## focus-last-terminal — return to last terminal (#77) (2026-06-06)
- [x] `terminalsStore.previousActiveId` tracks the terminal left behind on `setActive`; `getPreviousActiveId()` returns it only if still alive; cleared on `remove` _(verified: terminals.test.ts "previousActiveId (return to last terminal)" 5 cases — toggle, no-op on re-activate, cleared on removal)_
- [x] Action `focus-last-terminal` wired: ACTION_NAMES + unbound default, ACTION_META (Navigation), keyboard dispatch case, App.tsx handler calls `navigateToTerminal(prevId)` (switches repo/branch/tab/pane + focus) _(verified: tsc green; actionRegistry + useKeyboardShortcuts mocks updated and passing)_
- [HUMAN] Live: bind `focus-last-terminal` in Settings → Keyboard, open terminals across 2+ repos, switch to a terminal in another repo, press the hotkey → jumps back to the prior terminal (and its repo/branch); press again → toggles forward. Closing the previous terminal makes the hotkey a no-op.

## Jump to waiting terminal (#78) + Cmd+? help binding fix (2026-06-06)
- [x] New action `jump-waiting-terminal` (Cmd+U): cycles to the next terminal with `awaitingInput != null` across all repos/branches in tab order, skips detached, no-op when none waiting; uses `navigateToTerminal` for full repo/branch/pane context switch _(verified: App.tsx jumpWaitingTerminal + utils/nextWaitingTerminal.ts; nextWaitingTerminal.test.ts 6 cases incl wrap/empty/active-not-waiting; useKeyboardShortcuts.test.ts "Cmd+U" dispatch)_
- [x] Wired through canonical path: ACTION_NAMES + DEFAULT_BINDINGS (Cmd+U), ShortcutHandlers + dispatch case, ACTION_META + handlerMap (shows in Command Palette + Settings→Keyboard Shortcuts) _(verified: tsc + biome green; actionRegistry.test.ts passes with new mock)_
- [x] Bug fix: `toggle-help` default was `"Cmd+?"` which normalizes to `cmd+?` but a real `?` keypress holds Shift → event combo `cmd+shift+/`, so Help (Cmd+?) was dead. Changed default to `Cmd+Shift+/` (JS) + native menu `CmdOrCtrl+Shift+/` (menu.rs). Test that hid the bug fixed to fire a realistic shifted `?` _(verified: useKeyboardShortcuts.test.ts "Cmd+? toggles help" now fires shiftKey:true; cargo check green)_
- [HUMAN] Live: with 2+ agent terminals in different branches awaiting input, press Cmd+U repeatedly → focus jumps to each waiting terminal in turn (switching branch/repo), wraps around; does nothing when none waiting. Press Cmd+? → Help panel toggles (was dead before).

## Fix: "Investigate" toast button did nothing (2026-06-06)
- [x] Root cause: `ai-suggestion` toast (Command failed/timed out) fires for any agent terminal (state.rs:1337, gated on ai_suggestions_enabled default=agent present). Its Investigate onClick only called `startAgent(session_id, goal)` — which runs on `activeConversation()` (the focused terminal, not the failed session) and never opened the AI Chat panel, so nothing visible happened (and silently no-op'd if the active conv's agent was already running) _(verified: App.tsx:559-575; conversationStore.startAgent uses activeConversation/activeKey)_
- [x] Fix: onClick now mirrors the working "Fix this error" action — `setAiChatPanelVisible(true)` + `switchToTerminalBySession(session_id)` + `startAgent`. Extracted `switchToTerminalBySession` from contextMenuActions.ts to `src/utils/switchToTerminalBySession.ts` (DRY) _(verified: tsc + biome green; AIChatPanel/contextMenu tests 22 pass)_
- [x] Guard: if AI Chat is disabled, Investigate shows "Enable AI Chat in Settings to investigate" instead of invoking an invisible agent _(verified: App.tsx onClick isAiChatEnabled gate)_
- [HUMAN] Live: run an agent terminal, make a command fail (e.g. `false` or a bad git cmd) → "Command failed" toast appears → click Investigate → AI Chat panel opens, switches to that terminal's conversation, and an autonomous investigation starts on the correct session

## D&D pointer-migration tests + engine audit (2026-06-06)
- [x] Fixed 4 failing D&D tests: Sidebar.test.tsx + TabBar.test.tsx dispatched mouse* events while production migrated to pointer* — converted to fireEvent.pointerDown/pointerMove/pointerUp with explicit pointerId (handlers filter ev.pointerId); renamed "(mouse-based)" describe → "(pointer-based)" _(verified: 135/135 in both files; full suite 4135/4135 green)_
- [x] useMouseDrag engine audit (Boss: "è efficace?"): sound overall (pointer capture + threshold + ghost + cleanup + pointercancel/Escape). I initially "optimized" two things then REVERTED both — Boss flagged that the early preventDefault was the stuck-ghost fix: (1) deferring getBoundingClientRect and (2) deferring preventDefault past the threshold. The `ev.preventDefault()` at the TOP of handleMove (every move, incl sub-threshold) is LOAD-BEARING on WKWebView — it stops native NSDragging from starting and swallowing pointerup (ghost glued to cursor). Reverted to baseline + added an inline warning comment + mdkb entry usemousedrag-preventdefault-load-bearing _(verified: useMouseDrag.ts:47-53 preventDefault at top; tsc green; 135 drag tests green)_
- [HUMAN] Live macOS WKWebView: drag a tab, a repo, a group, a split-pane → ghost follows + releases on pointerup; plain click still selects text / activates; Escape and native-drag-cancel both abort cleanly

## Editor + diff viewer performance pass (#020-abfe) (2026-06-07)
- [x] `stat_path` extended with `modified_at` (ms) + `size`; editor 5s poll stat-gates before reading file content via `diskStatUnchanged` _(verified: fs.rs cargo test stat_path 5 pass + editorDiskStat.test.ts 4 cases incl truncate-rewrite same-mtime)_
- [x] Shared `DiffFileList` virtualizes multi-file diffs with @tanstack/solid-virtual; FileSection/sectionToRawDiff duplication removed from BranchDiffScrollView + PrDiffTab _(verified: tsc + biome + DiffFileList.test.tsx 3 + PrDiffTab/DiffViewer 19 pass; full suite 4157 green)_
- [x] DiffTab: extractHunks memoized; row→hunk/line cache built once per diff (no querySelectorAll per mousemove); HunkRevertOverlay positions memoized on diff+ResizeObserver (not hover) _(verified: code DiffTab.tsx getRowCache/findLineInfo/applyLineSelectionStyles; diffPatch 47 pass)_
- [VISUAL] Open a PR diff or working-tree scroll-mode diff with MANY files (50+) → only on-screen file sections are in the DOM; scroll is smooth; sticky per-file headers still stick; file paths still open on click (working-tree)
- [HUMAN] Single-file diff: hover a hunk → revert button appears at the correct row; drag-select lines across a hunk → correct lines highlight; Discard/Unstage selected still applies the right git patch (git-patch path unchanged)
- [VISUAL] Open a >3000-line single-file diff → "This diff is large (N lines) / Render anyway" guard shows; clicking Render anyway renders it
- [NOTE] DEFERRED: Cmd+F in virtualized scroll-mode only matches mounted (on-screen) sections — see DiffFileList.tsx DEFERRED comment. Needs data-level search for full coverage.

## Fix: MCP worktree create now refreshes the sidebar immediately (2026-06-08)
- [x] Root cause: MCP `worktree_create` emits the `worktree-created` Tauri event but never registers the branch in `repositoriesStore`; the listener (`useWorktreeSwitchPrompt.ts:28`) only added it via `handleBranchSelect` IF the user accepted the switch prompt. Dismiss → sidebar stale until the debounced file-watcher refresh. In-app create registers immediately via `setupNewWorktree`→`setBranch` (`useGitOperations.ts:1292`) _(verified: code inspection mcp_transport.rs:1449-1466 emits event only; RepoSection.tsx:554 renders Object.values(repo.branches))_
- [x] Fix: `worktree-created` listener now calls `repositoriesStore.setBranch(repo_path, branch, {worktreePath})` on arrival, guarded on repo existence, so the sidebar shows the new worktree instantly regardless of the switch prompt _(verified: tsc clean; useWorktreeSwitchPrompt.ts:30-38)_
- [HUMAN] Live (needs rebuild): from an MCP client run `repo action=worktree_create path=<open-repo>` → the new branch row appears in the sidebar under that repo IMMEDIATELY, before/without accepting the switch prompt

## Remove off-main-thread terminal renderer (2026-06-08)
- [x] Deleted worker-only modules: `renderer.worker.ts`, `workerProtocol.ts`, `fontAssets.ts` + their tests. `gridRenderer.ts` (the canvas2d renderer) stays — it was never worker-exclusive. `decideFrameGrid` (+ `FrameGridPrev`/`FrameGridDecision`) relocated from `workerGridState.ts` into `canvasTerminalUtils.ts` (still used by main `onFrame`); coverage preserved in `__tests__/decideFrameGrid.test.ts` _(verified: vitest 4112 pass; tsc + biome clean)_
- [x] `CanvasTerminal.tsx`: removed `rendererMode`/`renderWorker`/`workerRenderer`, the `chooseRenderer` mount branch, worker `postResize`/`postFrame`/`postFonts`, worker teardown, and the worker-only guards in `remeasure`/`paintFrame`/hidden-resize. `onFrame` now inlines ack→decode (was `receiveFrame`) _(verified: rg CLEAN — no stray worker refs; cargo check + clippy -D warnings clean)_
- [x] Removed `offscreenRenderer` setting end-to-end: `settings.ts` (state/default/save/hydrate/setter), `config.rs` (`offscreen_renderer` field + 2 defaults), AppearanceTab toggle, settings.test.ts block; dropped the stale `plans/terminal-worker-renderer.md` row from `docs/sync-matrix.md` _(verified: settings.test.ts pass; config.rs cargo check clean)_
- [VISUAL] Live (needs rebuild): open several terminals, type/scroll, resize panes, hide/show tabs, switch themes/fonts → glyphs render correctly, cursor/selection/links/scrollbar work, no blank canvas. (Off-thread path is gone; main-thread canvas2d is now the only renderer, as it was by default.)

## Crash fix: github-transition pushed/opened → undefined.cls (2026-06-08)
- [x] Root cause: Rust poller emits `github-transition` for ALL `PrTransition` variants incl. watcher-only `pushed`/`opened` (PR-opened/commit-pushed triggers carrying author/head_ref_oid). Frontend `handleTransition` blindly added every type as a PR notification; the popover render then did `NOTIFICATION_LABELS["pushed"]` → undefined → `info.cls` → `undefined is not an object (evaluating 't.cls')` whole-app crash _(verified: github_poller.rs:587/608 emit, github.ts handleTransition, Toolbar.tsx:545-548)_
- [x] Fix: `PrNotificationType` now derives from runtime `PR_NOTIFICATION_TYPES` allowlist + `isNotificationType()` guard; `handleTransition` skips any transition not in the allowlist (pushed/opened and any future watcher-only variant ignored by default, no crash) _(verified: prNotifications.test.ts isNotificationType 3 cases; 23 file tests + tsc + biome green)_
- [HUMAN] Live: open a brand-new PR on a watched branch (fires `opened`) and push a commit to an open PR (fires `pushed`) → app does NOT crash, no bogus popover notification appears for those

## Fix: agent intent title no longer overwrites a renamed tab (2026-06-10)
- [x] Root cause: `intent` event handler (`Terminal.tsx:417`) overwrote tab `name` from `parsed.title` whenever `intentTabTitle`/per-agent allowed — without the `nameIsCustom` guard the OSC 0/2 path already has. After "Rename Tab" set `nameIsCustom:true`, the next prompt's `intent:` token clobbered the custom name with the agent's intent title _(verified: code inspection Terminal.tsx:607 OSC guard vs 417 missing guard)_
- [x] Fix: extracted pure `shouldApplyIntentTitle()` (`intentTitle.ts`) that also checks `!nameIsCustom`; intent handler now routes through it _(verified: tsc clean; intentTitle.test.ts 5 pass)_
- [HUMAN] Live (needs rebuild): rename a terminal tab via "Rename Tab" → run a prompt that emits an `intent:` token → tab keeps the custom name (does NOT revert to the agent's intent title)

## #71 Phase 1: custom launcher cursor + base placeholder tokens (2026-06-09)
- [x] Rust `expand_placeholders` resolves `{path}`/`{file}` (focused file, else repo), `{repo}`, `{fileDir}` (file parent, else repo), `{cwd}` (else repo), `{home}` (dirs::home_dir), `{line}`/`{column}` (cursor, default 1); `open_in_custom` takes `LaunchContext` _(verified: agent.rs expand_placeholders/open_in_custom; 8 cargo tests green incl. no-file & cwd fallbacks)_
- [x] Cursor surfaced: `editorTabsStore.setCursor` + `cursorLine/cursorCol`; `CodeEditorTab` separate ungated updateListener pushes `(line.number, head-line.from+1)`; `Toolbar` passes cwd (`terminalsStore.getActive().cwd`) + cursor (`editorTabsStore.getActive()`) to `IdeLauncher`; `handleOpenCustom` sends real `ctx` (was `line/col=null`) _(verified: editorTabs setCursor tests + Toolbar cursor-wiring test green; make check exit 0)_
- [HUMAN] Live (needs rebuild): add a custom launcher with args `--file {file} --pos {line}:{column} --repo {repo} --cwd {cwd} --home {home} --dir {fileDir}` (exec e.g. `echo` into a logging wrapper), open a code file, move the cursor to a known line/col, focus a terminal in a known cwd → launch it → the spawned process receives the EXPANDED values (real cursor line/col, not 1), correct file/repo/cwd/home/fileDir

## Notification fixes: mic-prompt on settings open + Test rate-limit masking volume (2026-06-09)
- [x] Root cause (mic prompt): `NotificationsTab` enumerated audio devices eagerly via `createResource` at tab mount → `list_audio_output_devices` → cpal CoreAudio output scan, which triggers the macOS microphone TCC prompt (app declares `NSMicrophoneUsageDescription` + `audio-input` entitlement for dictation) _(verified: code inspection NotificationsTab.tsx old createResource:89; notification_sound.rs:253 list_output_devices uses rodio::cpal::default_host().output_devices(); live invoke returned 3 devices)_
- [x] Fix (mic prompt): enumeration is now LAZY — `null` until the user clicks **Choose output device…**. Opening the tab no longer touches cpal, so no mic prompt. Added a hint explaining the prompt + that notifications never record _(verified: tsc + biome clean; NotificationsTab.tsx:92-105,135-193)_
- [x] Root cause (volume "nothing changes"): the volume chain is correct end-to-end — slider→`setVolume`→manager+disk (`load_notification_config`=0.17 confirmed persistence), Rust scales amplitude linearly (`notification_sound.rs:217`), Boss confirmed audible 1.0-vs-0.1 difference. The masking was the Test button: `NotificationManager.play` rate-limits to 1/500ms per sound, silently dropping rapid A/B clicks _(verified: notifications.ts:56-58 rate limit; Boss A/B ear test = clear difference)_
- [x] Fix (volume): `play(sound, {force})` bypasses enabled/per-sound/backoff/rate-limit; `testSound` uses `force:true` (removed the old enabled-toggle hack). Agent notifications stay rate-limited _(verified: vitest 75 pass incl. 4 new force scenarios + rewritten testSound contract)_
- [VISUAL] Live (needs rebuild): open Settings > Notifications → NO microphone prompt appears. Click "Choose output device…" → device dropdown loads (prompt may appear here, once). Section renders cleanly per STYLE_GUIDE.
- [x] Preview on Master Volume: `SettingSlider` gained optional `onCommit` (DOM `change`/release); Master Volume wires it to `testSound("info")` so releasing the slider plays a short preview at the committed volume _(verified: SettingFields.tsx onChange→onCommit; NotificationsTab.tsx:129; tsc + biome clean)_
- [HUMAN] Live (needs rebuild): set Master Volume low, click a Test button repeatedly while changing volume → each click plays at the current volume (no dropped/identical-sounding plays). Drag the Master Volume slider and release → hear a short "info" preview at the new level each time.

## Help menu links + What's New star CTA (2026-06-11)
- [x] Help menu (`menu.rs:223`) gained **Online Guide** + **Changelog** items; `menu-action` handler routes them via `handleOpenUrl` to `https://tuicommander.com/docs/` and the CHANGELOG on GitHub _(verified: tsc + cargo check clean; App.tsx:2289 cases, menu.rs items)_
- [x] `WhatsNewDialog` gained a discreet star CTA (inline star SVG per STYLE_GUIDE, links to repo) below "View full changelog" — one-shot since the dialog is already per-version _(verified: tsc clean)_
- [VISUAL] Live (needs rebuild): native Help menu shows Online Guide + Changelog → each opens the right URL in the browser. After an update, the What's New dialog shows the "Star us on GitHub" row with a star icon, rendering cleanly per STYLE_GUIDE.

## Fix: PluginPanel same-origin guard no longer loops on its own about:blank reset (2026-06-11)
- [x] Root cause: `guardSameOriginNav` reset an app-origin iframe to `about:blank`, which is itself same-origin → the guard re-fired on the reset's own load event → infinite log spam ("navigated to app origin — blocked" every 1-2s) _(verified: code inspection PluginPanel.tsx:213; observed live spam from md-21 panel)_
- [x] Fix: early-return when `contentDocument.location.href === "about:blank"` (our own reset target, not an escape) _(verified: tsc clean)_
- [HUMAN] Live (needs rebuild): open a plugin panel whose URL reaches the app origin → exactly ONE "blocked" log, no repeating spam; the panel resets to blank and stays quiet.

## OAuth upstream re-auth prompt on expired token (2026-06-09)
- [x] `classify_refresh_error` maps fatal refresh failures (`invalid_grant` / no refresh token / HTTP 400 / HTTP 401) to `UpstreamError::AuthFailed`; transient/5xx/network errors stay `Other` (retryable) _(verified: 2 unit tests in http_client.rs green; `refresh_token_if_needed` now routes through it)_
- [x] `initialize_entry_with_oauth` adds an `AuthFailed` + OAuth-config arm → deletes the dead keyring token + `mark_entry_needs_auth` (→ `NeedsAuth`, "Authorize") instead of red `Failed`/`CircuitOpen`; non-OAuth upstreams with a bad static token still go red (guard `matches! OAuth2`). Health-check skips `NeedsAuth` (registry.rs:1150) so no delete/init loop _(verified: code inspection; 146 mcp_proxy+mcp_oauth tests green)_
- [HUMAN] Live (needs rebuild): with an OAuth upstream (e.g. publishwith-ai) whose access AND refresh tokens are expired/revoked → on connect/refresh the upstream goes to **NeedsAuth (yellow + "Authorize")**, NOT silent red. Clicking Authorize opens the browser and re-authorizes to fresh tokens → green/ready. (Auto-test impossible: needs real keyring-seeded expired token; codebase marks keyring write tests `#[ignore]`.)

## tools/list upstream-readiness race (bridge serves before upstreams connect) (2026-06-09)
- [x] `await_initial_settle(timeout)` + `mark_initial_connect_complete()` on the registry: the first `tools/list` blocks (≤3s) until boot auto-connect registered every upstream AND none is still `Connecting`, then serves; a global latch makes later calls no-ops; timeout serves a partial list rather than hanging _(verified: 2 unit tests — fast-path + timeout-then-latch — green; 402 mcp_http+mcp_proxy tests green)_
- [x] Wired: `auto_connect_saved_upstreams` marks complete at both exits (incl. empty-config); `tools/list` handler awaits before `merged_tool_definitions` _(verified: code inspection mcp_transport.rs tools/list arm; mcp_upstream_config.rs)_
- [HUMAN] Live (needs rebuild): cold-start the app with quill enabled → quill's proxied tools (`quill__*`) are present in Claude Code's tool list WITHOUT a manual refresh/reconnect (root cause was CC fetching tools/list before async upstream init finished + ignoring tools/list_changed).

## Smart Prompts: inject Target (Compose vs Terminal) + idle gate fix (2026-06-10)
- [x] New per-prompt `injectTarget: "terminal" | "compose"` (defaults to "compose"); `canExecuteInject` only applies the idle gate when target=="terminal" (so a busy agent no longer disables compose-bound prompts like Review Issue) _(verified: useSmartPrompts.ts:148-156; 5 new vitest cases in useSmartPrompts.test.ts green — compose-not-gated, terminal-busy-blocked, terminal-idle-allowed, requiresIdle=false, no-active-terminal)_
- [x] `executeInject` routes by target on ALL platforms (removed the 2026-04-23 desktop-always-compose hack + isTauri gate): compose→`openComposeWithText` (fallback write-without-Enter when no compose panel), terminal→`sendCommand` (autoExecute) or write-without-Enter for review _(verified: useSmartPrompts.ts executeInject; tsc + biome clean)_
- [x] User's per-prompt target survives relaunch (added `injectTarget` to the built-in merge preserve list) + invalid values reset on hydrate _(verified: promptLibrary.ts merge + validation)_
- [x] PromptDrawer.doInject honors injectTarget for consistency _(verified: PromptDrawer.tsx doInject)_
- [VISUAL] Live (needs rebuild): expand an issue while the active terminal's agent is BUSY → the "Review Issue" button is ENABLED (not greyed) and clicking it fills the Compose box (does not send).
- [VISUAL] Live (needs rebuild): Settings → Smart Prompts → edit an inject prompt → a "Target" dropdown shows (Compose box / Terminal). Selecting "Terminal" reveals the "Auto-execute" checkbox; "Compose box" hides it.
- [HUMAN] Live (needs rebuild): set a prompt's Target=Terminal, make the agent busy → its SmartButtonStrip button is disabled ("Agent is busy"); when idle it sends straight to the agent (Auto-execute on = immediate Enter, off = text awaiting review).

## grok onboarding: storm fix + first-class agent + no-alt-screen wheel scroll (2026-06-11)
- [x] Notification storm: grok's decorative `◆` timeline bullets ("Thought for 1.3s", "user_prompt_submit") no longer fire confident Question events — cliclack `◆` match now requires the text to end with `?` (Goose prompts do, grok labels don't) _(verified: output_parser.rs:1131-1140; test_grok_decorative_bullet_not_a_question + 3 cliclack tests green)_
- [x] grok registered as first-class agent: agents.ts (AgentType/AGENTS/MCP_SUPPORT/AGENT_DISPLAY), useAgentDetection binary map, classify_agent (pty.rs), discover_grok_session/verify_grok_session (agent_session.rs, `~/.grok/sessions/<%-encoded-cwd>/<UUIDv7>/`) _(verified: tsc clean; 33 agent_session + 4 grok/cliclack rust tests green; grok_path_encode matches live on-disk layout)_
- [HUMAN] Live (needs rebuild): launch grok by typing `grok` in a shell tab (not via spawn) → TUIC detects agent_type=grok (icon/badge), and after app restart the session resumes via `grok --resume <id>` (discovery picks the newest UUIDv7 dir).
- [VISUAL] Live (needs rebuild): run `grok --no-alt-screen`, produce enough output for a scrollbar, then **two-finger trackpad / wheel scroll** → the scrollback scrolls (previously dead because grok enables mouse mode `?1000h/?1002h/?1003h/?1006h` without alt-screen, so the wheel was forwarded to grok). Confirm vim/lazygit (real alt-screen) wheel still reaches the app, and **Shift+wheel** scrolls the scrollback in any mode (never sent to the app).

## Search/UI consistency: unified SearchBar + scrollbar overview + file-browser tracking (2026-06-11)
- [x] Shared `SearchBar` gains optional VS Code-style expandable Replace (chevron) — only shown when the consumer passes `onReplace`/`onReplaceAll` (editor yes; terminal/diff no) _(verified: code inspection SearchBar.tsx; tsc clean)_
- [x] Match counter ("1 of N"/"No results") moved INSIDE the input (absolute, masking bg) instead of a fixed-width flex slot → no dead space / layout shift _(verified: SearchBar.module.css .inputWrap/.counter; tsc clean)_
- [x] Code editor uses the shared `SearchBar` overlay instead of CodeMirror's built-in panel (Cmd+F / Cmd+Alt+F / Esc); `EditorSearch` drives `@codemirror/search` _(verified: 6 vitest cases editorSearchEngine.test.ts; searchOverview 5; tsc clean)_
- [x] Editor scrollbar search ticks: full-width 12px marks (`--attention`) that cover the scrollbar; git-change overview hidden while searching (`.cm-searching`) → only orange marks, like the terminal _(verified: searchOverview.ts; tsc clean)_
- [x] Git-change overview ruler coalesces contiguous same-type runs → a whole-new file shows ONE tick, not a solid bar _(verified: 5 vitest cases gitGutterRuns.test.ts)_
- [x] Diff panel search ticks on the scrollbar via `DomSearchEngine.matchFractions` + shared `DomSearchOverview` _(verified: code inspection; tsc clean)_
- [x] Active search match no longer bright yellow: `.cm-selectionMatch` themed to a subtle accent tint _(verified: theme.ts; tsc clean)_
- [x] File browser highlights the file open in the active editor (accent bar + tint), flat + tree, worktree-aware _(verified: FileBrowserPanel/TreeNode entryActive; tsc clean)_
- [x] File browser reveals the active file: tree auto-expands ancestor dirs then scrolls the row into view _(verified: FileBrowserPanel.revealActiveFile + effect; tsc clean; full suite 4167 green)_
- [VISUAL] Live (needs rebuild): open Cmd+F in the code editor → compact SearchBar pill (counter inside input); typing shows orange full-width ticks covering the scrollbar and hides the green git ticks; closing search brings the git overview back. Replace row expands via the chevron.
- [VISUAL] Live (needs rebuild): open a brand-new (untracked) file → the git-change overview shows a SINGLE tick at the top, not a solid green bar.
- [VISUAL] Live (needs rebuild): search inside a diff tab → orange match ticks appear on the diff scrollbar and track scroll.
- [VISUAL] Live (needs rebuild): editor scrollbar visually matches the terminal's (14px track, rounded inset thumb).
- [VISUAL] Live (needs rebuild): open a file deep in a subtree → the file browser (tree view) auto-expands its parents and scrolls it into view, highlighted with the accent bar. Switching the active editor tab moves the highlight.
- [ ] DEFERRED (story 041-cd15): HTML Preview search → shared SearchBar (in-iframe search needs a postMessage bridge); shared sidebar-filter component for Error Log / Knowledge History / Branch Switcher etc. ("consistency of a different kind" for narrow sidebars).
- [VISUAL] Live (story 040-29e1): open a tracked file in the code editor → a dim italic annotation "Author · relative time · summary" appears at the end of the active line and follows the cursor (no flicker, no fetch per keystroke). Edit a line → it shows "You · Uncommitted changes". Toggle `settingsStore.setInlineBlameEnabled(false)` → annotation disappears. External (absolute-path) files show no annotation.

## Native Agent Hooks — adapter live verification (Phase 1)

- [HUMAN] Grok hooks: enable "Use native agent hooks for status" for Grok in Settings → Agents, launch a `grok` session, confirm the badge flips busy on prompt-submit / tool-use and idle on Stop (hooks at `~/.grok/hooks/tuic.json`). Verify a co-existing user hook file in `~/.grok/hooks/` is untouched, and disabling deletes only `tuic.json`. (Event names verified against `~/.grok/docs/user-guide/10-hooks.md`; the install/uninstall/file-isolation is covered by `agent_hook_grok` unit tests — this confirms the events actually fire live.)
- [HUMAN] Grok awaiting: Grok has no dedicated "awaiting approval" hook event, so approval prompts still rely on the OSC-0 title heuristic (not suppressed under instrumentation). Trigger a permission prompt in an instrumented Grok session and confirm the awaiting badge still appears.
- [HUMAN] Codex hooks: enable instrumentation for Codex in Settings → Agents. Confirm `~/.codex/config.toml` gains `[features] hooks = true` (preserving other config) and `~/.codex/hooks.json` gains our SessionStart/UserPromptSubmit→busy, Stop→idle hooks (preserving any user Codex hook). Launch `codex`, submit a prompt → badge busy; on turn end → idle. Disable → both the flag AND our hooks removed, user config/hooks intact. (Merge/flag/state covered by `agent_hook_codex` unit tests — this confirms live event firing + that Codex actually honors the flag.)
- [HUMAN] OpenCode plugin: with OpenCode installed, enable instrumentation for OpenCode in Settings → Agents. Confirm `~/.config/opencode/plugin/tuic.ts` is written (carrying the `/* tuic-managed */` marker) and that a pre-existing UNmarked user plugin at that path is never overwritten. **Verify the plugin API shape against the installed OpenCode version** — the generated callbacks (`tool.execute.before`/`after`→busy, `permission.asked`→awaiting, `event` `session.idle`→idle) and the `/dev/tty` write must match OpenCode's actual plugin contract; adjust `opencode_plugin_source()` if the API differs. Launch OpenCode → badge busy/idle/awaiting flips. Disable → the marked file is deleted (unmarked files left). (File-management — clobber-guard, delete-only-marked, marker/OSC/guard in source — is covered by `agent_hook_opencode` unit tests; the runtime plugin contract is the open item.)

## Terminal scroll: duplication / disappearing / zoom-black (2026-06-18)

- [VISUAL] Live (needs rebuild): zoom a terminal (Cmd +/-) while an idle agent's static output fills the screen → it must repaint immediately, NOT go black until you scroll. (`resize_pty` now flushes a full frame after resizing the grid; backend covered by serialize tests.)
- [VISUAL] Live (needs rebuild): scroll a terminal up a little so it rests at a fractional (sub-line) scroll position, then resize the pane (close the file browser / MD sidebar, Cmd+E). The terminal MUST repaint — it must NOT go black until you click. Root cause: `scheduleRepaint()` bails while `scrollPosF != null` (fractional rest), and `remeasure` never reset it, so after the resize blanked the canvas every incoming frame refilled rowMap but never painted (stuck black; a click healed it only because mousedown exits the fractional rest). `remeasure` now calls `resetSmoothScroll()` first. This is the FRONTEND half of the resize-black bug — the `resize_pty` flush above was necessary but not sufficient.
- [VISUAL] Live (needs rebuild): after a smooth-scroll gesture comes to rest, there must be NO ghost/duplicate line peeking below the viewport (below the prompt/status). (`clearOverscan()` now wipes the overscan canvas on every return to rest.)
- [VISUAL] Live (needs rebuild): in a session that has streamed well over 10 000 lines (GRID_SCROLLBACK cap), scroll up through old output → no line is duplicated or shows the wrong content as the scrollback cap evicts. (Row cache now keyed by the eviction-stable absolute index `historyBase + grid-relative`; invariant covered by `styled_abs_is_eviction_stable_and_never_aliases`.)
