---
id: 299-f2ed
title: Fix handleRenameBranch data corruption and getWorktreePaths branch deletion
status: complete
priority: P1
created: "2026-02-20T19:23:06.082Z"
updated: "2026-02-20T19:36:15.753Z"
dependencies: []
---

# Fix handleRenameBranch data corruption and getWorktreePaths branch deletion

## Problem Statement

1) handleRenameBranch in useGitOperations.ts has no try/catch: if backend fails, UI store is updated showing the new name while git retains the old, and user sees a false success message. 2) getWorktreePaths in useRepository.ts returns {} on any error; caller refreshAllBranchStats prunes branches not in the result, silently deleting all branches from sidebar on transient IPC error.

## Acceptance Criteria

- [ ] Verify before: confirm no try/catch in handleRenameBranch and bare catch{} in getWorktreePaths
- [ ] handleRenameBranch: add try/catch, update store only on success, show error status on failure
- [ ] getWorktreePaths: add console.error logging in catch before returning {}
- [ ] Verify after: rename failure shows error message, not success; worktree error is logged
- [ ] Run make check and existing tests pass

## Files

- src/hooks/useGitOperations.ts
- src/hooks/useRepository.ts

## Work Log

### 2026-02-20T19:36:15.675Z - Added try/catch to handleRenameBranch (UI only updates on success, shows error status on failure), added console.error to getWorktreePaths catch block. tsc clean, 1628 tests pass.

