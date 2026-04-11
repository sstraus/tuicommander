# TUIC SDK

The TUIC SDK provides `window.tuic` inside iframes hosted by TUICommander, enabling child pages to open files, launch editors, and create terminals in the parent app.

## Two Injection Modes

### 1. Inline HTML Tabs (Plugins)

Plugins use `html` tabs — TUIC injects the SDK `<script>` directly into the iframe content. `window.tuic` is available immediately on load.

```js
// Plugin panel — window.tuic is injected automatically
window.tuic.open("/path/to/file.txt", { pinned: true });
window.tuic.edit("/path/to/file.rs", { line: 42 });
window.tuic.terminal("/repo/path");
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
       │── { type: "tuic:sdk-init", version } ───>│
       │                                           │ creates window.tuic
       │                                           │ dispatches "tuic:ready"
       │                                           │
       │  (fallback path — async listeners)        │
       │<── { type: "tuic:sdk-request" } ──────────│ child listener wasn't ready
       │── { type: "tuic:sdk-init", version } ───>│ parent re-sends
       │                                           │
       │<── { type: "tuic:open", path } ───────────│ (on user action)
       │<── { type: "tuic:edit", path, line } ─────│
       │<── { type: "tuic:terminal", repo } ───────│
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

```html
<button x-show="hasTuic && modalFilePath" @click="tuicOpen(modalFilePath)">
  Open in TUIC
</button>
```

#### Step 3: TUIC Host — Send Init and Handle Messages

The parent sends `tuic:sdk-init` when the iframe loads, and listens for SDK messages:

```js
// On iframe tab creation
iframe.addEventListener("load", () => {
  iframe.contentWindow.postMessage({ type: "tuic:sdk-init" }, "*");
});

// Handle SDK messages from child
window.addEventListener("message", (e) => {
  switch (e.data?.type) {
    case "tuic:open":
      // open file at e.data.path, pinned: e.data.pinned
      break;
    case "tuic:edit":
      // open editor at e.data.path, line: e.data.line
      break;
    case "tuic:terminal":
      // create terminal at e.data.repoPath
      break;
  }
});
```

## API Reference

### `window.tuic.open(path, opts?)`

Open a file in a TUIC tab.

| Param | Type | Description |
|-------|------|-------------|
| `path` | `string` | Absolute file path |
| `opts.pinned` | `boolean` | Pin the tab (default: `false`) |

### `window.tuic.edit(path, opts?)`

Open a file in the external editor.

| Param | Type | Description |
|-------|------|-------------|
| `path` | `string` | Absolute file path |
| `opts.line` | `number` | Line number to jump to (default: `0`) |

### `window.tuic.terminal(repoPath)`

Open a terminal in the given repository.

| Param | Type | Description |
|-------|------|-------------|
| `repoPath` | `string` | Repository root path |

## Timing Notes

The `<script>` bootstrap in the child page **must** be synchronous and in `<head>` to guarantee the `message` listener is registered before the parent's `iframe.onload` fires `tuic:sdk-init`. If your page loads the bootstrap asynchronously (e.g., as an ES module), there is a race condition — the init message may arrive before the listener exists.

If you cannot guarantee synchronous loading, implement a retry: have the child send `{ type: "tuic:sdk-request" }` to the parent on DOMContentLoaded (or whenever the listener is registered), and the parent will respond with `tuic:sdk-init`. This fallback is fully supported by the host.
