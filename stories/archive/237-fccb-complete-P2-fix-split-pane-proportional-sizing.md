---
id: 237-fccb
title: Fix split pane proportional sizing
status: complete
priority: P2
created: "2026-02-17T10:28:47.137Z"
updated: "2026-02-17T10:37:03.858Z"
dependencies: []
---

# Fix split pane proportional sizing

## Problem Statement

Split panes open like a sidebar instead of sizing proportionally due to flex: 0 0 calc() preventing natural flex distribution

## Acceptance Criteria

- [ ] Split panes use flex-grow based sizing
- [ ] Both panes display at 50/50 by default
- [ ] Resize handle still works correctly

## Files

- src/App.tsx

## Work Log

### 2026-02-17T10:37:00.632Z - Changed flex from 0 0 calc() to ratio 1 0% for proportional split sizing.

