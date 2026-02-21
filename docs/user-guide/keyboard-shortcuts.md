# Keyboard Shortcuts

All shortcuts use `Cmd` on macOS and `Ctrl` on Windows/Linux unless noted.

## Terminal Operations

| Shortcut | Action |
|----------|--------|
| `Cmd+T` | New terminal tab |
| `Cmd+W` | Close tab (or close active pane in split mode) |
| `Cmd+Shift+T` | Reopen last closed tab |
| `Cmd+R` | Run saved command |
| `Cmd+Shift+R` | Edit and run command |
| `Cmd+L` | Clear terminal |
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
| `Cmd+D` | Toggle diff panel |
| `Cmd+M` | Toggle markdown panel |
| `Cmd+N` | Toggle Ideas panel |
| `Cmd+E` | Toggle file browser |
| `Cmd+,` | Open settings |
| `Cmd+?` | Toggle help panel |
| `Cmd+K` | Prompt library |
| `Cmd+J` | Task queue |

Note: File browser, Markdown, and Diff panels are mutually exclusive — opening one closes any other that is open.

## Git & Lazygit

| Shortcut | Action |
|----------|--------|
| `Cmd+G` | Open lazygit in terminal |
| `Cmd+Shift+G` | Git operations panel |
| `Cmd+Shift+L` | Lazygit in split pane |

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
| Rename Tab | (double-click tab name) |

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
| Click | Status bar panel buttons | Toggle Diff/MD/FB/Ideas panels |
| Drag | Panel left edge | Resize right-side panel (200-800px) |
| Drag | Split pane divider | Resize split terminal panes |
