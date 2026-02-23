# Terminal Features

## Terminal Sessions

Each terminal tab runs an independent PTY (pseudo-terminal) session with your shell. Up to 50 concurrent sessions.

### Creating Terminals

- **Cmd+T** — New terminal for the active branch
- **`+` button** on sidebar branch — Add terminal to specific branch
- **`+` button** on tab bar — New terminal tab

New terminals inherit the working directory from the active branch's worktree path.

### Terminal Lifecycle

Terminals are **never unmounted** from the DOM. When you switch branches or tabs, terminals are hidden but remain alive. Switch back and your process, scroll position, and output are exactly as you left them.

### Closing Terminals

- **Cmd+W** — Close active tab (confirmation dialog if enabled in settings)
- **Middle-click** on tab — Close tab
- **Right-click → Close Tab** — Context menu
- **Right-click → Close Other Tabs** — Close all except this one
- **Right-click → Close Tabs to the Right** — Close tabs after this one

### Reopening Closed Tabs

- **Cmd+Shift+T** — Reopen the last closed tab
- Last 10 closed tabs are remembered with their name, font size, and working directory
- Reopened tabs start a fresh shell session in the original directory

## Tab Management

### Tab Names

- Default naming: "Terminal 1", "Terminal 2", etc.
- **Double-click** a tab to rename it (inline editing)
- Press **Enter** to confirm, **Escape** to cancel
- Custom names persist through the session

### Tab Reordering

Drag tabs to reorder them. Visual drop indicators show where the tab will land.

### Tab Indicators

| Indicator | Meaning |
|-----------|---------|
| Solid dot | Terminal has background activity |
| Question icon | Agent is asking a question |
| Error icon | Error detected in output |
| Progress bar | Operation in progress (OSC 9;4) |

### Tab Shortcuts

Hover a tab to see its shortcut badge: "Terminal N (Cmd+N)". Use `Cmd+1` through `Cmd+9` to jump directly.

## Zoom

Per-terminal font size control:

| Action | Shortcut | Effect |
|--------|----------|--------|
| Zoom in | `Cmd+=` | +2px font size |
| Zoom out | `Cmd+-` | -2px font size |
| Reset | `Cmd+0` | Back to default size |

Range: 8px to 32px. Each terminal has its own zoom level. The current zoom is shown in the status bar.

## Split Panes

Split the terminal area into two panes:

### Creating Splits

- **Cmd+\\** — Split vertically (side by side)
- **Cmd+Alt+\\** — Split horizontally (stacked)

The new pane opens a fresh terminal in the same working directory. Maximum 2 panes at a time.

### Navigating Split Panes

- **Alt+←/→** — Switch between vertical panes
- **Alt+↑/↓** — Switch between horizontal panes
- The active pane receives keyboard input

### Resizing Split Panes

Drag the divider between the two panes to adjust the split ratio. Both terminals re-fit automatically when you release.

### Closing Split Panes

- **Cmd+W** closes the active pane and collapses back to a single pane
- The surviving pane automatically receives focus

### Split Layout Persistence

Split layouts are stored per branch. When you switch branches and come back, your split configuration is restored.

## Detachable Tabs

Float any terminal into its own OS window:

1. **Right-click** a tab → **Detach to Window**
2. The terminal opens in an independent floating window
3. The PTY session stays alive — the floating window reconnects to the same session

When you close the floating window, the tab automatically returns to the main window.

**Requirements:** The tab must have an active PTY session. Tabs without a session (e.g., just created but not connected) cannot be detached.

## Find in Terminal

Search within terminal output with `Cmd+F`:

1. Press `Cmd+F` — a search overlay appears at the top of the active terminal pane
2. Type your search query — matches highlight as you type (yellow for all matches, orange for active match)
3. Navigate matches:
   - `Enter` or `Cmd+G` — Next match
   - `Shift+Enter` or `Cmd+Shift+G` — Previous match
4. Toggle search options: **Case sensitive**, **Whole word**, **Regex**
5. Match counter shows "N of M" results
6. Press `Escape` to close the search and refocus the terminal

Uses `@xterm/addon-search` for native integration with the terminal buffer.

## Copy & Paste

- **Copy:** Select text in the terminal, then `Cmd+C`
- **Paste:** `Cmd+V` writes clipboard content to the active terminal

## Clear Terminal

`Cmd+L` clears the terminal display. Running processes are unaffected.

## Clickable File Paths

File paths appearing in terminal output are automatically detected and become clickable links. Hover over a path to see the link underline, then click to open it.

- `.md` / `.mdx` files open in the Markdown viewer panel
- All other code files open in your configured IDE, at the line number if a `:line` or `:line:col` suffix is present

Paths are validated against the filesystem before becoming clickable — only real files show as links.

Recognized extensions include: `.rs`, `.ts`, `.tsx`, `.js`, `.jsx`, `.py`, `.go`, `.java`, `.css`, `.html`, `.json`, `.yaml`, `.toml`, `.sql`, and many more.

## Plan File Detection

When an AI agent emits a plan file path (e.g., `PLAN.md`), a button appears in the toolbar showing the file name. Click it to open the plan — Markdown files open in the viewer panel, others open in the IDE. Click the dismiss button (x) to hide it.

## Working with AI Agents

TUICommander detects rate limits, prompts, and status messages from AI agents:

- **Rate limit detection** — Recognizes rate limit messages from Claude, Aider, Gemini, OpenCode, Codex
- **Prompt interception** — Detects when agents ask yes/no questions or multiple choice
- **Status tracking** — Parses token usage and timing from agent output
- **Progress indicators** — Shows progress bars for long-running operations

When an agent asks a question, the tab indicator changes and a notification sound plays (if enabled).
