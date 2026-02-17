---
id: 016-55b6
title: Advanced keyboard shortcuts
status: complete
priority: P3
created: "2026-02-04T10:50:24.119Z"
updated: "2026-02-04T11:53:04.310Z"
dependencies: ["033-9a09"]
---

# Advanced keyboard shortcuts

## Problem Statement

Missing Cmd+1-9 for tab switching, Cmd+Shift+[ / ] for tab navigation, etc.

## Acceptance Criteria

- [x] Cmd+1-9: Switch to tab N
- [x] Cmd+Shift+[ / ]: Previous/next tab
- [x] Cmd+Shift+T: Reopen closed tab
- [x] Cmd+L: Clear terminal (Cmd+K is used for prompt library)

## Implementation Notes

- Added `closedTabs` signal to track closed terminals for reopening
- Added `navigateTab` function for prev/next tab navigation
- Added `clearTerminal` function using xterm.js clear()
- Added `reopenClosedTab` function that restores tab name and font size
- Added `clear` method to TerminalRef interface
- Task queue panel moved to Cmd+J

## Files

- src/App.tsx (keyboard shortcuts)
- src/stores/terminals.ts (TerminalRef interface)
- src/components/Terminal/Terminal.tsx (clear method)

## Work Log

- Implemented all keyboard shortcuts
- Build verified passing
