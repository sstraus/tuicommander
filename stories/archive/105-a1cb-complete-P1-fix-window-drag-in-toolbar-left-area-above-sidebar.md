---
id: 105-a1cb
title: Fix window drag in toolbar-left area (above sidebar)
status: complete
priority: P1
created: "2026-02-15T11:23:40.336Z"
updated: "2026-02-15T17:19:37.632Z"
dependencies: []
---

# Fix window drag in toolbar-left area (above sidebar)

## Problem Statement

Window dragging works on the right side of the toolbar (toolbar-center/toolbar-right area) but NOT on the left side (toolbar-left, above the sidebar near traffic lights). Story 101-5b21 removed the blanket no-drag CSS from containers but the issue persists. The toolbar-left div spans the full sidebar width (~300px) and contains only a small toggle button, leaving ~250px of dead space that should be draggable. Tauri config uses titleBarStyle: Overlay with hiddenTitle: true and trafficLightPosition offset. The issue may be: (1) toolbar-left still intercepting mouse events via its flex layout/padding, (2) Tauri Overlay titlebar treating the traffic light inset zone as non-draggable, (3) a WebKit compositing issue where the sidebar content below bleeds through the drag region, or (4) the hotkey-hint span inside the toggle button expanding beyond its visible bounds.

## Acceptance Criteria

- [ ] INVESTIGATE: Use browser DevTools (localhost:1420) to inspect what element receives mousedown at various points in toolbar-left empty space
- [ ] INVESTIGATE: Check if the issue is Tauri-specific (test in browser vs Tauri) or CSS-specific
- [ ] INVESTIGATE: Check if toolbar-left padding-left: 78px (macOS traffic light zone) creates an invisible interactive area
- [ ] Fix: Ensure all empty space in toolbar-left is draggable, matching the behavior of toolbar-center empty space
- [ ] Test: Window drag works from any point on the toolbar bar, both left of and right of the branch badge
- [ ] Test: Traffic lights (close/minimize/maximize) remain clickable
- [ ] Test: Sidebar toggle button remains clickable
- [ ] Preserve trafficLightPosition offset and titleBarStyle: Overlay configuration

## Files

- src/styles.css
- src/components/Toolbar/Toolbar.tsx
- src-tauri/tauri.conf.json

## Work Log

### 2026-02-15T17:19:37.569Z - Added height:100% to toolbar sections and explicit -webkit-app-region:drag to toolbar-left. Toolbar-left already had data-tauri-drag-region attribute.

