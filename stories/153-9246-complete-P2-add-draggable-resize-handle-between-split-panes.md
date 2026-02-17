---
id: 153-9246
title: Add draggable resize handle between split panes
status: complete
priority: P2
created: "2026-02-15T23:39:35.057Z"
updated: "2026-02-16T00:28:24.826Z"
dependencies: ["151-3dca"]
---

# Add draggable resize handle between split panes

## Problem Statement

When two panes are visible in a split, users need a draggable handle to adjust the ratio between them. The handle should have a wide hit area (8px) but thin visual line (1px). Ratio should be constrained to 0.2-0.8 to prevent unusably small panes.

## Acceptance Criteria

- [ ] Resize handle appears between split panes
- [ ] Handle has 8px transparent hit area with 1px visible border
- [ ] Dragging updates the split ratio in real-time
- [ ] Ratio constrained to 0.2-0.8 range
- [ ] Both terminals call xterm fit() after resize (debounced)
- [ ] Cursor changes to col-resize or row-resize on hover

## Files

- src/App.tsx
- src/styles.css

## Work Log

### 2026-02-16T00:28:24.762Z - Added draggable resize handle between split panes. 8px transparent hit area with 1px visible border, highlights on hover. Drag updates ratio in real-time via mousemove, clamped 0.2-0.8. Both terminals fit() on mouse up. Cursor changes to col-resize/row-resize.

