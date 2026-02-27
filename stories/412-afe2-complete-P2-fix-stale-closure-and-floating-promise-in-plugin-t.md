---
id: 412-afe2
title: Fix stale closure and floating promise in plugin toggle command palette entry
status: complete
priority: P2
created: "2026-02-26T21:07:12.805Z"
updated: "2026-02-27T10:59:09.421Z"
dependencies: []
---

# Fix stale closure and floating promise in plugin toggle command palette entry

## Problem Statement

In `App.tsx:780`, the plugin toggle command palette entry has two issues:
1. **Stale closure:** `execute: () => pluginStore.setEnabled(plugin.id, !plugin.enabled)` captures `plugin.enabled` at memo-build time, not at call time. Should read fresh state via `pluginStore.getPlugin()`.
2. **Floating promise:** `setEnabled` is async but `execute` is typed `() => void`. The promise is silently dropped. Needs `.catch()` like the adjacent `check-for-updates` entry.

## Acceptance Criteria

- [ ] `execute` reads fresh enabled state from `pluginStore.getPlugin(plugin.id)` at call time
- [ ] Add `.catch()` handler that logs via `appLogger.error`
- [ ] Follow the pattern used by `check-for-updates` entry in the same file

## QA

None — code review fix

## Work Log

### 2026-02-27T10:59:05.448Z - Completed: Fixed stale closure by reading fresh state via pluginStore.getPlugin() at execute time. Added .catch() handler with appLogger.error. TypeScript compiles clean.

