---
id: 197-ec30
title: Use platform config directory with migration
status: complete
priority: P1
created: "2026-02-16T13:20:02.391Z"
updated: "2026-02-16T13:48:44.239Z"
dependencies: []
---

# Use platform config directory with migration

## Problem Statement

Config dir is hardcoded to ~/.tui-commander/ via dirs::home_dir(). Violates Windows conventions. Should use %APPDATA% via dirs::config_dir().

## Acceptance Criteria

- [ ] Switch to dirs::config_dir().join(tui-commander)
- [ ] On macOS: ~/Library/Application Support/tui-commander
- [ ] On Windows: %APPDATA%/tui-commander
- [ ] On Linux: ~/.config/tui-commander
- [ ] Add one-time migration from old ~/.tui-commander if it exists
- [ ] Update all config_dir() call sites

## Files

- src-tauri/src/config.rs
- src-tauri/src/lib.rs
- src-tauri/src/dictation/model.rs

## Work Log

### 2026-02-16T13:48:44.172Z - Switched config_dir() to dirs::config_dir(), auto-migration from ~/.tui-commander/, centralized 6 duplicate path constructions

