---
id: 164-76bb
title: Implement atomic config file writes
status: complete
priority: P1
created: "2026-02-16T07:11:38.774Z"
updated: "2026-02-16T07:25:33.136Z"
dependencies: []
---

# Implement atomic config file writes

## Problem Statement

Non-atomic file writes plus concurrent frontend store saves equals last-write-wins race condition causing data loss. Also config files have default 0644 permissions exposing password hashes.

## Acceptance Criteria

- [ ] Write to temp file then rename (atomic on POSIX)
- [ ] Set 0600 permissions on config files
- [ ] Debounce or serialize concurrent saves from frontend stores
- [ ] Add tests for concurrent write scenarios

## Files

- src-tauri/src/config.rs
- src/stores/settings.ts
- src/stores/repositories.ts

## Related

- DATA-01
- DATA-02
- DATA-03

## Work Log

### 2026-02-16T07:25:33.071Z - Implemented atomic write (temp+rename), 0600 permissions on Unix, added 2 tests

