# Command Palette & Activity Dashboard

## Command Palette

Open with `Cmd+Shift+P`. The command palette gives you fuzzy-search access to every action in the app.

### How It Works

1. Press `Cmd+Shift+P` — the palette appears with a search input
2. Type to filter actions by name (fuzzy matching)
3. Navigate with `↑/↓` arrow keys
4. Press `Enter` to execute the selected action
5. Press `Escape` to close

### What You See

Each row shows:

- **Action label** — What the action does (e.g., "Toggle Diff Panel")
- **Category badge** — Color-coded category (Terminal, Panel, Git, etc.)
- **Keybinding hint** — The keyboard shortcut, if one is assigned

### Recency Ranking

Recently used actions float to the top. The palette learns your workflow — actions you use frequently appear first even before you start typing.

### Powered by the Action Registry

The palette is auto-populated from `actionRegistry.ts`. When new actions are added to the codebase, they appear in the palette automatically — no manual configuration needed.

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
