---
id: 169-6774
title: Debounce saveRepos() to fix N+1 persistence
status: complete
priority: P2
created: "2026-02-16T07:12:19.786Z"
updated: "2026-02-16T07:36:08.238Z"
dependencies: []
---

# Debounce saveRepos() to fix N+1 persistence

## Problem Statement

Every state mutation (branch stats, terminal reorder, collapse toggle) triggers full serialization plus IPC call. 11 call sites result in 50+ disk writes/sec during branch refresh.

## Acceptance Criteria

- [ ] Debounce saveRepos() with 500ms trailing delay
- [ ] Remove saveRepos() from updateBranchStats (stats are ephemeral)
- [ ] Verify no data loss on app close

## Files

- src/stores/repositories.ts

## Related

- PERF-01

## Work Log

### 2026-02-16T07:36:08.172Z - Added 500ms trailing debounce to saveRepos(). Rapid mutations (add, setBranch, toggleExpanded etc) now coalesce into single IPC call. updateBranchStats already didn't save (ephemeral). Tests updated to use fake timers.

