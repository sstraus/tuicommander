---
id: 293-5fbd
title: Arithmetic overflow in ensure_window_visible on corrupted dimensions
status: complete
priority: P2
created: "2026-02-20T13:57:16.839Z"
updated: "2026-02-20T14:12:06.937Z"
dependencies: []
---

# Arithmetic overflow in ensure_window_visible on corrupted dimensions

## Problem Statement

size.width as i32 silently wraps for large u32 values at lib.rs:43-44.

## Acceptance Criteria

- [ ] Use i32::try_from() with saturating_add

## Files

- src-tauri/src/lib.rs

## Work Log

### 2026-02-20T14:12:06.867Z - Used i32::try_from with saturating_add to prevent overflow on corrupted dimensions

