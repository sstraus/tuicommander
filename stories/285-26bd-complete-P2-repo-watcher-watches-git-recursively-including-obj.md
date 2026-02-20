---
id: 285-26bd
title: repo_watcher watches .git/ recursively including objects/
status: complete
priority: P2
created: "2026-02-20T13:57:16.825Z"
updated: "2026-02-20T14:11:29.025Z"
dependencies: []
---

# repo_watcher watches .git/ recursively including objects/

## Problem Statement

Git pack/fetch operations generate hundreds of events in objects/ that are all filtered out at repo_watcher.rs:85-88.

## Acceptance Criteria

- [ ] Watch .git/ non-recursively plus .git/refs/ recursively

## Files

- src-tauri/src/repo_watcher.rs

## Work Log

### 2026-02-20T14:11:28.946Z - Changed to NonRecursive on .git/ plus Recursive on .git/refs/ to avoid objects/ noise

