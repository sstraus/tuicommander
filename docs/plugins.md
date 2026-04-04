# Plugin Authoring Guide

TUICommander uses an Obsidian-style plugin system. Plugins extend the Activity Center (bell dropdown), watch terminal output, and interact with app state. Plugins can be **built-in** (compiled with the app) or **external** (loaded at runtime from the user's plugins directory).

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

> **Note:** All manifest fields use **camelCase** (`minAppVersion`, `agentTypes`, `contentUri`) — this matches the Rust serde serialization format. Do not use snake_case.

```json
// ✅ Correct
{ "minAppVersion": "0.5.0", "agentTypes": ["claude"] }
// ❌ Wrong
{ "min_app_version": "0.5.0", "agent_types": ["claude"] }
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
| `minAppVersion` | string | yes | Minimum TUICommander version required |
| `main` | string | yes | Entry point filename (e.g. `"main.js"`) |
| `description` | string | no | Short description |
| `author` | string | no | Author name |
| `capabilities` | string[] | no | Tier 3/4 capabilities needed (defaults to `[]`) |
| `allowedUrls` | string[] | no | URL patterns allowed for `net:http` (e.g. `["https://api.example.com/*"]`) |
| `agentTypes` | string[] | no | Agent types this plugin targets (e.g. `["claude"]`). Omit or `[]` for universal plugins. |
| `binaries` | string[] | no | CLI binaries this plugin may execute via `exec:cli` (e.g. `["rtk", "mdkb"]`) |

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

Write to the plugin's dedicated log ring buffer (max 500 entries). Viewable in Settings > Plugins > click "Logs" on any plugin row.

```typescript
host.log("info", "Plugin initialized");
host.log("error", "Failed to process", { code: 404 });
```

Levels: `"debug"`, `"info"`, `"warn"`, `"error"`. The optional `data` parameter accepts any JSON-serializable value and is displayed alongside the message.

Errors thrown inside `onload`, `onunload`, output watchers, and structured event handlers are automatically captured to the plugin's log. Use `host.log()` for additional diagnostic output. Error count badges appear on plugins with recent errors in the Settings panel.

### Tier 1: Activity Center + Watchers + Providers (always available)

All `register*()` methods return a `Disposable` with a `dispose()` method. You do **not** need to call `dispose()` manually — all registrations are automatically disposed when `onunload()` is called (including during hot reload). Only call `dispose()` if you need to dynamically remove a registration while the plugin is still running.

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

Resolves which repository owns a given terminal session by searching all repos and branches for a terminal matching the session ID. Returns `null` if the session is not associated with any repository (e.g. a standalone terminal or an unknown session ID). Useful in output watcher callbacks where `sessionId` is provided but you need the repo context.

```typescript
host.registerOutputWatcher({
  pattern: /Deployed: (\S+)/,
  onMatch(match, sessionId) {
    const repoPath = host.getRepoPathForSession(sessionId);
    if (!repoPath) return; // session not tied to a repo
    // repoPath = "/Users/me/project"
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

#### host.getTerminalState() -> TerminalStateSnapshot | null

Returns the active terminal's state snapshot.

```typescript
const state = host.getTerminalState();
// { sessionId, shellState: "busy"|"idle"|null, agentType: "claude"|null,
//   agentActive: boolean, awaitingInput: "question"|null, repoPath }
```

#### host.onStateChange(callback) -> Disposable

Register a callback for terminal/branch state changes. Fires on agent start/stop, branch change, shell state change, and awaiting-input change.

```typescript
const sub = host.onStateChange((event) => {
  // event.type: "agent-started" | "agent-stopped" | "branch-changed"
  //           | "shell-state-changed" | "awaiting-input-changed"
  // event.sessionId, event.terminalId, event.detail (branch name for branch-changed)
});
// sub.dispose() to unsubscribe
```

### Tier 2b: Git Read (capability-gated)

These methods require declaring `"git:read"` in `manifest.json`. They provide read-only access to git repository state.

#### host.getGitBranches(repoPath) -> Promise<Array<{ name, isCurrent }>>

```typescript
const branches = await host.getGitBranches("/Users/me/project");
// [{ name: "main", isCurrent: true }, { name: "feature/x", isCurrent: false }]
```

#### host.getRecentCommits(repoPath, count?) -> Promise<Array<{ hash, message, author, date }>>

```typescript
const commits = await host.getRecentCommits("/Users/me/project", 5);
// [{ hash: "abc1234", message: "fix: bug", author: "name", date: "2026-02-25" }]
```

#### host.getGitDiff(repoPath, scope?) -> Promise<string>

```typescript
const diff = await host.getGitDiff("/Users/me/project", "staged");
// Returns unified diff string
```

### Tier 3: Write Actions (capability-gated)

These methods require declaring capabilities in `manifest.json`. Calling without the required capability throws `PluginCapabilityError`.

#### host.writePty(sessionId, data) -> Promise<void>

Sends raw bytes to a terminal session. **Requires `"pty:write"` capability.**

> **Prefer `sendAgentInput()` for user input.** `writePty` sends raw data — it does not handle Enter key semantics for Ink-based agents. Use it only when you need exact byte control.

```typescript
await host.writePty(sessionId, "\x03"); // Send Ctrl-C
```

#### host.sendAgentInput(sessionId, text) -> Promise<void>

Sends user input to an agent session with correct Enter handling. **Requires `"pty:write"` capability.**

Ink-based agents (Claude Code, Codex, etc.) run in raw mode and need Ctrl-U + text in one write, then `\r` in a separate write. Shell sessions receive everything in a single write. This method handles both cases automatically based on the detected agent type.

```typescript
await host.sendAgentInput(sessionId, "y");       // confirm a prompt
await host.sendAgentInput(sessionId, "explain this code"); // send a message
```

#### host.openMarkdownPanel(title, contentUri) -> void

Opens a virtual markdown tab and shows the panel. **Requires `"ui:markdown"` capability.**

```typescript
host.openMarkdownPanel("CI Report", "my-scheme:report?id=123");
```

#### host.openMarkdownFile(absolutePath) -> void

Opens a local markdown file in the markdown panel. **Requires `"ui:markdown"` capability.** The path must be absolute. This is useful for plugins that ship a `README.md` or other documentation files.

```typescript
// Open the plugin's own README
host.openMarkdownFile("/Users/me/.config/tuicommander/plugins/my-plugin/README.md");
```

#### host.playNotificationSound(sound?) -> Promise<void>

Plays a notification sound. **Requires `"ui:sound"` capability.**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `sound` | `string` | `"info"` | One of: `"question"`, `"error"`, `"completion"`, `"warning"`, `"info"` |

```typescript
await host.playNotificationSound("error");      // CI failure, build error
await host.playNotificationSound("question");    // input prompt, awaiting user
await host.playNotificationSound("completion");  // task finished
await host.playNotificationSound();              // defaults to "info"
```

### Tier 3b: Filesystem Operations (capability-gated)

These methods provide sandboxed filesystem access. All paths must be absolute and within the user's home directory (`$HOME`).

#### host.readFile(absolutePath) -> Promise<string>

Read a file's content as UTF-8 text. Maximum file size: 10 MB. **Requires `"fs:read"` capability.**

```typescript
const content = await host.readFile("/Users/me/.claude/projects/foo/conversation.jsonl");
```

#### host.listDirectory(path, pattern?) -> Promise<string[]>

List filenames in a directory, optionally filtered by a glob pattern. Returns filenames only (not full paths), sorted alphabetically. **Requires `"fs:list"` capability.**

```typescript
const files = await host.listDirectory("/Users/me/.claude/projects/foo", "*.jsonl");
// ["conversation-1.jsonl", "conversation-2.jsonl"]
```

#### host.watchPath(path, callback, options?) -> Promise<Disposable>

Watch a path for filesystem changes. Emits batched events after a debounce period. **Requires `"fs:watch"` capability.**

```typescript
const watcher = await host.watchPath(
  "/Users/me/.claude/projects/foo",
  (events) => {
    for (const event of events) {
      console.log(event.type, event.path); // "create" | "modify" | "delete"
    }
  },
  { recursive: true, debounceMs: 500 },
);

// Later: stop watching
watcher.dispose();
```

**Options:**
- `recursive` — Watch subdirectories (default: `false`)
- `debounceMs` — Debounce window in milliseconds (default: `300`)

**FsChangeEvent:**
```typescript
interface FsChangeEvent {
  type: "create" | "modify" | "delete";
  path: string;
}
```

#### host.writeFile(absolutePath, content) -> Promise<void>

Write content to a file within `$HOME`. Creates parent directories if needed. Refuses to overwrite directories. Max 10 MB. **Requires `"fs:write"` capability.**

```typescript
await host.writeFile("/Users/me/project/stories/new-story.md", "---\nstatus: pending\n---\n# New Story");
```

#### host.renamePath(from, to) -> Promise<void>

Rename or move a file within `$HOME`. Both paths must be absolute. Source must exist. Creates parent directories for destination if needed. **Requires `"fs:rename"` capability.**

```typescript
await host.renamePath(
  "/Users/me/project/stories/old-name.md",
  "/Users/me/project/stories/new-name.md",
);
```

### Tier 3c: Status Bar Ticker (capability-gated)

The status bar has a shared ticker area that rotates messages from multiple plugins. Messages are grouped by priority tier:

| Tier | Priority | Behavior |
|------|----------|----------|
| Low | < 10 | Shown only in the popover, not in rotation |
| Normal | 10–99 | Auto-rotates every 5s in the ticker area |
| Urgent | >= 100 | Pinned — pauses rotation until cleared |

Users can click the counter badge (e.g. `1/3 ▸`) to cycle manually, or right-click the ticker to see all active messages in a popover.

#### host.setTicker(options) -> void

Set a ticker message in the shared status bar ticker. Preferred API — supports source labels. If a message with the same id from this plugin already exists, it is replaced. **Requires `"ui:ticker"` capability.**

```typescript
host.setTicker({
  id: "my-status",
  text: "Processing: 42%",
  label: "MyPlugin",         // Shown as "MyPlugin · Processing: 42%"
  icon: '<svg viewBox="0 0 16 16" fill="currentColor">...</svg>',
  priority: 10,
  ttlMs: 60000,
  onClick: () => { /* optional click handler */ },
});
```

**Options:**
- `id` — Unique message identifier (scoped to your plugin). Reusing an id replaces the previous message.
- `text` — Message text displayed in the ticker rotation.
- `label` — Optional human-readable source label shown before the text (e.g. `"Usage"`).
- `icon` — Optional inline SVG icon.
- `priority` — Priority tier (see table above). Default: `0`.
- `ttlMs` — Auto-expire after N milliseconds. `0` = persistent (must be removed manually). Default: `60000`.
- `onClick` — Optional callback invoked when the user clicks the message text.

#### host.clearTicker(id) -> void

Remove a ticker message by id. **Requires `"ui:ticker"` capability.**

```typescript
host.clearTicker("my-status");
```

#### host.postTickerMessage(options) -> void *(legacy)*

Alias for `setTicker` without `label` support. Prefer `setTicker` for new plugins.

#### host.removeTickerMessage(id) -> void *(legacy)*

Alias for `clearTicker`.

### Tier 3d: Panel UI (capability-gated)

#### host.openPanel(options) -> PanelHandle

Open an HTML panel in a sandboxed iframe tab. Returns a handle for updating content or closing the panel. If a panel with the same id is already open, it will be activated and updated. **Requires `"ui:panel"` capability.**

```typescript
const panel = host.openPanel({
  id: "my-dashboard",
  title: "Dashboard",
  html: "<html><body><h1>Hello</h1></body></html>",
  onMessage(data) {
    // Receive structured messages from the iframe
    console.log("Got message from iframe:", data);
    // Send response back
    panel.send({ type: "response", ok: true });
  },
});

// Update content later
panel.update("<html><body><h1>Updated</h1></body></html>");

// Send a message to the iframe at any time
panel.send({ type: "refresh", items: [...] });

// Close the panel
panel.close();
```

**Inside the iframe:**
```html
<script>
  // Send message to host
  window.parent.postMessage({ type: "save", config: { ... } }, "*");

  // Receive messages from host
  window.addEventListener("message", (e) => {
    if (e.data?.type === "response") {
      console.log("Host says:", e.data.ok);
    }
  });
</script>
```

**CSS Base Stylesheet + Theme Injection:** Every plugin panel iframe receives two automatic CSS injections:

1. **Base stylesheet** (`pluginBaseStyles.ts`) — a complete design foundation with reset, typography, buttons, inputs, cards, tables, badges, toasts, scrollbars, and empty states. All values use CSS custom properties from the app theme. Plugins get a polished, consistent look **without writing any CSS**.

2. **Theme variables** — all CSS custom properties from the app's `:root` are injected (e.g. `--bg-primary`, `--fg-primary`, `--border`, `--accent`, `--error`, `--warning`, `--success`, `--text-on-accent`). These match the user's active theme.

**Design strategy:** Write minimal plugin-specific CSS that overrides the base. The base provides:

| Base class | Description |
|------------|-------------|
| `body` | Themed background, font, color |
| `button`, `.btn` | Default button with hover/active states |
| `button.primary`, `.btn-primary` | Accent-colored button |
| `button.danger`, `.btn-danger` | Error-colored button |
| `input`, `textarea`, `select` | Themed form controls with focus ring |
| `.card` | Bordered container with hover elevation |
| `table`, `th`, `td` | Styled table with hover rows |
| `.badge` | Inline label (combine with `.badge-p1`, `.badge-error`, `.badge-success`, `.badge-accent`, `.badge-warning`, `.badge-muted`) |
| `label`, `.hint` | Form labels and help text |
| `.filter-bar` | Flex row for search/filter UI |
| `.empty-state` | Centered placeholder with `.hint` |
| `.toast`, `.toast.error`, `.toast.success` | Fixed-position notification (add `.show` to display) |
| `h1`–`h4` | Themed headings |
| `code`, `a`, `hr`, `small` | Themed inline elements |
| `::-webkit-scrollbar` | Styled scrollbar matching the app |

**Example — minimal plugin CSS:**

```html
<style>
  /* Only what's specific to this plugin */
  body { padding: 16px; }
  .my-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; }
</style>
```

All standard elements (buttons, inputs, tables) will look correct automatically.

**Available CSS variables** (from the app's active theme):

| Variable | Usage |
|----------|-------|
| `--bg-primary` | Main canvas |
| `--bg-secondary` | Sidebar-level surfaces |
| `--bg-tertiary` | Inputs, elevated surfaces |
| `--bg-highlight` | Hover states |
| `--fg-primary` | Primary text |
| `--fg-secondary` | Labels, secondary text |
| `--fg-muted` | Tertiary text |
| `--accent` | Links, primary actions |
| `--accent-hover` | Hover on accent |
| `--success` | Positive states |
| `--warning` | Caution states |
| `--error` | Error states |
| `--border` | All borders |
| `--text-on-accent` | Text on colored backgrounds |
| `--text-on-error` | Text on error backgrounds |
| `--text-on-success` | Text on success backgrounds |

**Security:** The iframe uses `sandbox="allow-scripts"` without `allow-same-origin`, blocking access to Tauri IPC and the parent page DOM. The `close-panel` message type is handled as a system message; all other messages are routed to the `onMessage` callback.

### Tier 3e: Sidebar Plugin Panels (capability-gated)

#### host.registerSidebarPanel(options) -> SidebarPanelHandle

Register a collapsible panel section in the sidebar, displayed below the branch list for each repo. **Requires `"ui:sidebar"` capability.**

Panels display structured data (not HTML) — the app renders items natively for visual consistency with the rest of the sidebar.

```typescript
interface SidebarPanelOptions {
  id: string;              // Unique panel ID (scoped to plugin)
  label: string;           // Section header text
  icon?: string;           // Inline SVG for header
  priority?: number;       // Lower = higher in sidebar (default 100)
  collapsed?: boolean;     // Initial collapsed state (default true)
}

interface SidebarPanelHandle {
  setItems(items: SidebarItem[]): void;     // Replace all items
  setBadge(text: string | null): void;      // Header badge (e.g. "3")
  dispose(): void;                          // Remove panel
}

interface SidebarItem {
  id: string;              // Unique item ID (scoped to panel)
  label: string;           // Primary text
  subtitle?: string;       // Secondary text (smaller, muted)
  icon?: string;           // Inline SVG (fill="currentColor")
  iconColor?: string;      // CSS color
  onClick?: () => void;    // Click handler
  contextMenu?: SidebarItemAction[];  // Right-click actions
}

interface SidebarItemAction {
  label: string;
  action: () => void;
  disabled?: boolean;
}
```

Example:

```typescript
const panel = host.registerSidebarPanel({
  id: "active-plans",
  label: "ACTIVE PLANS",
  icon: '<svg ...>...</svg>',
  priority: 10,
  collapsed: false,
});

panel.setItems([
  { id: "plan-1", label: "Feature Plan", subtitle: "In Progress · M", onClick: () => openPlan() },
]);
panel.setBadge("1");
```

**Behavior:**
- Panels appear inside `RepoSection`, below branches, only when the repo is expanded
- Items are rendered as native sidebar list items (same style as branches)
- Right-click on items shows a context menu with plugin-defined actions
- Badge appears as a small counter pill on the section header
- On plugin unload, panels are automatically removed

### Tier 3f: Context Menu Actions (capability-gated)

#### host.registerTerminalAction(action) -> Disposable

Register an action in the terminal right-click "Actions" submenu. **Requires `"ui:context-menu"` capability.**

The action handler receives a `TerminalActionContext` snapshot captured at right-click time (not at click time), avoiding race conditions if the user switches terminals between opening the menu and clicking.

```typescript
interface TerminalActionContext {
  sessionId: string | null;  // PTY session ID of the right-clicked terminal
  repoPath: string | null;   // Repository path that owns the terminal
}

interface TerminalAction {
  id: string;                                              // Unique action ID (scoped to plugin)
  label: string;                                           // Display label in the menu
  action: (ctx: TerminalActionContext) => void;             // Handler
  disabled?: (ctx: TerminalActionContext) => boolean;       // Evaluated at menu-open time
}
```

Example:

```typescript
const d = host.registerTerminalAction({
  id: "restart-agent",
  label: "Restart Agent",
  action: (ctx) => {
    if (ctx.sessionId) host.sendAgentInput(ctx.sessionId, "exit");
  },
  disabled: (ctx) => !ctx.sessionId,
});
```

**Behavior:**
- Actions from all plugins are shown in a flat list under the "Actions" submenu
- The submenu is hidden when no actions are registered
- `disabled` callback is re-evaluated each time the context menu opens
- On plugin unload, actions are automatically removed; stale handler references are no-ops

#### host.registerContextMenuAction(action) -> Disposable

Register an action in context menus for a specific target type. **Requires `"ui:context-menu"` capability.**

```typescript
type ContextMenuTarget = "terminal" | "branch" | "repo" | "tab";

interface ContextMenuAction {
  id: string;
  label: string;
  icon?: string;              // Inline SVG
  target: ContextMenuTarget;
  action: (ctx: ContextMenuContext) => void;
  disabled?: (ctx: ContextMenuContext) => boolean;
}

interface ContextMenuContext {
  target: ContextMenuTarget;
  sessionId?: string;     // terminal, tab
  repoPath?: string;      // branch, repo, terminal
  branchName?: string;    // branch only
  tabId?: string;         // tab only
}
```

Example:

```typescript
host.registerContextMenuAction({
  id: "deploy",
  label: "Deploy Branch",
  target: "branch",
  action: (ctx) => {
    if (ctx.branchName) deploy(ctx.repoPath, ctx.branchName);
  },
});
```

**Behavior:**
- Actions appear after built-in items, separated by a divider
- `disabled` callback is re-evaluated each time the context menu opens
- On plugin unload, actions are automatically removed

### Tier 3g: Credential Access (capability-gated)

#### host.readCredential(serviceName) -> Promise<string | null>

Read credentials from the system credential store by service name. Returns the raw credential JSON string, or `null` if not found. **Requires `"credentials:read"` capability.**

First call from an external plugin shows a user consent dialog. Built-in plugins skip the dialog.

```typescript
const credJson = await host.readCredential("Claude Code-credentials");
if (credJson) {
  const creds = JSON.parse(credJson);
  const token = creds.claudeAiOauth.accessToken;
}
```

**Platforms:**
- macOS: Reads from Keychain (`security find-generic-password -s <service> -w`)
- Linux/Windows: Reads from `~/.claude/.credentials.json`

### Tier 3h: HTTP Requests (capability-gated)

#### host.httpFetch(url, options?) -> Promise<HttpResponse>

Make an HTTP request. Non-2xx status codes are returned normally (not thrown as errors). **Requires `"net:http"` capability.**

External plugins can only fetch URLs matching their manifest's `allowedUrls` patterns.

```typescript
const resp = await host.httpFetch("https://api.example.com/data", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ key: "value" }),
});
if (resp.status === 200) {
  const data = JSON.parse(resp.body);
}
```

**HttpResponse:**
```typescript
interface HttpResponse {
  status: number;
  headers: Record<string, string>;
  body: string;
}
```

**Security and limits:**
- `file://`, `data://`, `ftp://` schemes are blocked
- 30-second timeout, 10 MB response limit, max 5 redirects
- Localhost (`localhost`, `127.0.0.1`, `::1`, `[::1]`, `0.0.0.0`) is blocked unless explicitly declared in `allowedUrls`
- Built-in plugins (no `capabilities` array) can fetch any `http://` or `https://` URL without restrictions

**`allowedUrls` pattern matching:**
- Patterns use prefix matching with an optional trailing `*` wildcard
- `"https://api.example.com/*"` — matches any path under that origin
- `"https://api.example.com/v2/data"` — matches that exact URL only
- `"http://localhost:8080/*"` — allows localhost on that port (required to unblock localhost)
- The URL must start with the pattern prefix (before `*`) to match

### Tier 3i: File Tail (capability-gated)

#### host.readFileTail(absolutePath, maxBytes) -> Promise<string>

Read the last N bytes of a file, skipping any partial first line. Useful for reading recent entries from large JSONL files. **Requires `"fs:read"` capability.**

```typescript
const tail = await host.readFileTail("/Users/me/.claude/hud-tracking.jsonl", 512 * 1024);
const lines = tail.split("\n").filter(Boolean);
```

### Tier 3j: CLI Execution (capability-gated)

#### host.execCli(binary, args, cwd?) -> Promise<string>

Execute a CLI binary declared in the plugin's manifest and return its stdout. **Requires `"exec:cli"` capability.**

Only binaries listed in the manifest's `binaries` field can be executed. The on-disk manifest is the source of truth — the frontend cannot grant access to undeclared binaries.

```json
// manifest.json
{ "capabilities": ["exec:cli"], "binaries": ["mdkb"] }
```

```typescript
const raw = await host.execCli("mdkb", ["--format", "json", "status"], "/Users/me/project");
const status = JSON.parse(raw);
console.log(status.index.documents); // 1486
```

**Security and limits:**
- Only binaries declared in the plugin's `binaries` manifest field can be executed
- Working directory must be absolute and within `$HOME`
- 30-second timeout
- 5 MB stdout limit
- Binary is resolved via PATH lookup and known install locations (`~/.cargo/bin/`, `/usr/local/bin/`, etc.)

### Tier 4: Scoped Tauri Invoke (whitelisted commands only)

#### host.invoke<T>(cmd, args?) -> Promise<T>

Invokes a whitelisted Tauri command. Non-whitelisted commands throw immediately.

**Whitelisted commands:**
| Command | Args | Returns | Capability |
|---------|------|---------|------------|
| `read_file` | `{ path: string, file: string }` | `string` | `invoke:read_file` |
| `list_markdown_files` | `{ path: string }` | `Array<{ path, git_status }>` | `invoke:list_markdown_files` |
| `read_plugin_data` | `{ plugin_id: string, path: string }` | `string` | none (always allowed) |
| `write_plugin_data` | `{ plugin_id: string, path: string, content: string }` | `void` | none (always allowed) |
| `delete_plugin_data` | `{ plugin_id: string, path: string }` | `void` | none (always allowed) |

**Plugin data storage** is sandboxed to `~/.config/tuicommander/plugins/{id}/data/`. No capability required — every plugin can store its own data.

```typescript
// Store cache data
await host.invoke("write_plugin_data", {
  plugin_id: "my-plugin",
  path: "cache.json",
  content: JSON.stringify({ lastCheck: Date.now() }),
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
| `pty:write` | `host.writePty()`, `host.sendAgentInput()` | Can send input to terminals |
| `ui:markdown` | `host.openMarkdownPanel()`, `host.openMarkdownFile()` | Can open panels and files in the UI |
| `ui:sound` | `host.playNotificationSound(sound?)` | Can play sounds (question, error, completion, warning, info) |
| `ui:panel` | `host.openPanel()` | Can render arbitrary HTML in sandboxed iframe |
| `ui:ticker` | `host.setTicker()`, `host.clearTicker()` | Can post messages to the shared status bar ticker |
| `credentials:read` | `host.readCredential()` | Can read system credentials (consent dialog shown) |
| `net:http` | `host.httpFetch()` | Can make HTTP requests (scoped to `allowedUrls`) |
| `invoke:read_file` | `host.invoke("read_file", ...)` | Can read files on disk |
| `invoke:list_markdown_files` | `host.invoke("list_markdown_files", ...)` | Can list directory contents |
| `fs:read` | `host.readFile()`, `host.readFileTail()` | Can read files within `$HOME` (10 MB limit) |
| `fs:list` | `host.listDirectory()` | Can list directory contents within `$HOME` |
| `fs:watch` | `host.watchPath()` | Can watch filesystem paths within `$HOME` for changes |
| `fs:write` | `host.writeFile()` | Can write files within `$HOME` (10 MB limit) |
| `fs:rename` | `host.renamePath()` | Can rename/move files within `$HOME` |
| `exec:cli` | `host.execCli()` | Can execute CLI binaries declared in manifest `binaries` field |
| `git:read` | `host.getGitBranches()`, `host.getRecentCommits()`, `host.getGitDiff()` | Read-only access to git repository state |
| `ui:context-menu` | `host.registerTerminalAction()` | Can add actions to the terminal right-click "Actions" submenu |
| `ui:sidebar` | `host.registerSidebarPanel()` | Can register collapsible panel sections in the sidebar |
| `ui:file-icons` | `host.registerFileIconProvider()` | Can provide file/folder icons for the file browser (e.g. VS Code icon themes) |

Tier 1, Tier 2, and plugin data commands are always available without capabilities.

## Agent-Scoped Plugins

Plugins can declare which AI agents they target via the `agentTypes` manifest field:

```json
{
  "id": "claude-usage",
  "agentTypes": ["claude"],
  ...
}
```

### Behavior

- **Universal plugins** (`agentTypes` omitted or `[]`): receive events from all terminals. This is the default.
- **Agent-scoped plugins** (`agentTypes: ["claude"]`): output watchers and structured event handlers only fire for terminals where the detected foreground process matches one of the listed agent types.

### What gets filtered

| Dispatch method | Filtered by agentTypes |
|----------------|----------------------|
| `registerOutputWatcher` callbacks | Yes |
| `registerStructuredEventHandler` callbacks | Yes |
| All other PluginHost methods (Tier 1-4) | No — always available |

### How agent detection works

TUICommander polls the foreground process of each terminal's PTY every 3 seconds (via `get_session_foreground_process`). The process name is classified into an agent type:

| Process name | Agent type |
|-------------|-----------|
| `claude` | `"claude"` |
| `gemini` | `"gemini"` |
| `opencode` | `"opencode"` |
| `aider` | `"aider"` |
| `codex` | `"codex"` |
| `amp` | `"amp"` |
| `cursor-agent` | `"cursor"` |
| `oz` | `"warp"` |
| `droid` | `"droid"` |
| `git` | `"git"` |
| (anything else) | `null` (plain shell) |

### Timing considerations

Agent detection is polled, not instant. When a user launches `claude` in a terminal, there is a brief window (up to 3 seconds) before the first poll detects it. During this window, agent-scoped plugins will not receive events from that terminal. This is by design — it avoids false matches during shell startup.

### Example: Claude-only plugin

```json
{
  "id": "claude-usage",
  "name": "Claude Usage Dashboard",
  "version": "1.0.0",
  "minAppVersion": "0.3.0",
  "main": "main.js",
  "agentTypes": ["claude"],
  "capabilities": ["fs:read", "ui:panel", "ui:ticker"]
}
```

This plugin's output watchers will only fire when the terminal is running Claude Code. If the user switches to a plain shell or runs Gemini, the watchers are silently skipped.

### Example: Multi-agent plugin

```json
{
  "agentTypes": ["claude", "gemini", "codex"]
}
```

Targets Claude, Gemini, and Codex terminals. All other terminals are ignored.

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

TUICommander registers the `tuic://` URL scheme for external integration:

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
| `plan` | `planPlugin.ts` | ACTIVE PLAN | `plan-file` structured events (repo-scoped) |

> **Note:** Session prompt tracking is now a native Rust feature (via `input_line_buffer.rs` and the Activity Dashboard). The former `sessionPromptPlugin` built-in has been removed.

See `examples/plugins/report-watcher/` for a template showing how to extract terminal output into Activity Center items with a markdown viewer.

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
it("external plugin without pty:write throws on sendAgentInput", async () => {
  let host;
  pluginRegistry.register(
    { id: "ext", onload: (h) => { host = h; }, onunload: () => {} },
    [], // no capabilities
  );
  await expect(host.sendAgentInput("s1", "hello")).rejects.toThrow(PluginCapabilityError);
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

Detected when a plan file path appears in terminal output. The path is always resolved to an absolute path before emission:

- Relative paths (e.g. `plans/foo.md`, `.claude/plans/bar.md`) are resolved against the terminal session's CWD
- Tilde paths (`~/.claude/plans/bar.md`) are expanded to the user's home directory
- Already-absolute paths are passed through unchanged

If the session has no CWD (rare), relative paths are emitted as-is and may fail to open.

```typescript
{ type: "plan-file", path: string }
// path: always absolute, e.g. "/Users/me/project/plans/foo.md",
//       "/Users/me/.claude/plans/graceful-rolling-quasar.md"
```

**Repo scoping:** The built-in `plan` plugin only displays plans from terminals whose CWD matches the active repository in the sidebar. Plans from other projects are silently filtered out.

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
| `claude-status` | 1 | none | **Agent-scoped** (`agentTypes: ["claude"]`), structured events |
| `telegram-notifier` | 1+3 | `net:http`, `ui:panel`, `ui:ticker` | Telegram push notifications, per-event toggles, settings panel |

## Distributable Plugins

Available from the [plugin registry](https://github.com/sstraus/tuicommander-plugins) (submodule at `plugins/`). Installable via Settings > Plugins > Browse.

| Plugin | Tier | Capabilities | Description |
|--------|------|-------------|-------------|
| `mdkb-dashboard` | 2+3 | `exec:cli`, `fs:read`, `ui:panel`, `ui:ticker` | mdkb knowledge base dashboard |
| `rtk-dashboard` | 3 | `exec:cli`, `ui:panel`, `ui:context-menu` | RTK token savings dashboard (`binaries: ["rtk"]`) |

## Troubleshooting

| Problem | Cause | Fix |
|---------|-------|-----|
| Plugin not loading | `manifest.json` missing or malformed | Check console for validation errors |
| `requires app version X.Y.Z` | `minAppVersion` too high | Lower `minAppVersion` or update app |
| `not in the invoke whitelist` | Calling non-whitelisted Tauri command | Only use commands listed in the whitelist table |
| `not declared in plugin ... manifest binaries` | Binary not in manifest `binaries` field | Add the binary name to the `binaries` array in `manifest.json` |
| `requires capability "X"` | Missing capability in manifest | Add the capability to `manifest.json` `capabilities` array |
| Module not found | `main` field doesn't match filename | Ensure `"main": "main.js"` matches your actual file |
| Changes not reflecting | Hot reload cache | Save the file again, or restart the app |
| `default export` error | Module doesn't `export default { ... }` | Ensure your module has a default export with `id`, `onload`, `onunload` |
