---
id: 151-3dca
title: Render split layout with 1 or 2 visible terminal panes
status: complete
priority: P2
created: "2026-02-15T23:39:35.056Z"
updated: "2026-02-16T00:25:47.335Z"
dependencies: ["150-08b3"]
---

# Render split layout with 1 or 2 visible terminal panes

## Problem Statement

App.tsx currently renders terminal panes with position:absolute and only shows the active one. With splits, a tab can show 2 terminals side-by-side or stacked using flexbox. The rendering must switch between single and split modes based on TabLayout state.

## Acceptance Criteria

- [ ] Single pane (direction: none) renders exactly as current behavior
- [ ] Vertical split renders two terminals side-by-side in a flex row
- [ ] Horizontal split renders two terminals stacked in a flex column
- [ ] Both terminals in a split are visible and functional simultaneously
- [ ] Active pane has a subtle accent-colored border indicator
- [ ] CSS uses min-width:0 and min-height:0 on pane containers

## Files

- src/App.tsx
- src/styles.css
- src/components/Terminal/Terminal.tsx

## Work Log

### 2026-02-16T00:25:47.276Z - Updated App.tsx to render split panes using TabLayout state with flex sizing. Added CSS for split-vertical/split-horizontal containers, split-pane visibility overrides, and active pane accent border indicator. Updated Terminal.tsx isVisible() to activate when in layout panes.

