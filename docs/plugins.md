# Plugin Authoring Guide

TUI Commander uses an Obsidian-style plugin system. Plugins extend the Activity Center (bell dropdown), watch terminal output, and interact with app state. Plugins can be **built-in** (compiled with the app) or **external** (loaded at runtime from the user's plugins directory).

## Quick Start: External Plugin

1. Create a directory: `~/.config/tuicommander/plugins/my-plugin/`
2. Create `manifest.json`:

```json
{
  "id": "my-plugin",
  "name": "My Plugin",
  "version": "1.0.0",
  "minAppVersion": "0.3.0",
  "main": "main.js"
}
```

3. Create `main.js` (ES module with default export):

```javascript
const PLUGIN_ID = "my-plugin";

export default {
  id: PLUGIN_ID,
  onload(host) {
    host.registerSection({
      id: "my-section",
      label: "MY SECTION",
      priority: 30,
      canDismissAll: false,
    });

    host.registerOutputWatcher({
      pattern: /hello (\w+)/,
      onMatch(match, sessionId) {
        host.addItem({
          id: `hello:${match[1]}`,
          pluginId: PLUGIN_ID,
          sectionId: "my-section",
          title: `Hello ${match[1]}`,
          icon: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><circle cx="8" cy="8" r="6"/></svg>',
          dismissible: true,
        });
      },
    });
  },
  onunload() {},
};
```

4. Restart the app (or save the file — hot reload will pick it up).

## Architecture

```
PTY output ──> pluginRegistry.processRawOutput()
                  |
                  +-- LineBuffer (reassemble lines)
                  +-- stripAnsi (clean ANSI codes)
                  +-- dispatchLine() --> OutputWatcher.onMatch()
                                              |
                                              +-- host.addItem() --> Activity Center bell
                                                                           |
                                                              user clicks item
                                                                           |
                                              markdownProviderRegistry.resolve(contentUri)
                                                                           |
                                                              MarkdownTab renders content

Tauri OutputParser --> pluginRegistry.dispatchStructuredEvent(type, payload, sessionId)
                            |
                            +-- structuredEventHandler(payload, sessionId)
```

## Plugin Lifecycle

1. **Discovery** — Rust `list_user_plugins` scans `~/.config/tuicommander/plugins/` for `manifest.json` files
2. **Validation** — Frontend validates manifest fields and `minAppVersion`
3. **Import** — `import("plugin://my-plugin/main.js")` loads the module via the custom URI protocol
4. **Module check** — Default export must have `id`, `onload`, `onunload`
5. **Register** — `pluginRegistry.register(plugin, capabilities)` calls `plugin.onload(host)`
6. **Active** — Plugin receives PTY lines, structured events, and can use the PluginHost API
7. **Hot reload** — File changes emit `plugin-changed` events; the plugin is unregistered and re-imported
8. **Unload** — `plugin.onunload()` is called, then all registrations are auto-disposed

### Crash Safety

Every boundary is wrapped in try/catch:
- `import()` — syntax errors or missing exports are caught
- Module validation — missing `id`, `onload`, or `onunload` logs an error and skips the plugin
- `plugin.onload()` — if it throws, partial registrations are cleaned up automatically
- Watcher/handler dispatch — exceptions are caught and logged, other plugins continue

A broken plugin produces a console error and is skipped. The app always continues.

## Manifest Reference

File: `~/.config/tuicommander/plugins/{id}/manifest.json`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Must match the directory name |
| `name` | string | yes | Human-readable display name |
| `version` | string | yes | Plugin semver (e.g. `"1.0.0"`) |
| `minAppVersion` | string | yes | Minimum TUI Commander version required |
| `main` | string | yes | Entry point filename (e.g. `"main.js"`) |
| `description` | string | no | Short description |
| `author` | string | no | Author name |
| `capabilities` | string[] | no | Tier 3/4 capabilities needed (defaults to `[]`) |

### Validation Rules

- `id` must match the directory name exactly
- `id` must not be empty
- `main` must not contain path separators or `..`
- All `capabilities` must be known strings (see Capabilities section)
- `minAppVersion` must be <= the current app version (semver comparison)

## Plugin Interface

```typescript
interface TuiPlugin {
  id: string;
  onload(host: PluginHost): void;
  onunload(): void;
}
```

The `onload` function receives a `PluginHost` object — this is your entire API surface. External plugins cannot import app internals; everything goes through `host`.

## PluginHost API Reference

### Tier 0: Logging (always available)

#### host.log(level, message, data?) -> void

Write to the plugin's log ring buffer (visible in Settings > Plugins > Logs).

```typescript
host.log("info", "Plugin initialized");
host.log("error", "Failed to process", { code: 404 });
```

Levels: `"debug"`, `"info"`, `"warn"`, `"error"`.

### Tier 1: Activity Center + Watchers + Providers (always available)

#### host.registerSection(section) -> Disposable

Adds a section heading to the Activity Center dropdown.

```typescript
host.registerSection({
  id: "my-section",        // Must match sectionId in addItem()
  label: "MY SECTION",     // Displayed as section header
  priority: 30,            // Lower number = higher position
  canDismissAll: false,     // Show "Dismiss All" button?
});
```

#### host.registerOutputWatcher(watcher) -> Disposable

Watches every PTY output line (after ANSI stripping and line reassembly).

```typescript
host.registerOutputWatcher({
  pattern: /Deployed: (\S+) to (\S+)/,
  onMatch(match, sessionId) {
    // match[0] = full match, match[1] = first capture group, etc.
    // sessionId = the PTY session that produced the line
    host.addItem({ ... });
  },
});
```

**Rules:**
- `onMatch` must be synchronous and fast (< 1ms) — it's in the PTY hot path
- `pattern.lastIndex` is reset before each test (safe to use global flag, but unnecessary)
- Input is ANSI-stripped but may contain Unicode (checkmarks, arrows, emoji)
- Arguments are positional: `onMatch(match, sessionId)` — NOT destructured

#### host.registerStructuredEventHandler(type, handler) -> Disposable

Handles typed events from the Rust OutputParser.

```typescript
host.registerStructuredEventHandler("plan-file", (payload, sessionId) => {
  const { path } = payload as { path: string };
  host.addItem({ ... });
});
```

See [Structured Event Types](#structured-event-types) for all types and payload shapes.

#### host.registerMarkdownProvider(scheme, provider) -> Disposable

Provides content for a URI scheme when the user clicks an ActivityItem.

```typescript
host.registerMarkdownProvider("my-scheme", {
  async provideContent(uri) {
    const id = uri.searchParams.get("id");
    if (!id) return null;
    try {
      return await host.invoke("read_file", { path: dir, file: name });
    } catch {
      return null;
    }
  },
});
```

#### host.addItem(item) / host.removeItem(id) / host.updateItem(id, updates)

Manage activity items:

```typescript
host.addItem({
  id: "deploy:api:prod",       // Unique identifier
  pluginId: "my-plugin",       // Must match your plugin id
  sectionId: "my-section",     // Must match your registered section
  title: "api-server",         // Primary text
  subtitle: "Deployed to prod", // Secondary text (optional)
  icon: '<svg .../>',          // Inline SVG with fill="currentColor"
  iconColor: "#3fb950",        // Optional CSS color for the icon
  dismissible: true,
  contentUri: "my-scheme:detail?id=api",  // Opens in MarkdownTab on click
  // OR: onClick: () => { ... },          // Mutually exclusive with contentUri
});

host.updateItem("deploy:api:prod", { subtitle: "Rolled back" });
host.removeItem("deploy:api:prod");
```

### Tier 2: Read-Only App State (always available)

#### host.getActiveRepo() -> RepoSnapshot | null

```typescript
const repo = host.getActiveRepo();
// { path: "/Users/me/project", displayName: "project", activeBranch: "main", worktreePath: null }
```

#### host.getRepos() -> RepoListEntry[]

```typescript
const repos = host.getRepos();
// [{ path: "/Users/me/project", displayName: "project" }, ...]
```

#### host.getActiveTerminalSessionId() -> string | null

```typescript
const sessionId = host.getActiveTerminalSessionId();
```

#### host.getRepoPathForSession(sessionId) -> string | null

Resolves which repository owns a given terminal session. Useful in output watcher callbacks where `sessionId` is provided but you need the repo context.

```typescript
host.registerOutputWatcher({
  pattern: /Deployed: (\S+)/,
  onMatch(match, sessionId) {
    const repoPath = host.getRepoPathForSession(sessionId);
    // repoPath = "/Users/me/project" or null
  },
});
```

#### host.getPrNotifications() -> PrNotificationSnapshot[]

```typescript
const prs = host.getPrNotifications();
// [{ id, repoPath, branch, prNumber, title, type }, ...]
```

#### host.getSettings(repoPath) -> RepoSettingsSnapshot | null

```typescript
const settings = host.getSettings("/Users/me/project");
// { path, displayName, baseBranch: "main", color: "#3fb950" }
```

### Tier 3: Write Actions (capability-gated)

These methods require declaring capabilities in `manifest.json`. Calling without the required capability throws `PluginCapabilityError`.

#### host.writePty(sessionId, data) -> Promise<void>

Sends input to a terminal session. **Requires `"pty:write"` capability.**

```typescript
await host.writePty(sessionId, "y\n");
```

#### host.openMarkdownPanel(title, contentUri) -> void

Opens a virtual markdown tab and shows the panel. **Requires `"ui:markdown"` capability.**

```typescript
host.openMarkdownPanel("CI Report", "my-scheme:report?id=123");
```

#### host.playNotificationSound() -> Promise<void>

Plays the notification sound. **Requires `"ui:sound"` capability.**

```typescript
await host.playNotificationSound();
```

### Tier 4: Scoped Tauri Invoke (whitelisted commands only)

#### host.invoke<T>(cmd, args?) -> Promise<T>

Invokes a whitelisted Tauri command. Non-whitelisted commands throw immediately.

**Whitelisted commands:**
| Command | Args | Returns | Capability |
|---------|------|---------|------------|
| `read_file` | `{ path: string, file: string }` | `string` | `invoke:read_file` |
| `list_markdown_files` | `{ path: string }` | `Array<{ path, git_status }>` | `invoke:list_markdown_files` |
| `read_plugin_data` | `{ plugin_id: string, path: string }` | `string` | none (always allowed) |
| `write_plugin_data` | `{ plugin_id: string, path: string, data: string }` | `void` | none (always allowed) |
| `delete_plugin_data` | `{ plugin_id: string, path: string }` | `void` | none (always allowed) |

**Plugin data storage** is sandboxed to `~/.config/tuicommander/plugins/{id}/data/`. No capability required — every plugin can store its own data.

```typescript
// Store cache data
await host.invoke("write_plugin_data", {
  plugin_id: "my-plugin",
  path: "cache.json",
  data: JSON.stringify({ lastCheck: Date.now() }),
});

// Read it back
const raw = await host.invoke("read_plugin_data", {
  plugin_id: "my-plugin",
  path: "cache.json",
});
const cache = JSON.parse(raw);
```

## Capabilities

Capabilities gate access to Tier 3 and Tier 4 methods. Declare them in `manifest.json`:

```json
{
  "capabilities": ["pty:write", "ui:sound"]
}
```

| Capability | Unlocks | Risk |
|------------|---------|------|
| `pty:write` | `host.writePty()` | Can send arbitrary input to terminals |
| `ui:markdown` | `host.openMarkdownPanel()` | Can open panels in the UI |
| `ui:sound` | `host.playNotificationSound()` | Can play sounds |
| `invoke:read_file` | `host.invoke("read_file", ...)` | Can read files on disk |
| `invoke:list_markdown_files` | `host.invoke("list_markdown_files", ...)` | Can list directory contents |

Tier 1, Tier 2, and plugin data commands are always available without capabilities.

## Content URI Format

```
scheme:path?key=value&key2=value2
```

Examples:
- `plan:file?path=%2Frepo%2Fplans%2Ffoo.md`
- `stories:detail?id=324-9b46&dir=%2Frepo%2Fstories`

## Icons

All icons must be monochrome inline SVGs with `fill="currentColor"` and viewBox `0 0 16 16`:

```javascript
const ICON = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="..."/></svg>';
```

Never use emoji — they render inconsistently across platforms.

## Hot Reload

When any file in a plugin directory changes, the app:
1. Emits a `plugin-changed` event with the plugin ID
2. Calls `pluginRegistry.unregister(id)` (runs `onunload`, disposes all registrations)
3. Re-imports the module with a cache-busting query (`?t=timestamp`)
4. Validates and re-registers the plugin

This means you can edit `main.js` and see changes without restarting the app.

## Build & Install

External plugins must be pre-compiled ES modules. Use esbuild:

```bash
esbuild src/main.ts --bundle --format=esm --outfile=main.js --external:nothing
```

Install by copying the directory to:
- macOS: `~/Library/Application Support/com.tuic.commander/plugins/my-plugin/`
- Linux: `~/.config/tuicommander/plugins/my-plugin/`
- Windows: `%APPDATA%/com.tuic.commander/plugins/my-plugin/`

Directory structure:
```
my-plugin/
  manifest.json
  main.js
```

## Plugin Management (Settings > Plugins)

The Settings panel has a **Plugins** tab with two sub-tabs:

### Installed

- Lists all plugins (built-in and external) with toggle, logs, and uninstall buttons
- Built-in plugins show a "Built-in" badge and cannot be toggled or uninstalled
- Error count badges appear on plugins with recent errors
- "Logs" button opens an expandable log viewer showing the plugin's ring buffer
- "Install from file..." button opens a file dialog accepting `.zip` archives

### Browse

- Shows plugins from the community registry (fetched from GitHub)
- "Install" button downloads and installs directly
- "Update available" badge when a newer version exists
- "Refresh" button forces a new registry fetch (normally cached for 1 hour)

### Enable/Disable

Plugin enabled state is persisted in `AppConfig.disabled_plugin_ids`. Disabled plugins appear in the Installed list but are not loaded.

## ZIP Plugin Installation

Plugins can be distributed as ZIP archives:

1. **From Settings:** Click "Install from file..." in the Plugins tab
2. **From URL:** Use `tuic://install-plugin?url=https://example.com/plugin.zip`
3. **From Rust:** `invoke("install_plugin_from_zip", { path })` or `invoke("install_plugin_from_url", { url })`

**ZIP requirements:**
- Must contain a valid `manifest.json` (at root or in a single top-level directory)
- All paths are validated for zip-slip attacks (no `..` traversal)
- If updating an existing plugin, the `data/` directory is preserved

## Deep Link Scheme (`tuic://`)

TUI Commander registers the `tuic://` URL scheme for external integration:

| URL | Action |
|-----|--------|
| `tuic://install-plugin?url=https://...` | Download ZIP, show confirmation, install |
| `tuic://open-repo?path=/path/to/repo` | Switch to repo (must already be in sidebar) |
| `tuic://settings?tab=plugins` | Open Settings to a specific tab |

**Security:** `install-plugin` requires HTTPS URLs and shows a confirmation dialog. `open-repo` only accepts paths already in the repository list.

## Plugin Registry

The registry is a JSON file hosted on GitHub (`sstraus/tuicommander-plugins` repo). The app fetches it on demand (Browse tab) with a 1-hour TTL cache.

Registry entries include: `id`, `name`, `description`, `author`, `latestVersion`, `minAppVersion`, `capabilities`, `downloadUrl`.

The Browse tab compares installed versions to detect available updates.

## Per-Plugin Error Logging

Each plugin has a dedicated ring buffer logger (500 entries max). Errors from `onload`, `onunload`, output watchers, and structured event handlers are automatically captured.

Plugins can also write to their log via `host.log(level, message, data)`.

View logs in Settings > Plugins > click "Logs" on any plugin row.

## Built-in Plugins

Built-in plugins are TypeScript modules in `src/plugins/` compiled with the app. They have unrestricted access (no capability checks).

| Plugin | File | Section | Detects |
|--------|------|---------|---------|
| `plan` | `planPlugin.ts` | ACTIVE PLAN | `plan-file` structured events |

The `wiz-stories` plugin was extracted to an external plugin in `examples/plugins/wiz-stories/`.

To create a built-in plugin, add it to `BUILTIN_PLUGINS` in `src/plugins/index.ts`.

## Testing

### Mock setup for plugin tests

```typescript
import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("../../invoke", () => ({
  invoke: vi.fn(),
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { invoke } from "../../invoke";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { activityStore } from "../../stores/activityStore";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";

beforeEach(() => {
  pluginRegistry.clear();
  activityStore.clearAll();
  markdownProviderRegistry.clear();
  vi.mocked(invoke).mockReset();
});
```

### Testing output watchers

```typescript
it("detects deployment from PTY output", () => {
  pluginRegistry.register(myPlugin);
  pluginRegistry.processRawOutput("Deployed: api-server to prod\n", "session-1");

  const items = activityStore.getForSection("my-section");
  expect(items).toHaveLength(1);
  expect(items[0].title).toBe("api-server");
});
```

### Testing capability gating

```typescript
it("external plugin without pty:write throws on writePty", async () => {
  let host;
  pluginRegistry.register(
    { id: "ext", onload: (h) => { host = h; }, onunload: () => {} },
    [], // no capabilities
  );
  await expect(host.writePty("s1", "data")).rejects.toThrow(PluginCapabilityError);
});
```

## CSS Classes

Activity items use these CSS classes (defined in `src/styles.css`):

| Class | Element |
|-------|---------|
| `activity-section-header` | Section heading row |
| `activity-section-label` | Section label text |
| `activity-dismiss-all` | "Dismiss All" button |
| `activity-item` | Individual item row |
| `activity-item-icon` | Item icon container |
| `activity-item-body` | Title + subtitle wrapper |
| `activity-item-title` | Primary text |
| `activity-item-subtitle` | Secondary text |
| `activity-item-dismiss` | Dismiss button |
| `activity-last-item-btn` | Shortcut button in toolbar |
| `activity-last-item-icon` | Shortcut button icon |
| `activity-last-item-title` | Shortcut button text |

## Structured Event Types

The Rust `OutputParser` detects patterns in terminal output and emits typed events. Handle them with `host.registerStructuredEventHandler(type, handler)`.

### plan-file

Detected when a plan file path appears in terminal output.

```typescript
{ type: "plan-file", path: string }
// path: relative or absolute, e.g. "plans/foo.md", ".claude/plans/bar.md"
```

### rate-limit

Detected when AI API rate limits are hit.

```typescript
{
  type: "rate-limit",
  pattern_name: string,           // e.g. "claude-http-429", "openai-http-429"
  matched_text: string,           // the matched substring
  retry_after_ms: number | null,  // ms to wait (default 60000)
}
```

Pattern names: `claude-http-429`, `claude-overloaded`, `openai-http-429`, `cursor-rate-limit`, `gemini-resource-exhausted`, `http-429`, `retry-after-header`, `openai-retry-after`, `openai-tpm-limit`, `openai-rpm-limit`.

### status-line

Detected when an AI agent emits a status/progress line.

```typescript
{
  type: "status-line",
  task_name: string,              // e.g. "Reading files"
  full_line: string,              // complete line trimmed
  time_info: string | null,       // e.g. "12s"
  token_info: string | null,      // e.g. "2.4k tokens"
}
```

### pr-url

Detected when a GitHub/GitLab PR/MR URL appears in output.

```typescript
{
  type: "pr-url",
  number: number,     // PR/MR number
  url: string,        // full URL
  platform: string,   // "github" or "gitlab"
}
```

### progress

Detected from OSC 9;4 terminal progress sequences.

```typescript
{
  type: "progress",
  state: number,  // 0=remove, 1=normal, 2=error, 3=indeterminate
  value: number,  // 0-100
}
```

### question

Detected when an interactive prompt appears (Y/N prompts, numbered menus, inquirer-style).

```typescript
{
  type: "question",
  prompt_text: string,  // the question line (ANSI-stripped)
}
```

### usage-limit

Detected when Claude Code reports usage limits.

```typescript
{
  type: "usage-limit",
  percentage: number,    // 0-100
  limit_type: string,    // "weekly" or "session"
}
```

## Example Plugins

See `examples/plugins/` for complete working examples:

| Example | Tier | Capabilities | Demonstrates |
|---------|------|-------------|--------------|
| `hello-world` | 1 | none | Output watcher, addItem |
| `auto-confirm` | 1+3 | `pty:write` | Auto-responding to Y/N prompts |
| `ci-notifier` | 1+3 | `ui:sound`, `ui:markdown` | Sound notifications, markdown panels |
| `repo-dashboard` | 1+2 | none | Read-only state, dynamic markdown |
| `wiz-stories` | 1+4 | `invoke:read_file`, `invoke:list_markdown_files`, `ui:markdown` | Story tracking, markdown provider, session-to-repo resolution |
| `wiz-reviews` | 1+4 | `invoke:read_file`, `ui:markdown` | Code review tracking, markdown provider |

## Troubleshooting

| Problem | Cause | Fix |
|---------|-------|-----|
| Plugin not loading | `manifest.json` missing or malformed | Check console for validation errors |
| `requires app version X.Y.Z` | `minAppVersion` too high | Lower `minAppVersion` or update app |
| `not in the invoke whitelist` | Calling non-whitelisted Tauri command | Only use commands listed in the whitelist table |
| `requires capability "X"` | Missing capability in manifest | Add the capability to `manifest.json` `capabilities` array |
| Module not found | `main` field doesn't match filename | Ensure `"main": "main.js"` matches your actual file |
| Changes not reflecting | Hot reload cache | Save the file again, or restart the app |
| `default export` error | Module doesn't `export default { ... }` | Ensure your module has a default export with `id`, `onload`, `onunload` |
