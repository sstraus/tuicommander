---
id: 170-3b1e
title: Parallelize sequential git operations in branch refresh
status: complete
priority: P2
created: "2026-02-16T07:12:19.788Z"
updated: "2026-02-16T07:52:51.260Z"
dependencies: []
---

# Parallelize sequential git operations in branch refresh

## Problem Statement

refreshAllBranchStats iterates repos then branches sequentially. 5 repos x 4 branches = 20 serial git calls taking ~800ms total.

## Acceptance Criteria

- [ ] Use Promise.all() for both repo-level and branch-level loops
- [ ] Add error handling for individual branch failures
- [ ] Target under 100ms for 20-branch refresh

## Files

- src/App.tsx

## Related

- PERF-02

## Work Log

### 2026-02-16T07:52:51.196Z - Wrapped repo iteration in Promise.all() and branch stats fetching in nested Promise.all(). Worktree sync stays sequential per-repo (dependency), but repos and branch stats run in parallel.

