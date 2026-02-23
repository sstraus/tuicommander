---
id: 192-72d8
title: Add system menu with View > Split Terminal entries
status: complete
priority: P3
created: "2026-02-16T13:12:30.104Z"
updated: "2026-02-16T18:33:17.243Z"
dependencies: []
---

# Add system menu with View > Split Terminal entries

## Problem Statement

Split terminal operations are only accessible via keyboard shortcuts Cmd/Ctrl+D and Cmd/Ctrl+Shift+D. There is no native system menu bar, which hurts discoverability and violates platform conventions on macOS, Windows, and Linux.

## Acceptance Criteria

- [ ] Add Tauri native menu bar with standard App, Edit, View, Window, Help menus (cross-platform)
- [ ] View menu includes Split Right and Split Down with correct platform shortcut labels
- [ ] Menu items invoke existing useSplitPanes hook logic via Tauri menu events
- [ ] Standard menu items work (Quit, Copy, Paste, Minimize, Zoom, etc.)
- [ ] Shortcuts use Cmd on macOS and Ctrl on Windows/Linux

## Files

- src-tauri/src/lib.rs
- src/App.tsx
- src/hooks/useSplitPanes.ts

## Work Log

### 2026-02-16T18:33:13.155Z - Implemented menu.rs with File/Edit/View/Go/Tools/Help submenus, PredefinedMenuItems for Edit, accelerators on all custom items. Wired into lib.rs .setup() with on_menu_event emitting menu-action to frontend.

