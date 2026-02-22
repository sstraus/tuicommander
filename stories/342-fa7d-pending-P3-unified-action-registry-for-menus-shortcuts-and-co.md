---
id: 342-fa7d
title: Unified action registry for menus, shortcuts, and context menu
status: pending
priority: P3
created: "2026-02-21T19:11:16.302Z"
updated: "2026-02-21T19:11:22.478Z"
dependencies: ["340-3473"]
---

# Unified action registry for menus, shortcuts, and context menu

## Problem Statement

Context menu is hardcoded arrays, native menu is in Rust, keyboard shortcuts are in useKeyboardShortcuts hook, menu-action dispatch is in App.tsx. Adding a new action requires touching 3-4 files. No single source of truth for available actions.

## Acceptance Criteria

- [ ] Inventory of all actions across: native menu, context menu, keyboard shortcuts, toolbar buttons
- [ ] Single action registry data structure: { id, label, shortcut, handler, when, icon }
- [ ] Context menu generated from registry with when-clause filtering
- [ ] Keyboard shortcuts derived from registry
- [ ] Native menu definition in Rust generated from or synced with registry
- [ ] Adding a new action requires one registry entry, not multiple file edits
- [ ] All existing shortcuts and menus work identically after migration

## Files

- src/hooks/useKeyboardShortcuts.ts
- src/components/ContextMenu/ContextMenu.tsx
- src/App.tsx
- src-tauri/src/menu.rs

## Related

- god component decomposition

## Work Log

