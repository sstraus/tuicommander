---
id: 397-939c
title: Implement orphan worktree cleanup detection and handling
status: complete
priority: P3
created: "2026-02-26T12:47:28.109Z"
updated: "2026-02-27T07:17:58.838Z"
dependencies: []
---

# Implement orphan worktree cleanup detection and handling

## Problem Statement

orphan_cleanup setting (Ask/On/Off) is stored but no code detects worktrees whose branch was deleted remotely. Setting is completely dead.

## Acceptance Criteria

- [ ] On branch stats refresh, detect worktrees present on filesystem but with no corresponding branch
- [ ] When orphans found and orphan_cleanup=On: auto-remove silently
- [ ] When orphan_cleanup=Ask: show confirmation dialog listing orphaned worktrees
- [ ] When orphan_cleanup=Off: do nothing

## Files

- src/hooks/useGitOperations.ts
- src-tauri/src/worktree.rs

## Work Log

### 2026-02-27T07:10:48.580Z - AUTONOMOUS DECISION: Detect orphans as linked worktrees in 'detached' HEAD state (git worktree list --porcelain shows 'detached' instead of 'branch refs/heads/...'). This is the git-native signal for a worktree whose branch was deleted while the worktree directory still exists. Implementing as: Rust fn detect_orphan_worktrees returns Vec<path> of detached linked worktrees; TypeScript calls it in refreshAllBranchStats, reads orphanCleanup from repoSettingsStore.getEffective, handles on/ask/off.

### 2026-02-27T07:17:58.099Z - Completed: detect_orphan_worktrees Tauri command + HTTP GET /repo/orphan-worktrees; remove_orphan_worktree command + HTTP POST /repo/remove-orphan; parse_orphan_worktrees pure fn (tested with 3 Rust unit tests); handleOrphanCleanup in refreshAllBranchStats reads orphanCleanup from repoSettingsStore (on=auto-remove, ask=confirm dialog, off=skip); confirmOrphanCleanup added to useConfirmDialog; 5 TS tests covering all 3 modes.

