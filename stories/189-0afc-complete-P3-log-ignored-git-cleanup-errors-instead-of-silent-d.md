---
id: 189-0afc
title: Log ignored git cleanup errors instead of silent discard
status: complete
priority: P3
created: "2026-02-16T07:17:10.815Z"
updated: "2026-02-16T07:38:23.398Z"
dependencies: []
---

# Log ignored git cleanup errors instead of silent discard

## Problem Statement

Git worktree prune errors are silently discarded with let _ pattern. Failures should be logged for debugging even if not critical.

## Acceptance Criteria

- [ ] Replace let _ with if let Err(e) and log warning
- [ ] Document why cleanup errors are non-fatal

## Files

- src-tauri/src/lib.rs

## Related

- RS-07

## Work Log

### 2026-02-16T07:38:23.256Z - Replaced let _ = with if let Err(e) + eprintln for git worktree prune and git branch -d operations.

