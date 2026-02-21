# File Browser & Code Editor

## File Browser Panel

Toggle with `Cmd+E` or the folder icon in the status bar. The file browser shows the directory tree of the active repository.

Note: The file browser, Markdown, and Diff panels are mutually exclusive — opening one closes any other that is open.

### Navigation

- **Arrow keys** (`Up` / `Down`) — Navigate files
- **Enter** — Open file or enter directory
- **Backspace** — Go to parent directory
- **Breadcrumb bar** — Click any path segment to jump directly to that directory. Click the root `/` to return to the repo root.

### Search Filter

Type in the search input at the top to filter files. Supports `*` and `**` glob wildcards (e.g., `*.ts` or `src/**/*.test.ts`).

### Git Status Indicators

The panel header shows a legend for git status colors:

| Color | Meaning |
|-------|---------|
| Orange | Modified (unstaged) |
| Green | Staged for commit |
| Blue | New (untracked) |

File and directory names inherit these colors based on their git status.

### Context Menu (Right-click)

Right-click any file to see available actions:

| Action | Shortcut | Notes |
|--------|----------|-------|
| Copy | `Cmd+C` | Files only (not directories) |
| Cut | `Cmd+X` | Files only (not directories) |
| Paste | `Cmd+V` | Pastes into current directory; disabled when clipboard is empty |
| Rename... | — | Opens a rename dialog |
| Delete | — | Files only (safety measure excludes directories) |
| Add to .gitignore | — | Disabled if already ignored |

The keyboard shortcuts (`Cmd+C`, `Cmd+X`, `Cmd+V`) also work when the file browser has focus, without needing the context menu.

### Panel Resize

Drag the left edge of the panel to resize it. Range: 200-800 px.

## Code Editor

Clicking a file in the file browser opens it in an in-app code editor tab (in the main tab area, alongside terminal tabs).

### Features

- **Syntax highlighting** — Auto-detected from file extension. Disabled for files larger than 500 KB.
- **Line numbers**, bracket matching, active line highlight, indentation support
- **Save** — `Cmd+S` saves the file when the editor tab is focused

### Read-Only Mode

Click the padlock icon in the editor tab header to toggle read-only mode. When locked, the file cannot be edited.

### Unsaved Changes

An unsaved-changes dot appears in both the tab bar and the editor header when the file has been modified but not saved.

### Disk Conflict Detection

If the file changes on disk while you have unsaved edits in the editor, a conflict banner appears with two options:

- **Reload** — Discard local edits and load the disk version
- **Keep mine** — Dismiss the banner; the next save overwrites the disk version

When the editor has no unsaved changes, files reload silently when they change on disk.
