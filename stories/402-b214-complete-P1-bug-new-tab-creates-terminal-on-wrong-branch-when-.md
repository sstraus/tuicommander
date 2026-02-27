---
id: 402-b214
title: "Bug: new tab (+) creates terminal on wrong branch when HEAD changed externally"
status: complete
priority: P1
created: "2026-02-26T16:21:04.089Z"
updated: "2026-02-27T06:09:40.186Z"
dependencies: []
---

# Bug: new tab (+) creates terminal on wrong branch when HEAD changed externally

## Problem Statement

When the HEAD of the main worktree changes externally (e.g. git checkout in a terminal, or another tool), the sidebar may not reflect the new active branch yet. Clicking + on the tab bar calls handleNewTab() which reads repositoriesStore.getActive().activeBranch at call time — if the store is stale, the new terminal is created on the wrong branch.

## Acceptance Criteria

- [ ] Reproduce: open wiz-agents repo, switch branch externally (git checkout develop in a terminal), click + before sidebar updates, verify new terminal appears on correct branch
- [ ] Investigate whether the head-changed event fired and was handled by useAppInit.ts:178
- [ ] Investigate if the scenario involves linked worktrees (where each worktree has its own HEAD not watched by the main repo head watcher)
- [ ] Investigate if activeRepoPath points to the correct worktree path when the sidebar shows a linked-worktree branch as active
- [ ] Fix root cause so that handleNewTab() always creates the terminal on the branch that is visually active in the sidebar
- [ ] Add regression test

## Files

- src/hooks/useGitOperations.ts
- src/hooks/useAppInit.ts
- src-tauri/src/head_watcher.rs
- src/stores/repositories.ts

## Work Log

### 2026-02-26T22:28:55.233Z - BUDGET STOP: quota >=100%, session stopped at limit. Story left in_progress.

### 2026-02-26T22:29:00.087Z - BUDGET STOP: quota >=100%, session stopped at limit. Story left in_progress.

### 2026-02-27T06:00:35.465Z - AUTONOMOUS DECISION: Starting fresh investigation of the bug. Reading relevant source files to understand the data flow.

### 2026-02-27T06:09:37.566Z - Completed: Fixed handleNewTab to use active terminal's branch registration to disambiguate when multiple branches share the same worktreePath (main checkout after HEAD change). Root cause: CWD-only match picks first insertion-order branch when both old-branch and new-branch have worktreePath='/repo'. Fix adds a prior check for which branch owns the active terminal, making it robust against the race between refreshAllBranchStats and setActiveBranch. Also fixed package-lock.json stale rollup entry (4.57.1 → 4.59.0) causing false-positive npm audit failure. All 2137 tests pass, make check clean.

