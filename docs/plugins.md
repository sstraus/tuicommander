# Writing Plugins

TUI Commander uses an Obsidian-style plugin system where built-in TypeScript modules extend the Activity Center (bell dropdown in the toolbar). Plugins detect patterns in terminal output and surface them as interactive items with optional markdown content.

## Architecture Overview

```
PTY output ──► pluginRegistry.processRawOutput()
                  │
                  ├── LineBuffer (reassemble lines)
                  ├── stripAnsi (clean ANSI codes)
                  └── dispatchLine() ──► OutputWatcher.onMatch()
                                              │
                                              └── host.addItem() ──► Activity Center bell
                                                                           │
                                                              user clicks item
                                                                           │
                                              markdownProviderRegistry.resolve(contentUri)
                                                                           │
                                                              MarkdownTab renders content
```

Structured events from the Rust backend follow a parallel path:

```
Tauri OutputParser ──► pluginRegistry.dispatchStructuredEvent(type, payload, sessionId)
                            │
                            └── structuredEventHandler(payload, sessionId)
```

## Plugin Interface

Every plugin implements `TuiPlugin` from `src/plugins/types.ts`:

```typescript
interface TuiPlugin {
  id: string;              // Unique identifier (e.g. "plan", "wiz-stories")
  onload(host: PluginHost): void;   // Register capabilities
  onunload(): void;                 // Dispose all registrations
}
```

## Step-by-Step: Creating a Plugin

### 1. Create the plugin file

Create `src/plugins/myPlugin.ts`:

```typescript
import type { Disposable, PluginHost, TuiPlugin } from "./types";

const PLUGIN_ID = "my-plugin";
const SECTION_ID = "my-section";

const ICON_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor">
  <!-- your 16x16 monochrome SVG path here -->
</svg>`;

class MyPlugin implements TuiPlugin {
  readonly id = PLUGIN_ID;
  private disposables: Disposable[] = [];

  onload(host: PluginHost): void {
    this.disposables = [];

    // 1. Register a section in the Activity Center dropdown
    this.disposables.push(
      host.registerSection({
        id: SECTION_ID,
        label: "MY SECTION",   // Uppercase label shown as header
        priority: 30,           // Lower = higher position
        canDismissAll: false,
      }),
    );

    // 2. Register output watchers (optional)
    // 3. Register structured event handlers (optional)
    // 4. Register markdown provider (optional)
  }

  onunload(): void {
    for (const d of this.disposables) {
      d.dispose();
    }
    this.disposables = [];
  }
}

export const myPlugin: TuiPlugin = new MyPlugin();
```

### 2. Register it in `src/plugins/index.ts`

```typescript
import { myPlugin } from "./myPlugin";

export const BUILTIN_PLUGINS: TuiPlugin[] = [planPlugin, wizStoriesPlugin, myPlugin];
```

That's it — `initPlugins()` is called in `App.tsx` on mount.

## PluginHost API Reference

Every `host.register*()` method returns a `Disposable`. Store them all and call `dispose()` in `onunload()`.

### registerSection(section)

Adds a section heading to the Activity Center dropdown. Each plugin should register exactly one section.

```typescript
host.registerSection({
  id: "my-section",        // Must match the sectionId you use in addItem()
  label: "MY SECTION",     // Displayed as section header
  priority: 30,            // Lower = higher in the dropdown
  canDismissAll: false,     // Show "Dismiss All" button?
});
```

### registerOutputWatcher(watcher)

Watches every PTY output line (after ANSI stripping and line reassembly). The `onMatch` callback **must be synchronous and fast** (< 1ms) — it runs in the PTY hot path.

```typescript
host.registerOutputWatcher({
  // Regex tested against each clean line. Use capture groups for extraction.
  // Avoid global flag (g) — lastIndex is reset automatically before each test.
  pattern: /✓ Deployed: (\S+) to (\S+)/,

  // Called when pattern matches. Positional args, NOT a destructured object.
  onMatch: (match, sessionId) => {
    const service = match[1];
    const env = match[2];
    host.addItem({
      id: `deploy:${service}:${env}`,
      pluginId: PLUGIN_ID,
      sectionId: SECTION_ID,
      title: service,
      subtitle: `Deployed to ${env}`,
      icon: ICON_SVG,
      dismissible: true,
    });
  },
});
```

**Important**: Validate your regex against actual terminal output. The input is ANSI-stripped but still contains Unicode characters (checkmarks, arrows, emoji).

### registerStructuredEventHandler(type, handler)

Handles events from the Rust `OutputParser` that have already been parsed into typed payloads (e.g. `"plan-file"`, `"rate-limit"`).

```typescript
host.registerStructuredEventHandler("plan-file", (payload, sessionId) => {
  const { path } = payload as { path: string };
  host.addItem({ ... });
});
```

### registerMarkdownProvider(scheme, provider)

Registers a content provider for a URI scheme. When the user clicks an ActivityItem with a `contentUri`, the system resolves `scheme:...` by calling your provider's `provideContent(uri)`.

```typescript
host.registerMarkdownProvider("my-scheme", {
  async provideContent(uri: URL): Promise<string | null> {
    const id = uri.searchParams.get("id");
    if (!id) return null;

    try {
      // Read file via Tauri backend
      return await invoke<string>("read_file", { path: dir, file: name });
    } catch {
      return null; // Content unavailable
    }
  },
});
```

The provider stack supports multiple registrations per scheme. Dispose removes your specific provider and restores the previous one (if any).

### addItem(item) / removeItem(id) / updateItem(id, updates)

Manage activity items in the store:

```typescript
// Add or update (deduplicates by id — same id replaces existing item)
host.addItem({
  id: "deploy:api:prod",       // Stable, unique identifier
  pluginId: PLUGIN_ID,         // Must match your plugin id
  sectionId: SECTION_ID,       // Must match your registered section
  title: "api-server",         // Primary text (larger)
  subtitle: "Deployed to prod", // Secondary text (smaller, muted)
  icon: ICON_SVG,              // Inline SVG with fill="currentColor"
  dismissible: true,           // User can dismiss
  contentUri: "my-scheme:detail?id=api",  // Opens in MarkdownTab on click
  // OR: onClick: () => { ... },          // Direct action (mutually exclusive)
});

// Update specific fields
host.updateItem("deploy:api:prod", { subtitle: "Rolled back" });

// Remove
host.removeItem("deploy:api:prod");
```

## Content URI Format

```
scheme:path?key=value&key2=value2
```

- **scheme** — matches what you passed to `registerMarkdownProvider(scheme, ...)`
- Query params are URL-encoded and parsed as `URL` by the provider

Examples from built-in plugins:
- `plan:file?path=%2Frepo%2Fplans%2Ffoo.md`
- `stories:detail?id=324-9b46&dir=%2Frepo%2Fstories`

## Reading Files from Plugins

Plugins run in the webview — no direct filesystem access. Use Tauri `invoke` to read files via the Rust backend:

```typescript
import { invoke } from "../invoke";

// Read a single file (path = directory, file = filename within it)
const content = await invoke<string>("read_file", {
  path: "/repo/plans",
  file: "my-plan.md",
});

// List markdown files in a directory
const files = await invoke<{ path: string; git_status: string }[]>(
  "list_markdown_files",
  { path: "/repo/stories" },
);
```

**Security constraint**: `read_file` requires the file to be within the given directory path (enforced by `read_file_impl` in Rust).

## Icons

All icons must be monochrome inline SVGs with `fill="currentColor"`. This ensures they inherit the current text color and work with all themes. Recommended viewBox: `0 0 16 16`.

```typescript
const ICON_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor">
  <path d="..."/>
</svg>`;
```

Never use emoji for icons — they render inconsistently across platforms.

## Testing

### Test file location

`src/__tests__/plugins/myPlugin.test.ts`

### Mock setup

```typescript
import { describe, it, expect, beforeEach, vi } from "vitest";

// Mock Tauri invoke
vi.mock("../../invoke", () => ({
  invoke: vi.fn(),
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { invoke } from "../../invoke";
import { pluginRegistry } from "../../plugins/pluginRegistry";
import { activityStore } from "../../stores/activityStore";
import { markdownProviderRegistry } from "../../plugins/markdownProviderRegistry";
import { myPlugin } from "../../plugins/myPlugin";

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  pluginRegistry.clear();
  activityStore.clearAll();
  markdownProviderRegistry.clear();
  mockedInvoke.mockReset();
});
```

### What to test

1. **Lifecycle** — section registered on load, removed on unload
2. **Watcher patterns** — match expected lines, ignore unrelated lines
3. **Item fields** — id, title, subtitle, icon, contentUri, sectionId, dismissible
4. **Deduplication** — same id updates rather than duplicates
5. **Multiple items** — different ids create separate items
6. **MarkdownProvider** — resolves content via mocked invoke
7. **Provider edge cases** — missing params return null, invoke errors return null

### Testing output watchers

Use `pluginRegistry.processRawOutput()` to simulate PTY output:

```typescript
it("detects deployment from PTY output", () => {
  pluginRegistry.register(myPlugin);
  pluginRegistry.processRawOutput("✓ Deployed: api-server to prod\n", "session-1");

  const items = activityStore.getForSection("my-section");
  expect(items).toHaveLength(1);
  expect(items[0].title).toBe("api-server");
});
```

### Testing injectable dependencies

If your plugin needs external state (like repo paths), use a factory pattern for testability:

```typescript
// In plugin file
export function createMyPlugin(
  getDirFn: (sessionId: string) => string | null = defaultGetDir,
): TuiPlugin {
  return new MyPluginImpl(getDirFn);
}

// In test file
const plugin = createMyPlugin(() => "/test/path");
pluginRegistry.register(plugin);
```

## Existing Plugins

| Plugin | File | Section | Detects |
|--------|------|---------|---------|
| `plan` | `planPlugin.ts` | ACTIVE PLAN | `plan-file` structured events from OutputParser |
| `wiz-stories` | `wizStoriesPlugin.ts` | STORIES | `✓ Updated:` and `✓ Added worklog to` PTY patterns |

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
