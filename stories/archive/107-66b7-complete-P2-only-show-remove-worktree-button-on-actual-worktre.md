---
id: 107-66b7
title: Only show remove worktree button on actual worktrees
status: complete
priority: P2
created: "2026-02-15T13:47:37.784Z"
updated: "2026-02-15T17:23:43.669Z"
dependencies: []
---

# Only show remove worktree button on actual worktrees

## Problem Statement

The orange X remove button on branch items and the Delete Worktree context menu item appear on ALL non-main branches, including regular branches that have no worktree. Clicking X on a non-worktree branch triggers handleRemoveBranch (App.tsx:551) which calls repo.removeWorktree -> Rust remove_worktree command. The Rust command (lib.rs:1322) searches for a worktree matching the branch name via git worktree list --porcelain, fails with 'No worktree found for branch', but the frontend swallows the error (App.tsx:568-571) and removes the branch from the store anyway. This means clicking X on a regular branch silently deletes it from the UI and attempts to git branch -d it. Dangerous and misleading.

## Acceptance Criteria

- [ ] SIDEBAR UI (Sidebar.tsx): Show the X remove button (line 302) only when branch.worktreePath is not null AND not isMain. Currently the condition is just !props.branch.isMain
- [ ] CONTEXT MENU (Sidebar.tsx): Show Delete Worktree menu item (line 250) only when branch.worktreePath is not null. Currently the condition is just !props.branch.isMain
- [ ] HANDLER (App.tsx:551): handleRemoveBranch should early-return with an error dialog if the branch has no worktreePath, as a safety net even if the UI guards are bypassed
- [ ] CONFIRMATION DIALOG (useConfirmDialog.ts:34): The dialog text says 'This deletes the worktree directory and its local branch' - this is only correct for worktrees. Should not be shown for non-worktree branches
- [ ] TEST: Verify main branch never shows remove button regardless of worktreePath value
- [ ] TEST: Verify branch with worktreePath=null does not show X button or Delete Worktree context menu item
- [ ] TEST: Verify branch with worktreePath set and isMain=false shows X button and context menu item

## Files

- src/components/Sidebar/Sidebar.tsx
- src/App.tsx
- src/hooks/useConfirmDialog.ts

## Work Log

### 2026-02-15T17:23:43.600Z - Guarded X remove button and Delete Worktree context menu with worktreePath check. Added safety check in handleRemoveBranch handler. Added 2 tests.

