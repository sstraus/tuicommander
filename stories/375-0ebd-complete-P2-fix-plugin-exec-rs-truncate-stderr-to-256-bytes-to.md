---
id: 375-0ebd
title: Fix plugin_exec.rs - truncate stderr to 256 bytes to prevent info leak
status: complete
priority: P2
created: "2026-02-25T07:45:22.407Z"
updated: "2026-02-25T08:27:47.564Z"
dependencies: []
---

# Fix plugin_exec.rs - truncate stderr to 256 bytes to prevent info leak

## Problem Statement

Failed command stderr returned verbatim to plugin. If CLI leaks secrets on stderr they surface to the plugin caller.

## Acceptance Criteria

- [ ] Truncate stderr to first 256 bytes before including in error returned to plugin
- [ ] Add comment explaining why (prevent accidental secret leakage)
- [ ] make check passes

## Files

- src-tauri/src/plugin_exec.rs

## Work Log

### 2026-02-25T08:27:47.498Z - Added MAX_STDERR_BYTES=256 constant. Truncated stderr slice before including in error message. Added stderr_truncation_boundary test.

