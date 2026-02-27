---
id: 407-8b09
title: Fix /dev/null hardcode in get_file_diff — breaks on Windows
status: complete
priority: P1
created: "2026-02-26T21:07:12.408Z"
updated: "2026-02-27T09:08:04.675Z"
dependencies: []
---

# Fix /dev/null hardcode in get_file_diff — breaks on Windows

## Problem Statement

`get_file_diff` in `git.rs:465` hardcodes `/dev/null` in `git diff --no-index -- /dev/null`. This is Unix-only; Windows needs `NUL`. AGENTS.md requires macOS/Windows/Linux support. On Windows this silently returns empty diff output.

## Acceptance Criteria

- [ ] Use `#[cfg(windows)]` to select `NUL` vs `/dev/null`
- [ ] Verify git on Windows understands `NUL` as null device in `--no-index`

## Work Log

### 2026-02-27T09:08:04.600Z - Replaced hardcoded /dev/null with NULL_DEVICE const using cfg(windows). Test added.

