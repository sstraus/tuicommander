---
id: 073-50dd
title: Add repository context menu with settings and remove
status: complete
priority: P2
created: "2026-02-05T11:52:51.117Z"
updated: "2026-02-05T12:28:19.558Z"
dependencies: []
---

# Add repository context menu with settings and remove

## Problem Statement

Users cannot remove a repository from TUI Commander sidebar. The only option is the settings button but there is no way to remove a repo from the list.


Context menu on repo header (click on ⋯ or right-click):
- Repo Settings
- Remove Repository

Simple dropdown menu with two options.

## Acceptance Criteria

- [ ] Show context menu when clicking ⋯ button on repo header
- [ ] Menu options: Repo Settings, Remove Repository
- [ ] Remove Repository shows confirmation dialog
- [ ] Removing repo closes all associated terminals
- [ ] Removing repo cleans up localStorage state
- [ ] Keep terminals running option (optional)

## Files

- src/components/Sidebar/Sidebar.tsx
- src/components/ContextMenu/ContextMenu.tsx
- src/stores/repositories.ts

## Work Log

