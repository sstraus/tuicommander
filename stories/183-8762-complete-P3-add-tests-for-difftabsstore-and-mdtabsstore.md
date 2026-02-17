---
id: 183-8762
title: Add tests for diffTabsStore and mdTabsStore
status: complete
priority: P3
created: "2026-02-16T07:12:39.085Z"
updated: "2026-02-16T07:57:48.967Z"
dependencies: []
---

# Add tests for diffTabsStore and mdTabsStore

## Problem Statement

243 lines of untested code. These stores manage diff/markdown tabs with complex deduplication logic. Same pattern as terminalsStore which is well-tested.

## Acceptance Criteria

- [ ] Add test files for both stores
- [ ] Cover open, close, activate, deduplication logic
- [ ] Follow terminals.test.ts as pattern

## Files

- src/stores/diffTabs.ts
- src/stores/mdTabs.ts

## Related

- TEST-02

## Work Log

### 2026-02-16T07:57:48.897Z - Added 16 tests across 2 new test files covering all public API methods: add, deduplication, remove, clearForRepo, clearAll, getForRepo.

