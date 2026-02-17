---
id: "060-9065"
title: "CI checks detailed popover"
status: complete
priority: P2
created: 2026-02-04T22:00:24.966Z
updated: 2026-02-04T22:00:24.966Z
dependencies: []
---

# CI checks detailed popover

## Problem Statement

The PR status badge only shows a simple status. Users need to see detailed breakdown of all CI checks (which passed, which failed, which are pending).

## Acceptance Criteria

- [ ] Click on PR status badge opens popover
- [ ] Popover shows list of all CI checks with status icons
- [ ] Scrollable list if many checks
- [ ] Click on check opens GitHub check URL
- [ ] Auto-refresh checks status

## Files

- src/components/StatusBar/StatusBar.tsx
- src/styles.css

## Work Log

