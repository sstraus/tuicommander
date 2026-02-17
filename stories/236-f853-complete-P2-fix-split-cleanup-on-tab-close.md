---
id: 236-f853
title: Fix split cleanup on tab close
status: complete
priority: P2
created: "2026-02-17T10:28:47.117Z"
updated: "2026-02-17T10:37:03.619Z"
dependencies: []
---

# Fix split cleanup on tab close

## Problem Statement

Closing a split terminal tab via X button leaves the split layout active, creating a broken layout with only one pane visible

## Acceptance Criteria

- [ ] closeTerminal() collapses split layout when closing a pane that belongs to a split
- [ ] Cmd+W handler delegates to closeTerminal() instead of duplicating split collapse logic
- [ ] Survivor pane becomes active and focused after split collapse

## Files

- src/hooks/useTerminalLifecycle.ts
- src/hooks/useKeyboardShortcuts.ts

## Work Log

### 2026-02-17T10:37:00.554Z - closeTerminal() now collapses split layout when closing a split pane. Cmd+W delegates to closeTerminal() instead of duplicating logic.

