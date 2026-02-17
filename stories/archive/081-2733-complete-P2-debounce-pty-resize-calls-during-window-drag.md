---
id: 081-2733
title: Debounce PTY resize calls during window drag
status: complete
priority: P2
created: "2026-02-08T10:18:04.002Z"
updated: "2026-02-08T10:46:45.466Z"
dependencies: []
---

# Debounce PTY resize calls during window drag

## Problem Statement

xterm.js fitAddon.fit() fires onResize which calls resize_pty via IPC. During window drag-resize this fires 30+ times per second, each sending a SIGWINCH to the PTY child process. Rapid SIGWINCH storms can cause full-screen apps (vim, htop, lazygit) to flicker or crash.

## Acceptance Criteria

- [ ] Debounce resize_pty calls in Terminal.tsx onResize handler with 150ms delay
- [ ] Final dimensions after resize drag are always sent (trailing edge debounce)
- [ ] No visual stutter during rapid window resize
- [ ] Resize still works immediately on tab switch (not debounced in that path)

## Files

- src/components/Terminal/Terminal.tsx (onResize handler)

## Work Log

