---
title: Centralized ring-buffer logging with app/plugin tiers
category: integration-issues
tags: [observability, error-handling, architecture, plugin-system, logging, solidjs, ring-buffer]
symptom: 200+ console.error/warn calls scattered across 54 files with no aggregation or user-facing surface
root_cause: No centralized logging infrastructure — errors siloed per-subsystem with no unified view
date: 2025-02-25
---

# Centralized Error Logging Architecture

## Problem

TUICommander had 200+ `console.error`/`console.warn` calls scattered across 54+ files. Errors from git operations, GitHub polling, plugin lifecycle, network transport, and app initialization all went directly to the browser console with no:

- **Aggregation** — each subsystem logged independently
- **User-facing surface** — operators had to open DevTools to see errors
- **Filtering** — no way to isolate errors by source or severity
- **Plugin visibility** — plugin logs were trapped in per-plugin ring buffers (500 entries each), invisible to app-wide diagnostics

## Investigation

1. Searched for all `console.error` and `console.warn` calls in `src/` — found 200+ across 54 files
2. Identified 9 distinct error source categories: app, plugin, git, network, terminal, github, dictation, store, config
3. Examined existing overlay patterns (ActivityDashboard, CommandPalette) for UI consistency
4. Reviewed plugin system's `PluginLogger` — per-plugin ring buffer, not connected to any global view

## Design Decision

Implemented a **two-tier centralized logging architecture**:

- **App layer**: `appLogger` — app-wide ring buffer (1000 entries) with reactive signals
- **Plugin layer**: `PluginLogger` — per-plugin ring buffer (500 entries, unchanged)
- **Bridge**: `host.log()` in pluginRegistry forwards to both tiers

### Alternatives Considered

| Alternative | Rejected Because |
|------------|------------------|
| File-based log | Adds filesystem I/O complexity; in-memory sufficient for interactive debugging |
| Per-panel logging | Fragments the view; operators want one place to check |
| Console-only | Already the status quo; invisible to non-developers |
| Unlimited buffer | Memory leak risk in long-running Tauri app |

### Trade-offs

- **Bounded buffer (1000 entries)**: Oldest entries silently dropped — acceptable for interactive debugging, not for audit trails
- **In-memory only**: Logs lost on app restart — future improvement could add opt-in file persistence
- **Console forwarding**: Every `appLogger.*()` call also hits `console.*()` for DevTools compatibility, meaning dual output

## Solution

### Architecture

```
Global handlers (window.onerror, unhandledrejection)
    |
    v
appLogger.push(level, source, message, data?)
    |
    +---> Ring buffer (1000 entries, circular index)
    +---> Reactive signals: revision(), unseenErrorCount()
    +---> Console forwarding: console.error/warn/info/debug("[source]", ...)
    |
    v
StatusBar badge (subscribes to unseenErrorCount)
    |
    v
ErrorLogPanel (overlay with level/source/text filters)
```

### Core: Ring Buffer Logger (`src/stores/appLogger.ts`)

```typescript
const MAX_ENTRIES = 1000;

function push(level, source, message, data?) {
  const entry = { id: nextId++, timestamp: Date.now(), level, source, message, data };
  if (count < MAX_ENTRIES) {
    buffer[count++] = entry;
  } else {
    buffer[head] = entry;        // Overwrite oldest
    head = (head + 1) % MAX_ENTRIES;  // Circular wrap
  }
  setRevision((r) => r + 1);    // Notify all subscribers
  if (level === "error" || level === "warn") {
    setUnseenErrorCount((c) => c + 1);  // Badge counter
  }
}
```

### Plugin Bridge (`src/plugins/pluginRegistry.ts`)

```typescript
// Inside buildHost() — one line bridges plugin logs to app logger
log(level, message, data) {
  logger.log(level, message, data);                           // Per-plugin buffer
  appLogger.push(level, "plugin", `[${pluginId}] ${message}`, data);  // App-wide
}
```

### Global Error Capture (`src/index.tsx`)

```typescript
window.addEventListener("error", (event) => {
  appLogger.error("app", `Uncaught: ${event.message}`, {
    filename: event.filename, lineno: event.lineno, colno: event.colno,
  });
});

window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason instanceof Error ? event.reason.message : String(event.reason);
  appLogger.error("app", `Unhandled rejection: ${reason}`, event.reason);
});
```

### Reactive Badge (`src/components/StatusBar/StatusBar.tsx`)

```tsx
<Show when={appLogger.unseenErrorCount() > 0}>
  <span class={s.toggleBadge}>{appLogger.unseenErrorCount()}</span>
</Show>
```

Badge resets when user opens the panel (`appLogger.markSeen()`).

## Prevention

- [ ] Add ESLint `no-restricted-globals` rule to block direct `console.error`/`console.warn` (with exception for `appLogger.ts`)
- [ ] Migrate remaining ~136 unrouted `console.error/warn` calls to `appLogger` (top targets: `settings.ts` 17, `dictation.ts` 12, `useAppInit.ts` 9)
- [ ] Add to AGENTS.md: "All errors/warnings MUST route through `appLogger`"
- [ ] Consider file-based log persistence for crash diagnostics

## Files Changed

### New Files (5)
- `src/stores/appLogger.ts` — Centralized ring buffer logger (150 lines)
- `src/stores/errorLog.ts` — Panel open/close state (29 lines)
- `src/components/ErrorLogPanel/ErrorLogPanel.tsx` — Overlay panel with filters (232 lines)
- `src/components/ErrorLogPanel/ErrorLogPanel.module.css` — Styling (269 lines)
- `src/components/ErrorLogPanel/index.ts` — Barrel export

### Modified Files (20)
- `src/App.tsx` — Wiring, replaced ~10 console.error calls
- `src/components/StatusBar/StatusBar.tsx` — Badge UI
- `src/keybindingDefaults.ts` — Cmd+Shift+E binding
- `src/actions/actionRegistry.ts` — Action meta + handler
- `src/hooks/useKeyboardShortcuts.ts` — Dispatch case
- `src/index.tsx` — Global error handlers
- `src/plugins/pluginRegistry.ts` — Plugin bridge + 4 console replacements
- `src/stores/github.ts` — 4 console replacements
- `src/stores/settings.ts` — 1 console replacement
- `src/transport.ts` — 1 console replacement
- `src/deep-link-handler.ts` — 2 console replacements
- `src/hooks/useGitOperations.ts` — 4 console replacements
- `src/hooks/useRepository.ts` — 1 console replacement
- `docs/FEATURES.md` — Section 3.12 + keyboard shortcut entry
- 6 test files — Updated to match new `[source]` tag format

## Related

- `docs/FEATURES.md` section 3.12 — Feature documentation
- `docs/plugins.md` — Plugin logging API reference
- `docs/frontend/STYLE_GUIDE.md` — UI patterns for overlay panels
- ActivityDashboard, CommandPalette — Same overlay pattern
