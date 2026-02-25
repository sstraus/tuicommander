---
id: 364-cad3
title: "Tab bar: hidden scrollbar with fade gradients and chevron arrows"
status: complete
priority: P2
created: "2026-02-24T20:49:45.188Z"
updated: "2026-02-24T21:16:57.664Z"
dependencies: []
---

# Tab bar: hidden scrollbar with fade gradients and chevron arrows

## Problem Statement

Tab bar uses overflow-x: auto which shows a native scrollbar (ugly on macOS). When many tabs are open the UX is poor.

## Acceptance Criteria

- [ ] Hide native scrollbar (scrollbar-width: none / ::-webkit-scrollbar hidden) while keeping scroll functional
- [ ] Add left/right fade gradient overlays that appear when there are tabs overflowing in that direction
- [ ] Add left/right chevron arrow buttons that appear when scrolling is possible in that direction
- [ ] Clicking arrow scrolls the tab list by a fixed amount (e.g. 200px)
- [ ] Active tab is always scrolled into view when selected
- [ ] Trackpad horizontal scroll continues to work
- [ ] Arrows and gradients hide when all tabs fit without scrolling

## Files

- src/components/TabBar/TabBar.tsx
- src/components/TabBar/TabBar.module.css

## Work Log

### 2026-02-24T21:16:54.742Z - Implemented hidden scrollbar with fade gradients and chevron arrows. CSS: added tabBarWrapper, scrollArrow buttons, fadeGradient overlays with visibility transitions. TSX: added scroll state tracking (canScrollLeft/canScrollRight), ResizeObserver, smooth scrollBy, auto-scroll active tab into view. All 51 tests pass, TypeScript clean.

