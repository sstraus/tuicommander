---
id: 238-6d7a
title: Add split menu to + button
status: complete
priority: P3
created: "2026-02-17T10:28:47.139Z"
updated: "2026-02-17T10:37:03.862Z"
dependencies: []
---

# Add split menu to + button

## Problem Statement

The + button only creates new tabs with no split options. Users want dropdown menu with split options like macOS reference

## Acceptance Criteria

- [ ] createContextMenu gets openAt(x,y) method for programmatic positioning
- [ ] + button click opens dropdown menu below the button
- [ ] Menu has New Tab, Split Vertically, Split Horizontally items
- [ ] Split options disabled when already in split mode

## Files

- src/components/TabBar/TabBar.tsx
- src/components/ContextMenu/ContextMenu.tsx
- src/App.tsx

## Work Log

### 2026-02-17T10:37:00.712Z - Added openAt() to createContextMenu. + button now opens dropdown menu with New Tab, Split Vertically, Split Horizontally options.

