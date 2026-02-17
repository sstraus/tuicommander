---
id: 245-abe6
title: Add settings persist error handling with rollback
status: complete
priority: P2
created: "2026-02-17T11:05:06.229Z"
updated: "2026-02-17T11:09:58.769Z"
dependencies: []
---

# Add settings persist error handling with rollback

## Problem Statement

setSplitTabMode and other setters update state optimistically but do not rollback on save failure. UI shows saved state that will not persist across restart.

## Acceptance Criteria

- [ ] setSplitTabMode: save previous value, rollback state on catch, re-throw error
- [ ] Add test: setSplitTabMode persist error triggers rollback
- [ ] Apply same rollback pattern to other setters lacking it (setIde, setFont, setTheme, setShell)
- [ ] npx vitest run passes

## Files

- src/stores/settings.ts
- src/__tests__/stores/settings.test.ts

## Work Log

### 2026-02-17T11:09:54.098Z - Added rollback to all 7 async setters in settings.ts. Added 5 new rollback tests, updated 2 existing. vitest 1492 passed.

