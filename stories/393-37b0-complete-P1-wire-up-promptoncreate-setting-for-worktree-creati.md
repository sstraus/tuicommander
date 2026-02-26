---
id: 393-37b0
title: Wire up promptOnCreate setting for worktree creation dialog
status: complete
priority: P1
created: "2026-02-26T12:47:28.107Z"
updated: "2026-02-26T15:00:06.925Z"
dependencies: []
---

# Wire up promptOnCreate setting for worktree creation dialog

## Problem Statement

handleAddWorktree always shows the creation dialog regardless of the promptOnCreate setting. When off, it should instantly create a worktree with the auto-generated name.

## Acceptance Criteria

- [ ] handleAddWorktree reads effective promptOnCreate for the repo
- [ ] When promptOnCreate is true (default): shows dialog as today
- [ ] When promptOnCreate is false: skips dialog and calls confirmCreateWorktree directly with auto-generated name
- [ ] Auto-generated name used when skipping dialog is the adjective-scifi-NNN format

## Files

- src/hooks/useGitOperations.ts

## Work Log

### 2026-02-26T15:00:06.755Z - Completed: handleAddWorktree reads promptOnCreate, skips dialog when false, creates worktree with auto-generated name. 3 new tests.

