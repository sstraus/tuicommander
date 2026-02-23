---
id: 156-5ae3
title: Debounce sidebar resize IPC persistence
status: complete
priority: P2
created: "2026-02-16T07:04:39.836Z"
updated: "2026-02-16T07:39:41.671Z"
dependencies: []
---

# Debounce sidebar resize IPC persistence

## Problem Statement

During sidebar drag resize, every mousemove pixel triggers uiStore.setSidebarWidth() which calls Tauri invoke() to persist. This floods IPC with unnecessary calls. CSS update should be immediate, persistence should be debounced.

## Acceptance Criteria

- [ ] Update CSS variable immediately on mousemove for smooth visual feedback
- [ ] Debounce uiStore.setSidebarWidth() call to ~100ms during drag
- [ ] Persist final value on mouseup regardless of debounce timer
- [ ] No visual difference during drag (stays smooth)

## Files

- src/components/Sidebar/Sidebar.tsx

## Work Log

### 2026-02-16T07:39:41.605Z - During drag: update CSS --sidebar-width directly with local clamping, no IPC. On mouseup: single setSidebarWidth() call persists final width. Eliminates IPC flood during drag.

