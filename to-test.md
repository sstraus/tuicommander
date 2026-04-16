# To Test

Features to test when TUICommander is more usable.

## AI Chat (Level 1)
- [ ] Settings > AI Chat tab: provider dropdown shows Ollama/Anthropic/OpenAI/OpenRouter/Custom
- [ ] Ollama selected + running: green dot, model list populated from /api/tags
- [ ] Ollama selected + not running: red dot with "Not detected" message
- [ ] API key field: masked, saved to keyring, "Key saved" indicator after save
- [ ] Test Connection button: success/error result inline
- [ ] Context lines slider: 50-500, persists across restart
- [ ] Temperature slider: 0.0-1.0, persists across restart
- [ ] Cmd+Alt+A toggles AI Chat panel open/closed
- [ ] Status bar: chat bubble icon toggles panel, highlighted when active
- [ ] Panel: terminal dropdown lists all open terminals, switching attaches
- [ ] Panel: pin button prevents auto-attach on terminal focus change
- [ ] Panel: send message with Cmd+Enter, Shift+Enter for newline
- [ ] Panel: streaming response shown as raw text, markdown rendered on completion
- [ ] Panel: code blocks have Copy and Run buttons after stream ends
- [ ] Panel: Run button sends code to attached terminal via sendCommand
- [ ] Panel: Stop button visible during streaming, cancels generation
- [ ] Panel: Clear conversation button resets all messages
- [ ] Panel: empty state "Ask me about your terminal output" when no messages
- [ ] Panel: error banner with Retry button on provider failure
- [ ] Panel: opening AI Chat closes other exclusive panels (markdown, git, file browser)
- [ ] Right-click terminal selection > "Explain with AI": opens panel, sends selection
- [ ] Right-click terminal (no selection) > "Explain with AI": sends last 50 buffer lines
- [ ] Right-click terminal > "Fix this error": sends error analysis prompt
- [ ] Selection >2000 chars truncated with "[... truncated]" marker

## AI Agent — Level 2 Loop (1299/1300/1301/1302)
- [ ] Start button in AI Chat header sends goal → agent banner appears with "running" + iter counter
- [ ] Tool-call cards render in order for each `ai_terminal_*` the agent emits (read_screen, send_input, wait_for, get_state)
- [ ] Pause button freezes iteration; resume continues from next tool call
- [ ] Cancel button clears banner and stops future iterations
- [ ] Destructive command (rm -rf, git reset --hard, DROP TABLE) triggers approval card; reject skips, approve executes
- [ ] Agent error (provider failure) surfaces in chat with Retry
- [ ] Rejoining session after reload: agent state recovered from store; tool-call history preserved (schema v2)

## AI Agent — External MCP Tools (1303)
- [ ] Remote MCP client (Claude Code / Cursor) lists six `ai_terminal_*` tools via `tools/list`
- [ ] `ai_terminal_read_screen` returns redacted screen text; respects `lines` cap
- [ ] `ai_terminal_send_input` on an idle session prompts user confirm dialog; rejects while internal agent loop is active on that session
- [ ] `ai_terminal_send_key` honours named keys (enter, tab, ctrl+c, escape, up/down) with same confirmation semantics
- [ ] `ai_terminal_wait_for` returns on regex match, timeout_ms, or stability window
- [ ] `ai_terminal_get_state` reflects current shell_state/cwd/terminal_mode/agent_type
- [ ] `ai_terminal_get_context` returns compact ~500-char summary aligned with SessionKnowledge.build_context_summary

## AI Agent — Filesystem Tools (1325-1331)
- [ ] `ai_terminal_read_file` returns line-numbered content; respects offset/limit; rejects binary and >10MB; secrets redacted
- [ ] `ai_terminal_write_file` creates a new file; overwrites existing; confirm dialog appears for MCP callers
- [ ] `ai_terminal_write_file` to `.env` or `Cargo.toml` triggers "sensitive path" rejection
- [ ] `ai_terminal_edit_file` replaces unique occurrence; rejects non-unique without replace_all; confirm dialog for MCP
- [ ] `ai_terminal_list_files` matches glob patterns (e.g. `**/*.rs`); reports dir vs file type; max 500
- [ ] `ai_terminal_search_files` finds regex matches with context; respects .gitignore; max 50 matches
- [ ] `ai_terminal_run_command` captures stdout/stderr; sanitized env (only PATH/HOME/TERM/LANG); safety blocks sudo; confirm dialog for MCP
- [ ] `ai_terminal_run_command` with 500ms timeout kills the process cleanly
- [ ] Filesystem tools only work within the session's sandbox root — `../` traversal rejected
- [ ] Agent system prompt now documents all 12 tools with when-to-use guidance

## AI Agent — Session Knowledge (1305/1306/1307/1309)
- [ ] OSC 133 shell (with `shell-integration.sh` sourced): running a command populates SessionKnowledgeBar with a Success/Error row and exit code
- [ ] Shell without OSC 133: busy→idle transition populates an `inferred` outcome row (no exit code, empty command text)
- [ ] Error classification tags match expected `error_type` for rust_compilation, npm_error, python_error, missing_tool, missing_file, permission, network
- [ ] Error→fix correlation: failing command followed within 3 commands by a success populates "Known Fixes" in the context summary
- [ ] SessionKnowledgeBar collapsed row shows commands count; "recent err" pill appears when errors exist; "tui:" pill appears when in fullscreen TUI
- [ ] SessionKnowledgeBar auto-refreshes ~2s after new pty-parsed events (debounced)
- [ ] Relaunch app: `{config_dir}/ai-sessions/{session_id}.json` files exist for recent sessions; bar reloads with history intact
- [ ] Agent system prompt now includes "## Session Knowledge" block (verify via debug logs)

## MCP Session Tombstone
- [x] `agent spawn` → `session output` after 1.8s → returns live buffer with `exited:false` (9b886c20 E2E validated 2026-04-10)
- [x] `session close` → `session output` → returns final buffer with `exited:true`, buffer preserved (9b886c20 E2E validated 2026-04-10)
- [x] `session kill` → `session output` → returns final buffer with `exited:true`, `exit_code:1` (9b886c20 + MCP E2E 2026-04-10; NOTE: actual exit_code is 1, not 129/SIGHUP as originally expected)
- [x] Unknown session id (never existed) → returns `{"error":"Session not found","reason":"session_not_found_or_reaped"}` (MCP E2E validated 2026-04-10)
- [ ] Close → wait >5 min (TOMBSTONE_TTL_MS) → output returns the same reaped error
- [ ] close_pty Tauri command (GUI "close terminal") still works and preserves post-mortem reads for subsequent MCP calls

## Global Workspace
- [ ] Open Activity Dashboard → globe icon on each terminal row → click toggles promoted
- [ ] Promote 2+ terminals → sidebar shows "Global" entry with badge count
- [ ] Click sidebar "Global" → switches to global workspace with promoted terminals in split view
- [ ] Each pane tab shows repo name + colored dot in global workspace
- [ ] Click sidebar "Global" again → switches back to repo view, both layouts preserved
- [ ] Cmd+Shift+X → toggles global workspace
- [ ] Close promoted terminal → auto-unpromoted, removed from global layout
- [ ] Close last promoted terminal while in global workspace → auto-deactivates
- [ ] Branch switch while in global workspace → auto-deactivates first
- [ ] File browser and git panel hidden while in global workspace
- [ ] Pane tab bar: globe icon on hover, filled when promoted, click toggles
- [ ] Globe icon hidden on tabs when in global workspace (redundant)
- [ ] Hover tab in global workspace → repo name overlay badge appears (inline, no layout shift)
- [ ] Overlay shows correct repo displayName per terminal
- [ ] Overlay NOT shown when hovering tabs in per-repo view

## Cross-Terminal Search (Command Palette)
- [ ] Open palette, type `~error` → shows matches from terminal buffers with terminal name + line
- [ ] Select a result → switches to the correct terminal tab and scrolls to the matched line (centered)
- [ ] Type `~` with < 3 chars → shows "Type at least 3 characters after ~"
- [x] Type `~nonexistent` → shows "No results" (MCP maccontrol verified 2026-04-10)
- [ ] Close all terminals, type `~test` → shows "No terminals open"
- [ ] Type "Search Terminals" in palette → command appears; selecting it pre-fills `~ `
- [ ] "Search Files" command pre-fills `! `, "Search in File Contents" pre-fills `? `
- [x] Footer shows `~ terminals` hint alongside `! files` and `? content` (MCP maccontrol verified 2026-04-10)
- [ ] Split pane: search result in non-active pane → activates the correct pane

## Unified Repo Watcher
- [ ] Edit a file from external terminal → ChangesTab updates within ~2s
- [ ] `git add` from terminal → ChangesTab updates within ~1s
- [ ] `git commit` from terminal → HistoryTab updates within ~1s
- [ ] `git checkout other-branch` → branch switches within ~0.5s
- [ ] Edit a gitignored file (e.g. in node_modules/) → no refresh triggered
- [ ] Modify `.gitignore` → new rules take effect without restart

## Global Hotkey Toggle
- [ ] Settings > Keyboard Shortcuts > Global Hotkey section visible (desktop only)
- [ ] Click "Click to set hotkey" → capture mode activates
- [ ] Press a key combo → registers and shows in the field
- [ ] Switch to another app → press hotkey → TUICommander appears focused
- [ ] Press hotkey again while focused → TUICommander minimizes
- [ ] Press hotkey while visible but unfocused → TUICommander gains focus
- [ ] Clear button removes the hotkey
- [ ] Hotkey persists across app restart
- [ ] Browser mode: Global Hotkey section is hidden

## File Browser Tree View
- [ ] Toggle flat/tree with toolbar buttons — buttons render on same row as filter
- [ ] Tree: click folder chevron → expands on first click, loads children lazily
- [ ] Tree: expand nested folders → correct indentation, file sizes shown
- [ ] Tree: switch to tree while in a subfolder → tree starts from repo root
- [ ] Tree: search query active → falls back to flat results
- [ ] Flat: breadcrumb navigation still works, ".." entry appears in subdirs

## Diff Scroll View
- [ ] Open diff tab → toolbar shows split/unified/scroll buttons
- [ ] Click scroll mode → all changed files shown in continuous scroll
- [ ] Each file section: collapsible via chevron, shows +/- stats
- [ ] Click filename in scroll view → opens file in editor tab
- [ ] Sticky header shows total files/additions/deletions
- [ ] Stage/unstage a file → scroll view updates reactively

## Command Palette File Search
- [x] Cmd+P → palette opens, footer shows ! files and ? content hints (MCP maccontrol verified 2026-04-10)
- [ ] Type `!readme` → filename results appear, Enter opens in editor
- [ ] Type `?import` → content results with highlighted matches and line numbers
- [ ] Enter on content result → opens file at matched line
- [ ] Delete prefix → returns to command mode
- [x] Footer hints visible in all modes (command, filename, content) (MCP maccontrol verified 2026-04-10)

## Agent Detection Fix
- [ ] Launch Claude Code in a TUICommander terminal → agent detected within 3 seconds (status bar shows agent badge)
- [ ] Smart Commit and other inject-mode prompts become enabled when agent is detected
- [ ] After HMR reload (save a .tsx file), terminal session survives and agent is still detected

## Smart Prompts Drawer (Cmd+Shift+K)
- [ ] Open Cmd+Shift+K → drawer shows compact prompt list with badges (inject/headless, built-in, placement)
- [ ] Click Edit on a prompt → modal shows variable dropdown under Content textarea
- [ ] Click a variable in dropdown → inserts `{variable}` at cursor in textarea
- [ ] Execution Mode and Auto-execute appear side by side
- [ ] Auto-execute ON → prompt sends Enter automatically after injection
- [ ] Auto-execute OFF → prompt text pasted without Enter, user can edit before sending

## Git Panel
- [ ] Unstaged section shows "Changes (unstaged)" label

## Notification Bell Enhancements
- [x] Click bell with no notifications → shows "No notifications" (not empty 1px dropdown)
- [x] Run `git push` via toolbar → git result appears in bell dropdown under "GIT" section
- [ ] Failed `git push` → shows error item with red icon
- [ ] PR CI transitions from failed to all-passing → "CI Passed" notification appears
- [ ] CI recovery when PR is also "ready" → only "Ready" shows (no duplicate ci_recovered)
- [x] Create worktree via MCP → "Worktree: branch-name" item appears under "WORKTREES"
- [x] Dismiss individual items and "Dismiss All" work for each section

## TCP Port Retry
- [x] Start two instances of TUIC → second instance binds to port+1 (check logs)
- [ ] Start three instances → third binds to port+2
- [ ] Start four → third fails with clear error message showing port range attempted

## Stale Suggestions Fix
- [ ] Agent emits suggestions → chips appear at bottom
- [ ] Dismiss suggestions → resize terminal → suggestions do NOT reappear
- [ ] Tab switch away and back → old suggestions do NOT reappear
- [ ] New agent cycle (user input) → new suggestions appear correctly

## Browser Mode Parsed Events
- [ ] Open TUI in browser (http://localhost:9876) → agent status-line shows task name
- [ ] Rate-limit events trigger warning notification in browser mode
- [ ] Suggest chips appear in browser mode when agent finishes
- [ ] Question detection works in browser mode (awaiting input indicator)

## MCP Ctrl+C Forwarding (904-5deb — verified non-bug)
- [x] Create PTY via MCP `session create`, run `sleep 1000`, send `session input special_key=ctrl+c` → sleep exits
- [x] Same with nested shell: `sh -c 'sleep 1000'` → ctrl+c kills inner sleep
- [x] Verify via `session output` that process exited (not hanging)

## Multi-instance Socket Coexistence (907-e4e9)
- [x] Start TUIC-preview.app, then `tauri dev` → both instances run, each with its own socket
- [ ] tuic-bridge connects to the correct instance (check `TUIC_SOCKET` override)
- [ ] Kill one instance → other still works, bridge reconnects if needed
- [ ] Stale `mcp-*.sock` files cleaned on startup

## Shell State Rust Derivation (741-3faf)
- [ ] Agent runs → tab shows blue busy indicator
- [ ] Agent stops → tab transitions to green idle (no mode-line flicker)
- [ ] Agent asks "Procedo?" → question notification fires (no false completion)
- [ ] Resize during idle → no brief blue flash on tab
- [ ] pendingInitCommand (worktree run script) executes on first idle
- [ ] Sub-agents running → terminal stays busy until they finish
- [ ] Terminal remount (tab switch) correctly syncs shell state from Rust
- [ ] Completion notification fires after agent works ≥5s then goes idle (background tab)
- [ ] No completion notification when terminal is awaiting input (question/error)
- [ ] No purple unseen dot while agent is actively working (status line timer ticking)

## Plan Panel (515-660c / 516-41a5 / 517-74c2)
- [x] `Cmd+Shift+P` opens plan panel on right side (MCP maccontrol verified 2026-04-10)
- [ ] Plan panel shows plans only for the active repository
- [ ] Click plan item opens it as markdown tab (frontmatter stripped)
- [ ] Switching repos changes visible plans in the panel
- [ ] New plan detected by agent auto-opens as background tab (no focus steal)
- [ ] Repeated detection of same plan does not open duplicate tabs
- [ ] Panel visibility persists across app restart
- [x] Plan panel is mutually exclusive with Diff/Markdown/FileBrowser panels
- [x] Plan count badge shows correct number

## PR Detection (071-cc1f)
- [ ] Run `gh pr view` in terminal - verify PR badge appears in sidebar
- [ ] Run `gh pr create` - verify PR URL is captured
- [ ] Verify PR badge shows in StatusBar
- [ ] Click PR in StatusBar - should open browser
- [ ] Switch branches - PR info should persist per branch
- [ ] GitLab MR URLs detection

## PR Merge & Cleanup Dialog
- [ ] Merge a ready PR → cleanup dialog appears with checkboxes
- [ ] Cleanup dialog stays visible for ≥5 seconds (poll doesn't kill it)
- [ ] Click "Execute" → all checked steps run (switch, pull, delete-local, delete-remote)
- [ ] After execution, popover auto-closes after ~600ms
- [ ] Click "Skip" → popover closes, no cleanup actions run
- [ ] Switching branches while cleanup dialog is open → popover closes (auto-close works)

## Rename Branch (072-d7d6)
- [ ] Double-click branch name - dialog should open
- [ ] Input pre-filled with current name
- [ ] Validate invalid names (spaces, special chars)
- [ ] Rename succeeds - branch updates in sidebar
- [ ] Terminal associations preserved after rename
- [ ] ESC closes dialog, Enter confirms

## Repository Context Menu (073-50dd)
- [x] Click ⋯ on repo header - menu appears
- [x] "Repo Settings" opens settings
- [ ] "Remove Repository" shows confirmation
- [ ] Confirm removal - all terminals close, repo removed
- [x] Click outside menu - menu closes

## Repository State Persistence
- [ ] Repos start expanded by default (not collapsed)
- [ ] Expanded/collapsed state persists across restarts

## macOS Option Key (056-cee9)
- [ ] Option+key combinations work in terminal
- [ ] Special characters via Option (@ # etc) work

## Rust Backend (batch 047-070)

### Git Branches Command (052)
- [ ] get_git_branches returns all local branches
- [ ] Works on repos with many branches
- [ ] Error handling for non-git directories

### CI Checks Command (060)
- [ ] get_ci_checks returns check details via gh run list
- [ ] Works when gh CLI is authenticated
- [ ] Graceful error when gh CLI not installed or not authenticated

### Adjective-Animal Worktree Names (063)
- [x] New worktrees get adjective-animal names
- [ ] Names are unique across worktrees
- [x] Name format is consistent (adjective-animal)

### Single Window Enforcement (065)
- [ ] Second app instance focuses existing window instead of opening new one
- [ ] Works across multiple desktops/spaces

### PTY Spawn Retry (059)
- [ ] Terminal creation succeeds on first attempt normally
- [ ] Retries up to 3 times on spawn failure
- [ ] Increasing delay between retries (100ms/200ms/300ms)
- [ ] Error message shown after all retries exhausted

## Frontend (batch 047-070)

### Git Quick Actions in Sidebar (050)
- [x] Pull/Push/Fetch/Stash buttons visible in sidebar
- [x] Each button sends correct git command to active terminal
- [ ] Commands execute in shell (not just displayed)
- [ ] Buttons disabled when no active terminal

### GitOperationsPanel Live Branches (052-frontend)
- [ ] Branch list populated from get_git_branches (not hardcoded)
- [ ] Branch list updates when branches change
- [ ] Current branch highlighted

### Help Panel (053)
- [x] Cmd+? opens help panel
- [x] All shortcuts listed and searchable
- [x] Search filters shortcuts in real-time
- [x] ESC or Cmd+? closes help panel

### Tab Drag & Drop (054)
- [ ] Drag tab to reorder within tab bar
- [ ] Visual indicator shows drop position (left/right)
- [ ] Tab order persists after reorder
- [ ] Dragging tab shows dragging state visual

### Quit Confirmation Dialog (057)
- [ ] Quit shows confirmation when active terminal sessions exist
- [ ] Dialog shows session count
- [ ] Cancel returns to app
- [ ] Force quit closes app

### Terminal Progress Bar - OSC 9;4 (058)
- [ ] Progress indicator appears in tab when OSC 9;4 sequence received
- [ ] Progress updates in real-time (0-100%)
- [ ] Progress clears when operation completes (state=0)
- [ ] Works with tools that emit OSC 9;4 (e.g. wget)

### CI Checks Popover (060-frontend)
- [ ] Click CI badge in status bar opens popover
- [ ] Popover shows individual check names and statuses
- [ ] Success/failure/pending icons correct
- [ ] Click check item opens URL in browser
- [ ] Loading state shown while fetching

### Optimized GitHub Polling (062)
- [ ] Polling interval increases on consecutive errors (exponential backoff)
- [ ] Polling slows when browser tab/window is hidden
- [ ] Polling resumes immediately when window becomes visible
- [ ] No excessive API calls visible in network

### Hotkey Hints (064)
- [x] Sidebar toggle shows ⌘[ hint
- [x] MD/Diff toggle buttons show ⌘M/⌘D hints
- [ ] New tab button shows hint
- [ ] Hints visible but not intrusive

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
- [ ] Download Model button works (downloads ~1.5GB large-v3-turbo)
- [ ] Download progress bar updates in real-time
- [ ] Model status shows "Ready" after download completes
- [ ] Attempting dictation without model opens Settings panel

### Push-to-Talk (Mic Button)
- [ ] Mic button visible in StatusBar when dictation enabled
- [ ] Hold mic button starts recording (button turns blue, pulses)
- [ ] Release mic button stops recording and transcribes
- [ ] Transcribed text injected into active terminal
- [ ] Mouse leave while recording stops recording
- [ ] Blue pulsing animation respects prefers-reduced-motion

### Push-to-Talk (Hotkey)
- [ ] Default hotkey F5 starts/stops recording
- [ ] Key held = recording, key released = transcribe + inject
- [ ] Key repeat events ignored (no double-start)
- [ ] Custom hotkey configurable in Settings > Dictation

### Text Corrections
- [ ] Default corrections loaded (Cloud Code → Claude Code)
- [ ] Add/remove corrections in Settings > Dictation
- [ ] Corrections applied to transcribed text before injection
- [ ] Import/export corrections as JSON

### Settings Tab
- [ ] Enable/disable toggle persists across restarts
- [ ] Language selector works (auto-detect, English, etc.)
- [ ] Hotkey recorder captures key combinations
- [ ] Audio devices listed (shows system default)

## Native System Menu Bar (Stories 192 + 193)
- [x] Menu bar visible on macOS (top of screen), Windows/Linux (under title bar)
- [x] macOS: App menu has About, Services, Hide, Hide Others, Show All, Quit (+ Check for Updates)
- [x] File menu: New Tab, Close Tab, Reopen Closed Tab, Settings, (Quit on non-macOS)
- [x] Edit menu: Undo, Redo, Cut, Copy, Paste, Select All, Clear Terminal
- [x] View menu: Toggle Sidebar, Split Right/Down, Zoom In/Out/Reset, Diff/Markdown/Notes panels
- [x] Go menu: Next/Previous Tab, Switch to Tab 1-9
- [x] Tools menu: Prompt Library, Run/Edit & Run Command, Git Panel, Branches, Diff Scroll, Task Queue
- [x] Help menu: Help Panel, About TUICommander (+ Check for Updates on non-macOS)
- [ ] Clicking menu items triggers correct action (same as keyboard shortcut)
- [x] Accelerator labels show correct modifier key per platform (CmdOrCtrl in code → Tauri resolves)
- [x] No double-firing: pressing Cmd+T creates one tab, not two
- [ ] Predefined Edit items (Copy/Paste/Undo/Redo) work correctly with native focus
- [ ] HelpPanel shows note about system menu bar

## Terminal Session Persistence
- [ ] Open 2 repos, each with a branch and 2 terminals. Quit app, reopen → same terminals recreated
- [ ] Run `claude` in a terminal, quit app, reopen → terminal auto-sends `claude --continue`
- [ ] Run `gemini` in a terminal, quit app, reopen → terminal auto-sends `gemini --resume`
- [ ] Plain shell terminal → restored without any agent command
- [ ] HMR reload (Vite dev) → uses existing reconnect path, not the new restore
- [ ] Delete a repo folder externally, reopen → skips that repo gracefully
- [ ] `hadTerminals` logic still works (no auto-spawn after intentional close-all)

## Repository Groups (Accordion UI)
- [ ] Create group from Settings > Groups tab
- [ ] Rename group (double-click name in settings)
- [ ] Delete group — repos move to ungrouped
- [ ] Assign color preset to group (5 presets + clear)
- [ ] Group appears as accordion section in sidebar
- [x] Click group header toggles collapse/expand
- [x] Group color dot visible when color set
- [ ] Drag repo within same group reorders
- [ ] Drag repo onto group header assigns to group
- [ ] Drag repo from group to ungrouped area removes from group
- [ ] Drag repo between groups moves correctly
- [ ] Drag group header to reorder groups
- [x] Right-click group header shows Rename/Color/Delete
- [x] Right-click repo shows "Move to Group" submenu
- [ ] Quick switcher force-expands collapsed groups
- [ ] Existing repos auto-migrate (all start ungrouped)
- [ ] Color inheritance: repo color > group color > default
- [ ] Empty group shows "Drag repos here" hint
- [ ] Group name uniqueness enforced (case-insensitive)

## Tab Title Improvements
- [ ] Run AI agent → tab title updates with task name → process exits → title reverts to original
- [ ] Rename tab → run agent → exit → custom name persists (not overwritten)
- [ ] Launch `FOO=bar claude` → tab should show `claude`, not the env vars
- [ ] New session in same tab → OSC titles update immediately (no stale timestamp delay)

## Plugin System v2
- [x] Settings > Plugins tab shows installed plugins with built-in badge
- [ ] Toggle enable/disable on external plugin, restart, verify state persists
- [ ] Click "Logs" on a plugin, verify log viewer opens with entries
- [ ] Install a plugin from ZIP file via "Install from file..." button
- [ ] Uninstall an external plugin via Uninstall button (confirm dialog)
- [ ] Browse tab loads registry entries (when registry repo exists)
- [ ] Install plugin from Browse tab downloads and installs
- [ ] Deep link: `tuic://settings?tab=plugins` opens Settings to Plugins
- [ ] Deep link: `tuic://install-plugin?url=https://...` shows confirmation then installs
- [ ] Deep link: `tuic://open-repo?path=...` switches to repo (only if in sidebar)
- [ ] Hot-reload still works after enable/disable/uninstall operations
- [ ] Plugin errors show error badge and are visible in log viewer

## Worktree Overhaul
- [ ] Settings → General → Worktree Defaults: all 7 dropdowns/toggles render and save
- [ ] Settings → Repository → Worktree: per-repo overrides with "Use global default" option
- [ ] Storage strategy: test sibling (`__wt`), app dir, and inside-repo paths
- [ ] `+` button (prompt on): dialog opens with branch list, base ref dropdown, generate name button
- [ ] `+` button (prompt off): instant creation with auto-generated name
- [ ] Right-click branch → Create Worktree: creates `{branch}--{random}` clone worktree
- [ ] Right-click worktree branch → Merge & Archive: merges into main, archives/deletes based on setting
- [ ] Merge with conflicts: error message shown, merge aborted, worktree intact
- [ ] External worktree created via CLI: detected in sidebar after refresh
- [ ] After merge "archive" mode: directory moved to `__archived/`
- [ ] After merge "delete" mode: worktree and branch removed entirely
- [ ] CreateWorktreeDialog base ref dropdown: default branch first, all local branches listed

## OSC 7 Terminal CWD Tracking
- [ ] cd into a worktree directory — toolbar should switch to worktree branch
- [ ] cd back to main repo — toolbar should switch back to main
- [ ] Agent creates worktree and cd's into it — tab reassigns automatically
- [ ] Background terminal cd's into worktree — switching to it shows correct branch
- [ ] Rapid cd's (build script) — no UI flicker, final state correct
- [ ] App restart — terminal in worktree reconnects to correct branch
- [ ] Shell without OSC 7 (vanilla bash) — no regression, behaves as before
- [ ] Test with zsh (default macOS) — OSC 7 emitted by default
- [ ] Test with fish — OSC 7 emitted natively

## Remote-Only PR Badge (reported intermittent)
- [ ] Blue badge with PR count visible on repo header when remote-only PRs exist
- [ ] Badge appears after GitHub polling completes (may take a few seconds on startup)
- [ ] NOT a collapsed-repo issue (confirmed by reporter)
- [ ] Suspect: polling hasn't completed yet, or circuit breaker is open
- [ ] Suspect: race between `localBranchNames()` update and GitHub poll — if branch names briefly match, PR is excluded from remote-only filter
- [ ] To diagnose: check `githubStore.state.repos[path]` in console when badge is missing

## File Browser Content Search (807-e295)
- [ ] `Cmd+Shift+F` opens file browser panel with content search mode active — **BUG FOUND**: action `toggle-file-browser-content-search` is registered in `keybindingDefaults.ts:113` and `actionRegistry.ts:51` but has NO handler in `useKeyboardShortcuts.ts` `dispatchAction()` switch — shortcut does nothing
- [ ] `C` button in search bar toggles between filename search and content search
- [ ] Results stream in progressively, grouped by file with match count
- [ ] Each result row shows file path, line number, and highlighted match context
- [ ] Click a result opens the file in code editor at the matched line
- [ ] Case-sensitive toggle works (uppercase vs lowercase match)
- [ ] Regex toggle works (e.g. `foo.*bar`)
- [ ] Whole-word toggle works (e.g. `foo` does not match `foobar`)
- [ ] Binary files are silently skipped (no error, not shown in results)
- [ ] Files larger than 1 MB are silently skipped
- [ ] Starting a new search cancels any in-progress search
- [ ] Empty query shows no results (no crash)

## Branch Panel (855-e86b)
- [x] `Cmd+G` opens Git Panel on the Branches tab
- [x] Clicking the "GIT" vertical label in the sidebar opens on Branches tab
- [x] Branch list shows local and remote sections (collapsible)
- [x] Each branch row shows ahead/behind counts, relative date, merged badge
- [ ] Stale branches (>30 days) are visually dimmed
- [ ] Recent branches section is populated from reflog
- [x] Inline search/filter narrows the branch list in real time
- [x] Prefix folding groups branches by `/` prefix (feature/, bugfix/, etc.) (MCP maccontrol verified 2026-04-10: POC-00168/, POC-00170/, POC-00171/ groups visible)
- [ ] Prefix folding toggle in panel header enables/disables grouping
- [ ] Checkout via `Enter` or double-click switches branch
- [ ] Checkout with dirty worktree shows stash/force/cancel dialog
- [ ] `n` key opens inline create-branch form
- [ ] Create branch with "Checkout after create" creates and switches
- [ ] `d` key deletes branch with confirmation (safe delete refuses unmerged)
- [ ] Force delete option available in confirmation
- [ ] Deleting current branch or default branch is blocked
- [ ] `R` key opens inline rename form pre-filled with current name
- [ ] `M` key merges selected branch into current
- [ ] `r` key rebases current onto selected
- [ ] `P` key pushes branch; auto-sets upstream if missing
- [ ] `p` key pulls current branch
- [ ] `f` key fetches all remotes
- [x] Context menu (right-click) shows all branch actions
- [ ] "Compare" context menu action shows diff --name-status
- [x] `Ctrl/Cmd+4` switches to Branches tab from within Git Panel
- [x] `Ctrl/Cmd+1/2/3` switches back to Changes/Log/Stashes tabs

## PWA / Mobile Output View
- [ ] Normal text wraps on narrow screens (no horizontal scroll)
- [ ] Box-drawing table output preserves alignment (│ ┌ ─ etc.)
- [ ] Tree view output preserves alignment (├── └──)
- [ ] No page-level horizontal scroll when viewing plain text
- [ ] Long lines without box-drawing characters wrap correctly
- [ ] Unicode emoji renders as text glyphs (font-variant-emoji: text)

## Smart Prompts Library (949-253b)
- [x] Cmd+Shift+K opens Smart Prompts Library drawer with search, categories, keyboard nav (MCP maccontrol verified 2026-04-10)
- [ ] Arrow keys navigate, Enter executes, Ctrl+N new, Ctrl+E edit, Ctrl+F favorite
- [ ] New prompt editor has placement checkboxes, auto-execute, shortcut fields
- [ ] Built-in prompts: name disabled, "Reset to Default" button, "built-in" badge, no delete
- [ ] Enable/disable toggle (circle SVG icon) works per prompt
- [ ] Variable dialog shows {varName} + description for unresolved variables
- [ ] All 24 built-in prompts show descriptions in list
- [ ] Settings panel no longer has "Smart Prompts" tab
- [x] Cmd+Shift+K opens SmartPromptsDropdown with status banner when disabled (MCP maccontrol verified 2026-04-10)
- [ ] SmartButtonStrip in Changes tab always visible (grayed out without agent)
- [ ] All icons in drawer are SVG (no emoji)

## Tailscale HTTPS
- [ ] With Tailscale running + HTTPS enabled: app serves HTTPS on same port
- [ ] QR code shows https:// URL with Tailscale FQDN
- [ ] Without Tailscale: HTTP works as before (no TLS)
- [ ] Settings > Services shows Tailscale status section
- [ ] Cookie gets Secure flag when accessed over HTTPS

## Base Branch Tracking
- [ ] Create branch with base ref selector → base stored in git config
- [ ] Sidebar shows yellow ⇣N badge when branch is behind base
- [ ] Badge tooltip shows base branch name
- [ ] Right-click branch → "Update from base (rebase)" fetches and rebases
- [ ] Remote base ref auto-fetched before branch creation

## UI Lock — Thundering Herd Fix (b59c659b)
- [ ] Switch repo with 5+ existing terminals → no UI freeze (was 1-3s)
- [ ] Open new terminal on a different repo → instant, no jank
- [ ] Agent running + repo switch → no freeze
- [ ] `git commit` in terminal → ChangesTab/BranchesTab update within 1s (bumpRevision deferred)
- [ ] Switch to repo with open PR → popover appears without UI jank (deferred via queueMicrotask)
- [ ] Activity dashboard dots still appear for active terminals (lastDataAt non-reactive Map)

## PTY Input Border Filter (f54ad157)
- [ ] Agent shows quota/budget line below input → silence timer NOT reset by it
- [ ] Question detection unaffected by status bar content below input border
- [ ] Completion notification not falsely triggered by post-input status updates

## Terminal Spawn Speed (696082ac)
- [ ] New terminal appears instantly when container has dimensions (check `spawnDelay` in logs — should be <50ms)
- [ ] Split-pane scenario where flex layout settles late → still works (falls back to ResizeObserver)

## PR Popover Load (36a1ba00)
- [ ] PR popover opens instantly with cached data, CI checks load after first paint
- [ ] Large PR (100+ commits) → popover doesn't freeze UI

## Smart Prompts Shell Script Mode (f60642c5)
- [ ] Create prompt with "Shell script" mode → runs `sh -c` with content directly
- [ ] Shell script with `{branch}` variable → resolves correctly
- [ ] Script timeout (>60s) → shows timeout error
- [ ] Script with non-zero exit → shows stderr in error

## Run Config Name Validation (9e02fbb4)
- [ ] Settings → Agents → Add Config → type existing name → red border + "already exists" error
- [ ] Save button disabled while name is duplicate
- [ ] Case-insensitive: "Claude" matches existing "claude"
- [ ] Cross-agent: name from claude configs rejected when adding to gemini

## Env Vars Editing per Run Config (e917dfdc)
- [ ] Settings → Agents → Add Config → "Environment Variables" section with + Add button
- [ ] Add KEY=value row, save config → env persists on reload
- [ ] Run config row shows "N env" badge when env vars are set
- [ ] Click "Env" button on saved config → inline edit panel opens
- [ ] Edit/remove env vars in saved config → changes persist

## Headless Agent Grouped Dropdown (e917dfdc)
- [ ] Settings → Agents → Headless Agent dropdown: agents with run configs show optgroup
- [ ] Agents without run configs show as single option
- [ ] Selecting a run config stores "type:name" in headless_agent field
- [ ] Same grouped dropdown in Smart Prompts tab

## Settings Nav Scroll (e27fae6c)
- [ ] Settings panel with 10+ repos → nav sidebar scrolls instead of compressing items

## Performance
- [ ] High-throughput output (e.g. `find /`) → terminal stays responsive (rAF coalescing)
- [ ] Edit a file in repo → git panel updates immediately (watcher-driven cache, not 5s delay)
- [ ] 5+ terminals open → no visible lag from process name polling (syscall, not ps fork)
- [ ] Multiple concurrent MCP tool calls → no serialization bottleneck (RwLock)

## Diff Tab Toolbar
- [ ] "Edit file" button opens file in default editor

## OSC 8 File Links
- [ ] Terminal file:// URIs from hyperlinks open in system file opener

## Smart Prompts API Mode
- [ ] Settings > Agents: LLM API section visible when API-mode prompt exists
- [ ] Select provider (OpenAI/Anthropic/etc.) → model placeholder updates
- [ ] Enter API key → shows "Stored" indicator after save
- [ ] OpenRouter/Ollama/Custom → Base URL field appears with default
- [ ] Test Connection → returns model response or error message
- [ ] Create prompt with "API (LLM direct)" mode → system prompt textarea appears
- [ ] Execute API-mode prompt → LLM responds, output routed to target (clipboard/commit-msg/toast)
- [ ] No API key configured → canExecute returns error with Settings link
- [ ] PWA/browser → API mode shows "requires desktop app" message
- [ ] Wrong API key → toast shows "Authentication failed" with Settings hint

## PWA Push Notifications
- [ ] Mobile Settings shows "Push notifications" toggle
- [ ] Enable push → browser prompts for permission → subscription stored
- [ ] Agent question → phone receives push notification
- [ ] Tap notification → PWA opens/focuses
- [ ] Disable push → unsubscribes and removes server-side subscription
- [ ] On HTTP (no HTTPS): shows "Push requires HTTPS" message
- [ ] On iOS in browser (not home screen): shows "Add to Home Screen" message

## Keepalive + Agent Detection Fix
- [x] Launch Claude Code, wait at prompt >5min idle → keepalive fires (check Activity Center stats)
- [ ] After 3 keepalives with no real user input → keepalives STOP (counter stays at 3/3)
- [ ] Phantom busy→idle after 3/3 → logs show "No real user message in JSONL → counter stays at 3/3"
- [ ] Real user sends message → logs show "Real user message in JSONL → counter reset" → next idle stretch gets fresh keepalives
- [ ] Overnight idle → max 3 pings total, then permanent stop (no infinite loop)
- [ ] Trigger "out of extra usage" → plugin shows "Rate limited" ticker, keepalives stop
- [ ] Resume manually (type in terminal) → rate limit cleared, keepalives resume
- [ ] Agent detection responds within ~1s of launch (not 3s)
- [ ] Run git/npm inside Claude Code → no agent type flicker in tab bar
- [ ] Status line ticking at idle prompt → shell-state transitions to idle within 3-4s
- [x] Reverse-map sync (commit 26688881): launch claude in a fresh terminal → `window.__TUIC__.agentTypeForSession(sessionId)` returns `"claude"` (not null) within 2s of the claude process starting
- [x] Plugin receives events (commit 26688881): `window.__TUIC__.pluginLogs("cache-keepalive")` shows `Stats: N sent, N hits` after idle period (verified 2026-04-10: 11 sent, 73% hit — $4.12 saved)
- [ ] Restore pigro preserves agent identity: close app with claude running → reopen → select branch → before the polling detector runs, verify terminal store has `agentType: "claude"` from savedTerminals

## Interactive GFM Checkboxes
- [ ] Open a `.md` file with `- [ ] task` items → checkboxes render as clickable inputs (not disabled)
- [ ] Click unchecked `[ ]` → toggles to `[x]`, file on disk updated
- [ ] Click checked `[x]` → toggles to `[~]` (indeterminate/in-progress)
- [ ] Click in-progress `[~]` → toggles to `[ ]` (unchecked)
- [ ] Nested checkboxes (`  - [ ]`) toggle the correct line
- [ ] Checkbox inside fenced code block is NOT rendered as interactive checkbox
- [ ] File with mixed content (headings, code blocks, checkboxes) → correct checkbox-to-line mapping
- [ ] Multiple rapid clicks → no race condition, each click writes correct state
- [ ] Tweak comments + checkboxes in same file → both features work independently

## GitHub Issues Panel
- [ ] Badge in repo header shows GitHub icon + combined count (PRs + issues)
- [ ] Click badge → unified panel opens with two collapsible sections
- [ ] PR section: all existing actions work (checkout, worktree, approve, merge, diff, post-merge cleanup)
- [ ] Issues section: shows issues filtered by assignee (default)
- [ ] Issue accordion: labels, assignees, milestone, timestamps, comment count
- [ ] Issue actions: Open in GitHub, Close/Reopen, Copy #number
- [ ] Filter dropdown (Assigned/Created/Mentioned/All) changes issue list
- [ ] Section collapse state persists in localStorage
- [ ] Escape key: closes expanded item first, then panel
- [ ] Arrow keys navigate items, Enter expands/collapses
- [ ] Rate limit: banner appears when circuit breaker trips, Retry button works
- [ ] Loading: skeleton rows shown during first issues fetch
- [ ] Empty state: "No remote-only PRs" / "No issues found" messages
- [ ] MCP: `curl localhost:PORT/repo/issues?path=...` returns issues JSON
- [ ] MCP: `curl -X POST localhost:PORT/repo/issues/close` with JSON body closes issue
- [ ] MCP tool: `github` action `issues` returns issues for repo
- [ ] Compact mode (`[data-compact]`): issue items render with reduced padding
- [ ] SmartButtonStrip: margin-left removal doesn't misalign across different placements (changes-tab, sidebar, prompt-drawer)

## Focus Mode (Cmd+Alt+Enter)
- [ ] Cmd+Alt+Enter hides sidebar, tab bar, and any open side panel (AI chat, git, markdown, notes, file browser)
- [ ] Toolbar (title bar) and StatusBar remain visible and functional
- [ ] Cmd+Alt+Enter again restores the previous layout (panel state preserved — the same panel that was open reappears)
- [ ] Setting `toggle-focus-mode` combo via KeyboardShortcuts tab changes the active hotkey
- [ ] Focus mode does NOT persist across restart (session-only)
- [ ] Does not collide with Cmd+Shift+Enter (zoom-pane) — both work independently

## Mobile iPad Fixes
- [ ] iPad: OutputView scrolls with touch drag (finger swipe up/down)
- [ ] iPad: Sidebar repo/branch selection works on first tap (no double-tap needed)
- [ ] iPad: Hover-revealed action buttons (⋯, +) not visible on touch devices

## ChoicePrompt (story 1296-ce3e)
- [ ] Claude Code edit-confirm dialog → PWA ChoicePromptOverlay appears with title + tappable buttons (1/2/3)
- [ ] Tap option key "1" → PTY receives single digit, Claude Code accepts and proceeds (no extra Enter, no Ctrl-U prefix)
- [ ] Repaint while dialog is open → overlay does not flicker/duplicate (dedup via `last_choice_prompt_sig`)
- [ ] Dialog dismissed by typing in terminal → `user-input` event clears `choice_prompt` and overlay disappears
- [ ] Agent resumes work (status-line emits) → `choice_prompt` cleared, overlay disappears
- [ ] Slash menu suppressed while ChoicePromptOverlay is visible (only one overlay at a time)
- [ ] Bash-confirm variant (Claude Code "Do you want to run this command?") surfaces identically
- [ ] Desktop: background tab with active dialog → warning sound plays via `notificationsStore.playWarning()`
- [ ] Desktop: active-tab dialog → no sound (user can see it)
- [ ] Highlighted option (`❯` glyph) renders with `.itemHighlighted` background in overlay
- [ ] Destructive option ("No"/"Cancel"/"Abort") renders with `.itemDestructive` color
- [ ] Option hint in parens (e.g. "Yes, and don't ask again (shift+tab)") renders as separate `.hint` span
- [ ] Codex numbered-choice dialog (if/when encountered) captured by parser — add fixture if not
- [ ] Aider confirmation dialog — add fixture if layout differs
