# Plugins

TUICommander has an Obsidian-style plugin system. Plugins can watch terminal output, push notifications to the Activity Center, render markdown panels, control PTY sessions, and more.

## Installing Plugins

### From the Community Registry

1. Open **Settings** (`Cmd+,`) → **Plugins** tab → **Browse**
2. Browse available plugins — each shows name, description, and author
3. Click **Install** on any plugin
4. The plugin is downloaded and activated immediately

An "Update available" badge appears when a newer version exists in the registry.

### From a ZIP File

1. Open **Settings** → **Plugins** → **Installed**
2. Click **Install from file...**
3. Select a `.zip` archive containing the plugin

### Via Deep Link

Click a link like `tuic://install-plugin?url=https://example.com/plugin.zip` — TUICommander shows a confirmation dialog, then downloads and installs the plugin. Only HTTPS URLs are accepted.

### Manual Installation

Copy the plugin directory to:

- **macOS:** `~/Library/Application Support/com.tuic.commander/plugins/my-plugin/`
- **Linux:** `~/.config/tuicommander/plugins/my-plugin/`
- **Windows:** `%APPDATA%/com.tuic.commander/plugins/my-plugin/`

A plugin directory contains at minimum `manifest.json` and `main.js`.

## Managing Plugins

### Settings → Plugins → Installed

The Installed tab lists all plugins (built-in and external):

- **Toggle switch** — Enable or disable a plugin. Disabled plugins are not loaded but remain installed.
- **Logs** — Click to expand the plugin's log viewer. Shows recent activity and errors (500-entry ring buffer).
- **Uninstall** — Remove the plugin directory (confirmation required). Built-in plugins cannot be uninstalled.

Error count badges appear on plugins that have logged errors.

### Built-in Plugins

TUICommander ships with built-in plugins (e.g., Plan Tracker). These show a "Built-in" badge in the list. They can be disabled but not uninstalled.

## How Plugins Work

Plugins interact with the app through a **PluginHost API** organized in 4 capability tiers:

| Tier | Access | Examples |
|------|--------|----------|
| **1** | Always available | Watch terminal output, add Activity Center items, provide markdown content |
| **2** | Always available | Read repository list, active branch, terminal sessions (read-only) |
| **3** | Requires capability | Write to terminals (`pty:write`), open markdown panels (`ui:markdown`), play sounds (`ui:sound`), read/list/watch files (`fs:read`, `fs:list`, `fs:watch`) |
| **4** | Requires capability | Invoke whitelisted Tauri commands (`invoke:read_file`, `invoke:list_markdown_files`) |

Capabilities are declared in the plugin's `manifest.json`. A plugin without `pty:write` cannot send input to your terminals.

## Activity Center

The toolbar bell icon is the Activity Center. Plugins contribute sections and items here:

- **Sections** — Grouped headings (e.g., "ACTIVE PLAN", "CI STATUS")
- **Items** — Individual notifications with icon, title, subtitle
- **Actions** — Click an item to open its detail (usually a markdown panel), or dismiss it

The bell shows a count badge when there are active items.

## Hot Reload

When you edit a plugin's files, TUICommander detects the change and automatically reloads the plugin — no restart needed. Save `main.js` and see changes in seconds.

## Example Plugins

TUICommander ships with example plugins in `examples/plugins/`:

| Plugin | What it does |
|--------|-------------|
| `hello-world` | Minimal example — watches terminal output, adds Activity Center items |
| `auto-confirm` | Auto-responds to Y/N prompts in terminal |
| `ci-notifier` | Sound notifications and markdown panels for CI events |
| `repo-dashboard` | Reads repo state, generates dynamic markdown summaries |
| `report-watcher` | Watches terminal for generated report files, shows them in Activity Center |

## Writing Your Own Plugin

See the [Plugin Authoring Guide](../plugins.md) for the full API reference, manifest format, capability details, structured event types, and testing patterns.

## Troubleshooting

| Problem | Fix |
|---------|-----|
| Plugin not appearing | Check that `manifest.json` exists and `id` matches the directory name |
| "Requires app version X.Y.Z" | Update TUICommander or lower `minAppVersion` in the manifest |
| "Requires capability X" | Add the capability to the `capabilities` array in `manifest.json` |
| Changes not taking effect | Save the file again to trigger hot reload, or restart the app |
| Plugin errors | Check Settings → Plugins → Logs for the plugin's error log |
