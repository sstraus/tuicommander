---
id: 072-d7d6
title: Add rename branch functionality
status: complete
priority: P2
created: "2026-02-05T11:51:43.325Z"
updated: "2026-02-05T12:28:19.506Z"
dependencies: []
---

# Add rename branch functionality

## Problem Statement

Users cannot rename branches/worktrees from TUI Commander. They must use git commands manually.


Modal dialog triggered by clicking branch name or context menu:
- Title: "Rename Branch"
- Input field with current name pre-filled and selected
- Cancel (outline) + Rename (primary blue) buttons
- Executes git branch -m oldname newname

## Acceptance Criteria

- [ ] Add rename option to branch context menu or double-click on name
- [ ] Show modal dialog with input field pre-filled with current name
- [ ] Validate new branch name (no spaces, valid git branch name)
- [ ] Execute git branch -m to rename the branch
- [ ] Update worktree path if it is a worktree
- [ ] Update repository store with new branch name
- [ ] Preserve terminal associations after rename

## Files

- src/components/Sidebar/Sidebar.tsx
- src/components/RenameBranchDialog/RenameBranchDialog.tsx
- src/hooks/useRepository.ts
- src-tauri/src/git.rs

## Work Log

