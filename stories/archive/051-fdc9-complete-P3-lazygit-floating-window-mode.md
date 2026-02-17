---
id: "051-fdc9"
title: "Lazygit floating window mode"
status: complete
priority: P3
created: 2026-02-04T17:06:31.284Z
updated: 2026-02-04T17:06:31.284Z
dependencies: []
---

# Lazygit floating window mode

## Problem Statement

Split pane mode reduces terminal space. Power users want lazygit in a separate OS window that floats above the main app. This allows full-screen lazygit while keeping terminal visible.

## Acceptance Criteria

- [ ] Add Cmd+Option+L to open lazygit in native OS window
- [ ] Use Tauri window API to create detached window
- [ ] Window spawns with PTY running lazygit
- [ ] Window title shows repo name and path
- [ ] Window remembers size/position
- [ ] Closing window properly cleans up PTY session

## Files

- src-tauri/src/lib.rs
- src/hooks/useLazygit.ts
- src-tauri/tauri.conf.json

## Work Log

