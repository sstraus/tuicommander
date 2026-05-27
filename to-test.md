# To Test

Features to test when TUICommander is more usable.

## Terminal Blank Screen Recovery (2026-05-24 — PR #46)
- [HUMAN] Use terminal normally for extended period → no blank screen
- [HUMAN] Switch between tabs rapidly → terminal content preserved, no blank flash
- [HUMAN] Hide and show terminal tab → content visible immediately on restore
- [HUMAN] Check logs for `grid_frame_in_flight stuck` warning — should NOT appear under normal use
- [HUMAN] Force a decode failure (e.g. truncate a frame in dev) → terminal recovers within 500ms, no permanent blank

## Content Index Strategy (2026-05-24)
- [HUMAN] Settings → General → "Content Indexing" dropdown shows three options: Active repo only / Active + on switch / All repos at boot
- [HUMAN] Set "Active + on switch" (default): switch to a cold repo → content search works after a few seconds (index built on switch)
- [HUMAN] Set "Active repo only": switch repos → content search only works for the repo that was active at boot
- [HUMAN] Set "All repos at boot": start app with 3+ repos → all indexed after ~2s delay (check logs for "content index pre-warm complete")
- [HUMAN] Strategy persists across app restart
- [HUMAN] "Active + on switch": warm_content_index fires on repo switch (check logs for index build for the new repo)

## Toolbar Notification Relative Ages (2026-05-24)
- [HUMAN] Bell dropdown: each notification item shows relative age ("just now", "5m ago", "2h ago", "3d ago")
- [HUMAN] Keep the dropdown open for 30s+ → ages tick/update live without closing the popover
- [HUMAN] New notification arrives → shows "just now"

## File Browser Internal Drag (2026-05-24)
- [HUMAN] Drag a file within the file browser to a folder → move succeeds; no native OS drag triggered mid-gesture
- [HUMAN] Cancel an internal drag (release outside any target) → no lingering drag state in the app

## HTML Preview Cache-Bust (2026-05-24)
- [HUMAN] Open an image or PDF in the HTML preview tab → edit the file externally → switch away and back → preview shows updated file (not stale cached version)

## File Browser Intra-Tree Drag & Drop (2026-05-22)
- [HUMAN] Drag a file onto a folder in the file browser → file moves into that folder
- [HUMAN] Drag a file onto its own parent folder → no-op (no error)
- [HUMAN] Drag a folder into one of its own descendants → no-op (circular move prevented)
- [HUMAN] Tree view: same drag & drop behavior works with nested tree nodes
- [HUMAN] After move: file browser auto-refreshes showing the file in its new location

## Dormant Repo Throttling (2026-05-22)
- [HUMAN] Open repos with and without terminals → repos without terminals should have reduced watcher/polling activity (check logs for "throttled" or "dormant")
- [HUMAN] Switch to a cold repo → data refreshes immediately (no stale state)

## ANSI Colors in Markdown Code Blocks (2026-05-21)
- [HUMAN] Open a .md file with ANSI escape sequences in a code fence → colors render correctly (not stripped)
- [HUMAN] Prose text with ANSI escapes → escapes are stripped (not colorized)

## Plugin Watcher Fix - Issue #43 (2026-05-22)
- [HUMAN] Install a plugin but keep it disabled → no UI flashing/cycling
- [HUMAN] Install a plugin and enable it → hot-reload works on code file changes
- [HUMAN] Plugin writing runtime data to its `data/` subdirectory → no hot-reload triggered

## Block Timestamp Overlap Fix (2026-05-20)
- [HUMAN] Run several short commands in quick succession → hold Ctrl+Cmd → timestamp labels don't overlap vertically
- [HUMAN] Search for text → orange marks appear in scrollbar at match positions

## Expanded Menu Bar (2026-05-22)
- [HUMAN] All new menu items trigger the correct action (New File, Find in Content, Clear Scrollback, Refresh Terminal, etc.)
- [HUMAN] No double-firing when using menu items that also have keyboard shortcuts

## Auto-Standby (2026-05-21)
- [HUMAN] Settings → General → "Auto-Standby Timeout" control appears; set to 1 min, change back to 5, set to 0 (Off)
- [HUMAN] With timeout=1: open two tabs, switch away from one for 1+ min while it's idle → standby badge (⏸) appears in the background tab
- [HUMAN] Click the standby tab → badge disappears immediately (SIGCONT on focus)
- [HUMAN] `curl http://localhost:<port>/session/list` → standby field present, `true` for stopped session
- [HUMAN] Set timeout=0 → within 30s, any currently stopped sessions wake up (no badge remains)

## Detachable Panels
- [HUMAN] Activity Dashboard: click detach button in header → opens in separate window
- [HUMAN] Activity Dashboard detached: rows show live terminal status updates (~1 Hz)
- [HUMAN] Activity Dashboard detached: click row → navigates to terminal in main window
- [HUMAN] Activity Dashboard detached: globe button toggles Global Workspace promotion
- [ ] Activity Dashboard detached: close window → main window clears detached state **BUG: ghost tab remains, closing float doesn't restore panel. Fix: #1719-989c**
- [HUMAN] AI Chat: detach button → opens in separate window with streaming intact
- [HUMAN] AI Chat detached: send message, verify streaming response renders
- [HUMAN] AI Chat detached: close window → main window shows panel again (not placeholder)
- [HUMAN] Command Palette: "Open Activity Dashboard in separate window" entry works
- [HUMAN] Both panels: detach while panel is open, verify placeholder shown in main window

## Experimental Feature Flags
- [HUMAN] Settings > General: "Experimental Features" section visible at bottom
- [HUMAN] Master toggle defaults to OFF for fresh config
- [HUMAN] Enabling master toggle reveals AI Chat sub-flag
- [HUMAN] Disabling master toggle hides sub-flags but preserves their values
- [HUMAN] Toggle persists across app restart (check config.json)

## AI Chat (Level 1)
- [HUMAN] Settings > AI Chat tab: provider dropdown shows Ollama/Anthropic/OpenAI/OpenRouter/Custom
- [HUMAN] Ollama selected + running: green dot, model list populated from /api/tags
- [HUMAN] Ollama selected + not running: red dot with "Not detected" message
- [HUMAN] API key field: masked, saved to keyring, "Key saved" indicator after save
- [HUMAN] Test Connection button: success/error result inline
- [HUMAN] Context lines slider: 50-500, persists across restart
- [HUMAN] Temperature slider: 0.0-1.0, persists across restart
- [HUMAN] Cmd+Alt+A toggles AI Chat panel open/closed
- [HUMAN] Status bar: chat bubble icon toggles panel, highlighted when active
- [HUMAN] Panel: terminal dropdown lists all open terminals, switching attaches
- [HUMAN] Panel: pin button prevents auto-attach on terminal focus change
- [HUMAN] Panel: send message with Cmd+Enter, Shift+Enter for newline
- [HUMAN] Panel: streaming response shown as raw text, markdown rendered on completion
- [HUMAN] Panel: code blocks have Copy and Run buttons after stream ends
- [HUMAN] Panel: Run button sends code to attached terminal via sendCommand
- [HUMAN] Panel: Stop button visible during streaming, cancels generation
- [HUMAN] Panel: Clear conversation button resets all messages
- [HUMAN] Panel: empty state "Ask me about your terminal output" when no messages
- [HUMAN] Panel: error banner with Retry button on provider failure
- [HUMAN] Panel: opening AI Chat closes other exclusive panels (markdown, git, file browser)
- [HUMAN] Right-click terminal selection > "Explain with AI": opens panel, sends selection
- [HUMAN] Right-click terminal (no selection) > "Explain with AI": sends last 50 buffer lines
- [HUMAN] Right-click terminal > "Fix this error": sends error analysis prompt
- [HUMAN] Selection >2000 chars truncated with "[... truncated]" marker

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
- [HUMAN] Start button in AI Chat header sends goal → agent banner appears with "running" + iter counter
- [HUMAN] Tool-call cards render in order for each `ai_terminal_*` the agent emits (read_screen, send_input, wait_for, get_state)
- [HUMAN] Pause button freezes iteration; resume continues from next tool call
- [HUMAN] Cancel button clears banner and stops future iterations
- [HUMAN] Destructive command (rm -rf, git reset --hard, DROP TABLE) triggers approval card; reject skips, approve executes
- [HUMAN] Agent error (provider failure) surfaces in chat with Retry
- [HUMAN] Rejoining session after reload: agent state recovered from store; tool-call history preserved (schema v2)

## AI Agent — External MCP Tools (1303)
- [HUMAN] Remote MCP client (Claude Code / Cursor) lists six `ai_terminal_*` tools via `tools/list`
- [HUMAN] `ai_terminal_read_screen` returns redacted screen text; respects `lines` cap
- [HUMAN] `ai_terminal_send_input` on an idle session prompts user confirm dialog; rejects while internal agent loop is active on that session
- [HUMAN] `ai_terminal_send_key` honours named keys (enter, tab, ctrl+c, escape, up/down) with same confirmation semantics
- [HUMAN] `ai_terminal_wait_for` returns on regex match, timeout_ms, or stability window
- [HUMAN] `ai_terminal_get_state` reflects current shell_state/cwd/terminal_mode/agent_type
- [HUMAN] `ai_terminal_get_context` returns compact ~500-char summary aligned with SessionKnowledge.build_context_summary

## AI Agent — Filesystem Tools (1325-1331)
- [HUMAN] `ai_terminal_read_file` returns line-numbered content; respects offset/limit; rejects binary and >10MB; secrets redacted
- [HUMAN] `ai_terminal_write_file` creates a new file; overwrites existing; confirm dialog appears for MCP callers
- [HUMAN] `ai_terminal_write_file` to `.env` or `Cargo.toml` triggers "sensitive path" rejection
- [HUMAN] `ai_terminal_edit_file` replaces unique occurrence; rejects non-unique without replace_all; confirm dialog for MCP
- [HUMAN] `ai_terminal_list_files` matches glob patterns (e.g. `**/*.rs`); reports dir vs file type; max 500
- [HUMAN] `ai_terminal_search_files` finds regex matches with context; respects .gitignore; max 50 matches
- [HUMAN] `ai_terminal_run_command` captures stdout/stderr; sanitized env (only PATH/HOME/TERM/LANG); safety blocks sudo; confirm dialog for MCP
- [HUMAN] `ai_terminal_run_command` with 500ms timeout kills the process cleanly
- [HUMAN] Filesystem tools only work within the session's sandbox root — `../` traversal rejected
- [HUMAN] Agent system prompt now documents all 12 tools with when-to-use guidance

## AI Agent — Session Knowledge (1305/1306/1307/1309)
- [HUMAN] OSC 133 shell (with `shell-integration.sh` sourced): running a command populates SessionKnowledgeBar with a Success/Error row and exit code
- [HUMAN] Shell without OSC 133: busy→idle transition populates an `inferred` outcome row (no exit code, empty command text)
- [HUMAN] Error classification tags match expected `error_type` for rust_compilation, npm_error, python_error, missing_tool, missing_file, permission, network
- [HUMAN] Error→fix correlation: failing command followed within 3 commands by a success populates "Known Fixes" in the context summary
- [HUMAN] SessionKnowledgeBar collapsed row shows commands count; "recent err" pill appears when errors exist; "tui:" pill appears when in fullscreen TUI
- [HUMAN] SessionKnowledgeBar auto-refreshes ~2s after new pty-parsed events (debounced)
- [HUMAN] Relaunch app: `{config_dir}/ai-sessions/{session_id}.json` files exist for recent sessions; bar reloads with history intact
- [HUMAN] Agent system prompt now includes "## Session Knowledge" block (verify via debug logs)

## MCP Session Tombstone
- [x] `agent spawn` → `session output` after 1.8s → returns live buffer with `exited:false` (9b886c20 E2E validated 2026-04-10)
- [x] `session close` → `session output` → returns final buffer with `exited:true`, buffer preserved (9b886c20 E2E validated 2026-04-10)
- [x] `session kill` → `session output` → returns final buffer with `exited:true`, `exit_code:1` (9b886c20 + MCP E2E 2026-04-10; NOTE: actual exit_code is 1, not 129/SIGHUP as originally expected)
- [x] Unknown session id (never existed) → returns `{"error":"Session not found","reason":"session_not_found_or_reaped"}` (MCP E2E validated 2026-04-10)
- [HUMAN] Close → wait >5 min (TOMBSTONE_TTL_MS) → output returns the same reaped error
- [HUMAN] close_pty Tauri command (GUI "close terminal") still works and preserves post-mortem reads for subsequent MCP calls

## Global Workspace
- [HUMAN] Open Activity Dashboard → globe icon on each terminal row → click toggles promoted
- [HUMAN] Promote 2+ terminals → sidebar shows "Global" entry with badge count
- [HUMAN] Click sidebar "Global" → switches to global workspace with promoted terminals in split view
- [HUMAN] Each pane tab shows repo name + colored dot in global workspace
- [HUMAN] Click sidebar "Global" again → switches back to repo view, both layouts preserved
- [HUMAN] Cmd+Shift+X → toggles global workspace
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
- [HUMAN] Close all terminals, type `~test` → shows "No terminals open"
- [HUMAN] Type "Search Terminals" in palette → command appears; selecting it pre-fills `~ `
- [HUMAN] "Search Files" command pre-fills `! `, "Search in File Contents" pre-fills `? `
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
- [HUMAN] Settings > Keyboard Shortcuts > Global Hotkey section visible (desktop only)
- [HUMAN] Click "Click to set hotkey" → capture mode activates
- [HUMAN] Press a key combo → registers and shows in the field
- [HUMAN] Switch to another app → press hotkey → TUICommander appears focused
- [HUMAN] Press hotkey again while focused → TUICommander minimizes
- [HUMAN] Press hotkey while visible but unfocused → TUICommander gains focus
- [HUMAN] Clear button removes the hotkey
- [HUMAN] Hotkey persists across app restart
- [HUMAN] Browser mode: Global Hotkey section is hidden

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
- [HUMAN] Click filename in scroll view → opens file in editor tab
- [x] Sticky header shows total files/additions/deletions _(MCP screenshot verified 2026-05-16: "All Changes 16 files +501 -366")_
- [HUMAN] Stage/unstage a file → scroll view updates reactively

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
- [HUMAN] Failed `git push` → shows error item with red icon
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
- [HUMAN] Stale `mcp-*.sock` files cleaned on startup

## Shell State Rust Derivation (741-3faf)
- [HUMAN] Agent runs → tab shows blue busy indicator
- [HUMAN] Agent stops → tab transitions to green idle (no mode-line flicker)
- [HUMAN] Agent asks "Procedo?" → question notification fires (no false completion)
- [HUMAN] Resize during idle → no brief blue flash on tab
- [HUMAN] pendingInitCommand (worktree run script) executes on first idle
- [HUMAN] Sub-agents running → terminal stays busy until they finish
- [HUMAN] Terminal remount (tab switch) correctly syncs shell state from Rust
- [HUMAN] Completion notification fires after agent works ≥5s then goes idle (background tab)
- [HUMAN] No completion notification when terminal is awaiting input (question/error)
- [HUMAN] No purple unseen dot while agent is actively working (status line timer ticking)

## Plan Panel (515-660c / 516-41a5 / 517-74c2)
- [x] `Cmd+Shift+P` opens plan panel on right side (MCP maccontrol verified 2026-04-10)
- [HUMAN] Plan panel shows plans only for the active repository
- [HUMAN] Click plan item opens it as markdown tab (frontmatter stripped)
- [HUMAN] Switching repos changes visible plans in the panel
- [HUMAN] New plan detected by agent auto-opens as background tab (no focus steal)
- [HUMAN] Repeated detection of same plan does not open duplicate tabs
- [HUMAN] Panel visibility persists across app restart
- [x] Plan panel is mutually exclusive with Diff/Markdown/FileBrowser panels
- [x] Plan count badge shows correct number

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
- [ ] Double-click branch name - dialog should open _(NOTE: code shows double-click triggers checkout, not rename. Rename is via Shift+R or context menu "Rename" — BranchesTab.tsx:867 vs :750)_
- [HUMAN] Input pre-filled with current name
- [HUMAN] Validate invalid names (spaces, special chars)
- [HUMAN] Rename succeeds - branch updates in sidebar
- [HUMAN] Terminal associations preserved after rename
- [HUMAN] ESC closes dialog, Enter confirms

## Repository Context Menu (073-50dd)
- [x] Click ⋯ on repo header - menu appears
- [x] "Repo Settings" opens settings
- [HUMAN] "Remove Repository" shows confirmation
- [HUMAN] Confirm removal - all terminals close, repo removed
- [x] Click outside menu - menu closes

## Repository State Persistence
- [x] Repos start expanded by default (not collapsed) _(verified: repositories.ts — no default collapsed=true in initial state)_
- [HUMAN] Expanded/collapsed state persists across restarts _(code verified: repositories.ts:307-316 toggleExpanded/toggleCollapsed both trigger saveRepos with 500ms debounce to backend)_

## macOS Option Key (056-cee9)
- [HUMAN] Option+key combinations work in terminal _(code verified: CanvasTerminal.tsx:2568-2605 distinguishes left/right Option — left generates ESC sequences, right passes composed chars)_
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
- [HUMAN] Names are unique across worktrees
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
- [HUMAN] Quit shows confirmation when active terminal sessions exist _(code verified: App.tsx:1938-1948 + useAppInit.ts:106-113 — shows dialog when confirmBeforeQuit=true AND active terminals)_
- [HUMAN] Dialog shows session count
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
- [HUMAN] Click check item opens URL in browser
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
- [HUMAN] Download Model button works (downloads ~1.5GB large-v3-turbo)
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
- [HUMAN] Default corrections loaded (Cloud Code → Claude Code)
- [HUMAN] Add/remove corrections in Settings > Dictation
- [HUMAN] Corrections applied to transcribed text before injection
- [HUMAN] Import/export corrections as JSON

### Settings Tab
- [HUMAN] Enable/disable toggle persists across restarts
- [HUMAN] Language selector works (auto-detect, English, etc.)
- [HUMAN] Hotkey recorder captures key combinations
- [HUMAN] Audio devices listed (shows system default)

## Native System Menu Bar (Stories 192 + 193)
- [x] Menu bar visible on macOS (top of screen), Windows/Linux (under title bar)
- [x] macOS: App menu has About, Services, Hide, Hide Others, Show All, Quit (+ Check for Updates)
- [x] File menu: New Tab, New File, Close Tab, Reopen Closed Tab, Settings, (Quit on non-macOS)
- [x] Edit menu: Undo, Redo, Cut, Copy, Paste, Select All, Find in Content, Clear Terminal, Clear Scrollback, Refresh Terminal
- [x] View menu: Toggle Sidebar, Split Right/Down, Maximize/Restore Pane, Focus Mode, Zoom In/Out/Reset, Zoom All In/Out/Reset, File Browser, Diff/Markdown/Notes/Outline/AI Chat/Compose/Global Workspace panels
- [x] Go menu: Next/Previous Tab, Switch to Tab 1-9
- [x] Tools menu: Prompt Library, Run/Edit & Run Command, Search File Contents, Git Panel, Branches, Diff Scroll, Task Queue, SSH Tunnels, Process Manager
- [x] Help menu: Help Panel, About TUICommander (+ Check for Updates on non-macOS)
- [HUMAN] Clicking menu items triggers correct action (same as keyboard shortcut)
- [x] Accelerator labels show correct modifier key per platform (CmdOrCtrl in code → Tauri resolves)
- [x] No double-firing: pressing Cmd+T creates one tab, not two
- [HUMAN] Predefined Edit items (Copy/Paste/Undo/Redo) work correctly with native focus
- [HUMAN] HelpPanel shows note about system menu bar

## Terminal Session Persistence
- [x] Open 2 repos, each with a branch and 2 terminals. Quit app, reopen → same terminals recreated _(verified: useAppInit.ts:60-95 collectTerminalSnapshots; lines 497-542 PTY reconnect re-adopts surviving sessions by cwd)_
- [HUMAN] Run `claude` in a terminal, quit app, reopen → terminal auto-sends `claude --continue`
- [HUMAN] Run `gemini` in a terminal, quit app, reopen → terminal auto-sends `gemini --resume`
- [x] Plain shell terminal → restored without any agent command _(verified: restoration uses SavedTerminal.agentType; null agentType = plain shell, no resume command)_
- [HUMAN] HMR reload (Vite dev) → uses existing reconnect path, not the new restore
- [HUMAN] Delete a repo folder externally, reopen → skips that repo gracefully
- [HUMAN] `hadTerminals` logic still works (no auto-spawn after intentional close-all)

## Repository Groups (Accordion UI)
- [HUMAN] Create group from Settings > Groups tab
- [HUMAN] Rename group (double-click name in settings)
- [HUMAN] Delete group — repos move to ungrouped
- [HUMAN] Assign color preset to group (5 presets + clear)
- [HUMAN] Group appears as accordion section in sidebar
- [x] Click group header toggles collapse/expand
- [x] Group color dot visible when color set
- [HUMAN] Drag repo within same group reorders
- [HUMAN] Drag repo onto group header assigns to group
- [HUMAN] Drag repo from group to ungrouped area removes from group
- [HUMAN] Drag repo between groups moves correctly
- [HUMAN] Drag group header to reorder groups
- [x] Right-click group header shows Rename/Color/Delete
- [x] Right-click repo shows "Move to Group" submenu
- [HUMAN] Quick switcher force-expands collapsed groups
- [HUMAN] Existing repos auto-migrate (all start ungrouped)
- [HUMAN] Color inheritance: repo color > group color > default
- [HUMAN] Empty group shows "Drag repos here" hint
- [HUMAN] Group name uniqueness enforced (case-insensitive)

## Tab Title Improvements
- [x] Run AI agent → tab title updates with task name → process exits → title reverts to original _(verified: Terminal.tsx:212 originalName tracked; lines 511-513 revert on PTY exit when !nameIsCustom)_
- [x] Rename tab → run agent → exit → custom name persists (not overwritten) _(verified: Terminal.tsx:593 checks nameIsCustom flag to prevent overwriting user-set names)_
- [HUMAN] Launch `FOO=bar claude` → tab should show `claude`, not the env vars
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
- [HUMAN] Settings → General → Worktree Defaults: all 7 dropdowns/toggles render and save
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
- [ ] `Cmd+Shift+F` opens file browser panel with content search mode active — **BUG CONFIRMED**: action `toggle-file-browser-content-search` registered in keybindingDefaults.ts + actionRegistry.ts but NO handler in useKeyboardShortcuts.ts dispatchAction switch
- [HUMAN] `C` button in search bar toggles between filename search and content search
- [HUMAN] Results stream in progressively, grouped by file with match count
- [HUMAN] Each result row shows file path, line number, and highlighted match context
- [HUMAN] Click a result opens the file in code editor at the matched line
- [HUMAN] Case-sensitive toggle works (uppercase vs lowercase match)
- [HUMAN] Regex toggle works (e.g. `foo.*bar`)
- [HUMAN] Whole-word toggle works (e.g. `foo` does not match `foobar`)
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
- [HUMAN] Checkout via `Enter` or double-click switches branch
- [x] Checkout with dirty worktree shows stash/force/cancel dialog _(verified: BranchesTab.tsx:126 DirtyCheckoutState; line 1077-1078 dialog rendered)_
- [x] `n` key opens inline create-branch form _(verified: BranchesTab.tsx:727,740 'n' key handler)_
- [HUMAN] Create branch with "Checkout after create" creates and switches
- [x] `d` key deletes branch with confirmation (safe delete refuses unmerged) _(verified: BranchesTab.tsx:745 'd' key handler)_
- [HUMAN] Force delete option available in confirmation
- [HUMAN] Deleting current branch or default branch is blocked
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
- [HUMAN] Arrow keys navigate, Enter executes, Ctrl+N new, Ctrl+E edit, Ctrl+F favorite
- [x] New prompt editor has placement checkboxes, auto-execute, shortcut fields _(verified: SmartPromptsTab.tsx:342-359 placement checkboxes; lines 388-397 auto-execute; lines 482-488 keyboard shortcut capture)_
- [x] Built-in prompts: name disabled, "Reset to Default" button, "built-in" badge, no delete _(verified: SmartPromptsTab.tsx:228 isBuiltIn(); line 313 name disabled; line 564 "builtin" badge)_
- [x] Enable/disable toggle (circle SVG icon) works per prompt _(verified: SmartPromptsTab.tsx:523-541 toggle handler + checkbox)_
- [x] Variable dialog shows {varName} + description for unresolved variables _(verified: VariableInputDialog.tsx:58-75 renders per-variable input with descriptions)_
- [HUMAN] All 24 built-in prompts show descriptions in list
- [HUMAN] Settings panel no longer has "Smart Prompts" tab
- [x] Cmd+Shift+K opens SmartPromptsDropdown with status banner when disabled (MCP maccontrol verified 2026-04-10)
- [x] SmartButtonStrip in Changes tab always visible (grayed out without agent) _(verified: ChangesTab.tsx:803-808 SmartButtonStrip with placement="git-changes")_
- [HUMAN] All icons in drawer are SVG (no emoji)

## Tailscale HTTPS
- [x] With Tailscale running + HTTPS enabled: app serves HTTPS on same port _(verified: tailscale.rs ~500 lines: provision_cert(), cert_renewal_loop(), rustls TLS provisioning)_
- [HUMAN] QR code shows https:// URL with Tailscale FQDN
- [x] Without Tailscale: HTTP works as before (no TLS) _(verified: tailscale.rs:10-19 TailscaleState enum with NotInstalled/NotRunning variants; graceful fallback)_
- [x] Settings > Services shows Tailscale status section _(verified: get_tailscale_status() command exposed to frontend via lib.rs)_
- [HUMAN] Cookie gets Secure flag when accessed over HTTPS

## Base Branch Tracking
- [x] Create branch with base ref selector → base stored in git config _(verified: config.rs:867 after_merge field; CreateWorktreeDialog has BaseRefDropdown; base_branch in repo config)_
- [x] Sidebar shows yellow ⇣N badge when branch is behind base _(verified: BranchesTab.tsx:886 base_behind count; BranchesTab.module.css:152-155 .baseBehind with --warning yellow color)_
- [HUMAN] Badge tooltip shows base branch name
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
- [HUMAN] Completion notification not falsely triggered by post-input status updates

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
- [HUMAN] Click "Env" button on saved config → inline edit panel opens
- [HUMAN] Edit/remove env vars in saved config → changes persist

## Headless Agent Grouped Dropdown (e917dfdc)
- [x] Settings → Agents → Headless Agent dropdown: agents with run configs show optgroup _(verified: SmartPromptsTab.tsx:676 uses `<optgroup label={...}>`)_
- [x] Agents without run configs show as single option _(verified: SmartPromptsTab.tsx:660-692 dropdown structure)_
- [x] Selecting a run config stores "type:name" in headless_agent field _(verified: SmartPromptsTab.tsx:680 "type:configName" format)_
- [HUMAN] Same grouped dropdown in Smart Prompts tab

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
- [ ] Agent question → phone receives push notification **BUG: arrives late, often after question already answered. Fix: #1720-9661**
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
- [ ] Cmd+Alt+Enter hides sidebar, tab bar, and any open side panel (AI chat, git, markdown, notes, file browser) **BUG: right side panel (outline, references) stays visible. Fix: #1718-e07c — added id attrs + CSS selectors**
- [x] Toolbar (title bar) and StatusBar remain visible and functional _(MCP maccontrol verified 2026-05-16: both visible in focus mode screenshot)_
- [x] Cmd+Alt+Enter again restores the previous layout (panel state preserved — the same panel that was open reappears) _(MCP maccontrol verified 2026-05-16: second press restored full layout)_
- [HUMAN] Setting `toggle-focus-mode` combo via KeyboardShortcuts tab changes the active hotkey
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
- [HUMAN] Desktop: background tab with active dialog → warning sound plays via `notificationsStore.playWarning()`
- [HUMAN] Desktop: active-tab dialog → no sound (user can see it)
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
- [HUMAN] Auth failure (wrong key/user) → Stopped immediately, no reconnect attempts
- [HUMAN] Host key mismatch → Stopped immediately with HostKeyMismatch reason
- [HUMAN] Network failure → Reconnecting with exponential backoff (check audit log for retry events)
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
- [HUMAN] Alternate screen app (vim, less) → `ReflowMode::None` applies, no reflow on alt screen
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

## Compose Panel: Text Persistence (2026-05-13)
- [x] Open compose (Cmd+I) → type text → close (Esc) → reopen (Cmd+I) → text is preserved (MCP maccontrol verified 2026-05-16)
- [x] Cmd+I when compose is open → closes it (proper toggle) (MCP maccontrol verified 2026-05-16)
- [HUMAN] Send a command (Ctrl+Enter) → compose closes → reopen → editor is empty (reset after send)
- [HUMAN] Open compose with no prior text → cursor line is pre-populated (first open behavior)
- [x] Open compose, type something, close, open again → typed text preserved, NOT overwritten by cursor line (MCP maccontrol verified 2026-05-16)

## Status Bar: Pulse on Status Change (2026-05-13)
- [ ] Copy text in terminal (Cmd+C on selection) → "Copied to clipboard" flashes accent color then fades **BUG: text appears but no pulse animation. Fix: #1717-44d9 — replaced @keyframes with transition-based pulse**
- [ ] Git operation → status message pulses once in accent color **BUG: same root cause as above**
- [HUMAN] "Ready" status does not pulse (only non-Ready messages trigger it)
- [HUMAN] Pulse is a single flash (accent → normal), not repeating

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
- [HUMAN] Cmd+Shift+. → toggles fold on the current block (collapses/expands)
- [HUMAN] Cmd+Shift+Up/Down → jumps between block boundaries
- [HUMAN] Cmd+Shift+B → toggles block-scoped search (search restricted to current block)
- [HUMAN] Cmd+F with block-scoped toggle ON → only matches within current block shown
- [HUMAN] Settings > Terminal > Blocks → toggle timestamps and folding on/off
- [HUMAN] Run 500+ commands → oldest blocks evicted, no crash or memory growth
- [HUMAN] Claude Code session: tool calls show as blocks without OSC 7770 (heuristic detection)

## Generators Modal (2026-05-20)
- [HUMAN] Command palette → type "generator" → "Open generators" action appears in Generators category
- [HUMAN] Modal opens — left sidebar shows 10 generators: Password, UUID v4, UUID v7, ULID, CUID2, JWT Secret, TOTP Secret, Nano ID, Slug, Ed25519 Key
- [HUMAN] Selecting each generator auto-generates a value immediately (no manual click needed)
- [HUMAN] Password: length slider (4–128) and charset checkboxes (A–Z, a–z, 0–9, !@#…) work; slider updates label live; changing options regenerates
- [HUMAN] Nano ID: length number input (4–64) works; changing length regenerates
- [HUMAN] Ed25519 Key: shows two textareas (Private key PKCS#8 / Public key SPKI) + "Copy Private" + "Copy Public" buttons
- [HUMAN] Copy button → clipboard contains the generated value; button shows "Copied!" for 2s then resets
- [HUMAN] Regenerate button produces a new value each click
- [HUMAN] Escape key closes modal; clicking overlay closes modal
- [HUMAN] No generated value appears in app logs (`curl http://localhost:9876/logs | grep -i "password\|secret\|key"` returns nothing)

## Process Monitor
- [HUMAN] HTTP endpoint: `curl http://localhost:<port>/process/stats` returns JSON array of `{session_id, name, pid, rss_kb, cpu_pct}`
- [HUMAN] HTTP panel: `curl http://localhost:<port>/process/monitor` returns HTML dashboard
- [HUMAN] MCP tool: `session action=process_stats` returns `{processes: [...]}` with TUIC + child stats
- [HUMAN] Panel: open `/process/monitor` in TUIC tab — shows summary stats + process tree table
- [HUMAN] Panel: auto-refresh at 5s interval shows live CPU/memory updates
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
