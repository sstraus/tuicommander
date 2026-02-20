---
id: 289-ce9a
title: ensure_window_visible recovery ops silently ignored
status: complete
priority: P2
created: "2026-02-20T13:57:16.827Z"
updated: "2026-02-20T14:12:06.792Z"
dependencies: []
---

# ensure_window_visible recovery ops silently ignored

## Problem Statement

let _ = on set_size set_position center at lib.rs:63-65. Guard may do nothing.

## Acceptance Criteria

- [ ] Log failures with if let Err(e)

## Files

- src-tauri/src/lib.rs

## Work Log

### 2026-02-20T14:12:06.718Z - Replaced let _ = with if let Err(e) logging for set_size, set_position, center

