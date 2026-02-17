---
id: 083-03dc
title: Auto-remove PTY session from storage on process exit
status: complete
priority: P2
created: "2026-02-08T10:18:04.004Z"
updated: "2026-02-08T10:53:34.728Z"
dependencies: []
---

# Auto-remove PTY session from storage on process exit

## Problem Statement

When a PTY process exits (EOF), the reader thread emits pty-exit but does NOT remove the session from the HashMap. The session with its writer, master handle, and child process persists in Rust memory until an explicit close_pty call. Over many branch switches this leaks file descriptors and memory since frontend unmount intentionally skips close_pty to support session persistence.

## Acceptance Criteria

- [ ] Reader thread removes session from DashMap on EOF or read error before emitting pty-exit
- [ ] Writer and MasterPty handles are properly dropped (closes FDs)
- [ ] Frontend receives pty-exit event before session is removed
- [ ] close_pty handles already-removed session gracefully (no error)
- [ ] No orphaned sessions visible in get_orchestrator_stats after shell exit

## Files

- src-tauri/src/lib.rs (reader thread exit path, close_pty)

## Work Log

