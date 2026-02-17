---
id: 137-2b4e
title: Fix repo options menu overlay blocking sidebar interaction
status: complete
priority: P1
created: "2026-02-15T22:10:08.440Z"
updated: "2026-02-15T22:12:27.368Z"
dependencies: []
---

# Fix repo options menu overlay blocking sidebar interaction

## Problem Statement

The repo three-dots menu (.repo-context-menu) uses position:absolute inside the scrollable .sidebar-content (overflow-y:auto) and #sidebar (overflow:hidden). This causes the menu to be clipped and overlapped by sibling branch items below. The ContextMenu component already solves this with position:fixed and viewport-relative coordinates — the repo menu should reuse it.

## Acceptance Criteria

- [ ] Repo options menu (Repo Settings / Remove Repository) uses the existing ContextMenu component with createContextMenu() hook
- [ ] Menu opens at the correct position relative to the three-dots button (below-right)
- [ ] Menu is not clipped by sidebar overflow boundaries
- [ ] Menu items underneath the popover remain clickable when menu is closed
- [ ] Click-outside and Escape still close the menu
- [ ] Remove unused .repo-context-menu, .repo-menu-container, .repo-menu-item CSS

## Files

- src/components/Sidebar/Sidebar.tsx
- src/styles.css

## Work Log

### 2026-02-15T22:12:27.290Z - Replaced custom inline repo menu (position:absolute, clipped by sidebar overflow) with existing ContextMenu component (position:fixed). Removed dead CSS (.repo-menu-container, .repo-context-menu, .repo-menu-item). Updated 65 Sidebar tests to use new selectors. All 916 tests pass.

