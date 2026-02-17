---
id: 150-08b3
title: Add TabLayout type and split state to terminal store
status: complete
priority: P2
created: "2026-02-15T23:39:35.054Z"
updated: "2026-02-16T00:23:41.265Z"
dependencies: []
---

# Add TabLayout type and split state to terminal store

## Problem Statement

Split panes need a data model to track layout state per tab. Currently each tab maps to a single terminal. We need a TabLayout structure that can represent 1 or 2 panes with a split direction and ratio.

## Acceptance Criteria

- [ ] Define TabLayout type: direction (none/vertical/horizontal), panes (1 or 2 terminal IDs), ratio (0.0-1.0), activePaneIndex (0 or 1)
- [ ] Each branch state tracks its TabLayout (default: direction none, single pane)
- [ ] Existing single-terminal tabs work identically (zero behavioral change)
- [ ] Store actions: splitPane(direction), closeSplitPane(index), setSplitRatio(ratio), setActivePaneIndex(index)
- [ ] Tests for all store actions

## Files

- src/stores/terminals.ts
- src/__tests__/stores/terminals.test.ts

## Related

- docs/proposals/split-panes.md

## Work Log

### 2026-02-16T00:23:41.198Z - Added TabLayout type (direction, panes, ratio, activePaneIndex) to terminal store. Added actions: setLayout, splitPane, closeSplitPane, setSplitRatio, setActivePaneIndex. splitPane inherits cwd from source, closeSplitPane collapses to survivor. 15 new tests, all passing.

