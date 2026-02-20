---
id: 308-cac9
title: "Fix performance: debounce useGitHub refresh and pre-compute sortedBranches PR statuses"
status: complete
priority: P2
created: "2026-02-20T19:25:07.617Z"
updated: "2026-02-20T20:04:29.212Z"
dependencies: []
---

# Fix performance: debounce useGitHub refresh and pre-compute sortedBranches PR statuses

## Problem Statement

useGitHub.ts createEffect fires refresh() immediately on every repo path change with no debounce — rapid tab switching floods backend. sortedBranches memo reads githubStore.getPrStatus inside sort comparator causing all RepoSection instances to re-sort on every 30s poll.

## Acceptance Criteria

- [ ] Verify before: confirm bare refresh() call in createEffect at useGitHub.ts:82 and getPrStatus inside sort at Sidebar.tsx:113
- [ ] Add 200ms debounce to refresh() in useGitHub createEffect with onCleanup clearTimeout
- [ ] Extract prStatuses createMemo before sortedBranches in Sidebar.tsx
- [ ] Run make check and all tests pass

## Files

- src/hooks/useGitHub.ts
- src/components/Sidebar/Sidebar.tsx

## Work Log

### 2026-02-20T20:04:29.136Z - Added 200ms debounce to createEffect in useGitHub.ts. Pre-computed prStatuses Map in Sidebar.tsx RepoSection to avoid repeated getPrStatus calls in sort comparator. Updated useGitHub test to flush 200ms debounce before recording callsBefore.

