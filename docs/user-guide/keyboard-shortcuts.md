# Keyboard Shortcuts

All shortcuts use `Cmd` on macOS and `Ctrl` on Windows/Linux unless noted.

## Customizing Keybindings

### From the UI

Open **Settings > Keyboard Shortcuts**, or **Help > Keyboard Shortcuts** (or `Cmd+?` → Keyboard Shortcuts) — both show the same editor. Click the pencil icon next to any shortcut to enter recording mode, then press your new key combination. The app warns you if the combo is already used by another action. Overridden shortcuts are highlighted in accent color with a reset icon to revert individually. A "Reset all to defaults" button is at the bottom.

**Extended function keys (macOS).** `F13`–`F20` are recordable, both here and for the Global Hotkey. macOS never forwards them to the app's WebView, so TUICommander captures them natively and feeds them into the recorder; if one of them still appears dead, macOS itself consumed the key first (`F14`/`F15` drive keyboard illumination on many Macs — remap or disable that in System Settings > Keyboard). `F21`–`F24` are accepted in `keybindings.json` but have no macOS key code, so no Mac keyboard can produce them.

### By editing the config file

You can also edit the `keybindings.json` file directly in your config directory:

- **macOS:** `~/Library/Application Support/tuicommander/keybindings.json`
- **Windows:** `%APPDATA%\tuicommander\keybindings.json`
- **Linux:** `~/.config/tuicommander/keybindings.json`

The file is a JSON array of override objects. Only include shortcuts you want to change — anything not listed uses the default:

```json
[
  { "action": "toggle-git-ops", "key": "Cmd+Shift+Y" },
  { "action": "toggle-markdown", "key": "Cmd+Shift+M" }
]
```

- `"key"` uses `Cmd` as the platform-agnostic modifier (resolved to Meta on macOS, Ctrl on Win/Linux)
- Set `"key": ""` or `"key": null` to unbind an action
- Changes made via the UI are saved to this same file
- The file is loaded at startup — restart TUICommander to pick up manual edits

See the action table below for all available action names.

## Global Hotkey

A configurable OS-level shortcut to toggle TUICommander's visibility from any application.

- **Configure:** Help > Keyboard Shortcuts > Global Hotkey (top of the tab)
- **No default** — you must set it yourself (e.g., `Ctrl+Space`, `Cmd+\``)
- **Toggle behavior:** hidden/minimized → show+focus, visible but unfocused → focus, focused → instant hide
- `Cmd` and `Ctrl` are treated as distinct modifiers
- Uses `tauri-plugin-global-shortcut` (no Accessibility permission required on macOS)
- Not available in browser/PWA mode
- Persists across app restarts

## AI Chat Composer

AI Chat's composer handles `Ctrl+S` locally to park or restore its prompt,
including on macOS. The same action is available from the composer button on a
phone; it is not a configurable global shortcut.

Press `Escape` in the AI Chat composer or transcript to stop a running turn
without clearing your draft. When Find is open, Escape closes Find first.

## Terminal Operations

| Shortcut | Action |
|----------|--------|
| `Cmd+T` | New terminal tab |
| `Cmd+W` | Close tab (or close active pane in split mode) |
| `Cmd+Shift+T` | Reopen last closed tab |
| `Cmd+Shift+R` | Run saved command |
| `Cmd+R` | Reload active web/preview tab |
| `Cmd+L` | Clear terminal |
| `Cmd+Shift+L` | Refresh terminal (fix rendering glyphs) |
| `Cmd+F` | Find in terminal / diff tab |
| `Cmd+G` | Git Panel — Branches tab (or Find next match when search is open) |
| `Enter` | Find next match (when search is open) |
| `Cmd+Shift+G` / `Shift+Enter` | Find previous match (when search is open) |
| `Escape` | Close search overlay |
| `Cmd+C` | Copy selection |
| `Cmd+V` | Paste to terminal |
| `Cmd+Home` | Scroll to top |
| `Cmd+End` | Scroll to bottom |
| `Shift+PageUp` | Scroll page up |
| `Shift+PageDown` | Scroll page down |
| `Cmd+Shift+.` | Toggle block folding |
| `Cmd+Shift+Up` | Jump to previous block |
| `Cmd+Shift+Down` | Jump to next block |
| `Cmd+Shift+B` | Toggle block-scoped search |

Hands-free conversation: press `Cmd+Alt+V` (`Ctrl+Alt+V` on Windows/Linux) to start or stop listening on the active terminal. Dictation must be enabled. A custom binding or explicit unbind in Keyboard Shortcuts overrides this default.

## Tab Navigation

| Shortcut | Action |
|----------|--------|
| `Cmd+1` through `Cmd+9` | Switch to tab by number |
| `Ctrl+Tab` | Next tab |
| `Ctrl+Shift+Tab` | Previous tab |

Hold `Cmd` alone (`Ctrl` on Windows/Linux) for 450 ms to show shortcut badges on the first nine terminal tabs. Release the modifier to hide them. Any other key, mouse button, wheel event, window blur, or visibility change cancels the hints. Tab shortcuts work immediately, even without badges. The existing quick-switcher chord is unchanged.

## Zoom

| Shortcut | Action |
|----------|--------|
| `Cmd+=` (or `Cmd++`) | Zoom in (active terminal CLI/Chat view or code editor) |
| `Cmd+-` | Zoom out (active terminal CLI/Chat view or code editor) |
| `Cmd+0` | Reset zoom to default (active terminal CLI/Chat view or code editor) |
| `Cmd+Shift+=` (or `Cmd+Shift++`) | Zoom in all terminals |
| `Cmd+Shift+-` | Zoom out all terminals |
| `Cmd+Shift+0` | Reset zoom all terminals |

Font size range: 8px to 32px, step 2px per action. A session shares one zoom level between its CLI and Chat views. The all-terminals shortcuts also update Chat views.

## Split Panes

| Shortcut | Action |
|----------|--------|
| `Cmd+\` | Split vertically (side by side) |
| `Cmd+Alt+\` | Split horizontally (stacked) |
| `Alt+←` / `Alt+→` | Navigate panes (vertical split) |
| `Alt+↑` / `Alt+↓` | Navigate panes (horizontal split) |
| `Cmd+W` | Close active pane (collapses to single) |
| `Cmd+Shift+Enter` | Maximize / restore active pane |
| `Cmd+Alt+Enter` | Focus mode — hide sidebar, tab bar, and all side panels (keeps toolbar + status bar) |

## Panels

| Shortcut | Action |
|----------|--------|
| `Cmd+[` | Toggle sidebar |
| `Cmd+Shift+D` | Toggle Git Panel |
| `Cmd+Shift+M` | Toggle markdown panel |
| `Cmd+Alt+N` | Toggle Ideas panel |
| `Cmd+E` | Toggle file browser |
| `Cmd+O` | Open file… (picker) |
| `Cmd+N` | New file… (picker for name + location) |
| `Cmd+,` | Open settings |
| `Cmd+U` | Jump to next waiting terminal |
| `Cmd+?` | Toggle help panel |
| `Cmd+Shift+K` | Prompt library |
| `Cmd+K` | Clear scrollback |
| `Cmd+Shift+W` | Worktree Manager |
| `Cmd+J` | Task queue |
| `Cmd+Shift+E` | Toggle error log |
| `Cmd+Shift+I` | MCP servers popup (per-repo) |

Note: File browser and Markdown panels are mutually exclusive — opening one closes the other.

## Navigation

| Shortcut | Action |
|----------|--------|
| `Cmd+P` | Command palette |
| `Cmd+Shift+P` | Open Terminal Progress |
| `Cmd+Shift+A` | Activity dashboard |

## Git

| Shortcut | Action |
|----------|--------|
| `Cmd+Shift+D` | Git Panel (opens on last active tab) |
| `Cmd+G` | Git Panel — Branches tab |
| `Cmd+B` | Quick branch switch (fuzzy search) |

## Branches Panel (when panel is focused)

The panel is mouse-first — branch actions (create, delete, rename, merge, rebase, push, pull, fetch, checkout) live in the right-click context menu, the **+** New-branch button, and row double-click. Only list navigation is on the keyboard.

| Shortcut | Action |
|----------|--------|
| `↑` / `↓` | Navigate branch list |
| `/` | Focus the filter |
| `Escape` | Clear filter / deselect |
| double-click | Checkout branch |
| `Ctrl/Cmd+1–4` | Switch Git Panel tab (1=Changes, 2=Log, 3=Stashes, 4=Branches) |

## Quick Branch Switcher

| Shortcut | Action |
|----------|--------|
| Hold `Cmd+Ctrl` (macOS) or `Ctrl+Alt` (Win/Linux) | Show quick switcher overlay |
| `Cmd+Ctrl+1-9` | Switch to branch by index (in the Global Workspace: switch to tab N; only the tab hints show) |

While holding the modifier, all branches show numbered badges. Press a number to switch instantly.

## File Browser (when panel is focused)

| Shortcut | Action |
|----------|--------|
| `↑` / `↓` | Navigate files |
| `Enter` | Open file or enter directory |
| `Backspace` | Go to parent directory |
| `Cmd+C` | Copy selected file |
| `Cmd+X` | Cut selected file |
| `Cmd+V` | Paste file into current directory |

## Code Editor (when editor tab is focused)

| Shortcut | Action |
|----------|--------|
| `Cmd+S` | Save file |
| `Alt+Z` | Toggle line wrapping and remember it for this file kind |

## Ideas Panel (when textarea is focused)

| Shortcut | Action |
|----------|--------|
| `Enter` | Submit idea |
| `Shift+Enter` | Insert newline |

## Voice Dictation

| Shortcut | Action |
|----------|--------|
| Hold `F5` | Push-to-talk (configurable in Settings) |

Hold to record, release to transcribe and inject text into active terminal.

## Tab Context Menu (Right-click on tab)

| Action | Shortcut |
|--------|----------|
| Close Tab | `Cmd+W` |
| Close Other Tabs | — |
| Close Tabs to the Right | — |
| Copy Path | — (diff/editor/markdown file tabs) |
| Rename Tab | (double-click tab name) |
| Detach to Window | — |

While a context menu is open, pressing a menu item's shortcut chord (modifier + key, or Enter) fires that action immediately — no click needed. Modifier-only keys (Cmd, Shift, Ctrl, Alt) do not close the menu so multi-key chords can form; any other non-matching key closes the menu.

## Mouse Actions

| Action | Where | Effect |
|--------|-------|--------|
| Click | Sidebar branch | Switch to branch |
| Double-click | Sidebar branch name | Rename branch |
| Double-click | Tab name | Rename tab |
| Right-click | Tab | Context menu |
| Right-click | Sidebar branch | Branch context menu |
| Middle-click | Tab | Close tab |
| Drag | Tab | Reorder tabs |
| Drag | Sidebar right edge | Resize sidebar (300-500px); sidebar text grows 1px at 350, 400 and 450px |
| Click | PR badge / CI ring | Open PR detail popover |
| Click | Status bar CWD path | Copy path to clipboard |
| Click | Status bar panel buttons | Toggle Git/MD/FB/Ideas panels |
| Drag | Panel left edge | Resize right-side panel (200-800px) |
| Drag | Split pane divider | Resize split terminal panes |

## Action Names Reference (for keybindings.json)

| Action Name | Default Shortcut | Description |
|-------------|-----------------|-------------|
| `zoom-in` | `Cmd+=` | Zoom in |
| `zoom-out` | `Cmd+-` | Zoom out |
| `zoom-reset` | `Cmd+0` | Reset zoom |
| `zoom-in-all` | `Cmd+Shift+=` | Zoom in all terminals |
| `zoom-out-all` | `Cmd+Shift+-` | Zoom out all terminals |
| `zoom-reset-all` | `Cmd+Shift+0` | Reset zoom all terminals |
| `new-terminal` | `Cmd+T` | New terminal tab |
| `close-terminal` | `Cmd+W` | Close terminal/pane |
| `reopen-closed-tab` | `Cmd+Shift+T` | Reopen closed tab |
| `clear-terminal` | `Cmd+L` | Clear terminal |
| `refresh-terminal` | `Cmd+Shift+L` | Refresh terminal (fix glyphs) |
| `run-command` | `Cmd+Shift+R` | Run saved command |
| `edit-command` | _(unbound)_ | Edit and run command |
| `reload-tab` | `Cmd+R` | Reload the active web/preview tab |
| `split-vertical` | `Cmd+\` | Split vertically |
| `split-horizontal` | `Cmd+Alt+\` | Split horizontally |
| `prev-tab` | `Ctrl+Shift+Tab` | Previous tab |

Hold `Cmd` alone (`Ctrl` on Windows/Linux) for 450 ms to show shortcut badges on the first nine terminal tabs. Release the modifier to hide them. Any other key, mouse button, wheel event, window blur, or visibility change cancels the hints. Tab shortcuts work immediately, even without badges. The existing quick-switcher chord is unchanged.
| `next-tab` | `Ctrl+Tab` | Next tab |
| `focus-last-terminal` | `Cmd+Ctrl+Backspace` | Return to last terminal (toggle, across repos) |
| `history-back` | `Cmd+Alt+ArrowLeft` | Back to the previous terminal in the activation history (last 10, across repos) |
| `history-forward` | `Cmd+Alt+ArrowRight` | Forward to the next terminal in the activation history |
| `jump-waiting-terminal` | `Cmd+U` | Jump to the next terminal awaiting input (cycles, across repos) |
| `switch-tab-1..9` | `Cmd+1..9` | Switch to tab N |
| `toggle-sidebar` | `Cmd+[` | Toggle sidebar |
| `toggle-markdown` | `Cmd+Shift+M` | Toggle markdown panel |
| `toggle-notes` | `Cmd+Alt+N` | Toggle ideas panel |
| `open-file` | `Cmd+O` | Open file picker |
| `new-file` | `Cmd+N` | Create new file |
| `toggle-file-browser` | `Cmd+E` | Toggle file browser |
| `prompt-library` | `Cmd+Shift+K` | Prompt library |
| `toggle-settings` | `Cmd+,` | Open settings |
| `toggle-task-queue` | `Cmd+J` | Task queue |
| `toggle-help` | `Cmd+?` | Toggle help panel |
| `toggle-git-ops` | `Cmd+Shift+D` | Git Panel |
| `toggle-branches-tab` | `Cmd+G` | Git Panel — Branches tab |
| `worktree-manager` | `Cmd+Shift+W` | Worktree Manager panel |
| `quick-branch-switch` | `Cmd+B` | Quick branch switch |
| `find-in-terminal` | `Cmd+F` | Find in terminal |
| `command-palette` | `Cmd+P` | Command palette |
| `progress` | `Cmd+Shift+P` | Open Terminal Progress |
| `activity-dashboard` | `Cmd+Shift+A` | Activity dashboard |
| `toggle-error-log` | `Cmd+Shift+E` | Toggle error log |
| `toggle-mcp-popup` | `Cmd+Shift+I` | MCP servers popup (per-repo) |
| `switch-branch-1..9` | `Cmd+Ctrl+1..9` | Switch to branch N |
| `scroll-to-top` | `Cmd+Home` | Scroll to top |
| `scroll-to-bottom` | `Cmd+End` | Scroll to bottom |
| `scroll-page-up` | `Shift+PageUp` | Scroll page up |
| `scroll-page-down` | `Shift+PageDown` | Scroll page down |
| `zoom-pane` | `Cmd+Shift+Enter` | Maximize/restore pane |
| `toggle-focus-mode` | `Cmd+Alt+Enter` | Focus mode — hide sidebar/tab bar/panels |
| `toggle-file-browser-content-search` | `Cmd+Shift+F` | File content search |
| `toggle-diff-scroll` | `Cmd+Shift+G` | Diff scroll view |
| `toggle-global-workspace` | `Cmd+Shift+X` | Toggle global workspace |
| `toggle-ai-chat` | `Cmd+Alt+A` | Toggle the ego-backed AI Chat panel. Listed only while **Experimental features** is on |
| `clear-scrollback` | `Cmd+K` | Clear scrollback |
| `open-folder` | `Cmd+Shift+O` | Open folder picker |
| `open-path` | `Cmd+Alt+O` | Open path… |
| `open-secondary-window` | — | Open secondary window |
| `command-overview` | — | Command overview |
| `open-tasks` | — | Open Tasks (listed only while a work-item provider plugin is loaded) |
| `toggle-outline` | `Cmd+Alt+L` | Toggle outline panel |
| `toggle-compose-panel` | `Cmd+I` | Toggle compose panel |
| `detach-activity-dashboard` | — | Open Activity Dashboard in separate window |
| `toggle-tunnels` | — | SSH Tunnels panel |
| `process-manager` | — | Process Manager |
| `open-generators` | — | Open generators |
| `show-remote-qr` | — | QR for Remote Mobile Connection |
| `block-fold-toggle` | `Cmd+Shift+.` | Toggle block fold |
| `block-prev` | `Cmd+Shift+Up` | Previous command block |
| `block-next` | `Cmd+Shift+Down` | Next command block |
| `block-search-toggle` | `Cmd+Shift+B` | Search in block |
| `answers-only` | `Cmd+Alt+R` | Show every prompt of the session with its 💬 answers (turns without an answer are left out); press again to return to the live terminal |
| `toggle-hands-free` | `Cmd+Alt+V` | Start/stop hands-free conversation with the active terminal |

## Plugin action discovery

A connected frontend publishes its registered plugin command IDs for the shared settings owner. Explicit keybinding-choice refresh includes those IDs and built-in actions. Unloading removes the plugin IDs from later refreshes. Before any frontend publication, plugin choices report unavailable rather than an empty successful list. Publication does not change shortcut overrides.
