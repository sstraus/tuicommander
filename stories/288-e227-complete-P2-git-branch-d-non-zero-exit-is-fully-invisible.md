---
id: 288-e227
title: git branch -d non-zero exit is fully invisible
status: complete
priority: P2
created: "2026-02-20T13:57:16.826Z"
updated: "2026-02-20T14:16:28.668Z"
dependencies: []
---

# git branch -d non-zero exit is fully invisible

## Problem Statement

if let Err(e) only catches spawn failures not git exit code at worktree.rs:260-266. Most common failure (unmerged branch) produces zero log.

## Acceptance Criteria

- [ ] Check output.status.success()
- [ ] Log stderr on failure

## Files

- src-tauri/src/worktree.rs

## Work Log

### 2026-02-20T14:16:28.601Z - Check output.status.success() and log stderr for git branch -d failures

