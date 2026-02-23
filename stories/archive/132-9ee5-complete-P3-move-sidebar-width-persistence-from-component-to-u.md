---
id: 132-9ee5
title: Move sidebar width persistence from component to ui store
status: complete
priority: P3
created: "2026-02-15T21:17:30.375Z"
updated: "2026-02-15T22:22:57.629Z"
dependencies: []
---

# Move sidebar width persistence from component to ui store

## Problem Statement

Sidebar width is persisted directly in Sidebar.tsx via localStorage, inconsistent with other UI state in ui.ts store.

## Acceptance Criteria

- [ ] Add sidebarWidth to ui store state
- [ ] Load/save via ui store instead of direct localStorage in Sidebar.tsx
- [ ] Remove localStorage calls from Sidebar.tsx

## Files

- src/stores/ui.ts
- src/components/Sidebar/Sidebar.tsx

## Work Log

### 2026-02-15T22:22:57.558Z - Added sidebarWidth to UIStoreState with localStorage persistence (key: tui-commander-sidebar-width). Added setSidebarWidth() with clamping (200-500). Removed SIDEBAR_STORAGE_KEY, local signal, and direct localStorage calls from Sidebar.tsx. Updated Sidebar tests. All 928 tests pass.

