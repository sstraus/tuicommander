---
id: 011-95bf
title: Session save/restore
status: complete
priority: P3
created: "2026-02-04T10:50:24.117Z"
updated: "2026-02-04T11:34:16.178Z"
dependencies: []
---

# Session save/restore

## Problem Statement

Preserve open terminals, worktrees, zoom levels across app restarts.

## Acceptance Criteria

- [ ] Save session state to ~/.tui-commander/session.json
- [ ] Restore on launch
- [ ] Cleanup stale worktrees
- [ ] Ask user on launch (Restore last session?)

## Files

- src-tauri/src/lib.rs
- src/main.ts

## Work Log

