/// AI-optimized plugin development reference for TUICommander.
/// Compiled as a const so it's zero-cost at runtime.
pub const PLUGIN_DOCS: &str = r###"# TUICommander Plugin Development Reference

## Installation

Create directory `{id}/` containing `manifest.json` + `main.js` under the platform plugins path:
- macOS: `~/Library/Application Support/com.tuic.commander/plugins/`
- Linux: `~/.config/tuicommander/plugins/`
- Windows: `%APPDATA%/com.tuic.commander/plugins/`

Hot reload: editing any file in the plugin directory triggers automatic unload + re-import.

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
  "allowedUrls": ["https://api.example.com/*"]
}
```

Constraints:
- `id` must match directory name exactly, non-empty
- `main` must be a filename only (no path separators or `..`)
- `minAppVersion` must be <= current app version (current: 0.3.x)
- `capabilities`: subset of `pty:write`, `ui:markdown`, `ui:sound`, `ui:panel`, `ui:ticker`, `net:http`, `credentials:read`, `invoke:read_file`, `invoke:list_markdown_files`, `fs:read`, `fs:list`, `fs:watch`
- `allowedUrls`: URL patterns for `net:http` (supports `*` wildcard for path prefix matching)
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

## PluginHost API

All Tier 1-2 methods are always available. Tier 3-4 require capabilities in manifest.json; calling without capability throws `PluginCapabilityError`.

### Tier 1: Activity Center (always available)

All register* methods return a Disposable (auto-cleaned on unload).

```typescript
// Section heading in Activity Center dropdown
host.registerSection({ id: string, label: string, priority: number, canDismissAll: boolean })

// Watch PTY output lines (ANSI-stripped)
host.registerOutputWatcher({ pattern: RegExp, onMatch(match: RegExpExecArray, sessionId: string): void })

// Handle structured events from Rust OutputParser
host.registerStructuredEventHandler(type: string, handler: (payload: unknown, sessionId: string) => void)

// Provide content when user clicks an item with contentUri
host.registerMarkdownProvider(scheme: string, { provideContent(uri: URL): Promise<string | null> })

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
host.getPrNotifications()  // [{ id, repoPath, branch, prNumber, title, type }]
host.getSettings(repoPath: string) // { path, displayName, baseBranch, color } | null
```

### Tier 3: Write Actions (capability-gated)

| Method | Capability |
|--------|------------|
| `await host.writePty(sessionId: string, data: string): Promise<void>` | `pty:write` |
| `host.openMarkdownPanel(title: string, contentUri: string): void` | `ui:markdown` |
| `await host.playNotificationSound(): Promise<void>` | `ui:sound` |
| `host.openPanel({ id, title, html }): PanelHandle` | `ui:panel` |
| `host.postTickerMessage({ id, text, icon?, priority?, ttlMs? }): void` | `ui:ticker` |
| `host.removeTickerMessage(id: string): void` | `ui:ticker` |
| `await host.readCredential(serviceName: string): Promise<string \| null>` | `credentials:read` |
| `await host.httpFetch(url: string, options?): Promise<HttpResponse>` | `net:http` |

PanelHandle: `{ tabId, update(html), close() }` — HTML rendered in sandboxed iframe.
HttpResponse: `{ status: number, headers: Record<string, string>, body: string }` — non-2xx is NOT an error.

### Tier 3b: Filesystem Operations (capability-gated)

All paths must be absolute and within `$HOME`. Resolved via canonicalize (symlinks, `..` resolved).

```typescript
// Read a file (max 10 MB, UTF-8)
const content = await host.readFile("/Users/me/.claude/projects/foo/conversation.jsonl");  // requires "fs:read"

// Read last N bytes of a file (skip partial first line)
const tail = await host.readFileTail("/Users/me/.claude/hud-tracking.jsonl", 512 * 1024);  // requires "fs:read"

// List directory (optional glob filter)
const files = await host.listDirectory("/Users/me/.claude/projects/foo", "*.jsonl");  // requires "fs:list"

// Watch for changes (returns Disposable)
const watcher = await host.watchPath(  // requires "fs:watch"
  "/Users/me/.claude/projects/foo",
  (events) => { /* FsChangeEvent[] with type + path */ },
  { recursive: true, debounceMs: 500 },
);
watcher.dispose();  // stop watching
```

FsChangeEvent: `{ type: "create" | "modify" | "delete", path: string }`

### Tier 4: Tauri Invoke (whitelisted only)

`await host.invoke<T>(cmd: string, args?: object): Promise<T>` — non-whitelisted commands throw immediately.

| Command | Args | Returns | Capability |
|---------|------|---------|------------|
| `read_file` | `{ path: string, file: string }` | `string` | `invoke:read_file` |
| `list_markdown_files` | `{ path: string }` | `Array<{ path, git_status }>` | `invoke:list_markdown_files` |
| `read_plugin_data` | `{ plugin_id: string, path: string }` | `string` | none |
| `write_plugin_data` | `{ plugin_id: string, path: string, data: string }` | `void` | none |
| `delete_plugin_data` | `{ plugin_id: string, path: string }` | `void` | none |

Plugin data sandboxed per-plugin (no capability required):
```javascript
// Store data
await host.invoke("write_plugin_data", {
  plugin_id: PLUGIN_ID,
  path: "cache.json",
  data: JSON.stringify({ lastCheck: Date.now() }),
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
        host.writePty(sessionId, "y\n").catch(err => console.error("[auto-confirm]", err));
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
3. Import: `import("plugin://{id}/main.js")` (custom URI protocol)
4. Module check: default export must have id, onload, onunload
5. Register: pluginRegistry.register() calls plugin.onload(host)
6. Active: plugin receives PTY lines, structured events, uses PluginHost API
7. Hot reload: file changes trigger unregister + re-import (cache-busted)
8. Unload: plugin.onunload() called, all registrations auto-disposed

Crash safety: all boundaries are try/catch wrapped. A broken plugin produces a console error and is skipped. The app always continues.

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
