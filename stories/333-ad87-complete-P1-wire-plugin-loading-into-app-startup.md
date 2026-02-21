---
id: 333-ad87
title: Wire plugin loading into app startup
status: complete
priority: P1
created: "2026-02-21T14:51:35.069Z"
updated: "2026-02-21T15:44:20.759Z"
dependencies: ["332-7538"]
---

# Wire plugin loading into app startup

## Problem Statement

initPlugins() is sync and only loads builtins, needs to become async and load user plugins too

## Acceptance Criteria

- [ ] initPlugins() becomes async
- [ ] Builtins registered first (sync)
- [ ] User plugins loaded via await loadUserPlugins()
- [ ] Plugin watcher started after initial load
- [ ] App.tsx updated to await initPlugins()
- [ ] Existing tests pass

## Files

- src/plugins/index.ts
- src/App.tsx

## Work Log

### 2026-02-21T15:44:20.609Z - Made initPlugins() async, call loadUserPlugins() after builtins. All 1846 tests pass.

