---
id: 138-47b5
title: Refresh sidebar and status bar after branch rename
status: complete
priority: P2
created: "2026-02-15T22:14:08.743Z"
updated: "2026-02-15T23:18:32.963Z"
dependencies: []
---

# Refresh sidebar and status bar after branch rename

## Problem Statement

When renaming a branch via the status bar button (git branch -m), only the tab title updates. The sidebar still shows the old branch name and the status bar shows a truncated/stale name. No refresh/sync is triggered after the rename.

## Acceptance Criteria

- [ ] After a successful branch rename, sidebar must refresh to show the new branch name
- [ ] After a successful branch rename, status bar must show the updated branch name
- [ ] No manual refresh or page reload should be required

## Work Log

### 2026-02-15T23:18:32.895Z - Fixed: StatusBar BranchPopover was ignoring oldName/newName args in onBranchRenamed, only calling github.refresh(). Added onBranchRenamed prop to StatusBarProps, wired from App.tsx to update repositoriesStore.renameBranch(), currentBranch, and statusInfo. TDD: wrote failing test first, 973 tests pass.

