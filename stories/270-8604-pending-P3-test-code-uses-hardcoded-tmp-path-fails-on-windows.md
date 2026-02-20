---
id: "270-8604"
title: "Test code uses hardcoded /tmp path - fails on Windows"
status: pending
priority: P3
created: 2026-02-20T07:38:38.180Z
updated: 2026-02-20T07:38:38.180Z
dependencies: []
---

# Test code uses hardcoded /tmp path - fails on Windows

## Problem Statement

mcp_http/mod.rs test_state() uses PathBuf::from("/tmp/test-worktrees"). /tmp does not exist on Windows, causing test failures if tests are run on Windows.

## Acceptance Criteria

- [ ] Test uses std::env::temp_dir() instead of hardcoded /tmp
- [ ] Tests pass on Windows

## Files

- src-tauri/src/mcp_http/mod.rs

## Work Log

