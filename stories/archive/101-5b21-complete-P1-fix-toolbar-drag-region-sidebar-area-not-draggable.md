---
id: 101-5b21
title: "Fix toolbar drag region: sidebar area not draggable after first focus"
status: complete
priority: P1
created: "2026-02-15T10:30:50.901Z"
updated: "2026-02-15T11:15:57.794Z"
dependencies: []
---

# Fix toolbar drag region: sidebar area not draggable after first focus

## Problem Statement

The toolbar uses -webkit-app-region: drag on #toolbar, then -webkit-app-region: no-drag on ALL three child divs (toolbar-left, toolbar-center, toolbar-right). This means the entire toolbar surface is no-drag except for tiny gaps between children. The sidebar toolbar area (toolbar-left) has width: var(--sidebar-width) and no-drag, so dragging the window from above the sidebar only works on first OS focus (WebKit bug: OS captures drag before webview processes no-drag). After that, it stops working entirely.

## Acceptance Criteria

- [ ] Remove blanket no-drag from toolbar-left, toolbar-center, toolbar-right containers - they should inherit drag from parent #toolbar
- [ ] Apply -webkit-app-region: no-drag only to interactive LEAF elements: toolbar-sidebar-toggle button, toolbar-branch button, IdeLauncher button, and any other clickable controls
- [ ] Verify toolbar-center still has data-tauri-drag-region attribute for Tauri compatibility
- [ ] Test: dragging from empty space in toolbar-left (above sidebar) works consistently, not just on first focus
- [ ] Test: dragging from empty space in toolbar-center (between branch badge and IDE launcher) works consistently
- [ ] Test: all toolbar buttons remain clickable (sidebar toggle, branch name, IDE launcher)
- [ ] Preserve macOS traffic light padding in toolbar-left (.platform-macos .toolbar-left padding-left: 78px)

## Files

- src/styles.css
- src/components/Toolbar/Toolbar.tsx

## Work Log

### 2026-02-15T11:15:57.725Z - Fixed toolbar drag region: removed blanket no-drag from containers, applied only to interactive elements. 891 tests pass.

