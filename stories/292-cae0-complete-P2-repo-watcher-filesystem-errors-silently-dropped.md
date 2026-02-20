---
id: 292-cae0
title: repo_watcher filesystem errors silently dropped
status: complete
priority: P2
created: "2026-02-20T13:57:16.833Z"
updated: "2026-02-20T14:11:29.342Z"
dependencies: []
---

# repo_watcher filesystem errors silently dropped

## Problem Statement

let Ok(events) = events else return at repo_watcher.rs:63. No log for watcher errors.

## Acceptance Criteria

- [ ] Add eprintln for watcher errors

## Files

- src-tauri/src/repo_watcher.rs

## Work Log

### 2026-02-20T14:11:29.273Z - Added error logging for watcher errors instead of silent return

