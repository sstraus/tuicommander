---
id: "397-939c"
title: "Implement orphan worktree cleanup detection and handling"
status: pending
priority: P3
created: 2026-02-26T12:47:28.109Z
updated: 2026-02-26T12:47:28.109Z
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

