---
id: 193-5482
title: Expose all keyboard shortcuts in system menu for discoverability
status: complete
priority: P3
created: "2026-02-16T13:12:57.551Z"
updated: "2026-02-16T18:33:17.410Z"
dependencies: ["192"]
---

# Expose all keyboard shortcuts in system menu for discoverability

## Problem Statement

The app has 20+ keyboard shortcuts but no system menu to surface them. Users must read the HelpPanel or guess. A proper menu bar is the standard way to make features discoverable on macOS, Windows, and Linux. Story 192 adds the menu with split entries only — this story covers all remaining shortcuts.

## Acceptance Criteria

- [ ] File menu: New Tab, Close Tab, Reopen Closed Tab, Settings, Quit
- [ ] Edit menu: Copy, Paste, Clear Terminal
- [ ] View menu: Toggle Sidebar, Zoom In/Out/Reset, Split Right, Split Down (from 192)
- [ ] Go menu: Next Tab, Previous Tab, Switch to Tab 1-9, Quick Switch Branch
- [ ] Tools menu: Prompt Library, Run Script, Lazygit, Lazygit Split, Git Operations, Task Queue
- [ ] Help menu: Help Panel, About
- [ ] All items show correct platform shortcut labels (Cmd on macOS, Ctrl on Windows/Linux)
- [ ] Menu items invoke existing hook/store actions — no new logic needed

## Files

- src-tauri/src/lib.rs
- src/App.tsx
- src/hooks/useKeyboardShortcuts.ts

## Related

- 192-72d8

## Work Log

### 2026-02-16T18:33:13.342Z - All 25+ shortcuts exposed in system menu. Frontend listen(menu-action) dispatches to existing ShortcutHandlers. menuDedup.ts prevents double-firing. HelpPanel updated with menu bar note. Manual test items added to to-test.md.

