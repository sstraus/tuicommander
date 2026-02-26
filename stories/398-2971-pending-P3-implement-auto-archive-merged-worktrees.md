---
id: "398-2971"
title: "Implement auto-archive merged worktrees"
status: pending
priority: P3
created: 2026-02-26T12:47:28.110Z
updated: 2026-02-26T12:47:28.110Z
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

