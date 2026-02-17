---
id: 152-b7a2
title: Add split keyboard shortcuts and new terminal spawning
status: complete
priority: P2
created: "2026-02-15T23:39:35.057Z"
updated: "2026-02-16T00:26:55.559Z"
dependencies: ["151-3dca"]
---

# Add split keyboard shortcuts and new terminal spawning

## Problem Statement

Users need keyboard shortcuts to create splits (Cmd+backslash for vertical, Cmd+Opt+backslash for horizontal). Splitting spawns a new terminal in the same cwd as the source pane. If already split, the shortcut does nothing (single-level limit).

## Acceptance Criteria

- [ ] Cmd+backslash creates a vertical split (new terminal to the right)
- [ ] Cmd+Opt+backslash creates a horizontal split (new terminal below)
- [ ] New terminal inherits cwd from the active pane
- [ ] Shortcut is no-op when tab is already split (single-level limit)
- [ ] New terminal gets a PTY session and is fully functional

## Files

- src/App.tsx

## Work Log

### 2026-02-16T00:26:55.492Z - Added Cmd+\ for vertical split and Cmd+Opt+\ for horizontal split. handleSplit initializes layout if needed, calls splitPane, tracks new terminal in branch, and sets it active. No-op when already split.

