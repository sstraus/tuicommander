---
id: 254-abaf
title: Disable add-worktree button and show loading state during creation
status: complete
priority: P1
created: "2026-02-18T17:16:24.883Z"
updated: "2026-02-18T17:17:47.398Z"
dependencies: []
---

# Disable add-worktree button and show loading state during creation

## Problem Statement

Creating a worktree is slow. The + button shows no feedback during the operation, so users think nothing happened and click multiple times, creating many duplicate worktrees.

## Acceptance Criteria

- [ ] The + add-worktree button is disabled while a worktree creation is in progress for that repo
- [ ] A visible loading indicator (spinner or changed label) is shown on the button during creation
- [ ] Multiple simultaneous creations for the same repo are prevented
- [ ] The button returns to normal state when creation succeeds or fails

## Files

- src/hooks/useGitOperations.ts
- src/components/Sidebar/Sidebar.tsx

## Work Log

### 2026-02-18T17:17:47.320Z - Added creatingWorktreeRepos: Set<string> signal to useGitOperations. handleAddWorktree guards against concurrent calls for the same repo and tracks in-progress state in the Set. Sidebar receives creatingWorktreeRepos prop; RepoSection shows '…' and disables the + button while creation is in progress.

