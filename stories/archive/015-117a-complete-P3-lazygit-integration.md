---
id: 015-117a
title: lazygit integration
status: complete
priority: P3
created: "2026-02-04T10:50:24.119Z"
updated: "2026-02-04T11:53:04.258Z"
dependencies: ["033-9a09"]
---

# lazygit integration

## Problem Statement

Use lazygit for git operations. Embed in panel or spawn in terminal.

## Acceptance Criteria

- [x] Spawn in terminal pane (Cmd+G)
- [x] Pass current repo path with `-p` flag
- [x] Added to context menu

## Implementation Notes

- Cmd+G sends `lazygit -p "/path/to/repo"` command to active terminal
- If no repo selected, just runs `lazygit` in current directory
- Added "Open Lazygit" option to terminal context menu
- Relies on lazygit being installed on the system

## Files

- src/App.tsx (keyboard shortcut, spawnLazygit function, context menu item)

## Work Log

- Added spawnLazygit function that writes command to active terminal
- Added Cmd+G keyboard shortcut
- Added to context menu with shortcut hint
- Build verified passing
