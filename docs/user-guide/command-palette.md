# Command Palette & Activity Dashboard

## Command Palette

Open with `Cmd+Shift+P` (macOS) / `Ctrl+Shift+P` (Windows/Linux). The command palette gives you fast keyboard access to every registered action in the app.

### How It Works

1. Press `Cmd+Shift+P` — the palette opens with a search input focused
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

### Powered by the Action Registry

The palette is auto-populated from `actionRegistry.ts`. Every action registered there — with its label, category, and keybinding — appears in the palette automatically. No manual configuration is needed, and plugin-contributed actions appear alongside built-in ones.

---

## Activity Dashboard

Open with `Cmd+Shift+A`. A real-time overview of all your terminal sessions.

### What You See

A compact list where each row shows:

| Column | Description |
|--------|-------------|
| **Terminal name** | The tab name |
| **Agent type** | Detected agent (Claude, Aider, etc.) with brand icon |
| **Status** | Current state with color indicator |
| **Last activity** | Relative timestamp ("2s ago", "1m ago") — auto-refreshes |

### Status Colors

| Color | Meaning |
|-------|---------|
| Green | Agent is actively working |
| Yellow | Agent is waiting for input |
| Red | Agent is rate-limited (with countdown) |
| Gray | Terminal is idle |

### Interactions

- **Click any row** — Switches to that terminal and closes the dashboard
- **Rate limit indicators** — Show countdown timers for when the limit expires

The dashboard is useful when running many agents in parallel — you can spot at a glance which ones need attention, which are stalled, and which are making progress.
