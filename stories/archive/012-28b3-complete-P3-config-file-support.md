---
id: 012-28b3
title: Config file support
status: complete
priority: P3
created: "2026-02-04T10:50:24.118Z"
updated: "2026-02-04T11:35:20.471Z"
dependencies: []
---

# Config file support

## Problem Statement

Settings like default shell, font, theme should persist. Currently hardcoded.

## Acceptance Criteria

- [ ] Config at ~/.tui-commander/config.toml
- [ ] Schema: shell, font_family, font_size, theme, worktree_dir
- [ ] Hot reload on file change
- [ ] UI for editing (Cmd+,)

## Files

- src-tauri/src/lib.rs

## Work Log

