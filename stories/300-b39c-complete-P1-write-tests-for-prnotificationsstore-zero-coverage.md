---
id: 300-b39c
title: Write tests for prNotificationsStore (zero coverage)
status: complete
priority: P1
created: "2026-02-20T19:23:06.083Z"
updated: "2026-02-20T19:44:50.028Z"
dependencies: []
---

# Write tests for prNotificationsStore (zero coverage)

## Problem Statement

prNotificationsStore drives the PR notification bell and auto-dismiss UX but has zero tests. The store has non-trivial logic: deduplication by pr+type, auto-dismiss via focus timer at 5 minutes, getActive filtering, clearAll.

## Acceptance Criteria

- [ ] Verify before: confirm no test file for prNotifications.ts exists
- [ ] Create src/__tests__/stores/prNotifications.test.ts
- [ ] Test add(): deduplication (ignores duplicate active, replaces dismissed), dismissed:false, focusedTimeMs:0
- [ ] Test dismiss(): marks single notification, does not affect others
- [ ] Test dismissAll(): marks all dismissed
- [ ] Test getActive(): returns only non-dismissed, empty when none
- [ ] Test startFocusTimer(): increments focusedTimeMs, auto-dismisses at threshold
- [ ] Run tests and confirm all pass

## Files

- src/stores/prNotifications.ts
- src/__tests__/stores/prNotifications.test.ts

## Work Log

### 2026-02-20T19:44:49.952Z - Created src/__tests__/stores/prNotifications.test.ts with 20 tests covering add() deduplication, dismiss(), dismissAll(), getActive(), startFocusTimer() with auto-dismiss, and clearAll(). All 20 tests pass.

