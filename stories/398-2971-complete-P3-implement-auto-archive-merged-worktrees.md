---
id: 398-2971
title: Implement auto-archive merged worktrees
status: complete
priority: P3
created: "2026-02-26T12:47:28.110Z"
updated: "2026-02-27T07:22:15.987Z"
dependencies: []
---

# Implement auto-archive merged worktrees

## Problem Statement

auto_archive_merged setting is stored but refreshAllBranchStats never acts on isMerged state to trigger archiving. No automatic archiving fires.

## Acceptance Criteria

- [ ] When a branch is detected as merged (isMerged=true) and auto_archive_merged=true: trigger archive
- [ ] Archive is non-destructive (moves dir, does not delete)
- [ ] User is notified via status bar when auto-archive fires
- [ ] Only fires once per branch (not on every refresh)

## Files

- src/hooks/useGitOperations.ts
- src-tauri/src/worktree.rs

## Work Log

### 2026-02-27T07:22:13.791Z - Completed: handleAutoArchiveMerged in useGitOperations.ts — reads autoArchiveMerged from repoSettingsStore.getEffective, filters branches where isMerged=true and worktreePath not null/not repoPath, calls finalizeMergedWorktree for each, sets status info. 3 tests covering: archives when enabled, skips when disabled, skips main worktree.

