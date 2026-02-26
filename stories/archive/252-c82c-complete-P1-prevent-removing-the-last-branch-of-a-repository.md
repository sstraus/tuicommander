---
id: 252-c82c
title: Prevent removing the last branch of a repository
status: complete
priority: P1
created: "2026-02-18T16:45:16.178Z"
updated: "2026-02-18T16:46:08.156Z"
dependencies: []
---

# Prevent removing the last branch of a repository

## Problem Statement

The remove worktree button is shown even when a branch is the only one in a repo. Removing it leaves the repo with no branches (shows "No branches loaded"), which is a broken state.

## Acceptance Criteria

- [ ] The remove button is hidden (or disabled) when the branch is the only one in its repository
- [ ] Removing a branch is only possible when at least one other branch remains
- [ ] This applies to both the branch item button and the context menu Delete Worktree option

## Files

- src/components/Sidebar/Sidebar.tsx

## Work Log

### 2026-02-18T16:46:08.076Z - Added canRemove prop to BranchItem (true only when sortedBranches().length > 1). Gates both the × button and the Delete Worktree context menu item. Single-branch repos can no longer be emptied.

