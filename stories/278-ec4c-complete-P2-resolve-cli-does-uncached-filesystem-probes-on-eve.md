---
id: 278-ec4c
title: resolve_cli does uncached filesystem probes on every git invocation
status: complete
priority: P2
created: "2026-02-20T13:57:16.819Z"
updated: "2026-02-20T14:16:28.384Z"
dependencies: []
---

# resolve_cli does uncached filesystem probes on every git invocation

## Problem Statement

1-4 stat() syscalls per git command via Path::exists() in agent.rs:69-77. Result is stable for app lifetime.

## Acceptance Criteria

- [ ] resolve_cli results cached per binary name
- [ ] No repeated filesystem probes for same binary

## Files

- src-tauri/src/agent.rs
- src-tauri/src/state.rs

## Work Log

### 2026-02-20T14:16:28.314Z - Extracted to cli.rs with OnceLock caching for both extra_bin_dirs and resolve_cli results

