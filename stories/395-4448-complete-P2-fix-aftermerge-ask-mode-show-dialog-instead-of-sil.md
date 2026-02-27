---
id: 395-4448
title: Fix afterMerge ask mode - show dialog instead of silently completing
status: complete
priority: P2
created: "2026-02-26T12:47:28.108Z"
updated: "2026-02-27T06:17:30.044Z"
dependencies: []
---

# Fix afterMerge ask mode - show dialog instead of silently completing

## Problem Statement

When afterMerge setting is ask, merge_and_archive_worktree returns action=pending but the frontend (useGitOperations.ts:644) ignores it and just logs success. No dialog is shown to the user.

## Acceptance Criteria

- [ ] When result.action === pending, show a dialog asking archive or delete
- [ ] Dialog choice triggers the appropriate follow-up Tauri call
- [ ] User can cancel (worktree stays as-is after merge)

## Files

- src/hooks/useGitOperations.ts
- src/components/

## Work Log

### 2026-02-27T06:09:52.088Z - AUTONOMOUS DECISION: Reading relevant code before starting implementation.

### 2026-02-27T06:11:45.503Z - AUTONOMOUS DECISION: Architecture - expose mergePendingCtx signal from useGitOperations + new MergePostActionDialog component in App.tsx. Adding finalize_merged_worktree Tauri command for the follow-up archive/delete. Reusing existing dialog.module.css for consistent styling.

### 2026-02-27T06:17:26.960Z - Completed: Added finalize_merged_worktree Tauri command (archive/delete follow-up for pending merge). MergePostActionDialog shows 3 buttons (Archive/Delete/Keep). mergePendingCtx signal in useGitOperations drives dialog. handleMergePendingChoice executes the choice and cleans up. Branch stays in sidebar until user decides. 5 new tests pass. make check clean.

