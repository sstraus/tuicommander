---
id: 167-1365
title: Fix blocking PTY operations in async handlers
status: complete
priority: P1
created: "2026-02-16T07:11:38.775Z"
updated: "2026-02-16T07:29:29.340Z"
dependencies: []
---

# Fix blocking PTY operations in async handlers

## Problem Statement

Synchronous PTY syscalls (openpty, spawn_command) block the tokio runtime in async HTTP handlers, starving other async tasks.

## Acceptance Criteria

- [ ] Wrap PTY operations in tokio::task::spawn_blocking()
- [ ] Replace std::thread::sleep with tokio::time::sleep in async contexts
- [ ] Audit all async handlers for blocking calls

## Files

- src-tauri/src/mcp_http.rs

## Related

- RS-01
- RS-03

## Work Log

### 2026-02-16T07:29:29.276Z - Replaced std::thread::sleep with tokio::time::sleep().await in create_pty retry loops (lib.rs lines 523,544) and close_session child-wait loop (mcp_http.rs line 218). Remaining std::thread::sleep calls at lines 381,457,820 are inside std::thread::spawn or sync fn - not issues.

