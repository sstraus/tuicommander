---
id: 144-8670
title: Middle-click tab to close (VS Code behavior)
status: complete
priority: P2
created: "2026-02-15T22:39:36.553Z"
updated: "2026-02-15T23:14:40.316Z"
dependencies: []
---

# Middle-click tab to close (VS Code behavior)

## Problem Statement

Users expect middle-click on a tab to close it, matching VS Code and browser conventions. Currently middle-click does nothing.

## Acceptance Criteria

- [ ] Add auxclick handler to terminal, diff, and markdown tab divs in TabBar.tsx
- [ ] Middle-click (button === 1) triggers the same close logic as the X button
- [ ] Prevent default to avoid paste-on-middle-click on Linux
- [ ] Add test coverage for middle-click close behavior

## Files

- src/components/TabBar/TabBar.tsx
- src/__tests__/components/TabBar.test.tsx

## Work Log

### 2026-02-15T23:14:40.253Z - Added onAuxClick handler to all 3 tab types in TabBar.tsx. Middle-click (button===1) closes tab with preventDefault for Linux. 2 new tests. All 33 TabBar tests pass.

