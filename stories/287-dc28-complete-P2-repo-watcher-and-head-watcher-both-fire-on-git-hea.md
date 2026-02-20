---
id: 287-dc28
title: repo_watcher and head_watcher both fire on .git/HEAD changes
status: complete
priority: P2
created: "2026-02-20T13:57:16.826Z"
updated: "2026-02-20T14:11:29.191Z"
dependencies: []
---

# repo_watcher and head_watcher both fire on .git/HEAD changes

## Problem Statement

Redundant IPC round-trips on branch switch. Both watchers fire for HEAD at repo_watcher.rs:33-36.

## Acceptance Criteria

- [ ] HEAD removed from repo_watcher relevance filter

## Files

- src-tauri/src/repo_watcher.rs

## Work Log

### 2026-02-20T14:11:29.110Z - Removed HEAD from repo_watcher sentinel list - handled exclusively by head_watcher

