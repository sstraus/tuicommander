---
id: 186-0a0c
title: Split lib.rs monolith into Rust modules
status: complete
priority: P1
created: "2026-02-16T07:17:10.814Z"
updated: "2026-02-16T09:38:17.465Z"
dependencies: []
---

# Split lib.rs monolith into Rust modules

## Problem Statement

lib.rs is 2992 lines combining PTY management, git operations, GitHub API, worktree lifecycle, file I/O, agent detection, and 60+ Tauri commands in one module.

## Acceptance Criteria

- [ ] Extract PTY session management into pty.rs
- [ ] Extract git operations into git.rs
- [ ] Extract worktree lifecycle into worktree.rs
- [ ] Extract agent detection into agents.rs
- [ ] lib.rs becomes thin glue with Tauri command registration
- [ ] Cargo tests still pass

## Files

- src-tauri/src/lib.rs

## Related

- ARCH-02

## Work Log

### 2026-02-16T09:38:13.339Z - Completed: eliminated _internal wrappers, moved tests to modules, lib.rs down to 262 lines from 2998

