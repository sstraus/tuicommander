---
id: 157-cb80
title: Avoid branch re-sort on every GitHub poll cycle
status: complete
priority: P2
created: "2026-02-16T07:04:39.837Z"
updated: "2026-02-16T07:43:38.697Z"
dependencies: []
---

# Avoid branch re-sort on every GitHub poll cycle

## Problem Statement

Sidebar sortedBranches createMemo reads githubStore reactively inside the sort comparator, causing O(n log n) re-sort every 30s on poll even when PR statuses have not changed. Should only re-sort when branch list or PR states actually change.

## Acceptance Criteria

- [ ] Branch sort only re-runs when branch list changes or PR state changes (not every poll)
- [ ] Merged/closed PRs still sort to bottom correctly
- [ ] No visual regression in sidebar branch ordering
- [ ] Test that sort is stable when poll returns same data

## Files

- src/components/Sidebar/Sidebar.tsx

## Work Log

### 2026-02-16T07:43:38.632Z - Root cause was updateRepoData() replacing entire branches object on every poll. Fixed by updating branches individually via setState path, allowing SolidJS to skip unchanged values. Sidebar sort comparator unchanged since the reactive granularity is now correct.

