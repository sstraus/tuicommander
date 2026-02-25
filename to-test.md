# To Test

Features to test when TUICommander is more usable.

## PR Detection (071-cc1f)
- [ ] Run `gh pr view` in terminal - verify PR badge appears in sidebar
- [ ] Run `gh pr create` - verify PR URL is captured
- [ ] Verify PR badge shows in StatusBar
- [ ] Click PR in StatusBar - should open browser
- [ ] Switch branches - PR info should persist per branch
- [ ] GitLab MR URLs detection

## Rename Branch (072-d7d6)
- [ ] Double-click branch name - dialog should open
- [ ] Input pre-filled with current name
- [ ] Validate invalid names (spaces, special chars)
- [ ] Rename succeeds - branch updates in sidebar
- [ ] Terminal associations preserved after rename
- [ ] ESC closes dialog, Enter confirms

## Repository Context Menu (073-50dd)
- [ ] Click ⋯ on repo header - menu appears
- [ ] "Repo Settings" opens settings
- [ ] "Remove Repository" shows confirmation
- [ ] Confirm removal - all terminals close, repo removed
- [ ] Click outside menu - menu closes

## Repository State Persistence
- [ ] Repos start expanded by default (not collapsed)
- [ ] Expanded/collapsed state persists across restarts

## macOS Option Key (056-cee9)
- [ ] Option+key combinations work in terminal
- [ ] Special characters via Option (@ # etc) work

## Rust Backend (batch 047-070)

### Lazygit Auto-Detection (048)
- [ ] Lazygit binary detected when installed
- [ ] Graceful fallback when lazygit not installed
- [ ] detect_lazygit_binary returns correct path

### Git Branches Command (052)
- [ ] get_git_branches returns all local branches
- [ ] Works on repos with many branches
- [ ] Error handling for non-git directories

### CI Checks Command (060)
- [ ] get_ci_checks returns check details via gh run list
- [ ] Works when gh CLI is authenticated
- [ ] Graceful error when gh CLI not installed or not authenticated

### Adjective-Animal Worktree Names (063)
- [ ] New worktrees get adjective-animal names
- [ ] Names are unique across worktrees
- [ ] Name format is consistent (adjective-animal)

### Single Window Enforcement (065)
- [ ] Second app instance focuses existing window instead of opening new one
- [ ] Works across multiple desktops/spaces

### PTY Spawn Retry (059)
- [ ] Terminal creation succeeds on first attempt normally
- [ ] Retries up to 3 times on spawn failure
- [ ] Increasing delay between retries (100ms/200ms/300ms)
- [ ] Error message shown after all retries exhausted

## Frontend (batch 047-070)

### Lazygit Split Pane (047)
- [ ] Cmd+Shift+L opens lazygit in split pane
- [ ] Lazygit runs in correct repo directory
- [ ] Closing pane cleans up terminal session
- [ ] Split pane resizes correctly with window

### Per-Repo Lazygit Config (049)
- [ ] .lazygit.yml in repo root is detected and used
- [ ] .lazygit.yaml also works
- [ ] Lazygit launches without config if none present

### Git Quick Actions in Sidebar (050)
- [ ] Pull/Push/Fetch/Stash buttons visible in sidebar
- [ ] Each button sends correct git command to active terminal
- [ ] Commands execute in shell (not just displayed)
- [ ] Buttons disabled when no active terminal

### Lazygit Floating Window (051)
- [ ] Can dock/undock lazygit between split and floating mode
- [ ] Floating window is draggable/resizable
- [ ] Terminal content preserved when switching modes

### GitOperationsPanel Live Branches (052-frontend)
- [ ] Branch list populated from get_git_branches (not hardcoded)
- [ ] Branch list updates when branches change
- [ ] Current branch highlighted

### Help Panel (053)
- [ ] Cmd+? opens help panel
- [ ] All shortcuts listed and searchable
- [ ] Search filters shortcuts in real-time
- [ ] ESC or Cmd+? closes help panel

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
- [ ] Sidebar toggle shows ⌘[ hint
- [ ] MD/Diff toggle buttons show ⌘M/⌘D hints
- [ ] New tab button shows hint
- [ ] Hints visible but not intrusive

### Consolidated Status Bar (069)
- [ ] Status bar renders as single inline row
- [ ] All elements (zoom, sessions, git status, toggles) properly spaced
- [ ] No empty gaps or orphaned sections

### Visual Density Improvements (070)
- [ ] Sidebar items have compact padding
- [ ] Tab bar tabs have reduced min-width
- [ ] Overall UI feels tighter without losing readability

## Voice Dictation (Stories 117-123)

### Model Management
- [ ] Settings > Dictation tab visible
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
- [ ] Menu bar visible on macOS (top of screen), Windows/Linux (under title bar)
- [ ] macOS: App menu has About, Services, Hide, Hide Others, Show All, Quit
- [ ] File menu: New Tab, Close Tab, Reopen Closed Tab, Settings, (Quit on non-macOS)
- [ ] Edit menu: Undo, Redo, Cut, Copy, Paste, Select All, Clear Terminal
- [ ] View menu: Toggle Sidebar, Split Right/Down, Zoom In/Out/Reset, Diff/Markdown panels
- [ ] Go menu: Next/Previous Tab, Switch to Tab 1-9
- [ ] Tools menu: Prompt Library, Run/Edit & Run Command, Lazygit, Git Operations, Task Queue
- [ ] Help menu: Help Panel, About TUICommander
- [ ] Clicking menu items triggers correct action (same as keyboard shortcut)
- [ ] Accelerator labels show correct modifier key per platform (Cmd on macOS, Ctrl on others)
- [ ] No double-firing: pressing Cmd+T creates one tab, not two
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
- [ ] Click group header toggles collapse/expand
- [ ] Group color dot visible when color set
- [ ] Drag repo within same group reorders
- [ ] Drag repo onto group header assigns to group
- [ ] Drag repo from group to ungrouped area removes from group
- [ ] Drag repo between groups moves correctly
- [ ] Drag group header to reorder groups
- [ ] Right-click group header shows Rename/Color/Delete
- [ ] Right-click repo shows "Move to Group" submenu
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
- [ ] Settings > Plugins tab shows installed plugins with built-in badge
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
