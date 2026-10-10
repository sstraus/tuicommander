# Command Palette & Activity Dashboard

## Command Palette

In the desktop app, open the palette with `Cmd+P` (macOS) / `Ctrl+P` (Windows/Linux). In browser mode, use the magnifying-glass button at the right of the toolbar; this remains reliable when the browser reserves or swallows the shortcut. The command palette gives you fast access to the actions available in the current client mode.

### How It Works

1. Press `Cmd/Ctrl+P` in the desktop app, or click the browser toolbar button — the palette opens with a search input focused
2. Type to filter actions by name or category (substring match, case-insensitive)
3. Navigate with `↑` / `↓` arrow keys
4. Press `Enter` to execute the selected action
5. Press `Escape` or click outside the palette to close

### What You See

Each row shows:

- **Action label** — What the action does (e.g., "Git panel", "New terminal tab")
- **Category badge** — The action's category (Terminal, Panels, Git, Navigation, Zoom, Split Panes, File Browser)
- **Keybinding hint** — The assigned keyboard shortcut, if any

### Search Behavior

Filtering matches against the action label and its category simultaneously. Typing "git" surfaces all Git actions; typing "panel" surfaces all panel-toggle actions regardless of category. There is no minimum query length — results update on every keystroke.

### Recency Ranking

When the search box is empty, recently used actions float to the top, ordered by most recent first. Remaining actions are sorted alphabetically. The ranking persists across palette opens so your most-used commands are always one keystroke away.

### Mouse Support

Hovering over a row highlights it (same as keyboard selection). Clicking a row executes the action immediately.

### Project Progress

Run **Open Terminal Progress** to open the active terminal's journal in a dialog.
Use the selector to inspect another terminal or the whole repository.
This action is available in desktop and browser mode. The same dialog opens from
the always available **Terminal Progress** entry in the notification bell, or
with `Cmd/Ctrl+Shift+P`. The bell badge still counts unread updates.

### Powered by the Action Registry

The palette is auto-populated from `actionRegistry.ts`. The desktop app exposes the complete registered action set. Browser mode uses an explicit allowlist and omits actions that require native dialogs, detached OS windows, the native updater, desktop-only MCP configuration, or user-plugin management. This fail-closed policy prevents a newly added desktop action from appearing in the browser before it has a working web or HTTP implementation. Browser-capable plugin actions appear alongside built-in ones.

### Search Modes

The command palette supports three search prefixes:

| Prefix | Mode | Description |
|--------|------|-------------|
| `!` | Filename search | Search files by name (min 1 char) |
| `?` | Content search | Search inside file contents (min 3 chars) |
| `~` | Terminal search | Search across all open terminal buffers (min 3 chars) |

- Leading spaces after the prefix are ignored (`~ error` = `~error`)
- File results show as a flat list with file path
- Content matches include line number and highlighted match text
- Terminal matches include terminal name, line number, and highlighted match text
- Press `Enter` or click to open the file in an editor tab, or navigate to the terminal match
- Terminal match navigation switches to the correct tab/pane and scrolls to the matched line
- Delete the prefix to return to command mode
- Search runs with a 300ms debounce
- Footer shows `!`, `?`, and `~` hints when in command mode
- In browser mode, filename and content searches use the same backend over HTTP. Each content search carries a random correlation ID local to the requesting page, so another window or panel cannot inject results into the palette

If no repository is selected, file/content modes show "No repository selected".
If no terminals are open, terminal mode shows "No terminals open".

### Discoverable Search Commands

You can also access search modes via explicit commands in the palette:

| Command | Action |
|---------|--------|
| Search Terminals | Opens palette with `~ ` prefix |
| Search Files | Opens palette with `! ` prefix |
| Search in File Contents | Opens palette with `? ` prefix |

These appear as regular commands in the palette — type "Search" to find them.

---

## Activity Dashboard

Open with `Cmd+Shift+A`. A real-time overview of all your terminal sessions.
The overlay stays compact in the main window and fits narrow windows; detaching
it keeps the same list in a separate window.

### What You See

A compact list where each row shows:

| Column | Description |
|--------|-------------|
| **Terminal name** | The tab name |
| **Agent type** | Detected agent (Claude, Aider, etc.) with brand icon |
| **Status** | Current state with color indicator |
| **Last activity** | Relative timestamp ("2s ago", "1m ago") — auto-refreshes |

Spawned agent rows include a robot-head marker. Hover it to see the parent
terminal's name, or "external agent" when its tab is unavailable.

### Status Colors

| Color | Meaning |
|-------|---------|
| Green | Agent is actively working |
| Yellow | Agent is waiting for input |
| Red | Agent is rate-limited (with countdown) |
| Gray | Terminal is idle |

A terminal with a ready input composer is shown as idle even if the agent left a
long-lived background terminal, such as a development server, running.

### Interactions

- **Click any row** — Switches to that terminal and closes the dashboard
- **Rate limit indicators** — Show countdown timers for when the limit expires

The dashboard is useful when running many agents in parallel — you can spot at a glance which ones need attention, which are stalled, and which are making progress.

### Automations

**Automations** opens scheduled agent runs on the connected app's machine.
The entry is available in desktop and browser mode. See the
[Automations guide](automations.md). It requires the Automations backend API.
