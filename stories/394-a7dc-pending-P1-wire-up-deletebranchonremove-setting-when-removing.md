---
id: "394-a7dc"
title: "Wire up deleteBranchOnRemove setting when removing worktree"
status: pending
priority: P1
created: 2026-02-26T12:47:28.108Z
updated: 2026-02-26T12:47:28.108Z
dependencies: []
---

# Wire up deleteBranchOnRemove setting when removing worktree

## Problem Statement

remove_worktree_by_branch always runs git branch -d unconditionally, ignoring the deleteBranchOnRemove config field (WorktreeDefaults, config.rs:546).

## Acceptance Criteria

- [ ] remove_worktree Tauri command accepts delete_branch: bool parameter
- [ ] remove_worktree_by_branch accepts delete_branch: bool and gates git branch -d on it
- [ ] Frontend reads effective deleteBranchOnRemove setting and passes it to removeWorktree RPC
- [ ] Default remains true (existing behavior) when setting is not configured

## Files

- src-tauri/src/worktree.rs
- src/hooks/useGitOperations.ts
- src/hooks/useRepository.ts

## Work Log

