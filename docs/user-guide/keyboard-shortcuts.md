# Keyboard Shortcuts

All shortcuts use `Cmd` on macOS and `Ctrl` on Windows/Linux unless noted.

## Customizing Keybindings

### From the UI

Open **Help > Keyboard Shortcuts** (or `Cmd+?` → Keyboard Shortcuts). Click the pencil icon next to any shortcut to enter recording mode, then press your new key combination. The app warns you if the combo is already used by another action. Overridden shortcuts are highlighted in accent color with a reset icon to revert individually. A "Reset all to defaults" button is at the bottom.

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

## Terminal Operations

| Shortcut | Action |
|----------|--------|
| `Cmd+T` | New terminal tab |
| `Cmd+W` | Close tab (or close active pane in split mode) |
| `Cmd+Shift+T` | Reopen last closed tab |
| `Cmd+R` | Run saved command |
| `Cmd+Shift+R` | Edit and run command |
| `Cmd+L` | Clear terminal |
| `Cmd+F` | Find in terminal / diff tab |
| `Cmd+G` | Git Panel — Branches tab (or Find next match when search is open) |
| `Enter` | Find next match (when search is open) |
| `Cmd+Shift+G` / `Shift+Enter` | Find previous match (when search is open) |
| `Escape` | Close search overlay |
| `Cmd+C` | Copy selection |
| `Cmd+V` | Paste to terminal |

## Tab Navigation

| Shortcut | Action |
|----------|--------|
| `Cmd+1` through `Cmd+9` | Switch to tab by number |
| `Cmd+Shift+]` | Next tab |
| `Cmd+Shift+[` | Previous tab |

## Zoom

| Shortcut | Action |
|----------|--------|
| `Cmd+=` (or `Cmd++`) | Zoom in (increase font size) |
| `Cmd+-` | Zoom out (decrease font size) |
| `Cmd+0` | Reset zoom to default |

Font size range: 8px to 32px, step 2px per action.

## Split Panes

| Shortcut | Action |
|----------|--------|
| `Cmd+\` | Split vertically (side by side) |
| `Cmd+Alt+\` | Split horizontally (stacked) |
| `Alt+←` / `Alt+→` | Navigate panes (vertical split) |
| `Alt+↑` / `Alt+↓` | Navigate panes (horizontal split) |
| `Cmd+W` | Close active pane (collapses to single) |

## Panels

| Shortcut | Action |
|----------|--------|
| `Cmd+[` | Toggle sidebar |
| `Cmd+Shift+D` | Toggle Git Panel |
| `Cmd+M` | Toggle markdown panel |
| `Cmd+N` | Toggle Ideas panel |
| `Cmd+E` | Toggle file browser |
| `Cmd+,` | Open settings |
| `Cmd+?` | Toggle help panel |
| `Cmd+K` | Prompt library |
| `Cmd+Shift+W` | Worktree Manager |
| `Cmd+J` | Task queue |
| `Cmd+P` | Toggle plan panel |
| `Cmd+Shift+E` | Toggle error log |

Note: File browser and Markdown panels are mutually exclusive — opening one closes the other.

## Navigation

| Shortcut | Action |
|----------|--------|
| `Cmd+Shift+P` | Command palette |
| `Cmd+Shift+A` | Activity dashboard |

## Git

| Shortcut | Action |
|----------|--------|
| `Cmd+Shift+D` | Git Panel (opens on last active tab) |
| `Cmd+G` | Git Panel — Branches tab |
| `Cmd+B` | Quick branch switch (fuzzy search) |

## Branches Panel (when panel is focused)

| Shortcut | Action |
|----------|--------|
| `↑` / `↓` | Navigate branch list |
| `Enter` / double-click | Checkout selected branch |
| `n` | Create new branch (inline form) |
| `d` | Delete branch (safe; hold to force) |
| `R` | Rename branch (inline edit) |
| `M` | Merge selected branch into current |
| `r` | Rebase current onto selected branch |
| `P` | Push branch (auto-sets upstream if missing) |
| `p` | Pull current branch |
| `f` | Fetch all remotes |
| `Ctrl/Cmd+1–4` | Switch Git Panel tab (1=Changes, 2=Log, 3=Stashes, 4=Branches) |

## Quick Branch Switcher

| Shortcut | Action |
|----------|--------|
| Hold `Cmd+Ctrl` (macOS) or `Ctrl+Alt` (Win/Linux) | Show quick switcher overlay |
| `Cmd+Ctrl+1-9` | Switch to branch by index |

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
| Drag | Sidebar right edge | Resize sidebar (200-500px) |
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
| `new-terminal` | `Cmd+T` | New terminal tab |
| `close-terminal` | `Cmd+W` | Close terminal/pane |
| `reopen-closed-tab` | `Cmd+Shift+T` | Reopen closed tab |
| `clear-terminal` | `Cmd+L` | Clear terminal |
| `run-command` | `Cmd+R` | Run saved command |
| `edit-command` | `Cmd+Shift+R` | Edit and run command |
| `split-vertical` | `Cmd+\` | Split vertically |
| `split-horizontal` | `Cmd+Alt+\` | Split horizontally |
| `prev-tab` | `Cmd+Shift+[` | Previous tab |
| `next-tab` | `Cmd+Shift+]` | Next tab |
| `switch-tab-1..9` | `Cmd+1..9` | Switch to tab N |
| `toggle-sidebar` | `Cmd+[` | Toggle sidebar |
| `toggle-markdown` | `Cmd+M` | Toggle markdown panel |
| `toggle-notes` | `Cmd+N` | Toggle ideas panel |
| `toggle-file-browser` | `Cmd+E` | Toggle file browser |
| `toggle-prompt-library` | `Cmd+K` | Prompt library |
| `toggle-settings` | `Cmd+,` | Open settings |
| `toggle-task-queue` | `Cmd+J` | Task queue |
| `toggle-help` | `Cmd+?` | Toggle help panel |
| `toggle-git-ops` | `Cmd+Shift+D` | Git Panel |
| `toggle-git-branches` | `Cmd+G` | Git Panel — Branches tab |
| `worktree-manager` | `Cmd+Shift+W` | Worktree Manager panel |
| `quick-branch-switch` | `Cmd+B` | Quick branch switch |
| `find-in-terminal` | `Cmd+F` | Find in terminal |
| `command-palette` | `Cmd+Shift+P` | Command palette |
| `activity-dashboard` | `Cmd+Shift+A` | Activity dashboard |
| `toggle-error-log` | `Cmd+Shift+E` | Toggle error log |
| `toggle-plan` | `Cmd+P` | Toggle plan panel |
| `switch-branch-1..9` | `Cmd+Ctrl+1..9` | Switch to branch N |
