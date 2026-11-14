/// AI-optimized plugin development reference for TUICommander.
/// Compiled as a const so it's zero-cost at runtime.
pub const PLUGIN_DOCS: &str = r###"# TUICommander Plugin Development Reference

## Settings startup authority

Plugin-scoped settings do not confer host account, executable or provider-login authority. Runtime shortcut-ID publication is metadata, not permission to invoke authentication or arbitrary commands. The host startup contract documents typed GitHub enumeration and Ego human-login owner workflows; these do not grant plugins host authentication authority (docs/backend/settings-startup.md).

## Installation

Create directory `{id}/` containing `manifest.json` + `main.js` under the platform plugins path:
- macOS: `~/Library/Application Support/com.tuic.commander/plugins/`
- Linux: `~/.config/tuicommander/plugins/`
- Windows: `%APPDATA%/com.tuic.commander/plugins/`

Hot reload: editing any file in the plugin directory triggers automatic unload + re-import. Adding a new plugin directory or symlink after the app has started is also discovered live — no restart needed. Reload batches deduplicate changed enabled IDs and share a serialized queue. Discovery reads only those plugin roots; unchanged payload file hashes are reused by modification time and size. Startup still uses full discovery.

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
- `agentTypes`: optional array of agent type strings. When set, output watchers and structured event handlers only fire for terminals running a matching agent. Omit or use `[]` for universal plugins. Valid terminal values: `claude`, `gemini`, `opencode`, `codex`, `amp`, `cursor`, `goose`, `grok`, `droid`, `pi`, `ego`, `git`.
- `binaries`: optional array of CLI binary names this plugin may execute via `exec:cli` (e.g. `["rtk", "mdkb"]`). The on-disk manifest is the source of truth — binaries not declared here are rejected.
- `providers`: optional account providers this plugin may use with `httpFetch({account})`; requires `net:http`. An entry is either a core provider id (only `"github"`) or a token-only descriptor that core registers when the plugin loads and removes when it unloads (accounts and credentials stay): `{ "id": "jira", "label": "Jira", "kinds": ["basic", "bearer"], "verify": { "path": "/rest/api/3/myself", "loginPointer": "/displayName" } }`. `kinds` accepts `bearer` and `basic`; `verify.path` is a path under the site URL that the account Test calls, and `loginPointer` is the JSON Pointer of the viewer name in its response. An id may appear once; a declared descriptor must not use a core id.
- Module default export must have `id`, `onload(host)`, `onunload()`

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

Core supplies typed migrations for build-cleaner, at-capacity-retry,
cache-keepalive, claude-wakeup and the Telegram notifier example. Registration
fills missing defaults without discarding saved values, validates the result and
atomically writes an owner-only config file (0600 on Unix). Existing bot tokens
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
`addItem`, `clearTicker`, `deleteBuildArtifact`, `execCli`, `getActiveRepo`, `getActiveRepoPath`, `getActiveTerminalSessionId`, `getClaudeProjectDir`, `getGitBranches`, `getGitDiff`, `getPrNotifications`, `getRecentCommits`, `getRemotes`, `getRepoPathForSession`, `getRepos`, `getSessionCwd`, `getSettings`, `getTerminalState`, `githubPrAction`, `githubReviewThreads`, `httpFetch`, `invoke`, `linkItem`, `listDirectory`, `log`, `onStateChange`, `openEditorTab`, `openMarkdownFile`, `openMarkdownFileBackground`, `openMarkdownPanel`, `openPanel`, `playNotificationSound`, `readCredential`, `readFile`, `readFileBase64`, `fileSize`, `readFileRange`, `readFileTail`, `readFiles`, `readSessionOutput`, `registerCommand`, `registerContextMenuAction`, `registerDashboard`, `registerFileIconProvider`, `registerFilePreview`, `registerMarkdownProvider`, `registerOutputWatcher`, `registerSection`, `registerSidebarPanel`, `registerStructuredEventHandler`, `registerTerminalAction`, `registerUrlResolver`, `registerWorkItemProvider`, `removeItem`, `renamePath`, `scanBuildArtifacts`, `sendAgentInput`, `sendAgentInputIfEmpty`, `setTicker`, `settings`, `startWork`, `trimBuildArtifact`, `unlinkItem`, `updateItem`, `watchPath`, `writeFile`, `writeFileBase64`, `writePty`.

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
host.getRemotes(repoPath)               // [{ name, url }] — every remote; credentials in URLs are removed
```

### Tier 3: Write Actions (capability-gated)

| Method | Capability |
|--------|------------|
| `await host.writePty(sessionId: string, data: string): Promise<void>` | `pty:write` |
| `await host.sendAgentInput(sessionId: string, text: string): Promise<void>` | `pty:write` |
| `await host.sendAgentInputIfEmpty(sessionId: string, text: string): Promise<boolean>` | `pty:write` |
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
| `await host.startWork({ repoPath, branch, baseRef?, prompt, agentType?, delivery?, link? }): Promise<StartWorkResult>` | `pty:write` |
| `host.registerUrlResolver(resolve(url, signal)): Disposable` | *(none)* |

TerminalActionContext (passed to action/disabled at right-click time): `{ sessionId: string | null, repoPath: string | null }`. Actions appear in terminal right-click "Actions" submenu. Submenu hidden when no actions registered. Stale handlers (after plugin unload) are no-ops.

ContextMenuAction targets: `"terminal"`, `"branch"`, `"repo"`, `"tab"`. ContextMenuContext: `{ target, sessionId?, repoPath?, branchName?, tabId? }`. Actions appear after built-in items in the respective context menus.

SidebarPanelHandle: `{ setItems(items), setBadge(text), dispose() }`. SidebarItem: `{ id, label, subtitle?, icon?, iconColor?, onClick?, contextMenu?: [{ label, action, disabled? }] }`. Panels appear below branches in the sidebar, scoped per-repo. Badge shows a counter pill on the header.

PanelHandle: `{ tabId, update(html), close(), send(data, transfer?) }` — HTML rendered in an iframe with `sandbox="allow-scripts allow-same-origin"` (not host-DOM isolation), automatic base stylesheet + CSS theme variable injection. Write minimal plugin-specific CSS only (see Panel CSS Design Strategy above). Use `onMessage` callback to receive messages from iframe, `send()` to post messages back. `send(data)` delivers data to the iframe SDK `tuic.onMessage` through a `tuic:host-message` envelope. Panels and bridges close on plugin unload; clean up your own timers/listeners in `onunload`. Pass an optional `Transferable[]` to transfer ownership of large buffers instead of copying them. Every iframe also receives the TUIC SDK (`window.tuic`) — use `tuic.open(path, {pinned?})` to open markdown files, `tuic.edit(path, {line?})` to open files in the code editor, `tuic.terminal(repoPath)` to open terminals, `tuic.openUrl(url)` to open http/https/mailto URLs externally, or `<a href="tuic://open/path">` / `<a href="tuic://edit/path?line=10">` links for automatic interception. Absolute http/https/mailto links in inline panels also open externally when clicked. `open` and `edit` accept an absolute path outside every known repo and open it as an external file; relative paths stay inside the panel's repo and `getFile` reads only inside a known repo. Cross-origin URL panels cannot expose their clicks, so use the tab menu's Open in Browser action.

`registerDashboard({ label?, icon?, open })`: Register a one-click entry point shown as a **Dashboard** button in *Settings → Plugins*. The host closes the Settings panel automatically before calling `open()`. A plugin may only register one dashboard; second calls replace the first. Pair this with the `.dashboard` style guide above.

`registerCommand({ id, title, defaultShortcut?, run })`: Register a user-rebindable command. Appears in *Settings → Keyboard Shortcuts* under "Plugin Commands". Action name is auto-namespaced as `plugin:<pluginId>:<id>`. The active frontend publishes these IDs to the Rust keybinding choices owner; unload removes them from later refreshes. This runtime publication never saves shortcut overrides. `defaultShortcut` is optional ("Cmd+Shift+K" etc.); on conflict the command is registered but left unbound with a warning.
`startWork(request)`: Create a worktree and a new `branch` (from `baseRef`, default the repo's default branch) in `repoPath`, then open an agent in it. The agent is `agentType` or, when omitted, the first enabled agent with a run config on the repo's machine; its run config supplies the command and env. `prompt` is never a shell argument: the host waits until the agent shows its composer and types the prompt there, inside an `<untrusted-content>` block with control bytes removed and a 12000-character cap. `delivery: "draft"` (default) leaves Enter to the user; `"submit"` sends it. `link: { reference, key, title, url, state }` links the item to the new worktree. Resolves to `{ repoPath, workspaceId, branch, worktreePath }`; rejects when the repo is already creating a worktree, the agent is unknown or disabled, or worktree creation fails.

`registerUrlResolver(resolve)`: Claim URLs pasted in the command palette. `resolve(url, signal)` returns `null` for a URL that is not yours, or `{ link, startWork? }` where `startWork` is a `startWork` request without `repoPath`/`link` (`repoPath` optional, default the active repo). Resolvers run in registration order and the first non-null result wins; a resolver that throws is skipped. When a worktree is already linked to `link.reference` the palette offers to open it; otherwise it offers start work with your request.

HttpResponse: `{ status: number, headers: Record<string, string>, body: string }` — non-2xx is NOT an error.

**setTicker notes:** Shared ticker area rotates messages from all plugins. Priority tiers: <10 = popover only, 10-99 = auto-rotate (5s), >=100 = urgent pin. `label` is shown as source prefix (e.g. "Usage · 5h: 42%"). Counter badge (1/3 ▸) shown when multiple tickers active. Click badge to cycle, right-click for popover. Removed aliases `postTickerMessage`/`removeTickerMessage` are not supported.


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

`httpFetch(url, { signal })` accepts a host-local `AbortSignal`. A pre-aborted signal starts no request; abort cancels the Rust request while waiting for headers or collecting the body and rejects with `AbortError`. Unloading the plugin also cancels its active HTTP requests. Signals never cross IPC/HTTP: the host sends a generated UUID and a plugin-scoped cancel command. Requests without a signal keep the same response contract.

Jira and GitLab expose a default project in their schema-driven plugin settings page. Jira uses a project ID; GitLab accepts a project path or ID. A project saved for an account in Tasks setup takes precedence. The default applies after the plugin reloads; empty means no fallback project. Account bindings remain in `data/work-items.json`, while these defaults use the settings service in `data/config.json`.

**httpFetch notes:** Localhost blocked unless declared in `allowedUrls`. Max 5 redirects. 30s timeout, 10 MB limit. Default `User-Agent: tuicommander-plugin/<version>`; a plugin `User-Agent` header overrides it. An empty `allowedUrls` list grants no HTTP access.

**Owner destinations:** the owner can list URL patterns in `plugin_http_destinations` in `config.json` (same syntax as `allowedUrls`). Every `net:http` plugin can then reach them without an account and without a manifest change, localhost and LAN addresses included. The manifest still needs its non-empty `allowedUrls` list. Only `http` and `https` pass. The first URL and each redirect hop are checked. An agent write through the settings API that adds a pattern needs the owner's confirmation; removing a pattern applies directly. Owner destinations never widen where an account request can start; a redirect hop to another scheme, host or port still loses `Authorization`.

**Account requests:** `httpFetch(url, { account: "<account id>" })` makes Rust inject that account's credential (`Authorization: Bearer <token>`, or `Basic` for Basic accounts). The plugin never sees the token. Rules: the manifest `providers` list must contain the account's provider; the URL must be under the account host (same scheme, host and port, path prefix of the site URL; GitHub accounts use their API base, `https://api.github.com/` or `https://<ghe>/api/`); the account host rule replaces the `allowedUrls` check for the first URL; you must not set `Authorization` yourself. Each redirect hop must be under the account host, match `allowedUrls` or match an owner destination, and a hop to another scheme, host or port loses the `Authorization` header. The token is read again for each request, so a re-entered credential applies at once; a 401 comes back to you as a normal response. Token values in the response body and headers are replaced with `[redacted]`.

### Tier 3b: Filesystem Operations (capability-gated)

All paths must be absolute. Resolved via canonicalize (symlinks, `..` resolved). Reads, listings and watches reach any path the user can access, also outside `$HOME`; OS access errors and size limits apply. Writes and renames stay within `$HOME`.

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

// Random-access binary previews, fs:read, existing absolute-path read policy.
const byteLength = await host.fileSize(absolutePath);
const range = await host.readFileRange(absolutePath, 0, 8); // exact Base64 bytes; 8 MiB hard cap
// Non-negative safe integer offsets/lengths; past EOF or short reads fail.

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

`await host.execCli(binary: string, args: string[], cwd?: string, options?: { timeoutMs?: number }): Promise<string>` — execute a CLI binary declared in the plugin's manifest `binaries` field, return stdout.

Only binaries listed in `manifest.json` `binaries` array are allowed. The declared bare name resolves to the first executable in the backend PATH (system folders and version-manager shims included), then `~/.local/bin`, `~/.npm-global/bin`, `~/go/bin`, and runs with that folder list as its PATH; on Windows `<name>.exe` or `<name>.cmd`. Working directory must be an absolute path to an existing directory. Timeout 30s by default; `timeoutMs` sets 1..3600000 ms (other values are refused). At most 4 concurrent commands per plugin; further calls wait for a slot (no per-minute quota). 5 MB limit per output stream.

```javascript
const raw = await host.execCli("mdkb", ["--format", "json", "status"], repoPath);
const status = JSON.parse(raw);
```

`sendAgentInputIfEmpty` uses a backend transaction: current idle, awaiting, and tracked draft checks plus split writes are serialized with raw input and bookkeeping. It returns false without writing when blocked, true after submission; it never answers dialogs or drains queued commands. Do not authorize a send with a separate composer read. Load/reload replay includes awaiting and shell-state agent_type.


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
`agentTypes`, `allowedUrls`, `author`, `binaries`, `capabilities`, `contentSha256`, `description`, `id`, `main`, `minAppVersion`, `name`, `providers`, `settings`, `version`.

### Core UX component inventory
`BrowseRow`, `PluginLogViewer`, `PluginRow`, `PluginsTab`.

#### PluginsTab

Settings > Plugins has Installed and Browse views. Installed lists loaded/discovered plugin state. Browse reads community registry entries and offers install, update and refresh actions. Installation accepts ZIP archives. The manifest `id` identifies the package, `main` selects its module, and `minAppVersion` controls compatibility. `author` is optional metadata. `allowedUrls`, `agentTypes`, `binaries` and `providers` drive host access/event routing, not editable settings controls.

#### PluginRow

The Installed row shows manifest `name` (falls back to `id`), `version`, `description` and `capabilities`. It shows load errors, error counts and available updates. Its toggle persists enabled state; Logs expands PluginLogViewer; external packages can be uninstalled. A README.md adds a documentation button. `host.registerDashboard({ label?, icon?, open })` adds a dashboard button: core closes Settings before calling `open`. Register only one dashboard per plugin. `host.registerCommand({ id, title, defaultShortcut?, run })` adds a rebindable entry under Settings > Keyboard Shortcuts > Plugin Commands.

#### PluginLogViewer

The expanded Logs view shows the plugin ring buffer's timestamp, level and message, or an empty state. Use `host.log(level, message, data?)`; host boundary errors are captured automatically.

#### BrowseRow

The Browse row shows registry name, version, description, author, tags and minimum app version. It offers Install for an absent package, Update when a newer version is available or same-version payload identities differ, or Installed otherwise. Registry metadata is separate from the installed manifest. `contentSha256` is derived by Rust from installed payload files, not accepted from manifest input. Mutable data and packaging files are excluded. Unknown identities retain version-only comparison, and newer installed versions are never downgraded.

### Settings schema and Tasks views

PluginManifest declares optional `settings: { schema, defaults }`. Loaded plugin catalogue entries render searchable Integrations pages in Settings; unloading removes the page. Core reads descriptors and values and writes revision-checked edits through `plugin_settings_action`, the same scoped Rust service as `host.settings`. Schema titles and descriptions label the controls. Secrets show only set/unset and accept a replacement without displaying the stored value. Core Tasks views use data-only WorkItemProvider ports, not manifest view declarations.

### Plugin settings schema type inventory
`object`, `array`, `string`, `boolean`, `integer`, `number`.

Objects group leaf fields and require closed properties. Arrays contain bounded non-object items: string enums use SettingMultiSelect, string lists use SettingList, other arrays accept JSON. Strings use text inputs or enum selects, booleans use toggles, integers and numbers use numeric inputs. Read-only fields use SettingStatus. Reset and edit operations come from the live catalogue, not a second persistence path.

### Settings field kinds

These are reusable core Solid controls, not manifest fields or plugin imports. The catalogue renderer owns persistence and maps its JSON schema and choices to controls. All controls take `label` and optional `hint` and `disabled`. They never save settings directly.

### Settings control inventory
`SettingAccount`, `SettingButton`, `SettingList`, `SettingMultiSelect`, `SettingStatus`.

#### SettingAccount
Takes `provider`, `value` (account ID), and `onInput(id)`. Loads the A1 secret-safe account list through `account_action` with `{request:{action:"list"}}` and displays only labels for the selected provider. `onAdd(provider)` opens the host's native secret-entry flow; it receives no token and resolves after save or cancel, then the control reloads the list. `listAccounts` can supply a host-owned list port. Backend error text is never rendered. No credential is accepted as a control prop.

#### SettingList
Takes `rows: {id,values:Record<string,string>}[]`, `columns: {id,label,placeholder?}[]`, and `onChange(rows)`. Edits and removal use stable row IDs. Optional `onAdd()` supplies a new row with a unique stable ID and initial values. The caller owns validation, defaults and persistence.

#### SettingMultiSelect
Takes `value:string[]`, `options:{value,label}[]`, and `onInput(values)`. Checkbox choices support keyboard and touch interaction; changing one choice preserves the other selected values.

#### SettingStatus
Read-only `value` text with optional `tone:neutral|info|success|warning|danger`. The text communicates the state as well as its colour.

#### SettingButton
Takes `value` (button text) and `onClick()`. It does not submit an enclosing form; the caller handles the action and its pending state with `disabled`.

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
5. Register: pluginRegistry.register() awaits plugin.onload(host, signal); rejected startup is logged and partial registrations are cleaned up
6. Active: plugin receives PTY lines, structured events, uses PluginHost API
7. Hot reload: file changes trigger unregister + re-import (cache-busted); a newly added plugin directory/symlink is discovered and loaded live
8. Unload: plugin.onunload() called, tracked registrations disposed, openPanel tabs and bridges closed; plugin-owned timers/listeners need explicit cleanup

Async onload may return a Promise. The optional AbortSignal is aborted on unload/reload. Old host calls then throw before changing registrations or state. Pass the signal to cancellable operations; the host cannot stop arbitrary plugin code or requests that ignore it. Replacement waits for backend teardown, not obsolete startup. Startup must settle (resolve or reject); there is no startup or backend teardown timeout. An active startup that never settles remains loading, and pending backend teardown blocks replacements for that plugin ID. A replacement removed before startup begins receives neither onload nor onunload.

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

## Tasks MCP access

The existing MCP `repo` tool exposes `work_item` (linked caller-worktree item, reference, URL or account-scoped key; detail and paged comments) and `work_item_act` (comment/transition via provider `act`). Writes require `mcp_work_item_writes=true`; default false. Item text is JSON inside the start-work untrusted-content block. Rust validates replies and suppresses provider errors; credentials remain in the existing account HTTP injection/redaction path. The typed frontend bridge dispatches to the registered provider and atomically claims each request so multiple clients cannot duplicate writes. Plugins need no new methods. A frontend with the provider loaded is required. A timed-out write may have completed: read before retrying.

## Tasks data contract (Phase A4)

Register via `host.registerWorkItemProvider(provider)`; its Disposable is automatically
cleaned on plugin unload. Registrations are scoped by providerId/accountId and duplicates
are rejected. The async port lives in `src/plugins/workItemProvider.ts`. It exposes
list/detail/comments/act, dynamic createFields, grouped pages and board projections
with per-card moves (columnId and action); core renders those projections without guessing legality.

`createFields()` without a type returns one enum field with id `itemType` whose options are
the creatable item types (a single-type provider returns one option). `createFields(type)`
returns the create schema of that type, without title or body fields: `WorkItemCreate`
carries those. An unknown type is an error, and `create` rejects an undeclared `itemType`.

Core owns the Solid Tasks views; provider plugins supply data, schemas and actions,
never view HTML. The Tasks shell mounts the registered providers and their list, board, roadmap, create
and detail views. Provider registration supplies runtime descriptors, not manifest fields.

### Work-item links (Phase A3b)

Link a worktree branch to a work item. The link persists across restarts and the
sidebar branch row shows its key, coloured by `state.tone`; a click opens `url`.
TUIC drops the link when it removes that worktree.

```typescript
await host.linkItem(repoPath, branch, {
  reference: { providerId, accountId, id }, // WorkItemRef of your provider
  key: "ACME-7", title: "Fix login",
  url: "https://acme.atlassian.net/browse/ACME-7", // http(s) only, else rejected
  state: { id: "3", name: "In Progress", tone: "info" }, // or null
});
await host.unlinkItem(repoPath, branch) // true when a link existed
```

`repoPath` is the main repository path; `branch` is the worktree branch name. On hover
the badge refreshes the state through your provider's `detail(reference)`.

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
`normal`, `review_thread` (resolved, path, line and reviewed commitId), or
`review_summary` (state and reviewed commitId). Bodies are markdown, not trusted HTML.

Actions are `create`, `edit_field`, `transition`, `comment`, and `start_work`.
Transitions use provider action IDs, not state display names. Board columns may carry a default edit/transition action or null; per-card moves
carry the actual legal action to dispatch on drop (workflow IDs can vary by item). Start-work carries repoPath,
branch, optional baseRef, prompt, agentType and explicit draft/submit delivery;
core executes the existing worktree/agent workflow and owns errors/rollback.

`work_items::demo::demo_provider()` supplies deterministic local-only data with
list, detail, comments, schemas, saved filters, board columns and mutations.
Its item types are the types of its seed items.
It touches no network, credentials, filesystem or subprocesses. It is a development
provider, not a fixture proving GitHub/Jira/GitLab API compatibility. Its search is
literal text; sort supports key/title/updatedAt. Its cursor expires on mutation.

Optional provider listBoards(signal) discovers read-only boards on view opening; GitHub needs read:project.

## Tasks core UX contracts

Complete lists with a stable total <=1000 use project_work_items (/tasks/project) for Rust all-field projection. Unknown or large totals retain server capabilities; no fabricated counts.

### Tasks component inventory
`BoardLane`, `CardMoveMenu`, `CreateField`, `InstallWorkItemPlugin`, `RoadmapTimeline`, `StartWorkMenu`, `TaskComment`, `TaskCreateField`, `TaskCreateOptional`, `TaskCreateUser`, `TaskField`, `TaskMarkdown`, `TaskRows`, `TasksBoard`, `TasksBoardView`, `TasksCreate`, `TasksDetail`, `TasksList`, `TasksRoadmap`, `TasksSetup`, `TasksShell`.

### Tasks DTO field inventory
`WorkItem.assignees`, `WorkItem.fields`, `WorkItem.itemType`, `WorkItem.key`, `WorkItem.labels`, `WorkItem.reference`, `WorkItem.state`, `WorkItem.title`, `WorkItem.updatedAt`, `WorkItem.url`, `WorkItemActionDescriptor.disabledReason`, `WorkItemActionDescriptor.enabled`, `WorkItemActionDescriptor.id`, `WorkItemActionDescriptor.kind`, `WorkItemActionDescriptor.name`, `WorkItemBoard.columns`, `WorkItemBoard.fieldId`, `WorkItemBoard.id`, `WorkItemBoard.name`, `WorkItemBoard.writeDisabledReason`, `WorkItemBoardCard.item`, `WorkItemBoardCard.moves`, `WorkItemBoardColumn.action`, `WorkItemBoardColumn.id`, `WorkItemBoardColumn.name`, `WorkItemBoardLane.cards`, `WorkItemBoardLane.columnId`, `WorkItemBoardMove.action`, `WorkItemBoardMove.columnId`, `WorkItemBoardPage.board`, `WorkItemBoardPage.lanes`, `WorkItemBoardPage.nextCursor`, `WorkItemComment.author`, `WorkItemComment.body`, `WorkItemComment.createdAt`, `WorkItemComment.id`, `WorkItemComment.url`, `WorkItemCommentPage.entries`, `WorkItemCommentPage.nextCursor`, `WorkItemCreate.body`, `WorkItemCreate.fields`, `WorkItemCreate.itemType`, `WorkItemCreate.title`, `WorkItemDetail.actions`, `WorkItemDetail.body`, `WorkItemDetail.comments`, `WorkItemDetail.fields`, `WorkItemDetail.item`, `WorkItemEdit.fieldId`, `WorkItemEdit.value`, `WorkItemFieldSchema.editable`, `WorkItemFieldSchema.filterable`, `WorkItemFieldSchema.groupable`, `WorkItemFieldSchema.id`, `WorkItemFieldSchema.kind`, `WorkItemFieldSchema.name`, `WorkItemFieldSchema.options`, `WorkItemFieldSchema.required`, `WorkItemFieldSchema.sortable`, `WorkItemFilter.fieldId`, `WorkItemFilter.values`, `WorkItemGroup.id`, `WorkItemGroup.items`, `WorkItemGroup.name`, `WorkItemGroupedPage.groups`, `WorkItemGroupedPage.nextCursor`, `WorkItemGroupedPage.total`, `WorkItemLocalQuery.fields`, `WorkItemLocalQuery.items`, `WorkItemLocalQuery.query`, `WorkItemOption.id`, `WorkItemOption.name`, `WorkItemPage.items`, `WorkItemPage.nextCursor`, `WorkItemPage.total`, `WorkItemProviderSnapshot.accountId`, `WorkItemProviderSnapshot.boards`, `WorkItemProviderSnapshot.fields`, `WorkItemProviderSnapshot.items`, `WorkItemProviderSnapshot.name`, `WorkItemProviderSnapshot.providerId`, `WorkItemProviderSnapshot.savedFilters`, `WorkItemQuery.cursor`, `WorkItemQuery.filters`, `WorkItemQuery.groupBy`, `WorkItemQuery.limit`, `WorkItemQuery.savedFilterId`, `WorkItemQuery.search`, `WorkItemQuery.sort`, `WorkItemRef.accountId`, `WorkItemRef.id`, `WorkItemRef.providerId`, `WorkItemReviewFinding.addressedIn`, `WorkItemReviewFinding.agentPrompt`, `WorkItemReviewFinding.category`, `WorkItemReviewFinding.externalId`, `WorkItemReviewFinding.severity`, `WorkItemReviewFinding.severityLabel`, `WorkItemReviewFinding.suggestion`, `WorkItemReviewFinding.title`, `WorkItemReviewFinding.tool`, `WorkItemReviewSummary.comment`, `WorkItemReviewSummary.commitId`, `WorkItemReviewSummary.state`, `WorkItemReviewThread.comments`, `WorkItemReviewThread.commitId`, `WorkItemReviewThread.finding`, `WorkItemReviewThread.id`, `WorkItemReviewThread.line`, `WorkItemReviewThread.originalLine`, `WorkItemReviewThread.outdated`, `WorkItemReviewThread.path`, `WorkItemReviewThread.resolved`, `WorkItemSavedFilter.id`, `WorkItemSavedFilter.name`, `WorkItemSavedFilter.query`, `WorkItemSort.descending`, `WorkItemSort.fieldId`, `WorkItemStartWork.agentType`, `WorkItemStartWork.baseRef`, `WorkItemStartWork.branch`, `WorkItemStartWork.delivery`, `WorkItemStartWork.prompt`, `WorkItemStartWork.repoPath`, `WorkItemState.id`, `WorkItemState.name`, `WorkItemState.tone`, `WorkItemUser.avatarUrl`, `WorkItemUser.bot`, `WorkItemUser.id`, `WorkItemUser.name`.

### Tasks shell view inventory
`board`, `list`, `roadmap`.

### Roadmap date role inventory
`startFieldId`, `targetFieldId`.

All DTO fields above use the runtime provider contract, not PluginManifest settings.
The list, create, detail and board descriptions below explain their controls and actions.
`WorkItemRef` uses `providerId`, `accountId` and opaque `id` for identity.
`WorkItemUser` supplies `id`, `name`, `avatarUrl`, `bot`; `WorkItemOption` supplies `id`, `name`.
`WorkItemState` supplies authoritative `id`, visible `name` and semantic `tone`.
`WorkItemFieldSchema` supplies `id`, `name`, `kind`, `required`, `editable`, `options`,
`sortable`, `groupable`, `filterable`.
`WorkItem` supplies `reference`, `key`, `title`, `itemType`, `state`, `assignees`,
`labels`, `updatedAt`, `url` and typed `fields` (missing means unloaded, null means unset).
`WorkItemQuery` supplies `filters`, `savedFilterId`, `search`, `sort`, `groupBy`, `cursor`,
`limit`; `WorkItemFilter` supplies `fieldId`, ORed `values`; `WorkItemSort` supplies
`fieldId`, `descending`. `WorkItemSavedFilter` supplies `id`, `name`, cursor-free `query`.
`WorkItemPage` supplies `items`, `nextCursor` and optional `total`; `WorkItemGroupedPage`
supplies `groups`, `nextCursor` and optional `total` (the count of every match across all
pages; omit it when the provider cannot know it, and the list shows the loaded count only); `WorkItemGroup` supplies stable `id`, display `name`, authoritative `items`.
`WorkItemDetail` supplies `item`, Markdown `body`, `fields`, `actions`, `comments`.
`WorkItemActionDescriptor` supplies action `id`, `name`, `kind`, `enabled`, `disabledReason`.
`WorkItemCreate` supplies `title`, `body`, `itemType`, `fields`; `WorkItemEdit` supplies
`fieldId`, nullable typed `value`. `WorkItemStartWork` supplies `repoPath`, `branch`,
`baseRef`, `prompt`, `agentType`, explicit `delivery` for host-owned execution.
`WorkItemCommentPage` supplies `entries`, `nextCursor`; `WorkItemComment` supplies
`id`, `author`, Markdown `body`, `createdAt`, optional `url`.
`WorkItemReviewThread` supplies `id`, `resolved`, `outdated`, `path`, optional `line`
(null when outdated), `originalLine`, `commitId`, `comments`, nullable `finding`;
`WorkItemReviewFinding` supplies nullable `tool`, `title`, normalised `severity`, vendor
`severityLabel`, `category`, `suggestion`, `agentPrompt`, `addressedIn`, `externalId`,
all untrusted text shown as text; `WorkItemReviewSummary` supplies `comment`, review `state`, `commitId`.
`WorkItemBoard` supplies `id`, `name`, `fieldId`, `columns`, optional nullable
`writeDisabledReason`. Core shows the read-only reason while keeping cards and
removing legal moves from all loaded pages. Providers must also reject direct writes
and mark affected field schemas non-editable; unrelated actions remain available.
`WorkItemBoardColumn`
supplies `id`, `name`, nullable default `action`. `WorkItemBoardPage` supplies `board`,
`lanes`, `nextCursor`; `WorkItemBoardLane` supplies `columnId`, `cards`;
`WorkItemBoardCard` supplies `item`, legal `moves`; `WorkItemBoardMove` supplies
`columnId`, dispatchable `action`. `WorkItemProviderSnapshot` supplies `providerId`,
`accountId`, `name`, `fields`, `savedFilters`, `boards`, initial detail `items`.

#### TasksShell

The core Tasks tab selects a registered provider by `providerId` and `accountId`,
displays its `name`, and offers List, Board, Roadmap and New task. Roadmap is hidden
while the selected provider declares no `kind: "date"` field. Changing the source
clears the create/detail selection. New task mounts TasksCreate; opening a reference
mounts TasksDetail alongside the current view. `visible` gates hidden-view work.
With no provider registered the shell mounts TasksSetup. Provider settings opens the
same form for the plugin behind the selected provider.

#### TasksSetup

Core renders install prompts and setup forms for the work-item plugins `github-work`,
`gitlab` and `jira` from a core-owned descriptor list. A plugin that is not installed
gets an install button fed by the plugin registry; the GitHub Work prompt notes when
GitHub is connected. An installed plugin gets a form for its account `id`, `repository`
(GitHub), `host`, `project` or `projectId`, and `boards` (one `id name` per line). Save
merges the account into the plugin's `data/config.json`, keeps other accounts and keys,
refuses to overwrite a file that is not valid JSON, then reloads the plugin. The
Settings Plugins tab shows the same install prompts for plugins that are missing.

#### InstallWorkItemPlugin

Install button for one work-item plugin. It reads the plugin registry and calls the
install command for the matching entry; a missing entry disables the button.

#### TasksBoardView

The shell's board loader selects `provider.boards` by `id` and displays `name`.
It calls `board(id, query, signal)` with an all-items query limited to 50, then passes
`board` and `lanes` to TasksBoard. Moves call `act(reference, action)` and reload the
projection. Provider changes abort old requests; only the latest response is adopted.
No boards and provider errors are visible. This loader currently does not expose
Load more for a board page's `nextCursor`.

#### BoardLane

Renders a column's `id`, `name` and virtualized `cards`. Cards display item `key`,
`title` and state `name`/`tone`. The move selector enables only destinations allowed
by that card's `moves`; pending cards show pending state instead of duplicate moves.

#### CardMoveMenu

A portalled, keyboard-accessible card move menu lists other columns and enables only destinations allowed by the card's moves.

#### StartWorkMenu

A card's actions button. Opening it reads `detail(reference)` and lists the item's `start_work` action descriptors (name, enabled state and disabled reason). Choosing one runs the same start-work request as the detail. The card shows the pending state, the started branch, or the error with Retry.

#### TaskRows

Virtualized rows render item `key`, `title`, state `name`/`tone` and open `reference`.
Grouped rows use the provider's group `id`, `name` and `items`, not inferred membership.
Query changes reset scrolling; `loading` marks the results busy.

#### RoadmapTimeline

Virtualized dated rows show item `key`, `title` and local start/target calendar days.
They use the host projection's spans and window, month/quarter zoom and `onOpen(reference)`.
The provider's typed date values and the explicit `startFieldId`/`targetFieldId` roles
(or the first two date descriptors) drive placement, not display names.

#### CardMoveMenu
Portalled board move menu. It lists other columns, disables forbidden moves and
calls the supplied move handler. Arrow keys move focus; Escape closes the menu
and restores focus. Outside pointer presses and scrolling dismiss it.

#### CreateField
Shared create-field label, hint and error wrapper. It gives the control stable
IDs and connects its label and accessible descriptions.

#### TaskCreateOptional
Collapsible optional-field section with a name filter. Hidden fields stay mounted
so filtering preserves drafts. Invalid fields remain visible.

#### TaskCreateUser
Provider-backed user search and selection. It keeps selected names, supports
removal, exposes loading and search errors, and respects single-user metadata.

#### TaskCreateField

The create schema's `name`, `kind`, `required` and `options` select shared inputs:
Markdown textarea, enum select, user/labels multiselect, or text/number/date/URL input.
Option IDs are saved, not names. `disabled` reflects pending form work; the caller
owns draft values and validation. Only editable fields are mounted by TasksCreate.

#### TaskField

Detail schema `name`, `kind`, `required`, `editable` and `options` drive its control.
Typed `value` initializes the draft. `canEdit` additionally requires an enabled
edit action; Save sends the typed value (or null), rejects invalid required/number
values, and adopts the authoritative value after success.

#### TaskComment

Renders the tagged comment entry: normal comment, review thread or review summary.
Author `name`, `createdAt` and Markdown `body` label comments. Threads add `path`,
`line`, `resolved` and their `comments`; summaries add review `state` and `comment`.
Reviewed `commitId` stays provider data; core does not guess a current commit.

#### TaskMarkdown

Renders provider body/comment Markdown with ContentRenderer's untrusted-content mode.
It strips executable HTML and preserves reader-owned disclosure expansion on refresh.

## Tasks detail

### Tasks detail component inventory
`TasksDetail`.

#### TasksDetail

Core renders work-item detail with Solid controls. Providers supply `WorkItemDetail` through the async `WorkItemProvider` port; they do not supply a view or iframe. The Tasks shell mounts `TasksDetail` with a provider and `{providerId, accountId, id}` reference, and passes `visible=false` when hidden. TasksShell selects the registered provider and mounts the detail alongside the active view.

- Header: key, title, type, authoritative state, updated time and provider link.
- Fields: the detail's descriptors select text, Markdown, enum, user, labels, date, number or link controls. Only editable fields with an enabled `edit_field` action can be saved. Values use the tagged field-value union; required empty values cannot be submitted.
- Body and all comment entries use `ContentRenderer` with `untrustedContent=true`. It keeps Markdown, tables, images and `<details>` while stripping scripts, event handlers, style tags and style attributes. Reader expansion survives refresh.
- Comments: `normal`, `review_thread` (file, line and resolved state), and `review_summary` (review state) render together. Load more uses `comments(reference,cursor,50)` and the provider cursor. Add comment dispatches `comment`; a rejected request keeps the draft.
- Status: buttons use the declared transition action IDs, enabled state and disabled reasons. The pending action label appears immediately; state IDs are never inferred from the label. Success adopts the returned detail; rejection restores the authoritative label and reports the error.
- Start work: a `start_work` descriptor shows a Start work button here and in the board card's actions menu (read from the item's detail when opened). Core sends the provider a `start_work` action (active repository, branch, item brief, `draft` delivery); the provider returns the intent and core runs the host start-work workflow with the item link. The button shows a pending state, then the started branch, or the error with Retry. The branch is the item field `branch` (text) when the provider fills it, else a slug of key and title; the field is a hint and needs no schema entry. Copy branch name copies it.
- Refresh: manual Refresh plus a 60-second timer, enabled by default only while open, the Tasks view is visible and the document is visible. Already-loaded comment pages are refreshed too. Old-item responses cannot replace the current item.

## Tasks create

### Tasks create component inventory
`TasksCreate`.

#### TasksCreate

Core renders the create form with the shared setting controls (`SettingField`, `SettingMultiSelect`). Providers supply item types and field schemas through `createFields`; they do not supply a view or iframe. The host mounts `TasksCreate` with `provider`, `onCreated(detail)` and optional `onCancel`, and opens the returned item.

- Item types: `createFields()` without a type MUST return an enum field with id `itemType` whose options are the creatable types. One option is preselected. Without it the form reports that the provider declares no item types.
- Fields: `createFields(type)` returns the schema of that type. The form renders its editable fields; `itemType` is never a field. Title and Description are fixed inputs mapped to `WorkItemCreate.title` and `body`, so do not return title, summary or description fields.
- Type change: the form reloads the schema. A value survives only where the new type has the same field id and kind. A late schema for an earlier type is dropped.
- Validation: Create stays disabled until Title, Type and every required field of the current type hold a value; numbers must be finite and enum values must be declared options.
- Submit: dispatches `act(null, {kind: "create", value: {title, body, itemType, fields}})` once. `fields` holds every schema field of the current type; unset values are null. A rejection shows the error and keeps the draft. A `{kind: "item"}` result goes to `onCreated`.

## Tasks list

#### TasksList

Core TasksList consumes WorkItemProvider data, never plugin HTML or iframes.
Providers supply providerId, accountId, name, fields, savedFilters, and async list(query, signal).
Core renders source and preset selectors, descriptor filters, 250 ms debounced search,
provider sorting, virtual rows, and Load more using nextCursor.
Queries carry filters, savedFilterId, search, sort, groupBy, cursor and limit.
Grouped results come from grouped(query, signal); core renders provider group names and members.
The LRU cache holds at most 32 pages, partitioned by provider, account and query.
Obsolete responses cannot replace newer results. Repeated references update a row;
repeated cursors stop pagination. Failed refreshes retain rows marked Stale results.
Opening an item calls the host onOpen(reference); list rendering does not fetch detail.
TasksShell mounts this view for the selected provider.

## Tasks roadmap

#### TasksRoadmap

Core TasksRoadmap renders WorkItemProvider.list pages and provider.fields date descriptors.
The host passes provider, onOpen(reference), optional query/visible/initialDate and optional
startFieldId/targetFieldId. Absent IDs use the first and second kind=date descriptors in
provider order; field names are never guessed. Provider-declared date roles are deferred.
Month/quarter zoom, previous/next and Today navigate the local calendar. ISO date-only
values keep their calendar day; timestamps require an explicit offset and display in the
viewer local time zone. Bars include the target day and clip to the visible period.
One unset endpoint gives a one-day milestone. Missing, invalid or reversed dates remain
clickable in the Without usable dates side list. Out-of-period tasks remain listed.
Both lists use virtual rows. Load more follows provider cursors; failed refreshes retain
stale results, and obsolete provider responses cannot replace the current view.
TasksShell mounts this view for the selected provider without explicit date role IDs.
No manifest view declaration is needed.

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

## Tasks board

#### TasksBoard

The core renders `TasksBoard` with Solid controls, never plugin HTML or an iframe.
Declare boards as `{id,name,fieldId,columns:[{id,name,action}]}`. Each column's
`action` is a nullable default for simple field boards. Workflow transitions can differ
per item: column IDs and transition IDs are distinct.

`provider.board(id, query, signal)` returns `{board,lanes,nextCursor}`. Each lane
contains `{columnId,cards:[{item,moves:[{columnId,action}]}]}`. Supply authoritative
lane assignments and per-item move actions from core/provider data. The view never
infers transitions from display names. Dispatch the matching card move action,
never a guessed transition ID or the column default. Unknown or forbidden columns
cannot be selected or dropped onto. Cards are virtualized inside each lane.

Pointer capture keeps dragging stable across the board. Pointer cancellation,
lost capture, Escape, window blur, hiding the document and unmount clean up the
ghost without dispatching a move. A native move menu provides the same action
with a keyboard or touch. Pending cards cannot be moved twice. The view shows
an optimistic placement, retains the authoritative item state, and restores
the origin on rejection. Errors are visible and successful moves are announced.

Consumers pass a provider/account/query `scopeKey`, board and lane projections,
`onMove(reference, action)` and `onUpdated(result)` to refresh the projection.
Changing the scope discards pending presentation state; late responses cannot
change the new board. Accepted cards await a fresh projection before another
move, so stale transition permissions are never reused. The board writes no
story files and sends no terminal commands.

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
    const TASK_DETAIL: &str = include_str!("../../../src/components/Tasks/detail/TasksDetail.tsx");
    const TASK_CREATE: &str = include_str!("../../../src/components/Tasks/create/TasksCreate.tsx");
    const SETTING_FIELDS: &str =
        include_str!("../../../src/components/SettingsPanel/SettingFields.tsx");

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

    // Catches: newly added Tasks field kinds are missing from the plugin authoring guide.
    #[test]
    fn plugin_docs_matches_work_item_field_kinds() {
        let model = include_str!("../work_items/model.rs");
        let kinds = section(model, "pub enum WorkItemFieldKind {", "\n}");
        let expected = captures(kinds, r"(?m)^    ([A-Za-z]+),")
            .into_iter()
            .map(|kind| kind.to_lowercase())
            .collect();
        assert_inventory(
            PLUGIN_DOCS,
            "### Work-item field kind inventory\n",
            expected,
        );
    }

    // Catches: plugin schema types change without corresponding authoring documentation.
    #[test]
    fn plugin_docs_matches_plugin_settings_schema_types() {
        let source = include_str!("../settings/plugin_schema.rs");
        let accepted = section(
            source,
            r#"match schema["type"].as_str() {"#,
            "_ => return Err(fail()),",
        );
        assert_inventory(
            PLUGIN_DOCS,
            "### Plugin settings schema type inventory\n",
            captures(
                &captures(accepted, r"(?m)^        Some\(([^)]+)\) =>")
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(" "),
                r#""([a-z]+)""#,
            ),
        );
    }

    fn tasks_components(directory: &std::path::Path) -> BTreeSet<String> {
        let mut components = BTreeSet::new();
        for entry in std::fs::read_dir(directory).expect("Tasks sources must exist") {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().unwrap() != "__tests__" {
                    components.extend(tasks_components(&path));
                }
            } else if path.extension().is_some_and(|extension| extension == "tsx") {
                let source = std::fs::read_to_string(&path).unwrap();
                // Only exported production components, not test/harness entry points.
                if !path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains(".test.")
                {
                    components.extend(captures(
                        &source,
                        r"(?m)^export function ([A-Z][A-Za-z]+)\(",
                    ));
                    components.extend(captures(
                        &source,
                        r"(?m)^export const ([A-Z][A-Za-z]+): Component",
                    ));
                }
            }
        }
        components
    }

    // Catches: a new Tasks view or field ships undocumented (or removed entries remain).
    #[test]
    fn plugin_docs_matches_core_plugin_ux() {
        assert_inventory(
            PLUGIN_DOCS,
            "### Tasks component inventory\n",
            tasks_components(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/components/Tasks"),
            ),
        );
        for component in inventory(PLUGIN_DOCS, "### Tasks component inventory\n") {
            assert!(
                PLUGIN_DOCS.contains(&format!("#### {component}\n")),
                "each Tasks component needs UX instructions: {component}"
            );
        }
        assert_inventory(
            PLUGIN_DOCS,
            "### Tasks shell view inventory\n",
            captures(
                section(
                    include_str!("../../../src/components/Tasks/TasksShell.tsx"),
                    "const VIEWS:",
                    "\n];",
                ),
                r#"id: "([^"]+)""#,
            ),
        );
        let structs = Regex::new(r"(?ms)^pub struct (WorkItem[A-Za-z]*) \{(.*?)^\}").unwrap();
        let mut fields = BTreeSet::new();
        for source in [
            include_str!("../work_items/model.rs"),
            include_str!("../work_items/protocol.rs"),
        ] {
            for schema in structs.captures_iter(source) {
                for field in captures(&schema[2], r"(?m)^    pub ([a-z_]+):") {
                    let mut parts = field.split('_');
                    let mut name = parts.next().unwrap().to_owned();
                    for part in parts {
                        let mut chars = part.chars();
                        name.extend(chars.next().unwrap().to_uppercase());
                        name.push_str(chars.as_str());
                    }
                    fields.insert(format!("{}.{name}", &schema[1]));
                }
            }
        }
        assert_inventory(PLUGIN_DOCS, "### Tasks DTO field inventory\n", fields);
        assert_inventory(
            PLUGIN_DOCS,
            "### Roadmap date role inventory\n",
            captures(
                section(
                    include_str!("../../../src/components/Tasks/roadmap/calendar.ts"),
                    "export interface RoadmapDateFields {",
                    "\n}",
                ),
                r"(?m)^\t([A-Za-z]+)\?:",
            ),
        );
        assert_inventory(
            PLUGIN_DOCS,
            "### Settings control inventory\n",
            captures(SETTING_FIELDS, r"(?m)^export \{ (Setting[A-Za-z]+),"),
        );
        for control in inventory(PLUGIN_DOCS, "### Settings control inventory\n") {
            assert!(
                PLUGIN_DOCS.contains(&format!("#### {control}\n")),
                "each setting control needs UX instructions: {control}"
            );
        }
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
        assert_inventory(
            PLUGIN_DOCS,
            "### Tasks detail component inventory\n",
            captures(TASK_DETAIL, r"(?m)^export function ([A-Za-z]+)\("),
        );
        for component in inventory(PLUGIN_DOCS, "### Tasks detail component inventory\n") {
            assert!(PLUGIN_DOCS.contains(&format!("#### {component}\n")));
        }
        assert_inventory(
            PLUGIN_DOCS,
            "### Tasks create component inventory\n",
            captures(TASK_CREATE, r"(?m)^export function ([A-Za-z]+)\("),
        );
        for component in inventory(PLUGIN_DOCS, "### Tasks create component inventory\n") {
            assert!(PLUGIN_DOCS.contains(&format!("#### {component}\n")));
        }
    }
}
