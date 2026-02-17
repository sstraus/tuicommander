---
id: "065-db0e"
title: "Single window enforcement"
status: complete
priority: P3
created: 2026-02-04T22:00:24.969Z
updated: 2026-02-04T22:00:24.969Z
dependencies: []
---

# Single window enforcement

## Problem Statement

Opening the app multiple times creates multiple windows which can be confusing. Should focus existing window instead.

## Acceptance Criteria

- [ ] Detect if app window already exists on launch
- [ ] Focus existing window instead of creating new
- [ ] Handle edge case of window being minimized
- [ ] Works correctly on macOS, Windows, Linux

## Files

- src-tauri/src/lib.rs
- src-tauri/src/main.rs

## Work Log

