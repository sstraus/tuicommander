---
id: 022-95fe
title: Context menu with right-click
status: complete
priority: P3
created: "2026-02-04T11:31:14.249Z"
updated: "2026-02-04T11:53:04.417Z"
dependencies: ["033-9a09"]
---

# Context menu with right-click

## Problem Statement

No context menu for terminal panes. Users need right-click access to Copy, Paste, Split horizontal/vertical actions.

## Acceptance Criteria

- [x] Right-click opens context menu on terminal pane
- [x] Menu options: Copy, Paste, Clear, Close Terminal
- [x] Keyboard shortcut hints in menu
- [x] Menu closes on click outside or Escape

## Implementation Notes

- Created reusable ContextMenu component with createContextMenu hook
- Menu auto-positions to stay within viewport
- Copy uses window.getSelection() and Clipboard API
- Paste uses Clipboard API and writes directly to terminal
- Animated entrance with scale/fade effect

## Files

- src/components/ContextMenu/ContextMenu.tsx
- src/components/ContextMenu/index.ts
- src/components/index.ts (export)
- src/App.tsx (integration)
- src/styles.css (context menu styles)

## Work Log

- Created ContextMenu component with items, position, visibility
- Added createContextMenu hook for state management
- Integrated into App.tsx with right-click on terminal-panes
- Added Copy, Paste, Clear, Close Terminal actions
- Build verified passing
