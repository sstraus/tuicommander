---
id: 046-11a7
title: Fix tab keyboard shortcuts off-by-one error
status: complete
priority: P2
created: "2026-02-04T16:54:03.274Z"
updated: "2026-02-07T22:26:50.514Z"
dependencies: ["033-9a09"]
---

# Fix tab keyboard shortcuts off-by-one error

## Problem Statement

Tab bar shows terminals as main 1 (⌘1), main 2 (⌘2), etc. but the actual keyboard shortcuts are shifted by one - pressing ⌘2 switches to main 1, ⌘3 switches to main 2. The displayed shortcut does not match the actual shortcut.

## Acceptance Criteria

- [ ] ⌘1 switches to the first tab (index 0)
- [ ] ⌘2 switches to the second tab (index 1)
- [ ] Displayed shortcut in tab matches actual keyboard binding
- [ ] Fix either the keyboard handler or the display label to be consistent

## Files

- src/main.ts
- index.html

## Work Log

