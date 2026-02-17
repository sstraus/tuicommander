---
id: 174-775b
title: Introduce thiserror types for Rust error handling
status: wontfix
priority: P2
created: "2026-02-16T07:12:19.791Z"
updated: "2026-02-16T07:53:19.638Z"
dependencies: []
---

# Introduce thiserror types for Rust error handling

## Problem Statement

All functions return Result<T, String> which loses context and cannot be programmatically handled. String errors are unstructured.

## Acceptance Criteria

- [ ] Add thiserror crate
- [ ] Create error enums for major domains (Pty, Git, Config, Worktree)
- [ ] Migrate key functions from String errors to typed errors

## Files

- src-tauri/src/lib.rs
- src-tauri/Cargo.toml

## Related

- RS-05

## Work Log

### 2026-02-16T07:53:19.576Z - WONTFIX: Tauri commands return Result<T, String> across IPC boundary. Frontend only sees string messages. thiserror types add a crate dependency and boilerplate (28 functions to migrate) without practical benefit - error variants can't be matched across the IPC boundary. The current map_err pattern produces clear error messages.

