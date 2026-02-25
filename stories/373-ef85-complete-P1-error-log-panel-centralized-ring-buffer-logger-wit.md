---
id: 373-ef85
title: Error Log Panel - centralized ring-buffer logger with overlay UI
status: complete
priority: P1
created: "2026-02-25T07:45:09.879Z"
updated: "2026-02-25T08:32:53.322Z"
dependencies: []
---

# Error Log Panel - centralized ring-buffer logger with overlay UI

## Problem Statement

200+ console.error/warn calls across 54 files go to browser console only. No centralized log store, no user-facing error surface, no plugin error aggregation. Session 0b1dd1e3 partially implemented appLogger + ErrorLogPanel but was cut off by budget before wiring and committing.

## Acceptance Criteria

- [ ] appLogger store with ring buffer (1000 entries), log levels (error/warn/info/debug), source tags (app/plugin/git/network), reactive signals
- [ ] ErrorLogPanel overlay component with: level filter tabs, source filter, text search, timestamp + level badge + message display, clear and copy-to-clipboard buttons
- [ ] Keyboard shortcut Cmd+Shift+E in actionRegistry.ts and keybindingDefaults.ts
- [ ] StatusBar toggle button for the panel
- [ ] Plugin errors routed through appLogger
- [ ] docs/FEATURES.md updated, CHANGELOG.md entry, make check passes

## Files

- src/stores/appLogger.ts
- src/components/ErrorLogPanel/ErrorLogPanel.tsx
- src/keybindingDefaults.ts
- src/actions/actionRegistry.ts
- src/components/StatusBar/StatusBar.tsx
- docs/FEATURES.md

## Work Log

### 2026-02-25T08:32:53.250Z - Already fully implemented and committed as 570a15e by prior session. appLogger store, ErrorLogPanel overlay, Cmd+Shift+E shortcut, status bar badge, global error capture, plugin log forwarding — all complete.

