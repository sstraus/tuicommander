---
id: 276-302c
title: resolve_cli and extra_bin_dirs have zero unit tests
status: complete
priority: P1
created: "2026-02-20T13:56:35.899Z"
updated: "2026-02-20T13:59:43.978Z"
dependencies: []
---

# resolve_cli and extra_bin_dirs have zero unit tests

## Problem Statement

Critical platform function for release-build PATH resolution with no tests at agent.rs:17-77.

## Acceptance Criteria

- [ ] Tests for fallback behavior
- [ ] Tests for non-empty dirs
- [ ] Tests for duplicate prevention

## Files

- src-tauri/src/agent.rs

## Work Log

### 2026-02-20T13:59:43.903Z - Added 5 unit tests: non-empty dirs, no duplicates, no empty strings, fallback to name, finds known binary

