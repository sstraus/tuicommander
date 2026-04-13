# TUIC SDK

The TUIC SDK provides `window.tuic` inside iframes hosted by TUICommander, enabling plugins and external pages to interact with the host app: open files, read content, launch terminals, copy to clipboard, receive theme updates, and more.

## Two Injection Modes

### 1. Inline HTML Tabs (Plugins)

Plugins use `html` tabs — TUIC injects the SDK `<script>` directly into the iframe content. `window.tuic` is available immediately on load.

```js
// Plugin panel — window.tuic is injected automatically
tuic.open("README.md");                         // relative to active repo
tuic.open("/absolute/path/file.txt", { pinned: true });
tuic.edit("src/App.tsx", { line: 42 });
tuic.terminal(tuic.activeRepo());
```

See [Plugin Authoring Guide](plugins.md) for full plugin details.

### 2. URL Tabs (External Pages)

Tabs created with `url` load content from a remote server in an iframe. The parent **cannot** inject scripts into cross-origin iframes, so the page must opt in to the SDK via a `postMessage` handshake.

#### Handshake Protocol

```
┌─────────────┐                            ┌─────────────┐
│  TUIC Host   │                            │  iframe URL  │
│  (parent)    │                            │  (child)     │
└──────┬──────┘                            └──────┬──────┘
       │  iframe onload                            │
       │── tuic:sdk-init ────────────────────────>│
       │── tuic:repo-changed ────────────────────>│
       │── tuic:theme-changed ───────────────────>│
       │                                           │ creates window.tuic
       │                                           │ dispatches "tuic:ready"
       │                                           │
       │  (fallback path — async listeners)        │
       │<── tuic:sdk-request ──────────────────────│
       │── tuic:sdk-init + repo + theme ─────────>│
       │                                           │
       │<── tuic:open, tuic:edit, ... ─────────────│ (on user action)
       │── tuic:get-file-result ─────────────────>│ (async response)
       │── tuic:host-message ────────────────────>│ (push from host)
```

Both paths are implemented in `src/components/PluginPanel/PluginPanel.tsx`. The `version` field carries `TUIC_SDK_VERSION` so the child can feature-detect.

#### Step 1: Child Page — Bootstrap Listener

Add this `<script>` in the `<head>` of your page, **before** any framework initialization. It must be synchronous so the listener is registered before the parent's `onload` fires.

```html
<script>
(function () {
  window.addEventListener("message", function (e) {
    if (!e.data || e.data.type !== "tuic:sdk-init") return;

    window.tuic = {
      version: "1.0",
      open: function (path, opts) {
        parent.postMessage({ type: "tuic:open", path: path, pinned: !!(opts && opts.pinned) }, "*");
      },
      edit: function (path, opts) {
        parent.postMessage({ type: "tuic:edit", path: path, line: (opts && opts.line) || 0 }, "*");
      },
      terminal: function (repoPath) {
        parent.postMessage({ type: "tuic:terminal", repoPath: repoPath }, "*");
      }
    };

    window.dispatchEvent(new Event("tuic:ready"));
  });
})();
</script>
```

> **Note:** For URL-mode pages, only the basic methods (open, edit, terminal) are shown above. To use the full SDK (activeRepo, getFile, theme, etc.), copy the complete SDK from `src/components/PluginPanel/tuicSdk.ts` or use the inline HTML mode.

#### Step 2: Child Page — React to SDK Availability

Use the `tuic:ready` event to update your UI (e.g., show an "Open in TUIC" button):

```js
// Alpine.js example
Alpine.data("myApp", () => ({
  _tuicReady: false,
  get hasTuic() { return this._tuicReady; },

  init() {
    window.addEventListener("tuic:ready", () => { this._tuicReady = true; });
    // If SDK was already initialized before Alpine mounted
    if (window.tuic) this._tuicReady = true;
  },

  tuicOpen(filePath) {
    if (window.tuic) window.tuic.open(filePath, { pinned: true });
  }
}));
```

## API Reference

### Files

#### `tuic.open(path, opts?)`

Open a file in a TUIC tab.

| Param | Type | Description |
|-------|------|-------------|
| `path` | `string` | File path — relative (resolved against active repo) or absolute |
| `opts.pinned` | `boolean` | Pin the tab (default: `false`) |

#### `tuic.edit(path, opts?)`

Open a file in the external editor.

| Param | Type | Description |
|-------|------|-------------|
| `path` | `string` | File path — relative or absolute |
| `opts.line` | `number` | Line number to jump to (default: `0`) |

#### `tuic.getFile(path): Promise<string>`

Read a file's text content from the active repo.

| Param | Type | Description |
|-------|------|-------------|
| `path` | `string` | File path — relative or absolute |

Returns a Promise that resolves with the file content string, or rejects with an Error if the file is not found, the path escapes the repo root, or no active repo is set.

```js
tuic.getFile("package.json")
  .then(content => JSON.parse(content))
  .catch(err => console.error("Cannot read:", err.message));
```

### Path Resolution

All file methods (`open`, `edit`, `getFile`) accept both relative and absolute paths:

- **Relative paths** (e.g., `"README.md"`, `"src/App.tsx"`) are resolved against the active repository root.
- **Absolute paths** (e.g., `"/Users/me/code/repo/file.ts"`) are matched against known repositories (longest prefix wins).
- **Path traversal** (`../`) that escapes the repo root is blocked and returns an error.
- **`./` prefixes** are supported and normalized.

```js
tuic.open("README.md");                    // → /active/repo/README.md
tuic.open("src/../README.md");             // → /active/repo/README.md
tuic.open("/Users/me/repo/file.ts");       // → absolute, matched to repo
tuic.getFile("../../../etc/passwd");       // → rejected (traversal)
```

#### `tuic://` Links

HTML `<a>` tags with `tuic://` href are automatically intercepted:

```html
<a href="tuic://open/README.md">View README</a>
<a href="tuic://edit/src/main.rs?line=42">Edit main.rs:42</a>
<a href="tuic://terminal?repo=/path/to/repo">Open terminal</a>
```

Link pathnames are treated as **relative paths** (the leading `/` from URL parsing is stripped).

### Repository

#### `tuic.activeRepo(): string | null`

Returns the path of the currently active repository, or `null` if none is active.

```js
var repo = tuic.activeRepo();
// "/Users/me/code/myproject" or null
```

#### `tuic.onRepoChange(callback)`

Register a listener that fires when the active repo changes.

| Param | Type | Description |
|-------|------|-------------|
| `callback` | `(repoPath: string \| null) => void` | Called with the new active repo path |

#### `tuic.offRepoChange(callback)`

Unregister a previously registered repo-change listener.

#### `tuic.terminal(repoPath)`

Open a terminal in the given repository.

| Param | Type | Description |
|-------|------|-------------|
| `repoPath` | `string` | Repository root path (absolute) |

### UI Feedback

#### `tuic.toast(title, opts?)`

Show a native toast notification in the host app.

| Param | Type | Description |
|-------|------|-------------|
| `title` | `string` | Toast title (required) |
| `opts.message` | `string` | Optional body text |
| `opts.level` | `"info" \| "warn" \| "error"` | Severity (default: `"info"`) |

```js
tuic.toast("Import complete", { message: "42 items imported" });
tuic.toast("Rate limited", { message: "Try again in 30s", level: "warn" });
```

#### `tuic.clipboard(text)`

Copy text to the system clipboard. Works from sandboxed iframes (which cannot access `navigator.clipboard` directly).

| Param | Type | Description |
|-------|------|-------------|
| `text` | `string` | Text to copy |

### Messaging

#### `tuic.send(data)`

Send structured data to the host. The host receives it via `pluginRegistry.handlePanelMessage()`.

| Param | Type | Description |
|-------|------|-------------|
| `data` | `any` | JSON-serializable payload |

#### `tuic.onMessage(callback)`

Register a listener for messages pushed from the host.

| Param | Type | Description |
|-------|------|-------------|
| `callback` | `(data: any) => void` | Called with the message payload |

#### `tuic.offMessage(callback)`

Unregister a previously registered message listener.

### Theme

#### `tuic.theme: object | null`

Read-only property containing the current theme as a key-value object. Keys are camelCase versions of CSS custom properties (e.g., `--bg-primary` → `bgPrimary`).

```js
var theme = tuic.theme;
// { bgPrimary: "#1e1e2e", fgPrimary: "#cdd6f4", accent: "#89b4fa", ... }
```

#### `tuic.onThemeChange(callback)`

Register a listener that fires when the host theme changes.

| Param | Type | Description |
|-------|------|-------------|
| `callback` | `(theme: object) => void` | Called with the new theme object |

#### `tuic.offThemeChange(callback)`

Unregister a previously registered theme-change listener.

### Version

#### `tuic.version: string`

The SDK version string (currently `"1.0"`).

## Testing the SDK

An interactive test page is included at `docs/examples/sdk-test.html`. It runs automatic verification of all SDK methods and provides buttons for interactive testing.

### How to launch it

**From an AI agent (Claude Code, etc.):**

Use the TUIC MCP `ui` tool to open it as an inline HTML tab:

```
mcp__tuicommander__ui action=tab id="sdk-test" title="SDK Test Suite" html="<contents of docs/examples/sdk-test.html>" pinned=false focus=true
```

**From a plugin:**

Register a plugin that serves the HTML content as a panel tab. The SDK is injected automatically into inline HTML tabs.

**From JavaScript (dev console or app code):**

```js
// Read the file and open as a tab
const html = await invoke("fs_read_file", { repoPath: "/path/to/tuicommander", file: "docs/examples/sdk-test.html" });
mdTabsStore.addHtml("sdk-test", "SDK Test Suite", html);
```

The test page verifies:
- SDK presence and version
- `activeRepo()` return value
- `onRepoChange` listener registration
- Theme delivery and `onThemeChange`
- `onMessage` listener registration
- `getFile("README.md")` reads file content
- `getFile("../../../etc/passwd")` is blocked by traversal guard

Interactive buttons test: `open`, `edit`, `terminal`, `toast` (all levels), `clipboard`, `getFile`, and `send`.

## Timing Notes

The `<script>` bootstrap in the child page **must** be synchronous and in `<head>` to guarantee the `message` listener is registered before the parent's `iframe.onload` fires `tuic:sdk-init`. If your page loads the bootstrap asynchronously (e.g., as an ES module), there is a race condition — the init message may arrive before the listener exists.

If you cannot guarantee synchronous loading, implement a retry: have the child send `{ type: "tuic:sdk-request" }` to the parent on DOMContentLoaded (or whenever the listener is registered), and the parent will respond with `tuic:sdk-init`. This fallback is fully supported by the host.

## Source Files

| File | Description |
|------|-------------|
| `src/components/PluginPanel/tuicSdk.ts` | SDK script injected into iframes |
| `src/components/PluginPanel/PluginPanel.tsx` | Host-side message handlers |
| `src/components/PluginPanel/resolveTuicPath.ts` | Path resolution (relative + traversal guard) |
| `docs/examples/sdk-test.html` | Interactive test/example page |
