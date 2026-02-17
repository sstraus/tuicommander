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

### Closing Split Panes

- **Cmd+W** closes the active pane and collapses back to a single pane
- The surviving pane automatically receives focus

### Split Layout Persistence

Split layouts are stored per branch. When you switch branches and come back, your split configuration is restored.

## Copy & Paste

- **Copy:** Select text in the terminal, then `Cmd+C`
- **Paste:** `Cmd+V` writes clipboard content to the active terminal

## Clear Terminal

`Cmd+L` clears the terminal display. Running processes are unaffected.

## Working with AI Agents

TUI Commander detects rate limits, prompts, and status messages from AI agents:

- **Rate limit detection** — Recognizes rate limit messages from Claude, Aider, Gemini, OpenCode, Codex
- **Prompt interception** — Detects when agents ask yes/no questions or multiple choice
- **Status tracking** — Parses token usage and timing from agent output
- **Progress indicators** — Shows progress bars for long-running operations

When an agent asks a question, the tab indicator changes and a notification sound plays (if enabled).
