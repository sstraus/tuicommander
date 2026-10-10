/// AI-optimized plugin development reference for TUICommander.
/// Compiled as a const so it's zero-cost at runtime.
pub const PLUGIN_DOCS: &str = r###"# TUICommander Plugin Development Reference

## Installation

Create directory `{id}/` containing `manifest.json` + `main.js` under the platform plugins path:
- macOS: `~/Library/Application Support/com.tuic.commander/plugins/`
- Linux: `~/.config/tuicommander/plugins/`
- Windows: `%APPDATA%/com.tuic.commander/plugins/`

Hot reload: editing any file in the plugin directory triggers automatic unload + re-import. Adding a new plugin directory or symlink after the app has started is also discovered live — no restart needed.

## manifest.json

```json
{
  "id": "my-plugin",
  "name": "My Plugin",
  "version": "1.0.0",
  "minAppVersion": "0.3.0",
  "main": "main.js",
  "description": "Optional",
  "author": "Optional",
  "capabilities": [],
  "allowedUrls": ["https://api.example.com/*"],
  "agentTypes": ["claude"],
  "binaries": ["mdkb"]
}
```

Constraints:
- `id` must match directory name exactly, non-empty
- `main` must be a filename only (no path separators or `..`)
- `minAppVersion` must be <= current app version (current: 0.3.x)
- `capabilities`: subset of `pty:write`, `pty:read`, `ui:markdown`, `ui:sound`, `ui:panel`, `ui:ticker`, `ui:context-menu`, `ui:sidebar`, `ui:file-icons`, `ui:file-preview`, `net:http`, `credentials:read`, `invoke:read_file`, `invoke:list_markdown_files`, `fs:read`, `fs:list`, `fs:watch`, `fs:write`, `fs:rename`, `fs:scan`, `fs:delete`, `exec:cli`, `git:read`
- `allowedUrls`: non-empty URL patterns required for `net:http` (supports `*` wildcard for path prefix matching); manifests with an empty or omitted list are rejected
- `agentTypes`: optional array of agent type strings. When set, output watchers and structured event handlers only fire for terminals running a matching agent. Omit or use `[]` for universal plugins. Valid terminal values: `claude`, `gemini`, `opencode`, `aider`, `codex`, `amp`, `cursor`, `goose`, `grok`, `droid`, `pi`, `ego`, `git`.
- `binaries`: optional array of CLI binary names this plugin may execute via `exec:cli` (e.g. `["rtk", "mdkb"]`). The on-disk manifest is the source of truth — binaries not declared here are rejected.
- Module default export must have `id`, `onload(host)`, `onunload()`

## Complete main.js Template

```javascript
const PLUGIN_ID = "my-plugin";
const SECTION_ID = "my-section";
const ICON = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><circle cx="8" cy="8" r="6"/></svg>';

export default {
  id: PLUGIN_ID,
  onload(host) {
    // --- Section in Activity Center dropdown ---
    host.registerSection({
      id: SECTION_ID,
      label: "MY SECTION",
      priority: 30,       // lower = higher position
      canDismissAll: true, // show "Dismiss All" button
    });

    // --- Watch terminal output (ANSI-stripped lines) ---
    host.registerOutputWatcher({
      pattern: /some pattern (\S+)/,
      onMatch(match, sessionId) {
        // MUST be synchronous and fast (<1ms) -- PTY hot path
        // match[0] = full match, match[1]+ = capture groups
        // Input is ANSI-stripped but may contain Unicode
        // pattern.lastIndex is reset before each test (safe to use global flag)
        host.addItem({
          id: `${PLUGIN_ID}:${match[1]}`,
          pluginId: PLUGIN_ID,
          sectionId: SECTION_ID,
          title: match[1],
          subtitle: "Optional secondary text",
          icon: ICON,
          iconColor: "#3fb950",   // optional, CSS color or var(--css-var)
          dismissible: true,
          contentUri: "my-scheme:detail?id=" + encodeURIComponent(match[1]),
          // OR onClick: () => { ... }  (mutually exclusive with contentUri)
        });
      },
    });

    // --- Provide markdown content for contentUri clicks ---
    host.registerMarkdownProvider("my-scheme", {
      async provideContent(uri) {
        const id = uri.searchParams.get("id");
        if (!id) return null;
        return `# Detail for ${id}\n\nMarkdown content here.`;
      },
    });

    // --- Handle structured events from Rust OutputParser ---
    host.registerStructuredEventHandler("plan-file", (payload, sessionId) => {
      const { path } = payload;
      // See "Structured Event Types" section for all types and payloads
    });
  },
  onunload() {
    // All registrations are auto-disposed; only clean up custom resources here
  },
};
```

Icons: monochrome inline SVG, `fill="currentColor"`, `viewBox="0 0 16 16"`. Never emoji.
Content URIs: `scheme:path?key=value` (e.g. `plan:file?path=%2Frepo%2Fplans%2Ffoo.md`)

## Panel CSS Design Strategy

Every `ui:panel` iframe gets two automatic injections: (1) a **base stylesheet** with reset, themed typography, buttons (`.primary`, `.danger`), inputs, `.card`, tables, `.badge` variants (`.badge-p1`/`.badge-error`/`.badge-success`/`.badge-accent`/`.badge-muted`), `.filter-bar`, `.empty-state`, `.toast` (`.error`/`.success` + `.show`), scrollbars, AND the dashboard class family (`.dashboard`, `.dash-header`, `.dash-title`, `.dash-subtitle`, `.dash-section`, `.dash-section-title`, `.dash-section-hint`, `.dash-stat-grid`, `.dash-stat`, `.dash-stat-label`, `.dash-stat-value`, `.dash-stat-sub`, `.dash-meter`, `.dash-meter-fill` + `.ok`/`.warn`/`.critical`, `.num`); (2) all **CSS theme variables** (`--bg-primary`, `--bg-secondary`, `--bg-tertiary`, `--bg-highlight`, `--fg-primary`, `--fg-secondary`, `--fg-muted`, `--accent`, `--accent-hover`, `--success`, `--warning`, `--error`, `--border`, `--text-on-accent`, `--text-on-error`, `--text-on-success`).

**Write minimal CSS** — only plugin-specific layout. Standard elements are styled automatically. Example:
```html
<style>.my-grid { display: grid; gap: 8px; }</style>
```

Additional base classes: `.btn` is the button-style alias, `.btn-primary` and `.btn-danger` are its action variants. `.badge-p2`/`.badge-warning` use warning colors; `.badge-p3` uses muted colors. `.hint` is secondary helper text. Toast modifiers `.show`, `.error` and `.success`, and meter modifiers `.ok`, `.warn` and `.critical`, apply to their base element, not standalone containers.

## MANDATORY — Dashboard Style Guide

**If your plugin renders a dashboard (analytics, status, reports, summaries) you MUST use the shared `.dashboard`/`.dash-*` classes from the base stylesheet.** Hand-rolling layout/card/stat CSS is forbidden — dashboards must look like a native part of TUICommander, not a third-party widget. The native Claude Usage dashboard is the reference.

Rules:
1. Wrap the entire dashboard in `<div class="dashboard">`.
2. Use `.dash-header` with `.dash-title` for the top bar (title + optional refresh button).
3. Group content in `.dash-section` + `.dash-section-title` (uppercase, muted).
4. Use `.dash-stat-grid` + `.dash-stat` (with `.dash-stat-label` / `.dash-stat-value` / optional `.dash-stat-sub`) for headline numbers — NEVER invent `.stat-card`/`.stat-grid` variants.
5. Never hardcode colors — use CSS variables listed above. No `#rrggbb` literals, no fixed backgrounds.
6. Never override `h1`/`h2`/`h3` sizes, `.card` padding, or table typography. Those are global.
7. Call `host.registerDashboard({ label, icon, open })` so *Settings → Plugins* shows a one-click Dashboard button for your plugin.

Minimum dashboard skeleton:
```html
<div class="dashboard">
  <div class="dash-header">
    <h1 class="dash-title">My Plugin</h1>
    <button class="primary" id="refresh">Refresh</button>
  </div>
  <div class="dash-section">
    <h2 class="dash-section-title">Overview</h2>
    <div class="dash-stat-grid">
      <div class="dash-stat">
        <div class="dash-stat-label">Commands</div>
        <div class="dash-stat-value">1.2k</div>
      </div>
    </div>
  </div>
</div>
```

Full reference, checklist, and do/don'ts: `docs/plugins-style.md` in the TUICommander repo. Treat that file as authoritative — if it conflicts with this reference, it wins.

Plugin packages are external desktop ES modules; browser/PWA clients do not import them. Desktop hot reload watches `.js`, `.mjs` and `.json` code outside `data/` and new top-level directories/symlinks. The watcher does not follow symlink targets. There is no `BUILTIN_PLUGINS` registration list.

## Source-checked surface inventories

These inventories are checked against the capability validator, the host object and the injected stylesheet. The API sections below explain how to use them. CSS names are class tokens: use a leading dot in selectors, not in HTML class attributes.

### Capability inventory
`credentials:read`, `exec:cli`, `fs:delete`, `fs:list`, `fs:read`, `fs:rename`, `fs:scan`, `fs:watch`, `fs:write`, `git:read`, `invoke:list_markdown_files`, `invoke:read_file`, `net:http`, `pty:read`, `pty:write`, `ui:context-menu`, `ui:file-icons`, `ui:file-preview`, `ui:markdown`, `ui:panel`, `ui:sidebar`, `ui:sound`, `ui:ticker`.

### Host method inventory
`addItem`, `clearTicker`, `deleteBuildArtifact`, `execCli`, `getActiveRepo`, `getActiveRepoPath`, `getActiveTerminalSessionId`, `getClaudeProjectDir`, `getGitBranches`, `getGitDiff`, `getPrNotifications`, `getRecentCommits`, `getRepoPathForSession`, `getRepos`, `getSessionCwd`, `getSettings`, `getTerminalState`, `httpFetch`, `invoke`, `listDirectory`, `log`, `onStateChange`, `openEditorTab`, `openMarkdownFile`, `openMarkdownFileBackground`, `openMarkdownPanel`, `openPanel`, `playNotificationSound`, `readCredential`, `readFile`, `readFileBase64`, `readFileTail`, `readFiles`, `readSessionOutput`, `registerCommand`, `registerContextMenuAction`, `registerDashboard`, `registerFileIconProvider`, `registerFilePreview`, `registerMarkdownProvider`, `registerOutputWatcher`, `registerSection`, `registerSidebarPanel`, `registerStructuredEventHandler`, `registerTerminalAction`, `removeItem`, `renamePath`, `scanBuildArtifacts`, `sendAgentInput`, `setTicker`, `trimBuildArtifact`, `updateItem`, `watchPath`, `writeFile`, `writeFileBase64`, `writePty`.

### Base CSS class inventory
`badge`, `badge-accent`, `badge-error`, `badge-muted`, `badge-p1`, `badge-p2`, `badge-p3`, `badge-success`, `badge-warning`, `btn`, `btn-danger`, `btn-primary`, `card`, `critical`, `danger`, `dash-header`, `dash-meter`, `dash-meter-fill`, `dash-section`, `dash-section-hint`, `dash-section-title`, `dash-stat`, `dash-stat-grid`, `dash-stat-label`, `dash-stat-sub`, `dash-stat-value`, `dash-subtitle`, `dash-title`, `dashboard`, `empty-state`, `error`, `filter-bar`, `hint`, `num`, `ok`, `primary`, `show`, `success`, `toast`, `warn`.

## PluginHost API

All Tier 1-2 methods are always available. Tier 3-4 require capabilities in manifest.json; calling without capability throws `PluginCapabilityError`.

### Tier 0: Logging (always available)

```typescript
host.log(level: "debug" | "info" | "warn" | "error", message: string, data?: unknown): void
```

Writes to the plugin's ring buffer (500 entries max), visible in Settings > Plugins > Logs.
Errors from onload/onunload/watchers/handlers are auto-captured. Use host.log() for extra diagnostics.

### Tier 1: Activity Center (always available)

All register* methods return a Disposable (auto-cleaned on unload — no need to manually dispose).

```typescript
// Section heading in Activity Center dropdown
host.registerSection({ id: string, label: string, priority: number, canDismissAll: boolean })

// Watch PTY output lines (ANSI-stripped)
host.registerOutputWatcher({ pattern: RegExp, onMatch(match: RegExpExecArray, sessionId: string): void })

// Handle structured events from Rust OutputParser
host.registerStructuredEventHandler(type: string, handler: (payload: unknown, sessionId: string) => void)

// Provide content when user clicks an item with contentUri
host.registerMarkdownProvider(scheme: string, { provideContent(uri: URL): Promise<string | null> })

// Custom file-tree icons (requires "ui:file-icons"). Last provider wins.
host.registerFileIconProvider({ resolveFileIcon(name: string, isDir: boolean): string | null })

// Custom file preview/open for given extensions (requires "ui:file-preview")
host.registerFilePreview({ extensions: string[], onOpen(ctx: { filePath: string, repoPath: string, fsRoot: string }): void })

// Manage activity items
host.addItem({
  id: string,             // Unique identifier
  pluginId: string,       // Must match your plugin id
  sectionId: string,      // Must match registered section
  title: string,          // Primary text (truncated with ellipsis)
  subtitle?: string,      // Secondary text (truncated with ellipsis)
  icon: string,           // Inline SVG with fill="currentColor"
  iconColor?: string,     // CSS color for the icon
  dismissible: boolean,
  contentUri?: string,    // Opens MarkdownTab on click
  onClick?: () => void,   // Custom click handler (mutually exclusive with contentUri)
})
host.updateItem(id: string, updates: Partial<ActivityItem>)
host.removeItem(id: string)
```

### Tier 2: Read-Only State (always available)

```typescript
host.getActiveRepo()       // { path, displayName, activeBranch, worktreePath } | null
host.getRepos()            // [{ path, displayName }]
host.getActiveTerminalSessionId()  // string | null
host.getRepoPathForSession(sessionId: string)  // string | null (repo owning this terminal session)
host.getSessionCwd(sessionId: string)          // string | null (working directory of this terminal session)
host.getActiveRepoPath()                        // string | null (active repo path, shortcut for getActiveRepo()?.path)
host.getClaudeProjectDir(repoPath: string)     // Promise<string | null> — requires "fs:read"
host.getPrNotifications()  // [{ id, repoPath, branch, prNumber, title, type }]
host.getSettings(repoPath: string) // { path, displayName, baseBranch, color } | null
host.getTerminalState()    // { sessionId, shellState, agentType, agentActive, awaitingInput, repoPath } | null
host.onStateChange(cb)     // Disposable — agent-started/stopped, repo-changed, branch-changed, shell-state-changed, awaiting-input-changed
```

Final shell/awaiting transitions emitted together with session teardown retain the old session ID. Replacement sessions establish a new baseline without replaying initial state.

### Tier 2b: Git Read (requires `git:read` capability)

```typescript
host.getGitBranches(repoPath)           // [{ name, isCurrent }]
host.getRecentCommits(repoPath, count?) // [{ hash, message, author, date }]
host.getGitDiff(repoPath, scope?)       // unified diff string (scope: "staged" | "unstaged")
```

### Tier 3: Write Actions (capability-gated)

| Method | Capability |
|--------|------------|
| `await host.writePty(sessionId: string, data: string): Promise<void>` | `pty:write` |
| `await host.sendAgentInput(sessionId: string, text: string): Promise<void>` | `pty:write` |
| `await host.readSessionOutput(sessionId: string, maxLines?: number): Promise<string>` | `pty:read` |
| `host.openMarkdownPanel(title: string, contentUri: string): void` | `ui:markdown` |
| `host.openMarkdownFile(absolutePath: string): void` | `ui:markdown` |
| `host.openMarkdownFileBackground(absolutePath: string): boolean` — opens a pinned background tab; false when no registered repo owns the path | `ui:markdown` |
| `host.openEditorTab(filePath: string, repoPath: string, opts?: { fsRoot?: string, line?: number }): void` | `ui:panel` |
| `await host.playNotificationSound(sound?: "question" \| "error" \| "completion" \| "warning" \| "info"): Promise<void>` | `ui:sound` |
| `host.openPanel({ id, title, html, onMessage? }): PanelHandle` | `ui:panel` |
| `host.setTicker({ id, text, label?, icon?, priority?, ttlMs?, onClick? }): void` | `ui:ticker` |
| `host.clearTicker(id: string): void` | `ui:ticker` |
| `await host.readCredential(serviceName: string): Promise<string \| null>` | `credentials:read` |
| `await host.writeFile(absolutePath: string, content: string): Promise<void>` | `fs:write` |
| `await host.renamePath(from: string, to: string): Promise<void>` | `fs:rename` |
| `await host.scanBuildArtifacts(repoPaths: string[], options?: { forceRefresh?: boolean }): Promise<ArtifactEntry[]>` | `fs:scan` |
| `await host.deleteBuildArtifact(path: string, repoPaths: string[]): Promise<void>` | `fs:delete` |
| `await host.trimBuildArtifact(path: string, repoPaths: string[]): Promise<void>` — removes only the artifact's regenerable intermediates, keeping the built executables. Prefer over `deleteBuildArtifact`; offer it when `ArtifactEntry.trimmable_bytes > 0`. | `fs:delete` |
| `await host.httpFetch(url: string, options?): Promise<HttpResponse>` | `net:http` |
| `host.registerTerminalAction({ id, label, action(ctx), disabled?(ctx) }): Disposable` | `ui:context-menu` |
| `host.registerContextMenuAction({ id, label, target, action(ctx), disabled?(ctx) }): Disposable` | `ui:context-menu` |
| `host.registerSidebarPanel({ id, label, icon?, priority?, collapsed? }): SidebarPanelHandle` | `ui:sidebar` |
| `host.registerDashboard({ label?, icon?, open }): Disposable` | *(none)* |
| `host.registerCommand({ id, title, defaultShortcut?, run }): Disposable` | *(none)* |

TerminalActionContext (passed to action/disabled at right-click time): `{ sessionId: string | null, repoPath: string | null }`. Actions appear in terminal right-click "Actions" submenu. Submenu hidden when no actions registered. Stale handlers (after plugin unload) are no-ops.

ContextMenuAction targets: `"terminal"`, `"branch"`, `"repo"`, `"tab"`. ContextMenuContext: `{ target, sessionId?, repoPath?, branchName?, tabId? }`. Actions appear after built-in items in the respective context menus.

SidebarPanelHandle: `{ setItems(items), setBadge(text), dispose() }`. SidebarItem: `{ id, label, subtitle?, icon?, iconColor?, onClick?, contextMenu?: [{ label, action, disabled? }] }`. Panels appear below branches in the sidebar, scoped per-repo. Badge shows a counter pill on the header.

PanelHandle: `{ tabId, update(html), close(), send(data, transfer?) }` — HTML rendered in an iframe with `sandbox="allow-scripts allow-same-origin"` (not host-DOM isolation), automatic base stylesheet + CSS theme variable injection. Write minimal plugin-specific CSS only (see Panel CSS Design Strategy above). Use `onMessage` callback to receive messages from iframe, `send()` to post messages back. `send(data)` delivers data to the iframe SDK `tuic.onMessage` through a `tuic:host-message` envelope. Panels and bridges close on plugin unload; clean up your own timers/listeners in `onunload`. Pass an optional `Transferable[]` to transfer ownership of large buffers instead of copying them. Every iframe also receives the TUIC SDK (`window.tuic`) — use `tuic.open(path, {pinned?})` to open markdown files, `tuic.edit(path, {line?})` to open files in the code editor, `tuic.terminal(repoPath)` to open terminals, `tuic.openUrl(url)` to open http/https/mailto URLs externally, or `<a href="tuic://open/path">` / `<a href="tuic://edit/path?line=10">` links for automatic interception. Absolute http/https/mailto links in inline panels also open externally when clicked. Paths must be within a known repo; cross-origin URL panels cannot expose their clicks, so use the tab menu's Open in Browser action.

`registerDashboard({ label?, icon?, open })`: Register a one-click entry point shown as a **Dashboard** button in *Settings → Plugins*. The host closes the Settings panel automatically before calling `open()`. A plugin may only register one dashboard; second calls replace the first. Pair this with the `.dashboard` style guide above.

`registerCommand({ id, title, defaultShortcut?, run })`: Register a user-rebindable command. Appears in *Settings → Keyboard Shortcuts* under "Plugin Commands". Action name is auto-namespaced as `plugin:<pluginId>:<id>`. `defaultShortcut` is optional ("Cmd+Shift+K" etc.); on conflict the command is registered but left unbound with a warning.
HttpResponse: `{ status: number, headers: Record<string, string>, body: string }` — non-2xx is NOT an error.

**setTicker notes:** Shared ticker area rotates messages from all plugins. Priority tiers: <10 = popover only, 10-99 = auto-rotate (5s), >=100 = urgent pin. `label` is shown as source prefix (e.g. "Usage · 5h: 42%"). Counter badge (1/3 ▸) shown when multiple tickers active. Click badge to cycle, right-click for popover. Removed aliases `postTickerMessage`/`removeTickerMessage` are not supported.

**httpFetch notes:** Localhost blocked unless declared in `allowedUrls`. Max 5 redirects. 30s timeout, 10 MB limit. An empty `allowedUrls` list grants no HTTP access.

### Tier 3b: Filesystem Operations (capability-gated)

All paths must be absolute and within `$HOME`. Resolved via canonicalize (symlinks, `..` resolved).

```typescript
// Resolve a repo path to its Claude Code project directory
const projectDir = await host.getClaudeProjectDir("/Users/me/my-project");  // requires "fs:read"
// → "/Users/me/.claude/projects/-Users-me-my-project"

// Read a file (max 10 MB, UTF-8)
const content = await host.readFile(`${projectDir}/conversation.jsonl`);  // requires "fs:read"

// Read up to 1000 files in one round trip, in request order.
// An unreadable or vanished file returns null in its slot.
const contents = await host.readFiles(absolutePaths);  // requires "fs:read"

// Read a binary file as base64 (10 MiB default; positive per-call maxBytes,
// clamped by the host to 512 MiB)
const encoded = await host.readFileBase64("/Users/me/Documents/spec.docx");  // requires "fs:read"
const database = await host.readFileBase64("/Users/me/data.sqlite", { maxBytes: 256 * 1024 * 1024 });

// Atomically replace a binary file from Base64 (10 MiB default; custom maxBytes
// is clamped to 512 MiB)
await host.writeFileBase64("/Users/me/data.sqlite", encodedDatabase, { maxBytes: 256 * 1024 * 1024 });  // requires "fs:write"

// Read last N bytes of a file (skip partial first line)
const tail = await host.readFileTail("/Users/me/.claude/hud-tracking.jsonl", 512 * 1024);  // requires "fs:read"

// List directory (optional glob filter, optional sort: "name" default or "mtime" newest-first)
const files = await host.listDirectory(projectDir, "*.jsonl");  // requires "fs:list"
const recent = await host.listDirectory(projectDir, "*.jsonl", { sortBy: "mtime" });

// Watch for changes (returns Disposable)
const watcher = await host.watchPath(  // requires "fs:watch"
  "/Users/me/.claude/projects/foo",
  (events) => { /* FsChangeEvent[] with type + path */ },
  { recursive: true, debounceMs: 500 },
);
watcher.dispose();  // stop watching

// Write a file (max 10 MB, creates parent dirs)
await host.writeFile("/Users/me/project/stories/new-file.md", content);  // requires "fs:write"

// Rename/move a file
await host.renamePath("/Users/me/project/old.md", "/Users/me/project/new.md");  // requires "fs:rename"
```

FsChangeEvent: `{ type: "create" | "modify" | "delete", path: string }`

### CLI Execution (exec:cli)

`await host.execCli(binary: string, args: string[], cwd?: string): Promise<string>` — execute a CLI binary declared in the plugin's manifest `binaries` field, return stdout.

Only binaries listed in `manifest.json` `binaries` array are allowed. Working directory must be absolute and within `$HOME`. 30s timeout, 5 MB stdout limit.

```javascript
const raw = await host.execCli("mdkb", ["--format", "json", "status"], repoPath);
const status = JSON.parse(raw);
```

### Tier 4: Tauri Invoke (whitelisted only)

`await host.invoke<T>(cmd: string, args?: object): Promise<T>` — non-whitelisted commands throw immediately.

| Command | Args | Returns | Capability |
|---------|------|---------|------------|
| `read_file` | `{ path: string, file: string }` | `string` | `invoke:read_file` |
| `list_markdown_files` | `{ path: string }` | `Array<{ path, git_status }>` | `invoke:list_markdown_files` |
| `read_plugin_data` | `{ plugin_id: string, path: string }` | `string` | none |
| `write_plugin_data` | `{ plugin_id: string, path: string, content: string }` | `void` | none |
| `delete_plugin_data` | `{ plugin_id: string, path: string }` | `void` | none |
| `get_input_buffer_content` | `{ sessionId: string }` | `string` | `pty:read` |

Plugin data sandboxed per-plugin (no capability required):
```javascript
// Store data
await host.invoke("write_plugin_data", {
  plugin_id: PLUGIN_ID,
  path: "cache.json",
  content: JSON.stringify({ lastCheck: Date.now() }),
});
// Read data
const raw = await host.invoke("read_plugin_data", {
  plugin_id: PLUGIN_ID,
  path: "cache.json",
});
const cache = JSON.parse(raw);
// Delete data
await host.invoke("delete_plugin_data", {
  plugin_id: PLUGIN_ID,
  path: "cache.json",
});
```

## Core-rendered plugin UX

Core renders the following Solid components. Plugins supply manifest metadata and host registrations; they do not import or mount these components.

### Manifest UX field inventory
`agentTypes`, `allowedUrls`, `author`, `binaries`, `capabilities`, `description`, `id`, `main`, `minAppVersion`, `name`, `version`.

### Core UX component inventory
`BrowseRow`, `PluginLogViewer`, `PluginRow`, `PluginsTab`.

#### PluginsTab

Settings > Plugins has Installed and Browse views. Installed lists loaded/discovered plugin state. Browse reads community registry entries and offers install, update and refresh actions. Installation accepts ZIP archives. The manifest `id` identifies the package, `main` selects its module, and `minAppVersion` controls compatibility. `author` is optional metadata. `allowedUrls`, `agentTypes` and `binaries` drive host access/event routing, not editable settings controls.

#### PluginRow

The Installed row shows manifest `name` (falls back to `id`), `version`, `description` and `capabilities`. It shows load errors, error counts and available updates. Its toggle persists enabled state; Logs expands PluginLogViewer; external packages can be uninstalled. A README.md adds a documentation button. `host.registerDashboard({ label?, icon?, open })` adds a dashboard button: core closes Settings before calling `open`. Register only one dashboard per plugin. `host.registerCommand({ id, title, defaultShortcut?, run })` adds a rebindable entry under Settings > Keyboard Shortcuts > Plugin Commands.

#### PluginLogViewer

The expanded Logs view shows the plugin ring buffer's timestamp, level and message, or an empty state. Use `host.log(level, message, data?)`; host boundary errors are captured automatically.

#### BrowseRow

The Browse row shows registry name, version, description, author, tags and minimum app version. It offers Install for an absent package, Update when a newer version is available, or Installed for the current version. Registry metadata is separate from the installed manifest.

### Settings schema and Tasks views

The current PluginManifest has no settings schema, field-kind union or generic Tasks-view declaration. Core does not render schema-driven plugin settings or generic plugin Tasks views. Do not invent manifest fields or import core Settings components. Plugins can open their own UI with `host.openPanel` or `host.openMarkdownPanel`. When a settings schema or Tasks-view API is added, document its fields, kinds and core-rendered controls here and extend the source-derived UX drift check to its real schema/renderer. The manifest field and host method inventories fail on new API entries until the guide is updated.

## Structured Event Types

The Rust OutputParser detects patterns in terminal output and emits typed events. Handle them with `host.registerStructuredEventHandler(type, handler)`.

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
  pattern_name: string,      // e.g. "claude-http-429", "openai-http-429"
  matched_text: string,      // the matched substring
  retry_after_ms: number | null,  // ms to wait (default 60000)
}
```
Pattern names: `claude-http-429`, `claude-overloaded`, `openai-http-429`, `cursor-rate-limit`, `gemini-resource-exhausted`, `http-429`, `retry-after-header`, `openai-retry-after`, `openai-tpm-limit`, `openai-rpm-limit`.

### status-line
Detected when an AI agent emits a status/progress line.
```typescript
{
  type: "status-line",
  task_name: string,         // e.g. "Reading files"
  full_line: string,         // complete line trimmed
  time_info: string | null,  // e.g. "12s"
  token_info: string | null, // e.g. "2.4k tokens"
}
```

### pr-url
Detected when a GitHub/GitLab PR/MR URL appears in output.
```typescript
{
  type: "pr-url",
  number: number,    // PR/MR number
  url: string,       // full URL
  platform: string,  // "github" or "gitlab"
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
Detected when an interactive prompt appears.
```typescript
{
  type: "question",
  prompt_text: string,  // the question line (ANSI-stripped)
}
```
Matches: Y/N prompts, numbered menus, inquirer-style prompts, "Would you like..." patterns.

### usage-limit
Detected when Claude Code reports usage limits.
```typescript
{
  type: "usage-limit",
  percentage: number,   // 0-100
  limit_type: string,   // "weekly" or "session"
}
```

## Example Plugins

### Tier 1: Hello World (output watcher)
```javascript
const PLUGIN_ID = "hello-world";
const ICON = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M8 1a7 7 0 1 0 0 14A7 7 0 0 0 8 1z"/></svg>';
let count = 0;
export default {
  id: PLUGIN_ID,
  onload(host) {
    host.registerSection({ id: "hellos", label: "HELLOS", priority: 50, canDismissAll: true });
    host.registerOutputWatcher({
      pattern: /hello\s+(\w+)/i,
      onMatch(match, sessionId) {
        host.addItem({
          id: `hello:${++count}`,
          pluginId: PLUGIN_ID, sectionId: "hellos",
          title: `Hello ${match[1]}!`,
          subtitle: `Session ${sessionId}`,
          icon: ICON, dismissible: true,
        });
      },
    });
  },
  onunload() {},
};
```

### Tier 3: Auto-Confirm (PTY write)
Requires `"capabilities": ["pty:write"]` in manifest.json.
```javascript
const PLUGIN_ID = "auto-confirm";
export default {
  id: PLUGIN_ID,
  onload(host) {
    host.registerSection({ id: "confirms", label: "AUTO-CONFIRM", priority: 40, canDismissAll: true });
    host.registerOutputWatcher({
      pattern: /\[y\/N\]|\[Y\/n\]|\(y\/n\)/i,
      onMatch(match, sessionId) {
        host.sendAgentInput(sessionId, "y").catch(err => console.error("[auto-confirm]", err));
        host.addItem({
          id: `confirm:${Date.now()}`,
          pluginId: PLUGIN_ID, sectionId: "confirms",
          title: "Auto-confirmed", subtitle: match[0],
          icon: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.75.75 0 1 1 1.06-1.06L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0z"/></svg>',
          iconColor: "#3fb950", dismissible: true,
        });
      },
    });
  },
  onunload() {},
};
```

### Tier 2: Repo Dashboard (read-only state + markdown)
```javascript
const PLUGIN_ID = "repo-dashboard";
let hostRef = null;
export default {
  id: PLUGIN_ID,
  onload(host) {
    hostRef = host;
    host.registerSection({ id: "dashboard", label: "DASHBOARD", priority: 10, canDismissAll: false });
    host.addItem({
      id: "dashboard:overview", pluginId: PLUGIN_ID, sectionId: "dashboard",
      title: "Repo Dashboard", subtitle: "View repository overview",
      icon: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor"><path d="M2 2h12v12H2z"/></svg>',
      dismissible: false,
      contentUri: "dashboard:overview",
    });
    host.registerMarkdownProvider("dashboard", {
      provideContent() {
        if (!hostRef) return null;
        const active = hostRef.getActiveRepo();
        const repos = hostRef.getRepos();
        const prs = hostRef.getPrNotifications();
        let md = "# Repository Dashboard\n\n";
        if (active) {
          md += `**Active:** ${active.displayName} (${active.activeBranch})\n\n`;
        }
        md += `**Repositories:** ${repos.length}\n\n`;
        if (prs.length > 0) {
          md += "## Open PRs\n\n| PR | Branch | Type |\n|---|---|---|\n";
          for (const pr of prs) {
            md += `| #${pr.prNumber} ${pr.title} | ${pr.branch} | ${pr.type} |\n`;
          }
        }
        return md;
      },
    });
  },
  onunload() { hostRef = null; },
};
```

## Build (TypeScript plugins)

```bash
esbuild src/main.ts --bundle --format=esm --outfile=main.js --external:nothing
```

## Lifecycle

1. Discovery: Rust scans plugins dir for manifest.json
2. Validation: manifest fields + minAppVersion check
3. Import: `import("plugin://{id}/main.js")` (custom URI protocol; on Windows rewritten to `http://plugin.localhost/{id}/main.js` for WebView2)
4. Module check: default export must have id, onload, onunload
5. Register: pluginRegistry.register() calls plugin.onload(host)
6. Active: plugin receives PTY lines, structured events, uses PluginHost API
7. Hot reload: file changes trigger unregister + re-import (cache-busted); a newly added plugin directory/symlink is discovered and loaded live
8. Unload: plugin.onunload() called, tracked registrations disposed, openPanel tabs and bridges closed; plugin-owned timers/listeners need explicit cleanup

Crash safety: all boundaries are try/catch wrapped. A broken plugin produces a console error and is skipped. The app always continues.

## Worktree Management via MCP

Models MUST use the MCP HTTP API to create worktrees — never raw `git worktree add` directly.
Using the API ensures user settings (storage strategy, path conventions) are respected.
For the native `repo` MCP tool, pass the repository root as `path` and a linked
worktree's branch as `branch` to `worktree_lifecycle` or `worktree_remove`.
`worktree_create` still returns both `branch` and `workspace_id` for parity with HTTP.
Its `spawn_session=true` option opens a bare shell PTY; launch an agent separately.

### Creating a worktree

```
POST /worktrees
Content-Type: application/json

{
  "base_repo": "/path/to/repo",   // required: absolute path to the main repo
  "branch_name": "my-feature",    // required: branch name only — you choose this
  "base_ref": "main"              // optional: start point (commit/branch). Defaults to HEAD
}
```

**Model responsibility:** Provide only the `branch_name`. The storage path, directory naming,
and worktree location are determined entirely by user configuration. Do not attempt to compute
or specify the worktree path — the API handles it.

**Response (201 Created):**
```json
{
  "name": "my-feature",
  "path": "/path/to/worktrees/my-feature",
  "branch": "my-feature",
  "base_repo": "/path/to/repo"
}
```

### Listing worktrees

```
GET /worktrees
```

Returns active worktree sessions (worktrees with open PTY sessions).

### Removing a worktree

```
DELETE /worktrees/{branch}?repoPath=/path/to/repo&deleteBranch=true
```

### Getting the configured worktree directory

```
GET /worktrees/dir?repo_path=/path/to/repo
```

Returns the directory where worktrees for this repo will be created (based on user config).
Useful for informational purposes — do not use this to construct paths yourself.

## Troubleshooting

| Error | Fix |
|-------|-----|
| Plugin not loading | Check console for manifest validation errors |
| `requires app version X.Y.Z` | Lower minAppVersion or update app |
| `not in the invoke whitelist` | Only whitelisted commands allowed (see Tier 4 table) |
| `requires capability "X"` | Add to manifest.json capabilities array |
| Module not found | main field must match filename |
| Changes not reflecting | Save again or restart app |
| `default export` error | Must export default { id, onload, onunload } |
"###;

#[cfg(test)]
mod tests {
    use super::PLUGIN_DOCS;
    use regex::Regex;
    use std::collections::BTreeSet;

    const PLUGINS: &str = include_str!("../plugins.rs");
    const HOST: &str = include_str!("../../../src/plugins/pluginRegistry.ts");
    const CSS: &str = include_str!("../../../src/components/PluginPanel/pluginBaseStyles.ts");
    const UX: &str = include_str!("../../../src/components/SettingsPanel/tabs/PluginsTab.tsx");

    fn captures(source: &str, pattern: &str) -> BTreeSet<String> {
        Regex::new(pattern)
            .unwrap()
            .captures_iter(source)
            .map(|c| c[1].to_owned())
            .collect()
    }

    fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        source
            .split_once(start)
            .expect("source section must exist")
            .1
            .split_once(end)
            .expect("source section must end")
            .0
    }

    fn inventory(guide: &str, heading: &str) -> BTreeSet<String> {
        captures(section(guide, heading, "\n\n"), r"`([^`]+)`")
    }

    fn assert_inventory(guide: &str, heading: &str, expected: BTreeSet<String>) {
        assert!(
            !expected.is_empty(),
            "source extraction must not pass by absence"
        );
        assert_eq!(
            inventory(guide, heading),
            expected,
            "guide drift in {heading}"
        );
    }

    // Catches: added or removed capabilities/methods/classes leave agents with a stale API.
    #[test]
    fn plugin_docs_matches_live_plugin_surface() {
        assert_inventory(
            PLUGIN_DOCS,
            "### Capability inventory\n",
            captures(
                section(PLUGINS, "const KNOWN_CAPABILITIES: &[&str] = &[", "];"),
                r#""([^"]+)""#,
            ),
        );
        assert_inventory(
            PLUGIN_DOCS,
            "### Host method inventory\n",
            captures(
                section(HOST, "function buildHost(", "// Lifecycle"),
                r"(?m)^\t\t\t(?:async )?([A-Za-z][A-Za-z0-9]*)(?:<[^>]+>)?\(",
            ),
        );
        let documented_methods = captures(PLUGIN_DOCS, r"host\.([A-Za-z][A-Za-z0-9]*)\(");
        assert_eq!(
            documented_methods,
            inventory(PLUGIN_DOCS, "### Host method inventory\n"),
            "host methods need actual usage documentation, not only an inventory"
        );
        let css = Regex::new(r"(?s)/\*.*?\*/").unwrap().replace_all(CSS, "");
        let selectors = css
            .split('}')
            .filter_map(|rule| rule.split_once('{').map(|(s, _)| s))
            .collect::<Vec<_>>()
            .join("\n");
        assert_inventory(
            PLUGIN_DOCS,
            "### Base CSS class inventory\n",
            captures(&selectors, r"\.([a-zA-Z][a-zA-Z0-9_-]*)"),
        );
    }

    // Catches: new manifest-driven UI or core plugin components are absent from the UX guide.
    #[test]
    fn plugin_docs_matches_core_plugin_ux() {
        let manifest = section(PLUGINS, "pub struct PluginManifest {", "\n}");
        let field = Regex::new(r#"(?s)(?:#\[serde\(([^]]*)\)\]\s*)?pub ([a-z_]+):"#).unwrap();
        let fields = field
            .captures_iter(manifest)
            .map(|c| {
                c.get(1)
                    .and_then(|attr| {
                        Regex::new(r#"rename = "([^"]+)""#)
                            .unwrap()
                            .captures(attr.as_str())
                            .map(|rename| rename[1].to_owned())
                    })
                    .unwrap_or_else(|| c[2].to_owned())
            })
            .collect();
        assert_inventory(PLUGIN_DOCS, "### Manifest UX field inventory\n", fields);
        assert_inventory(
            PLUGIN_DOCS,
            "### Core UX component inventory\n",
            captures(UX, r"(?m)^(?:export )?const ([A-Za-z]+): Component"),
        );
        for component in inventory(PLUGIN_DOCS, "### Core UX component inventory\n") {
            assert!(
                PLUGIN_DOCS.contains(&format!("#### {component}\n")),
                "each core component needs UX instructions: {component}"
            );
        }
    }
}
