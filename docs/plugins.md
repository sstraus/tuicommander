# Plugin Authoring Guide

## Scoped plugin settings (#1785-c7a1)

Loaded plugins expose `plugins.<id>` descriptors through the Rust settings owner.
`plugin_settings_action(pluginId, action)` and `POST /api/plugins/{id}/settings`
share describe/get/set/reset/patch semantics. The backend fixes both the owner and
`plugins/<id>/data/config.json`; requests cannot choose a store, target or file.
Get requires explicit paths; writes require the last `expectedRevision`.

A configurable manifest declares `settings: { schema, defaults }`. The schema is
closed (`additionalProperties: false`), bounded to 128 annotated leaves, and uses
only the supported object, scalar and scalar-array constraints. Every leaf needs
`x-setting` sensitivity, apply mode, effect, operations and `confirmationClass`.
Opaque objects, unsupported constraints, missing metadata and secret defaults are
rejected. A plugin with an existing config file and no schema cannot load or write
that config. Other plugin data, including statistics, remains separate.
Tasks setup stores GitHub Work, Jira and GitLab provider account selections in
`data/work-items.json` through the plugin data API, not the Settings catalogue.
Configure these providers in Tasks setup; credential accounts stay in Settings > Accounts.

Core supplies typed migrations for build-cleaner, at-capacity-retry,
cache-keepalive, claude-wakeup and the Telegram notifier example. Registration
fills missing defaults without discarding saved values, validates the result and
atomically writes an owner-only config file (0600 on Unix). Numeric settings retain
their typed domains without adding new policy limits to legacy values. Data, config
and lock paths must stay inside the canonical plugin root and cannot be symlinks;
links of the whole plugin directory remain supported. Existing bot tokens
remain available to the notifier's private config reader; the settings service
returns only set/unset. Catalogue definitions, errors and reload events contain no
stored credentials. Plugin enablement stays in `app.disabled_plugin_ids`; native
agent usage has no private plugin store.

Patches validate the entire batch against current state under the shared config
and cross-process file locks. Stale revisions conflict, failed writes emit no
reload event, and unload removes descriptors and revokes retained owner handles.
After persistence, `plugin-changed` is dual-emitted to desktop and SSE so existing
unload/load callbacks reread config. `applied: false` reports scheduled reload,
not completed callback acknowledgement. Backend unregistration is awaited before
reload registration. General MCP settings publication remains a separate cutover.

Plugins can call `host.settings({ action: "get", paths: ["/enabled"] })` and pass
the resulting revision to set/reset/patch. The host supplies its plugin ID.


TUICommander uses an Obsidian-style plugin system. Plugins extend the Activity Center (bell dropdown), watch terminal output, and interact with app state. Plugins are external ES modules loaded at runtime from the user's plugins directory in the desktop app. The native agent usage feature is initialized separately; there is no built-in plugin registration list.

## Quick Start: External Plugin

1. Create a directory: `~/.config/com.tuic.commander/plugins/my-plugin/`
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

4. In the desktop app, save a `.js`, `.mjs`, or `.json` code file outside `data/` to trigger hot reload. New top-level plugin directories and symlinks are discovered live. The watcher does not follow symlink targets, so edits within a linked target may need an explicit reload or restart. Browser/PWA clients do not load these external packages or run this watcher.

## Architecture

```
PTY reader thread (Rust) ──> output_watchers.rs
                                 |
                                 +-- StreamLines (assemble lines — the only assembler)
                                 +-- clean_line (strip ANSI codes + backticks)
                                 +-- OutputWatcherRegistry (compiled patterns, per client)
                                 +-- WatcherLineBatcher (one event per 100ms, lossless)
                                            |
                                            v
                            pty-watcher-lines event  [{text, matched_ids}]
                            (matched lines only, or every line while a
                             pattern stays in the WebView)
                                            |
                                            v
                            pluginRegistry.handleWatcherLines()
                                            |
                                            +-- fire the ids Rust matched
                                            +-- dispatchLine() — the watchers Rust
                                                could not compile, skipping the
                                                ids already fired
                                            |
                                            v
                                    OutputWatcher.onMatch()
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

1. **Discovery** — Rust `list_user_plugins` scans `~/.config/com.tuic.commander/plugins/` for `manifest.json` files
2. **Validation** — Frontend validates manifest fields and `minAppVersion`
3. **Import** — `import("plugin://my-plugin/main.js")` loads the module via the custom URI protocol (on Windows the loader rewrites this to `http://plugin.localhost/my-plugin/main.js`, since WebView2 only serves custom schemes under `http://{scheme}.localhost/...`)
4. **Module check** — Default export must have `id`, `onload`, `onunload`
5. **Register** — `pluginRegistry.register(plugin, capabilities)` awaits `plugin.onload(host, signal)`
6. **Active** — Plugin receives PTY lines, structured events, and can use the PluginHost API
7. **Hot reload** — File changes emit `plugin-changed` events; the plugin is unregistered and re-imported. Creating a new top-level plugin directory or symlink after startup also emits the event (the watcher does not descend into symlink targets, so the create of the link itself is the trigger), and the new plugin is discovered and loaded without a restart
8. **Unload** — `plugin.onunload()` is called only if its `onload` started, then tracked registrations are disposed and `openPanel` tabs close. Timers, DOM listeners and other resources created directly by the plugin remain its responsibility.

Async startup receives an `AbortSignal`. Unload or reload aborts it and stops waiting for obsolete startup work. Pass the signal to cancellable operations. After unload, calls through the old host throw an abort error before they can register providers or change host state. The host cannot stop arbitrary plugin code or cancel requests that ignore the signal. A replacement waits for backend teardown, not for obsolete startup to finish. Startup must settle (resolve or reject); the host sets no startup or backend teardown timeout. An active startup that never settles remains loading, and a pending backend teardown blocks replacements for that plugin ID. A replacement removed before its startup begins receives neither `onload` nor `onunload`.

### Crash Safety

Every boundary is wrapped in try/catch:
- `import()` — syntax errors or missing exports are caught
- Module validation — missing `id`, `onload`, or `onunload` logs an error and skips the plugin
- `plugin.onload()` — synchronous throws and rejected promises are logged and partial registrations are cleaned up automatically
- Watcher/handler dispatch — exceptions are caught and logged, other plugins continue
- UI callbacks registered through `PluginHost` (file icons/previews, activity
  items, context menu actions, ticker messages, dashboards, commands) are
  isolated the same way; failures are written both to the app log and the
  plugin's dedicated log buffer

A broken plugin produces a console error and is skipped. The app always continues.

## Manifest Reference

File: `~/.config/com.tuic.commander/plugins/{id}/manifest.json`

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
| `allowedUrls` | string[] | for `net:http` | Non-empty URL patterns required for HTTP access (e.g. `["https://api.example.com/*"]`) |
| `agentTypes` | string[] | no | Agent types this plugin targets (e.g. `["claude"]`). Omit or `[]` for universal plugins. |
| `binaries` | string[] | no | CLI binaries this plugin may execute via `exec:cli` (e.g. `["rtk", "mdkb"]`) |
| `providers` | (string \| object)[] | no | Account providers for `httpFetch({account})`; requires `net:http`. A core id (`"github"`) or a token-only descriptor registered on load (see [Core account registry integration](#core-account-registry-integration)) |

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
  onload(host: PluginHost, signal?: AbortSignal): void | Promise<void>;
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

All `register*()` methods return a `Disposable` with a `dispose()` method. You do **not** need to call `dispose()` manually — all registrations are automatically disposed when `onunload()` is called (including during hot reload). Panels opened through `openPanel` also close on unload, and their message, visibility and close bridges are removed. Resources created directly by plugin code, such as timers or DOM listeners, must be released by `onunload()`. Only call `dispose()` if you need to dynamically remove a registration while the plugin is still running.

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

Watches every PTY output line (after ANSI stripping, backtick removal, and line reassembly).

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
- `onMatch` must be synchronous and fast (< 1ms) — it runs on the interface main thread (deferred to a microtask, but a slow handler still blocks painting)
- `pattern.lastIndex` is reset before each test (safe to use global flag, but unnecessary)
- Input is ANSI-stripped but may contain Unicode (checkmarks, arrows, emoji)
- Arguments are positional: `onMatch(match, sessionId)` — NOT destructured

**Where the matching runs.** The registry sends the pattern source and flags of every
watcher to Rust (`set_plugin_output_watchers`). The PTY reader thread assembles the
lines, cleans them (`tuic-terminal/src/output_watchers.rs`, a port of `stripAnsi` plus the backtick strip),
and tests them against the compiled patterns. Rust is the only line assembler: the
WebView reassembles nothing, so a watcher that registers halfway through a line still
sees that line whole. The WebView is only woken for a line that matched: it receives that
cleaned line, runs your `RegExp` on it again, and gives `onMatch` a genuine
`RegExpExecArray`. The `i`, `m` and `s` flags apply on both sides.

Rust may match a little more than your `RegExp` does, never less — the re-run in the
WebView filters the difference. One accepted divergence: a pattern that counts UTF-16
code units of astral characters (`/^.{2}$/` against an emoji) matches in JavaScript only.

**Patterns that stay in the WebView.** The Rust `regex` crate is not a superset of JS
`RegExp` — it has no lookaround, no backreferences, and no negated class escape inside a
character class (`[\D.]`). Rust reports such a pattern as rejected instead of compiling
it to something different, and the registry keeps matching that watcher itself, on the
same assembled lines. The behavior of the watcher does not change; only the thread that
tests the line does. One rejected pattern makes Rust ship every line instead of the
matched ones alone, so prefer a portable pattern when you have the choice.

Every line is tested. The events are rate-limited to one per 100 ms, but the lines inside
a window are concatenated in order, never dropped, so no line is lost and no line is
fabricated from two spliced chunks.

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

#### host.getSessionCwd(sessionId) -> string | null

Returns the current working directory of the given terminal session, or `null` if the session is unknown or has no recorded CWD. No capability required.

```typescript
const cwd = host.getSessionCwd(sessionId);
// → "/Users/me/project" (or null)
```

#### host.getActiveRepoPath() -> string | null

Shortcut for the active repository's path (equivalent to `host.getActiveRepo()?.path`). Returns `null` when no repository is active. No capability required.

```typescript
const repoPath = host.getActiveRepoPath();
```

#### `host.getClaudeProjectDir(repoPath) -> Promise<string | null>`

Resolves a repository path to the absolute path of its Claude Code project directory (`~/.claude/projects/<slug>`). The slug encoding is handled by the Rust side — plugins should use this instead of constructing paths manually. Requires `"fs:read"` capability.

```typescript
const projectDir = await host.getClaudeProjectDir("/Users/me/my-project");
// → "/Users/me/.claude/projects/-Users-me-my-project"
const files = await host.listDirectory(projectDir, "*.jsonl", { sortBy: "mtime" });
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
  //           | "repo-changed" | "shell-state-changed" | "awaiting-input-changed"
  // event.detail: new branch, repo path, shell state or awaiting-input kind; undefined when cleared
  // Branch/repo events describe active navigation (sessionId: null, terminalId: "").
  // Shell/awaiting events identify the changed terminal and its session.
});
// sub.dispose() to unsubscribe
```

State events report changes after the runtime starts; use snapshot getters for initial values. Terminal events observe existing terminal sessions, including remote updates. Branch events describe the active workspace's branch, not every repository HEAD in the background.

When teardown clears a session and changes its final shell or awaiting-input state together, those events retain the old session ID. A replacement session establishes a new baseline without replaying its initial state.

### Tier 2b: Git Read (capability-gated)

These methods require declaring `"git:read"` in `manifest.json`. They provide read-only access to git repository state.

#### `host.getGitBranches(repoPath) -> Promise<Array<{ name, isCurrent }>>`

```typescript
const branches = await host.getGitBranches("/Users/me/project");
// [{ name: "main", isCurrent: true }, { name: "feature/x", isCurrent: false }]
```

#### `host.getRecentCommits(repoPath, count?) -> Promise<Array<{ hash, message, author, date }>>`

```typescript
const commits = await host.getRecentCommits("/Users/me/project", 5);
// [{ hash: "abc1234", message: "fix: bug", author: "name", date: "2026-02-25" }]
```

#### `host.getGitDiff(repoPath, scope?) -> Promise<string>`

```typescript
const diff = await host.getGitDiff("/Users/me/project", "staged");
// Returns unified diff string
```

#### `host.getRemotes(repoPath) -> Promise<Array<{ name, url }>>`

```typescript
const remotes = await host.getRemotes("/Users/me/project");
// [{ name: "origin", url: "https://github.com/acme/app.git" }] — credentials in URLs are removed
```

### Tier 3: Write Actions (capability-gated)

These methods require declaring capabilities in `manifest.json`. Calling without the required capability throws `PluginCapabilityError`.

#### `host.writePty(sessionId, data) -> Promise<void>`

Sends raw bytes to a terminal session. **Requires `"pty:write"` capability.**

> **Prefer `sendAgentInput()` for user input.** `writePty` sends raw data — it does not handle Enter key semantics for Ink-based agents. Use it only when you need exact byte control.

```typescript
await host.writePty(sessionId, "\x03"); // Send Ctrl-C
```

#### `host.sendAgentInput(sessionId, text) -> Promise<void>`

Sends user input to an agent session with correct Enter handling. **Requires `"pty:write"` capability.**



Ink-based agents (Claude Code, Codex, etc.) run in raw mode and need Ctrl-U, the text, and `\r` as three writes with a real time gap between them, so each arrives in its own read (Claude Code strips a Ctrl-U that arrives inside a long text and then refuses the Enter). Shell sessions receive Ctrl-U + text in one write, then `\r`. This method handles both cases automatically based on the detected agent type.

```typescript
await host.sendAgentInput(sessionId, "y");       // confirm a prompt
await host.sendAgentInput(sessionId, "explain this code"); // send a message
```

#### `host.sendAgentInputIfEmpty(sessionId, text) -> Promise<boolean>`

Requires `"pty:write"`. For automated idle messages, use `host.sendAgentInputIfEmpty(sessionId, text)`. The backend checks the current idle, awaiting, and tracked composer state and serializes the complete submission with raw user input and its bookkeeping. It returns `false` without writing when blocked; it returns `true` after submission. This mode never answers a dialog or drains queued commands. Do not use a separate composer read to authorize a later send. Plugin load and reload replay the current `awaiting` state and shell state with `agent_type`. Older hosts do not expose this method, so plugins fail closed instead of falling back to destructive input.

#### host.openMarkdownPanel(title, contentUri) -> void

Opens a virtual markdown tab and shows the panel. **Requires `"ui:markdown"` capability.**

```typescript
host.openMarkdownPanel("CI Report", "my-scheme:report?id=123");
```

#### host.openMarkdownFile(absolutePath) -> void

Opens a local markdown file in the markdown panel. **Requires `"ui:markdown"` capability.** The path must be absolute. This is useful for plugins that ship a `README.md` or other documentation files.

```typescript
// Open the plugin's own README
host.openMarkdownFile("/Users/me/.config/com.tuic.commander/plugins/my-plugin/README.md");
```

#### host.openMarkdownFileBackground(absolutePath) -> boolean

Opens a local Markdown file as a pinned background tab without changing the active pane. **Requires `"ui:markdown"` capability.** Returns `false` when no registered repository owns the absolute path; otherwise returns `true`, including when the existing tab was reused.

```typescript
const owned = host.openMarkdownFileBackground("/Users/me/project/plans/feature.md");
```

#### `host.playNotificationSound(sound?) -> Promise<void>`

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

#### `host.readFile(absolutePath) -> Promise<string>`

Read a file's content as UTF-8 text. Maximum file size: 10 MB. **Requires `"fs:read"` capability.**

```typescript
const content = await host.readFile("/Users/me/.claude/projects/foo/conversation.jsonl");
```

#### `host.readFiles(absolutePaths) -> Promise<(string | null)[]>`

Read up to 1000 files in one round trip, in request order. An entry is `null` when that path could not be read — a file that vanished between the listing and the read does not lose the batch. **Requires `"fs:read"` capability.**

Use this instead of looping over `readFile` whenever the paths are known up front: listing a directory and reading every entry costs two calls instead of one per file.

```typescript
const names = await host.listDirectory(dir, "*.md");
const contents = await host.readFiles(names.map((name) => `${dir}/${name}`));
const found = names
  .map((filename, i) => ({ filename, content: contents[i] }))
  .filter((entry) => entry.content != null);
```

#### `host.readFileBase64(absolutePath, options?) -> Promise<string>`

Read a file's raw bytes and return them as a base64 string. The default maximum is 10 MiB. A plugin may request a positive per-call `options.maxBytes`; the host clamps it to 512 MiB to bound the Base64 and decoded-buffer memory peak. Use this for binary previews such as `.docx`, images, archives, or bounded in-memory database snapshots. **Requires `"fs:read"` capability.**

```typescript
const encoded = await host.readFileBase64("/Users/me/Documents/spec.docx");
const bytes = Uint8Array.from(atob(encoded), (ch) => ch.charCodeAt(0));

const database = await host.readFileBase64("/Users/me/data.sqlite", {
  maxBytes: 256 * 1024 * 1024,
});
```

#### `host.writeFileBase64(absolutePath, content, options?) -> Promise<void>`

Atomically replace a file with bytes supplied as Base64. The default maximum is 10 MiB; a positive `options.maxBytes` is clamped to the 512 MiB host ceiling. The complete decoded payload is validated before the temporary file is atomically renamed over the destination. **Requires `"fs:write"` capability.**

```typescript
await host.writeFileBase64("/Users/me/data.sqlite", encodedDatabase, {
  maxBytes: 256 * 1024 * 1024,
});
```

#### `host.listDirectory(path, pattern?, options?) -> Promise<string[]>`

List filenames in a directory, optionally filtered by a glob pattern. Returns filenames only (not full paths). **Requires `"fs:list"` capability.**

Options:
- `sortBy`: `"name"` (default, alphabetical) or `"mtime"` (newest first). Use `"mtime"` to efficiently find the most recently modified file when the directory contains many historical entries.

```typescript
const files = await host.listDirectory("/Users/me/.claude/projects/foo", "*.jsonl");
// ["conversation-1.jsonl", "conversation-2.jsonl"]

// Find the currently active session JSONL among 100+ historical ones:
const recent = await host.listDirectory(dir, "*.jsonl", { sortBy: "mtime" });
const activeFile = recent[0]; // most recently written
```

#### `host.watchPath(path, callback, options?) -> Promise<Disposable>`

Watch a path for filesystem changes. Emits batched events after a debounce period. **Requires `"fs:watch"` capability.**

Each watch delivers only its own events — a plugin with several watches gets one callback per change, not one per watch.

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

#### `host.writeFile(absolutePath, content) -> Promise<void>`

Write content to a file within `$HOME`. Creates parent directories if needed. Refuses to overwrite directories. Max 10 MB. **Requires `"fs:write"` capability.**

```typescript
await host.writeFile("/Users/me/project/stories/new-story.md", "---\nstatus: pending\n---\n# New Story");
```

#### `host.renamePath(from, to) -> Promise<void>`

Rename or move a file within `$HOME`. Both paths must be absolute. Source must exist. Creates parent directories for destination if needed. **Requires `"fs:rename"` capability.**

```typescript
await host.renamePath(
  "/Users/me/project/stories/old-name.md",
  "/Users/me/project/stories/new-name.md",
);
```

#### `host.scanBuildArtifacts(repoPaths, options?) -> Promise<ArtifactEntry[]>`

Recursively scan the given repo roots for build-artifact directories (Rust/Maven `target/`, `node_modules/`, JS framework caches like `.next`/`.turbo`, Python `.venv`/`__pycache__`/tool caches, .NET `obj`/`bin`, Gradle/CMake/Flutter `build/`, SwiftPM `.build`/`Pods`, `.terraform`, Elixir `_build`, Zig, Haskell, Composer `vendor/`). Read-only. Unlike `listDirectory`, this **ignores `.gitignore`** (artifact dirs are gitignored by design) and stops descending on a match, so a nested `node_modules` is folded into the outer entry — never double counted. Ambiguously-named dirs (`target`, `bin`/`obj`, `build`, `vendor`, …) only count when a toolchain marker sits beside them (e.g. `Cargo.toml` or `pom.xml` for `target`; a `.csproj`/`.fsproj`/`.vbproj`/`.sln`/`.slnx` for `bin`/`obj`; `build.gradle`/`CMakeLists.txt`/`pubspec.yaml` for `build`; `composer.json` for `vendor`) — a Go sysroot `bin` or an Xcode `PIFCache/target` is walked like any other dir, not claimed. The full rule table is `ARTIFACT_RULES` in `src-tauri/src/plugin_fs.rs`. Each `repoPaths` entry is `$HOME`-scoped **and intersected server-side with the app's actual registered-repository list** — a path that isn't equal to or nested under a genuinely registered repo root is silently dropped, so a plugin cannot widen its scan surface by passing arbitrary `$HOME` paths. Results for the same normalized set of roots are shared across concurrent callers and reused for 30 seconds. Pass `{ forceRefresh: true }` to bypass a completed cached result; an already-running scan for the same roots is still shared. **Requires `"fs:scan"` capability.**

Each `ArtifactEntry` is `{ path, kind, size_bytes, trimmable_bytes, last_modified_secs, repo }` — `kind` is one of `rust | maven | node | jscache | python | dotnet | gradle | cmake | swift | flutter | terraform | elixir | zig | haskell | php`, `last_modified_secs` is the max mtime of the dir's direct children (a "last build" signal), and `trimmable_bytes` is the share of `size_bytes` that `trimBuildArtifact` would reclaim (0 for kinds with no separable intermediates). Both sizes come from one filesystem walk.

```typescript
const entries = await host.scanBuildArtifacts(host.getRepos().map((r) => r.path));
const refreshed = await host.scanBuildArtifacts(host.getRepos().map((r) => r.path), {
  forceRefresh: true,
});
```

#### `host.deleteBuildArtifact(path, repoPaths) -> Promise<void>`

Delete a build-artifact directory. Destructive. The backend guard requires (all must hold): the path canonicalizes to a basename that is a known artifact dir, it is strictly inside one of `repoPaths` (never a repo root itself), that containing root is itself a **server-verified registered repository** (caller-supplied roots not backed by a real registered repo are dropped — the plugin cannot claim an arbitrary `$HOME` directory as a "repo"), it is `$HOME`-scoped, and — for ambiguously-named dirs — the same toolchain marker required by the scanner sits beside it (so Rust `src/bin` sources are refused). Only then is `remove_dir_all` run. **Requires `"fs:delete"` capability.**

```typescript
await host.deleteBuildArtifact("/Users/me/project/target", ["/Users/me/project"]);
```

#### `host.trimBuildArtifact(path, repoPaths) -> Promise<number>`

Remove only a build-artifact directory's **regenerable intermediates**, leaving the built executables in place. Prefer this over `deleteBuildArtifact`: a full clean of a Rust `target/` also deletes the binary the user may be running, while reclaiming (measured across 5 real repos) only ~1% more disk.

Authorization is `deleteBuildArtifact`'s, unchanged. On top of it, only paths produced by expanding the matched toolchain rule's own trim patterns are removed, and each is re-verified to sit strictly inside the artifact dir; symlinked directories are never followed or expanded into.

Which sub-paths count as intermediates is a per-rule table (`ARTIFACT_RULES[].trim` in `src-tauri/src/plugin_fs.rs`):

| Kind | Trimmed | Kept |
| --- | --- | --- |
| `rust` (`target/`) | `<profile>/{deps,build,incremental,.fingerprint}`, plus the same under a target triple | the linked executables at each profile root |
| `swift` (`.build/`) | `index-build/`, `<triple>/<profile>/{ModuleCache,index,<Module>.build}` | the product binary, `Modules/`, `checkouts/`, `repositories/` |
| `maven` (`target/`) | `classes`, `test-classes`, `generated-*`, `maven-status`, `maven-archiver`, `*-reports` | the packaged `*.jar`/`*.war` |
| `gradle` (`build/`) | `classes`, `tmp`, `kotlin`, `intermediates`, `generated`, `reports`, `test-results`, `jacoco` | `libs/`, `outputs/`, `distributions/`, `install/` |

The bar for inclusion is that rebuilding costs only local CPU — anything needing the network to restore (cargo registry, SwiftPM `checkouts`/`repositories`) is deliberately kept. Every other kind has an empty trim list: it reports `trimmable_bytes: 0` and rejects a trim, because there is no subset that spares an output (`node_modules`, `.venv`, `vendor`) or because the whole dir is already the intermediate half of a pair (.NET `obj` vs `bin`, which the scanner reports as two separate entries).

Check `trimmable_bytes > 0` before offering it. It resolves with the bytes it actually reclaimed, measured as it deleted; `0` means there was nothing left to trim. Patch cached totals with that number and never with the `trimmable_bytes` of the last scan — a build between the scan and the trim moves the estimate, and subtracting it publishes a total no measurement supports. **Requires `"fs:delete"` capability** — same blast-radius class, on a subset.

```typescript
const reclaimed = await host.trimBuildArtifact("/Users/me/project/target", ["/Users/me/project"]);
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

The removed `postTickerMessage` and `removeTickerMessage` aliases are not supported. Use `setTicker` and `clearTicker`.

### Tier 3d: Panel UI (capability-gated)

#### host.openPanel(options) -> PanelHandle

Open an HTML panel in a sandboxed iframe tab. Returns a handle for updating content or closing the panel. If a panel with the same id is already open, it will be activated and updated. **Requires `"ui:panel"` capability.**

When the panel is hidden, its iframe is unloaded; showing it loads the page again. Keep state that must survive tab switches outside the iframe. This also applies to URL tabs.

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
  onVisibilityChange(visible) {
    // A panel stays mounted behind display:none when another tab is selected.
    // Skip work while hidden and catch up here.
    if (visible && stale) rebuild();
  },
  onClose() {
    // The tab is gone for good, whoever closed it — the ×, a middle-click, the
    // context menu, or your own close(). A hidden panel comes back; a closed one
    // does not, and isVisible() reports false for both. Release what the panel
    // owned here or it outlives the tab.
    watch.dispose();
    panel = null;
  },
});

// …and query it at any time, e.g. before an expensive re-render
if (!panel.isVisible()) { stale = true; return; }

// Update content later. Returns false once the user has closed the tab — a
// plugin that renders on a timer or a file event should stop, or re-open with
// openPanel() if the render was user-requested.
if (!panel.update("<html><body><h1>Updated</h1></body></html>")) panel = null;

// Send a message to the iframe at any time
panel.send({ type: "refresh", items: [...] });

// Transfer ownership of large buffers instead of copying them. The sender's
// buffer is detached after postMessage succeeds.
panel.send({ type: "database", buffer }, [buffer]);

// Close the panel
panel.close();
```

**Inside the iframe:**
```html
<script>
  // Send message to host
  window.parent.postMessage({ type: "save", config: { ... } }, "*");

  // Receive messages from host
  // The SDK unwraps the tuic:host-message envelope from panel.send().
  window.tuic.onMessage((data) => {
    if (data?.type === "response") {
      // Use data.ok to update the panel.
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

**Dashboard layout classes.** For analytics/status dashboards, the base stylesheet also ships a `.dashboard`/`.dash-*` class family that mirrors the native Claude Usage dashboard. Use them instead of inventing layout CSS — see [`docs/plugins-style.md`](./plugins-style.md) for the full guide, checklist, and class reference.

#### host.openEditorTab(filePath, repoPath, opts?)

Open a file in the CodeMirror editor tab. **Requires `"ui:panel"` capability.**

```typescript
host.openEditorTab("src/main.ts", "/Users/me/project", { line: 42 });
```

| Param | Type | Description |
|-------|------|-------------|
| `filePath` | `string` | Relative or absolute file path |
| `repoPath` | `string` | Repository root path |
| `opts.fsRoot` | `string?` | Filesystem root (defaults to `repoPath`) |
| `opts.line` | `number?` | Line number to scroll to |

#### host.registerFilePreview(options) -> Disposable

Register a custom file preview handler for specific extensions. When the user opens a file matching one of the claimed extensions, the `onOpen` callback is invoked instead of the default editor. **Requires `"ui:file-preview"` capability.**

```typescript
host.registerFilePreview({
  extensions: ["csv", "tsv"],
  async onOpen(ctx) {
    const raw = await host.readFile(`${ctx.fsRoot}/${ctx.filePath}`);
    host.openPanel({ id: `csv-${ctx.filePath}`, title: "CSV", html: buildTable(raw) });
  },
});
```

| Param | Type | Description |
|-------|------|-------------|
| `options.extensions` | `string[]` | File extensions to claim (without dot, case-insensitive) |
| `options.onOpen` | `(ctx: FilePreviewContext) => void` | Handler called when the user opens a matching file |

**FilePreviewContext:**

| Field | Type | Description |
|-------|------|-------------|
| `filePath` | `string` | Relative path within the repo |
| `repoPath` | `string` | Repository root path |
| `fsRoot` | `string` | Filesystem root for resolving absolute paths |

On plugin unload, claimed extensions are released and files revert to the default open behavior.

#### host.registerDashboard(options) -> Disposable

Register a one-click entry point for the plugin's dashboard. When registered, *Settings → Plugins* shows a **Dashboard** button in the plugin row that calls `options.open()` and automatically closes the Settings panel so the dashboard becomes visible.

```typescript
host.registerDashboard({
  label: "My Plugin",       // optional, defaults to "Dashboard"
  icon: MY_PLUGIN_ICON,     // optional inline SVG string
  open: () => openDashboard(host),
});
```

A plugin may only register one dashboard — calling `registerDashboard` a second time replaces the previous entry. Dispose the returned handle in `onunload` (or rely on automatic cleanup via the plugin's disposable tracker).

#### host.registerCommand(options) -> Disposable

Register a plugin command that users can bind to a keyboard shortcut. The command appears in *Settings → Keyboard Shortcuts* under a dedicated "Plugin Commands" section and can be rebound by the user. The action name is auto-namespaced as `plugin:<pluginId>:<id>`. The active frontend publishes these IDs to the Rust keybinding choices owner; unload removes them from later refreshes. This runtime publication never saves shortcut overrides.

```typescript
host.registerCommand({
  id: "open-dashboard",          // unique per plugin
  title: "Open My Dashboard",    // label in the Shortcuts UI
  defaultShortcut: "Cmd+Shift+M",// optional — leave unbound by default
  run: () => openDashboard(host),
});
```

If `defaultShortcut` conflicts with an existing binding (built-in or another plugin), the command is registered but left unbound; a warning is logged and the user can pick a free combo via Settings. The handle returned is automatically tracked and released on plugin unload.

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

**Security:** The iframe uses `sandbox="allow-scripts allow-same-origin"`. This permits scripts and same-origin access; it is not an isolation boundary from the host DOM. Plugin modules also execute in the host JavaScript realm. The `close-panel` message type is handled as a system message; all other messages are routed to the `onMessage` callback.

#### TUIC SDK (`window.tuic`)

Every plugin iframe automatically receives the TUIC SDK — a lightweight JavaScript API for host integration. The SDK is injected alongside the base CSS and theme variables.

**Feature detection:**

```javascript
if (window.tuic) {
  // Running inside TUICommander — SDK is available
  console.log("TUIC SDK version:", window.tuic.version);
}
```

**Programmatic API:**

| Method | Description |
|--------|-------------|
| `tuic.version` | SDK version string (e.g. `"1.0"`) |
| `tuic.open(path, opts?)` | Open a markdown file in a new tab. `path` is absolute. `opts.pinned` pins the tab. |
| `tuic.openUrl(url)` | Open an `http`, `https`, or `mailto` URL outside TUICommander. Other schemes are rejected by the host. |
| `tuic.edit(path, opts?)` | Open a file in the code editor. `opts.line` (1-based) jumps to a line; omit for the top. |
| `tuic.terminal(repoPath)` | Open a new terminal in the given repository. |

```javascript
// Open a file
tuic.open("/Users/me/myrepo/README.md");

// Open a pinned file
tuic.open("/Users/me/myrepo/docs/guide.md", { pinned: true });

// Open a terminal in a repo
tuic.terminal("/Users/me/myrepo");

// Open a website in the system browser
tuic.openUrl("https://example.com/docs");
```

**Link interception:** Standard HTML links with `tuic://` scheme are intercepted automatically — no JavaScript required:

Absolute `http`, `https`, and `mailto` links in inline plugin HTML also open through the host when clicked. Relative links remain blocked so an inline panel cannot navigate into the TUICommander app. A panel loading a cross-origin URL cannot have its clicks inspected by the host; use the tab menu's **Open in Browser** action if that page needs external links.

```html
<!-- Opens a markdown file -->
<a href="tuic://open/Users/me/myrepo/README.md">View README</a>

<!-- Opens a pinned markdown file -->
<a href="tuic://open/Users/me/myrepo/docs/guide.md" data-pinned>Pinned Guide</a>

<!-- Opens a terminal -->
<a href="tuic://terminal?repo=/Users/me/myrepo">Open Terminal</a>
```

**URL format:**

| URL | Action |
|-----|--------|
| `tuic://open/<absolute-path>` | Open file in markdown tab |
| `tuic://edit/<absolute-path>?line=N` | Open file in the code editor (optional `line`) |
| `tuic://terminal?repo=<repo-path>` | Open terminal in repository |

**Security:** Paths are validated against the list of known repositories. Paths outside any registered repo are rejected with a warning. The SDK runs inside the sandbox and communicates with the host exclusively via `postMessage`.

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

#### `host.readCredential(serviceName) -> Promise<string | null>`

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

#### `host.httpFetch(url, options?) -> Promise<HttpResponse>`

`httpFetch(url, { signal })` accepts a host-local `AbortSignal`. A pre-aborted signal starts no request; abort cancels the Rust request while waiting for headers or collecting the body and rejects with `AbortError`. Unloading the plugin also cancels its active HTTP requests. Signals never cross IPC/HTTP: the host sends a generated UUID and a plugin-scoped cancel command. Requests without a signal keep the same response contract.

Make an HTTP request. Non-2xx status codes are returned normally (not thrown as errors). **Requires `"net:http"` capability.**

Plugins can only fetch URLs matching their manifest's `allowedUrls` patterns. A manifest declaring `net:http` with empty or omitted `allowedUrls` is rejected. An empty list never grants unrestricted access, including localhost or private destinations.

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

**Account requests:** `httpFetch(url, { account: "<account id>" })` makes Rust inject that account's credential (`Authorization: Bearer <token>`, or `Basic` for Basic accounts). The plugin never sees the token. Rules: the manifest `providers` list must contain the account's provider; the URL must be under the account host (same scheme, host and port, path prefix of the site URL; GitHub accounts use their API base, `https://api.github.com/` or `https://<ghe>/api/`); the account host rule replaces the `allowedUrls` check for the first URL; you must not set `Authorization` yourself. Each redirect hop must be under the account host or match `allowedUrls`, and a hop to another scheme, host or port loses the `Authorization` header. The token is read again for each request, so a re-entered credential applies at once; a 401 comes back to you as a normal response. Token values in the response body and headers are replaced with `[redacted]`.

```typescript
const me = await host.httpFetch(`${site}/rest/api/3/myself`, { account: accountId });
```

**Security and limits:**
- `file://`, `data://`, `ftp://` schemes are blocked
- 30-second timeout, 10 MB response limit, max 5 redirects
- Localhost (`localhost`, `127.0.0.1`, `::1`, `[::1]`, `0.0.0.0`) is blocked unless explicitly declared in `allowedUrls`
- Every HTTP request must match a declared pattern; an empty list permits no requests

**`allowedUrls` pattern matching:**
- Patterns use prefix matching with an optional trailing `*` wildcard
- `"https://api.example.com/*"` — matches any path under that origin
- `"https://api.example.com/v2/data"` — matches that exact URL only
- `"http://localhost:8080/*"` — allows localhost on that port (required to unblock localhost)
- The URL must start with the pattern prefix (before `*`) to match

### Tier 3i: File Tail (capability-gated)

#### `host.readFileTail(absolutePath, maxBytes) -> Promise<string>`

Read the last N bytes of a file, skipping any partial first line. Useful for reading recent entries from large JSONL files. **Requires `"fs:read"` capability.**

```typescript
const tail = await host.readFileTail("/Users/me/.claude/hud-tracking.jsonl", 512 * 1024);
const lines = tail.split("\n").filter(Boolean);
```

### Tier 3j: CLI Execution (capability-gated)

#### `host.execCli(binary, args, cwd?) -> Promise<string>`

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
- 5 MiB limit per stdout/stderr stream; both streams drain concurrently. Overflow or read failure stops the process with an error.
- Binary is resolved via PATH lookup and known install locations (`~/.cargo/bin/`, `/usr/local/bin/`, etc.)

### Tier 4: Scoped Tauri Invoke (whitelisted commands only)

#### `host.invoke<T>(cmd, args?) -> Promise<T>`

Invokes a whitelisted Tauri command. Non-whitelisted commands throw immediately.

**Whitelisted commands:**
| Command | Args | Returns | Capability |
|---------|------|---------|------------|
| `read_file` | `{ path: string, file: string }` | `string` | `invoke:read_file` |
| `list_markdown_files` | `{ path: string }` | `Array<{ path, git_status }>` | `invoke:list_markdown_files` |
| `read_plugin_data` | `{ plugin_id: string, path: string }` | `string` | none (always allowed) |
| `write_plugin_data` | `{ plugin_id: string, path: string, content: string }` | `void` | none (always allowed) |
| `delete_plugin_data` | `{ plugin_id: string, path: string }` | `void` | none (always allowed) |
| `get_input_buffer_content` | `{ sessionId: string }` | `string` | `pty:read` |

**Plugin data storage** is sandboxed to `~/.config/com.tuic.commander/plugins/{id}/data/`. No capability required — every plugin can store its own data.

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

> **What capabilities are, and what they are not.** Capabilities gate what a
> plugin **declares** — a plugin that never asked for `exec:cli` cannot reach the
> exec commands under its own id. They do **not** gate what a plugin can
> **impersonate**. Plugins load with a bare dynamic `import()` into the *same
> JavaScript realm* as the host, and `plugin_id` is caller-supplied — it is the
> only key the Rust capability checks consult. Any plugin can therefore import
> `invoke` itself and pass another plugin's id to inherit that plugin's exec,
> credential and allowed-URL grants.
>
> **Plugins are isolated from the host's declared surface, not from each other.**
> Treat every installed plugin as trusted with the union of all installed
> capabilities, and vet what you install. This matches TUICommander's threat
> model — a local tool where the human user is the trust boundary.
>
> A per-plugin token was considered and deliberately rejected: same-realm
> JavaScript can read it out of the module that holds it, proxy the function that
> uses it, or monkey-patch the transport, so it would be a boundary in name only.
> Real isolation requires running each plugin off-realm (a Worker or sandboxed
> iframe) with a host-created `MessagePort` as its only channel to the backend,
> so a plugin's identity is the port it was handed rather than a string it
> supplies. That is tracked as future work, not a gap being quietly ignored.

Capabilities gate access to Tier 3 and Tier 4 methods. Declare them in `manifest.json`:

```json
{
  "capabilities": ["pty:write", "ui:sound"]
}
```

| Capability | Unlocks | Risk |
|------------|---------|------|
| `pty:write` | `host.writePty()`, `host.sendAgentInput()`, `host.sendAgentInputIfEmpty()` | Can send input to terminals |
| `pty:read` | `host.invoke("get_input_buffer_content", …)` | Can read the terminal input line buffer |
| `ui:markdown` | `host.openMarkdownPanel()`, `host.openMarkdownFile()`, `host.openMarkdownFileBackground()` | Can open panels and files in the UI |
| `ui:sound` | `host.playNotificationSound(sound?)` | Can play sounds (question, error, completion, warning, info) |
| `ui:panel` | `host.openPanel()` | Can render arbitrary HTML in sandboxed iframe |
| `ui:ticker` | `host.setTicker()`, `host.clearTicker()` | Can post messages to the shared status bar ticker |
| `credentials:read` | `host.readCredential()` | Can read system credentials (consent dialog shown) |
| `net:http` | `host.httpFetch()` | Requires a non-empty `allowedUrls` list; requests must match a declared pattern |
| `invoke:read_file` | `host.invoke("read_file", ...)` | Can read files on disk |
| `invoke:list_markdown_files` | `host.invoke("list_markdown_files", ...)` | Can list directory contents |
| `fs:read` | `host.readFile()`, `host.readFiles()`, `host.readFileBase64()`, `host.readFileTail()` | Can read files within `$HOME` (10 MiB default; binary reads may request up to the 512 MiB host ceiling) |
| `fs:list` | `host.listDirectory()` | Can list directory contents within `$HOME` |
| `fs:watch` | `host.watchPath()` | Can watch filesystem paths within `$HOME` for changes |
| `fs:write` | `host.writeFile()`, `host.writeFileBase64()` | Can atomically write files within `$HOME` (10 MiB default; binary writes may request up to the 512 MiB host ceiling) |
| `fs:rename` | `host.renamePath()` | Can rename/move files within `$HOME` |
| `fs:scan` | `host.scanBuildArtifacts()` | Can recursively scan registered repos for build-artifact directories (read-only; ignores `.gitignore`) |
| `fs:delete` | `host.deleteBuildArtifact()`, `host.trimBuildArtifact()` | Can delete a build-artifact directory inside a registered repo (guarded `remove_dir_all`), or remove just its intermediates |
| `exec:cli` | `host.execCli()` | Can execute CLI binaries declared in manifest `binaries` field |
| `git:read` | `host.getGitBranches()`, `host.getRecentCommits()`, `host.getGitDiff()`, `host.getRemotes()` | Read-only access to git repository state |
| `ui:context-menu` | `host.registerTerminalAction()` | Can add actions to the terminal right-click "Actions" submenu |
| `ui:sidebar` | `host.registerSidebarPanel()` | Can register collapsible panel sections in the sidebar |
| `ui:file-icons` | `host.registerFileIconProvider()` | Can provide file/folder icons for the file browser (e.g. VS Code icon themes) |
| `ui:file-preview` | `host.registerFilePreview()` | Can claim file extensions and provide custom preview UIs |

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
| `codex` | `"codex"` |
| `amp` | `"amp"` |
| `cursor-agent` | `"cursor"` |
| `goose` | `"goose"` |
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

Reload batches deduplicate changed enabled IDs and run one discovery request for
that batch. Concurrent notifications share a serialized reload queue. Only the
selected plugin roots are read; payload hashing reuses unchanged file hashes by
modification time and size. Same-version source edits still change the computed
payload digest. Full discovery remains available at startup.

When any file in a plugin directory changes, the app:
1. Emits a `plugin-changed` event with the plugin ID
2. Calls `pluginRegistry.unregister(id)` (runs `onunload`, disposes all registrations)
3. Re-imports the module under a fresh path version (`plugin://<id>/~v<timestamp>/main.js`). Relative imports inherit the version, so every file the entry imports is re-read too
4. Validates and re-registers the plugin

This means you can edit `main.js` or any file it imports and see changes without restarting the app. The `~v<digits>` segment exists only in the URL; do not name a plugin directory that way.

## Build & Install

External plugins must be pre-compiled ES modules. Use esbuild:

```bash
esbuild src/main.ts --bundle --format=esm --outfile=main.js --external:nothing
```

Install by copying the directory to:
- macOS: `~/Library/Application Support/com.tuic.commander/plugins/my-plugin/`
- Linux: `~/.config/com.tuic.commander/plugins/my-plugin/`
- Windows: `%APPDATA%/com.tuic.commander/plugins/my-plugin/`

Directory structure:
```
my-plugin/
  manifest.json
  main.js
```

## Core-rendered plugin UX


Core renders the following Solid components. Plugins supply manifest metadata and host registrations; they do not import or mount these components.

### Manifest UX field inventory
`agentTypes`, `allowedUrls`, `author`, `binaries`, `capabilities`, `description`, `id`, `main`, `minAppVersion`, `name`, `providers`, `version`.

### Core UX component inventory
`BrowseRow`, `PluginLogViewer`, `PluginRow`, `PluginsTab`.

#### PluginsTab

Settings > Plugins has Installed and Browse views. Installed lists loaded/discovered plugin state. Browse reads community registry entries and offers install, update and refresh actions. Installation accepts ZIP archives. The manifest `id` identifies the package, `main` selects its module, and `minAppVersion` controls compatibility. `author` is optional metadata. `allowedUrls`, `agentTypes`, `binaries` and `providers` drive host access/event routing, not editable settings controls.

#### PluginRow

The Installed row shows manifest `name` (falls back to `id`), `version`, `description` and `capabilities`. It shows load errors, error counts and available updates. Its toggle persists enabled state; Logs expands PluginLogViewer; external packages can be uninstalled. A README.md adds a documentation button. `host.registerDashboard({ label?, icon?, open })` adds a dashboard button: core closes Settings before calling `open`. Register only one dashboard per plugin. `host.registerCommand({ id, title, defaultShortcut?, run })` adds a rebindable entry under Settings > Keyboard Shortcuts > Plugin Commands.

#### PluginLogViewer

The expanded Logs view shows the plugin ring buffer's timestamp, level and message, or an empty state. Use `host.log(level, message, data?)`; host boundary errors are captured automatically.

#### BrowseRow

The Browse row shows registry name, version, description, author, tags and minimum app version. It offers Install for an absent package, Update when a newer version is available, or Installed for the current version. Registry metadata is separate from the installed manifest.

### Settings schema and Tasks views

#### Tasks list

Core `TasksList` consumes the async `WorkItemProvider` port. Providers supply
account and provider IDs, a display name, field descriptors, saved filters and
`list(query, signal)`. Core renders Solid controls and virtual rows; providers
return data only. Source selection partitions cached pages by provider and
account. Queries include filters, saved-filter ID, search, sort and cursor.
Search waits 250 ms after input; sorting and filters run in the provider.
Grouping calls `grouped(query, signal)` with `groupBy`; core renders the returned
group names and members without inferring membership.
`nextCursor` enables Load more; repeated cursors stop pagination, and repeated
item references update the existing row. The cache holds at most 32 pages.
An obsolete request cannot replace current rows. A failed refresh retains
available rows and marks them stale. Detail data is requested by the detail
view after the host receives `onOpen(reference)`. Shell mounting is separate.

The optional manifest `settings: { schema, defaults }` registers a scoped settings owner. Core renders one searchable Integrations page per live plugin catalogue and removes it on unload. All reads, edits and resets use the existing revision-checked plugin settings service. Titles and descriptions label controls; secrets show only set/unset. Objects group leaves; string enums render selects, booleans toggles, integers and numbers numeric inputs. Bounded arrays use multi-selects for string enums, lists for strings, and JSON inputs for other item types. Read-only fields render status controls. Plugins do not import core Settings components or save a second configuration file.

### GitHub Work adapter status

`plugins/github-work/provider.js` exports `createGitHubProvider` against the A4
contract and an injected account-scoped fetch port. It returns data only; core owns
rendering and start-work execution. Recorded GitHub fixtures cover REST read shapes.
The adapter does not receive tokens or maintain a separate rate limiter.

The plugin lifecycle and manifest use `host.httpFetch(url, {account})` from
story 1697-4cdc; the catalog lists GitHub Work. Configure `data/config.json` with
account IDs and repositories as described in the module README. B6/B7 core ports and the duplicate-picker consumer also need integration.
The current `host.invoke` whitelist must not be bypassed to reach core PR commands.
Projects v2 success paths remain unverified because the recording credential lacks
`read:project`. See the module README for the open integration tasks.

## Plugin Management (Settings > Plugins)

The Settings panel has a **Plugins** tab with two sub-tabs:

### Installed

- Lists external plugins with toggle, logs, and uninstall buttons; native agent usage is managed in Settings > Agents
- The UI retains a legacy "Built-in" badge path, but no built-in plugin packages are registered by the current startup code
- Error count badges appear on plugins with recent errors
- "Logs" button opens an expandable log viewer showing the plugin's ring buffer
- "Install from file..." button opens a file dialog accepting `.zip` archives

### Browse

- Shows plugins from the community registry (fetched from GitHub)
- "Install" button downloads and installs directly
- "Update available" badge when a newer version exists or equal-version payloads differ
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
| `tuic://open/<path>` | Open markdown file in tab (iframe SDK only) |
| `tuic://terminal?repo=<path>` | Open terminal in repo (iframe SDK only) |

**Security:** `install-plugin` requires HTTPS URLs and shows a confirmation dialog. `open-repo` only accepts paths already in the repository list. `open` and `terminal` validate paths against known repos (available only inside plugin iframes via the TUIC SDK, not as OS-level deep links).

## Plugin Registry

The registry is a JSON file hosted on GitHub (`sstraus/tuicommander-plugins` repo). The app fetches it on demand (Browse tab) with a 1-hour TTL cache.

Registry entries include: `id`, `name`, `description`, `author`, `latestVersion`, `minAppVersion`, `capabilities`, `downloadUrl`, and optional `contentSha256`.

The Browse and Installed tabs compare versions and, at the same version, payload identities. Rust recomputes installed `contentSha256` during discovery; a manifest cannot supply it. A missing identity keeps version-only behavior, and a newer installed version is never downgraded.

The community packager generates `contentSha256` from sorted UTF-8 relative paths (using `/`), NUL, each file's lowercase hexadecimal SHA-256, and LF. It hashes that complete sequence again with SHA-256. Directories named `data`, `node_modules`, `scripts`, `.claude`, `.git`, and files matching `package*.json` are excluded. Thus user settings and ZIP timestamps do not cause updates. Internal symlinks or unreadable files leave the installed identity unknown; the plugin can still load. This is content identity, not publisher authentication. The raw source catalogue can precede release completion; wait for the plugin release workflow before installing.

## Per-Plugin Error Logging

Each plugin has a dedicated ring buffer logger (500 entries max). Errors from `onload`, `onunload`, output watchers, structured event handlers, and PluginHost-registered UI callbacks are automatically captured.

Plugins can also write to their log via `host.log(level, message, data)`.

View logs in Settings > Plugins > click "Logs" on any plugin row.

## Preinstalled External Plugins

Plan Tracker (`plan`) and Stories Ticker (`stories-ticker`) are external packages installed into the normal plugin directory by a one-time migration. They use only `PluginHost`, declare capabilities in their manifests, and can be disabled, uninstalled, or updated like any other external plugin. The migration marker prevents an explicitly uninstalled package from being recreated on later launches.

External packages are desktop-only. Browser/PWA mode does not import plugin
code from the desktop configuration directory.

> **Note:** Session prompt tracking is now a native Rust feature (via `input_line_buffer.rs` and the Activity Dashboard). The former `sessionPromptPlugin` built-in has been removed.

See `examples/plugins/report-watcher/` for a template showing how to extract terminal output into Activity Center items with a markdown viewer.

Create new plugins as external packages with a manifest and ES module entry point. `src/plugins/index.ts` initializes native agent usage and loads external packages; it has no `BUILTIN_PLUGINS` list.

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

Rust assembles the lines, so a test pushes an assembled line rather than raw PTY text:

```typescript
it("detects deployment from PTY output", async () => {
  pluginRegistry.register(myPlugin);
  pluginRegistry.handleWatcherLines("session-1", [
    { text: "Deployed: api-server to prod", matched_ids: [] },
  ]);
  // onMatch is deferred with queueMicrotask
  await new Promise<void>((resolve) => queueMicrotask(resolve));

  const items = activityStore.getForSection("my-section");
  expect(items).toHaveLength(1);
  expect(items[0].title).toBe("api-server");
});
```

`registerOutputWatcher` also calls `set_plugin_output_watchers`, so the `invoke` mock must
return a promise. Until it resolves to `{ applied: true, rejected: [] }`, the registry
treats every watcher as its own and scans the line itself — which is what the example
above exercises with an empty `matched_ids`.

To test the Rust path instead, resolve the sync with `{ applied: true, rejected: [] }` and
put the qualified id (`<clientId>/<watcherId>`, both read from the invoke payload) in
`matched_ids`. A watcher fires **once** per line either way: an id Rust flagged is skipped
by the frontend scan that follows. A reply with `applied: false` is ignored, because the
set it describes is not the set the backend holds.

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

Detected when a plan file path appears in terminal output. The emitted `path` is **relative or absolute, as it appeared in the output** — the parser does NOT resolve relative paths to absolute:

- Relative paths (e.g. `plans/foo.md`, `.claude/plans/bar.md`) are emitted unchanged
- Tilde paths (`~/.claude/plans/bar.md`) are the only ones rewritten — `~` is expanded to the user's home directory
- Already-absolute paths are passed through unchanged

Relative-to-absolute resolution (against the terminal session's CWD via `host.getSessionCwd`) happens later, in the external Plan Tracker plugin; plans whose CWD cannot be determined are skipped.

```typescript
{ type: "plan-file", path: string }
// path: relative or absolute, e.g. "plans/foo.md", ".claude/plans/bar.md",
//       "/Users/me/.claude/plans/graceful-rolling-quasar.md"
```

**Repo scoping:** Plan Tracker asks `host.openMarkdownFileBackground()` to resolve the repository that owns each absolute plan path. Unowned paths are tracked but not opened, preventing cross-repository tab leakage.

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
| `csv-preview` | 3 | `ui:file-preview`, `ui:panel`, `fs:read` | Preview CSV/TSV files as sortable HTML tables |
| `docx-preview` | 3 | `ui:file-preview`, `ui:panel`, `fs:read` | Preview Word `.docx`/`.dotx` files as HTML with Mammoth.js |
| `xlsx-preview` | 3 | `ui:file-preview`, `ui:panel`, `fs:read` | Preview Excel `.xlsx`/`.xlsm`/`.xlsb`/`.xls` and `.ods` spreadsheets as sortable tables with SheetJS |

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

## Core account registry integration

Provider accounts and sites are core-owned. Metadata reads expose credential presence only. Manifest `providers` descriptors (`{id,label,kinds,verify:{path,loginPointer}}`) register with `accounts::register_provider(<plugin id>, descriptor)` when the plugin loads; non-core providers use bearer/basic tokens only. Unload unregisters descriptors but keeps accounts and credentials.

`providers`: optional account providers this plugin may use with `httpFetch({account})`; requires `net:http`. An entry is either a core provider id (only `"github"`) or a token-only descriptor that core registers when the plugin loads and removes when it unloads (accounts and credentials stay): `{ "id": "jira", "label": "Jira", "kinds": ["basic", "bearer"], "verify": { "path": "/rest/api/3/myself", "loginPointer": "/displayName" } }`. `kinds` accepts `bearer` and `basic`; `verify.path` is a path under the site URL that the account Test calls, and `loginPointer` is the JSON Pointer of the viewer name in its response. An id may appear once; a declared descriptor must not use a core id.

Authenticated HTTP goes through `host.httpFetch(url, { account })` (see Tier 3h). Account actions are not currently a grant in `host.invoke`. Secret entry belongs in the core private native sink, which calls the write-only account service; plugins must never receive token values. See [Shared account registry](backend/github.md#shared-account-registry).

## Core setting controls

The core provides account, editable-list, multiselect, read-only status and action
button controls for catalogue-backed settings and work-item forms. Plugins do not
import these Solid components or gain new manifest fields from them. The host
owns account selection and native secret entry; no token enters a plugin prop.
See `plugin_dev_guide` → **Settings field kinds** for the core-rendered contracts.

## Tasks roadmap

Core `TasksRoadmap` renders `WorkItemProvider.list` pages and the provider's date
descriptors. The host passes `provider`, `onOpen(reference)`, optional `query`,
`visible`, `initialDate`, `startFieldId` and `targetFieldId`. Absent field IDs use
the first and second `kind: "date"` descriptors in provider order. Field names are
never guessed. Provider-declared start/target roles need a later contract extension.

Month/quarter zoom and Previous, Next and Today navigate the local calendar.
ISO date-only values retain their calendar day. Timestamps must carry an explicit
offset and display in the viewer's local time zone. Bars include the target day
and clip to the visible period. A single endpoint gives a one-day milestone.
Missing, invalid or reversed dates stay clickable in the **Without usable dates**
side list; out-of-period tasks stay listed in the timeline.

The shell hides the Roadmap view while the selected provider declares no
`kind: "date"` field.

Both lists use virtual rows. **Load more** follows provider cursors. Failed
refreshes retain stale results; obsolete provider responses cannot replace the
active view. This component adds no manifest, IPC or HTTP API. Tasks shell mounting
is a separate integration step.

## Tasks data contract (Phase A4)

Register via `host.registerWorkItemProvider(provider)`, returning an auto-cleaned
Disposable. Provider/account pairs must be unique; core consumers use
`workItemProviderRegistry.get(providerId, accountId)` or `.list(providerId?)`.
The async `WorkItemProvider` port includes list/detail/comments/act, dynamic
createFields(itemType?), grouped(query) and board(id, query). Grouping uses query.groupBy;
board lanes include per-card moves (columnId and action) so UI never guesses legal moves.
The test/development fake is `createInMemoryWorkItemProvider()` in
`src/plugins/testing/inMemoryWorkItemProvider.ts`, seeded from the same synthetic
JSON as the Rust fake. No fake routes or IPC commands are installed.


Core owns the Solid Tasks views; provider plugins supply data, schemas and actions,
never view HTML. A4 defines DTOs only: registration, transport and visible views
are delivered by the following integration steps.

### Work-item links (Phase A3b)

`host.linkItem(repoPath, branch, link)` persists a link from a worktree branch to a
work item; `host.unlinkItem(repoPath, branch)` removes it and resolves `true` when a
link existed. No capability is required. `link` is
`{ reference: { providerId, accountId, id }, key, title, url, state: { id, name, tone } | null }`;
`url` must be http(s). The sidebar branch row shows `key` coloured by `state.tone`;
hovering refreshes the state through the provider's `detail(reference)`, and a click
opens `url` without selecting the branch. TUIC drops the link when it removes the
worktree. `repoPath` is the main repository path, `branch` the worktree branch name.

### Work-item field kind inventory
`text`, `markdown`, `enum`, `user`, `labels`, `date`, `number`, `link`

- `text`: a plain text input; `markdown`: the shared markdown renderer/editor.
- `enum`: a select using provider option IDs; `user`: a user picker, multiple users.
- `labels`: multiple labels with stable IDs; `date`: ISO-8601 date input.
- `number`: finite numeric input; `link`: URL/link control.

Display-only fields set `sortable`, `filterable` and `groupable` to false. The roadmap still reads date fields.

The canonical DTOs are `work_items::model` and `work_items::protocol` in Rust.
Generate their TypeScript declarations with
`python3 scripts/generate-work-item-types.py`; import from
`src/types/work-items.gen.ts`. Fields use camelCase; discriminated unions use
`{kind, value}` with snake_case kinds. `WorkItemRef` scopes opaque item IDs by
`providerId` and `accountId`. Missing field keys mean not loaded; null means unset.

Each field schema declares `sortable`, `groupable` and `filterable` booleans for
the provider list endpoint. Tasks offers only those declared query controls.
Create and detail schemas set these to false when they are not list fields.
A rejected query restores the last successful controls and shows the reason.

`WorkItemQuery` carries ANDed filters (values are ORed), saved filter ID, native
search, sort, bounded page size and opaque cursor. Providers must bind cursors to
the account/query; selecting a preset starts with no cursor. `WorkItemPage`
contains items, `nextCursor` and an optional `total` (all matches, when the provider knows it). Details load body, field schemas, allowed action
descriptors and a separately paginated comment page on open. Comment entries are
`normal`, `review_thread` (resolved, outdated, path, line, originalLine, reviewed
commitId and a nullable parsed `finding`), or `review_summary` (state and reviewed
commitId). Comment authors carry `bot`. Bodies are markdown, not trusted HTML; finding
text is untrusted bot data, shown as plain text and never applied or executed.

Actions are `create`, `edit_field`, `transition`, `comment`, and `start_work`.
Transitions use provider action IDs, not state display names. Board columns may carry a default edit/transition action or null; per-card moves
carry the actual legal action to dispatch on drop (workflow IDs can vary by item). Start-work carries repoPath,
branch, optional baseRef, prompt, agentType and explicit draft/submit delivery;
core executes the existing worktree/agent workflow and owns errors/rollback.
Core `CardMoveMenu` provides a portalled, keyboard-accessible menu of other columns and enables only destinations allowed by the card's moves.

`work_items::demo::demo_provider()` supplies deterministic local-only data with
list, detail, comments, schemas, saved filters, board columns and mutations.
It touches no network, credentials, filesystem or subprocesses. It is a development
provider, not a fixture proving GitHub/Jira/GitLab API compatibility. Its search is
literal text; sort supports key/title/updatedAt. Its cursor expires on mutation.

## Tasks detail

Core's `TasksDetail` consumes the async `WorkItemProvider` port and a `WorkItemRef`.
The Tasks shell supplies the selected provider/reference and `visible` state.
Providers supply data and declared actions; they do not render this view.

The header shows key, title, type, state and updated time. Detail field schemas
select text, Markdown, enum, user, labels, date, number and link controls. Saving
requires an editable field and an enabled `edit_field` action. Required fields
cannot be cleared. Values retain their tagged field-value shape. The detail shows
Description first, then the fields, then Comments. A schema of more than eight
fields folds behind a "Fields (N)" summary so a wide Jira project does not push
Description and Comments out of view; no field is dropped.

Descriptions and normal comments, review threads and review summaries render
through `ContentRenderer` with `untrustedContent`. Markdown, tables, images and
`<details>` survive; scripts, event handlers and provider CSS do not. Review
threads show file/line and resolved state. The provider's comment cursor drives
Load more. Add comment retains its draft on rejection.
An accepted comment clears its draft even if reloading opened comment pages
fails. Core keeps the returned item, reports the history error and lets Retry
reload those pages.

Transition buttons use declared IDs and disabled reasons. The pending action
label appears immediately while the authoritative state ID stays intact.
Success adopts the returned detail; rejection restores the state label and
shows the error. Core does not infer a target state from a transition name.

A `start_work` descriptor in `actions` shows a Start work button in the detail and
in the board card's actions menu (the menu reads the item's detail when opened).
Core sends the provider a `start_work` action (active repository, branch, item
brief, `draft` delivery); the provider returns the intent and core runs the host
start-work workflow with the item link. The button shows a pending state, then
the started branch, or the error with Retry. The branch is the item field
`branch` (text) when the provider fills it, else a slug of key and title. This
field is a hint and needs no schema entry. "Copy branch name" copies that branch.

Refresh is manual and periodic (60 seconds by default) while the detail, Tasks
view and document are visible. Refresh preserves disclosure state and reloads
comment pages already opened. Switching items rejects stale responses.

## Start work and URL paste

`host.startWork({ repoPath, branch, baseRef?, prompt, agentType?, delivery?, link? })`
(capability `pty:write`) creates a worktree on a new `branch` (from `baseRef`, default
the repo's default branch) and opens an agent in it. The agent is `agentType` or, when
omitted, the first enabled agent with a run config on the repo's machine; that run
config supplies the command and env. The prompt sits inside an `<untrusted-content>`
block, control bytes removed, capped at 12000 characters. `delivery: "draft"`
(default) waits for the agent's composer and types the prompt there, leaving Enter to
the user. `"submit"` starts the agent with the prompt as its first message, quoted as
one POSIX shell argument after a `--` option terminator, for claude, codex, gemini and grok
(their `--help` lists a positional prompt for an interactive session and they honour `--`); other agents (pi has no `--`
handling), prompts over 16384
characters and cmd/PowerShell shells get the prompt typed and submitted instead. `link` links the item to the
new worktree. It resolves to `{ repoPath, workspaceId, branch, worktreePath }`.
The GitHub Auto-fix action uses the same path with `delivery: "submit"`.

`host.registerUrlResolver((url, signal) => ...)` claims URLs pasted in the command
palette. Return `null` for a URL that is not yours, or `{ link, startWork? }`.
Resolvers run in registration order; the first non-null result wins and a resolver
that throws is skipped. When a worktree is already linked to `link.reference`, the
palette offers **Open worktree**; otherwise it offers **Start work** with your
request (`repoPath` defaults to the active repository).

## Tasks create

Core's `TasksCreate` consumes `createFields` and `act` of the `WorkItemProvider` port.
The host supplies `provider`, `onCreated(detail)` to open the created item, and
optional `onCancel`. Providers supply types and schemas; they do not render the form.

`createFields()` without a type must return an enum field with id `itemType` that
lists the creatable types; a single type is preselected. `createFields(type)`
returns the fields of that type (an unknown type is an error, and `create`
rejects an undeclared `itemType`). Title and Description are fixed inputs, so a
schema does not repeat them. Only editable fields render.

Create lists Type, Title, Description and the required fields first. Optional
fields sit in a folded "Optional fields (N)" group with a name filter; the filter
only hides a field and never hides one that blocks Create.

A type change reloads the schema; values survive only for the same field id and
kind, and a late schema of an earlier type is dropped. Create stays disabled until
Title, Type and every required field of the current type are set. Submit sends one
`create` action with every current field (unset is null). A rejection keeps the draft.

Create user controls use optional `lookupUsers(fieldId, query, signal?)`, which
returns `{id, name}` choices for that provider/account. Search text is never a
selected user; rejected searches stay visible and late responses are discarded.
Static user options remain selectable. Labels without static options accept
comma-separated free text. Required-field errors are linked to their controls.
The Jira provider omits configured Project and selected Issue Type from its
create schema, searches project-assignable users for Assignee, and searches users
for Reporter. Cloud choices use account IDs; Data Center choices use usernames.
A provider may declare optional `singleUser(fieldId)`; Jira does, so a second pick in
a user field replaces the first. The control shows and announces the chosen user.
Jira create metadata that carries `hasDefaultValue` (Reporter) is not required in
the form, and empty optional fields are left out of the create payload so Jira
applies its own defaults.

## Tasks setup

With no work-item provider registered, the Tasks tab shows install prompts for
`github-work`, `gitlab` and `jira`. Once a plugin is installed, a form edits its
account (`id`, `repository`, `host`, `project`/`projectId`, `boards`) and merges it
into the plugin's `data/config.json`, then reloads the plugin. Other accounts and keys
stay; a file that is not valid JSON is never overwritten. **Provider settings** in the
Tasks header opens the same form. The Plugins settings tab lists install hints for
the work-item plugins that are missing. Open Tasks is always offered.

A provider page with `total <= 1000` can enable complete local Tasks queries after every page is collected and its unique item count matches the stable total. Omit `total` when unknown; never infer a count from a cursor or loaded rows. Local query projection is the host Rust command `project_work_items`, shared with `/tasks/project`. Collection preserves GitLab type, state and preset scope. GitLab username filters remain server-side because list rows hold numeric user identities, not usernames. Roadmap keeps provider paging and does not collect complete scopes automatically.

GitHub Tasks boards (#1846-55d7): the optional provider `listBoards(signal)` port discovers the current user Projects v2 when Board opens. The existing picker selects a project; paging preserves project identities. Missing `read:project` stays visible instead of claiming there are no boards. Providers without discovery retain their configured boards. Discovery does not persist selections or write to GitHub.

### Account-scoped GitHub review and action ports

`host.githubReviewThreads(account, reference, after)` returns the core B6 review
thread page, including head SHA, bot authors and parsed findings.
`host.githubPrAction(account, reference, request)` calls core B7 policy.
Both require `net:http` and an on-disk manifest declaring `providers: ["github"]`.
The reference must identify a GitHub pull request in the selected account. Core
loads that exact account credential, never a local repository default. Cross-account
references fail before credential lookup. No token is returned to the plugin.

Requests use `action`: `state`, `auto_merge` (`enabled`, optional `mergeMethod`,
`expectedHeadSha`), `queue` (`enqueued`, optional `expectedHeadSha`), `merge`
(`mergeMethod`, `expectedHeadSha`), `rerun` (`runId`), or `review` (`draft` with
`headSha`, `body`, `event`, `comments`). Existing core reviewed-head validation,
merge queue policy and shared request budgets apply unchanged. The desktop command
`plugin_github_action` and `POST /api/plugins/{plugin_id}/github` accept identical
`{account, reference, request}` payloads (IPC also supplies `pluginId`).
The GitHub Work runtime binds both ports when it registers each configured account.
No live write or desktop-load verification is implied by recorded-fixture tests.

### Settings startup is not a plugin capability

Plugin-scoped settings do not confer host account, executable or provider-login authority. Runtime shortcut-ID publication is metadata, not permission to invoke authentication or arbitrary commands. The host [startup contract](backend/settings-startup.md) documents typed GitHub enumeration and Ego human-login owner workflows; these do not grant plugins host authentication authority.
Jira and GitLab expose a default project in their schema-driven plugin settings page. Jira uses a project ID; GitLab accepts a project path or ID. A project saved for an account in Tasks setup takes precedence. The default applies after the plugin reloads; empty means no fallback project. Account bindings remain in `data/work-items.json`, while these defaults use the settings service in `data/config.json`.
