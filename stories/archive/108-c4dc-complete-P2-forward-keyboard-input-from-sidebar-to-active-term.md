---
id: 108-c4dc
title: Forward keyboard input from sidebar to active terminal
status: complete
priority: P2
created: "2026-02-15T13:52:19.401Z"
updated: "2026-02-15T17:27:28.352Z"
dependencies: []
---

# Forward keyboard input from sidebar to active terminal

## Problem Statement

When focus is on the sidebar, typing characters does nothing. Other tools forward non-shortcut keystrokes to the active terminal, so users can type commands without explicitly clicking the terminal pane first. This reduces friction when navigating between sidebar and terminal.

## Acceptance Criteria

- [ ] Keystrokes typed while sidebar is focused are forwarded to the active terminal
- [ ] Modifier keys (Cmd, Ctrl, Alt) are NOT forwarded (they trigger shortcuts)
- [ ] Tab and Enter keys are NOT forwarded (used for sidebar navigation)
- [ ] No forwarding if no terminal is active

## Files

- src/components/Sidebar/Sidebar.tsx
- src/components/Terminal/Terminal.tsx
- src/stores/terminals.ts

## Work Log

### 2026-02-15T17:27:28.291Z - Already implemented via useKeyboardRedirect hook (src/hooks/useKeyboardRedirect.ts). Hook is active in App.tsx and has comprehensive tests. All acceptance criteria met.

