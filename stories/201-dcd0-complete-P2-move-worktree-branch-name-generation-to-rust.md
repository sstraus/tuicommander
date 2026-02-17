---
id: 201-dcd0
title: Move worktree branch name generation to Rust
status: complete
priority: P2
created: "2026-02-16T17:25:18.069Z"
updated: "2026-02-16T17:37:15.846Z"
dependencies: []
---

# Move worktree branch name generation to Rust

## Problem Statement

Worktree branch name generation (worktree-001 pattern with dedup loop) is in src/hooks/useGitOperations.ts:269-274. Backend already has generate_worktree_name_cmd but frontend duplicates the logic.

## Acceptance Criteria

- [ ] Frontend uses existing generate_worktree_name_cmd Tauri command
- [ ] Duplicate JS name generation logic removed
- [ ] Worktree creation still works correctly
- [ ] Characterization test written BEFORE migration that passes with JS implementation
- [ ] Same test passes AFTER migration with identical results from Rust implementation

## Files

- src/hooks/useGitOperations.ts
- src-tauri/src/worktree.rs

## Work Log

### 2026-02-16T17:37:15.768Z - Replaced JS worktree-001 name generation loop with call to existing Rust generate_worktree_name_cmd. Added generateWorktreeName to useRepository.ts. Updated 3 tests. All 1415 TS tests pass.

