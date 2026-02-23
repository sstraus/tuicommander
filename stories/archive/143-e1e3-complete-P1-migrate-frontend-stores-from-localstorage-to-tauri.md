---
id: 143-e1e3
title: Migrate frontend stores from localStorage to Tauri invoke
status: complete
priority: P1
created: "2026-02-15T22:32:27.445Z"
updated: "2026-02-15T23:12:06.198Z"
dependencies: ["141", "142"]
---

# Migrate frontend stores from localStorage to Tauri invoke

## Problem Statement

8 stores persist to localStorage instead of Tauri backend, violating the logic-in-Rust architecture rule and preventing external tools (MCP) from reading settings.

## Acceptance Criteria

- [ ] Each store replaces localStorage.getItem with invoke(load_xxx_config)
- [ ] Each store replaces localStorage.setItem with invoke(save_xxx_config)
- [ ] One-time migration per store: read localStorage, save to Tauri, removeItem
- [ ] Stores initialize with defaults then hydrate async from Tauri
- [ ] Migration order: ui.ts, settings.ts, notifications.ts, errorHandling.ts+agentFallback.ts, repoSettings.ts, promptLibrary.ts, repositories.ts
- [ ] Each sub-migration committed separately
- [ ] Zero localStorage usage in stores after migration (except migration reads)
- [ ] All tests pass
- [ ] Existing user settings preserved after first launch migration

## Files

- src/stores/ui.ts
- src/stores/settings.ts
- src/stores/notifications.ts
- src/stores/errorHandling.ts
- src/stores/agentFallback.ts
- src/stores/repoSettings.ts
- src/stores/promptLibrary.ts
- src/stores/repositories.ts

## Related

- C
- D

## Work Log

### 2026-02-15T23:12:06.132Z - Migrated all 8 stores from localStorage to Tauri invoke: notifications, errorHandling, agentFallback, repoSettings, promptLibrary, ui, settings, repositories. Each store has hydrate() for async loading, fire-and-forget invoke for saves, and one-time localStorage migration. Updated all test files. 970 tests pass.

