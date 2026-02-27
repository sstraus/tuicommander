---
id: 417-ddb8
title: Remove hotkey-hint badges from tab bar, toolbar, and status bar
status: complete
priority: P3
created: "2026-02-27T06:38:01.084Z"
updated: "2026-02-27T11:10:44.242Z"
dependencies: []
---

# Remove hotkey-hint badges from tab bar, toolbar, and status bar

## Problem Statement

The hotkey-hint badges (e.g. ⌘T on new-tab button, ⌘[ on sidebar toggle, ⌘D/⌘M/⌘N etc. on status bar) add visual clutter. Shortcuts are already discoverable via Settings > Keyboard Shortcuts and the Command Palette. Remove all hotkey-hint span elements and the associated CSS (.hotkey-hint, .hotkey-hint.quick-switcher-active rules in styles.css).

## Acceptance Criteria

- [ ] Remove hotkey-hint spans from TabBar.tsx (⌘T), Toolbar.tsx (⌘[), StatusBar.tsx (6 badges)
- [ ] Remove .hotkey-hint CSS rules from styles.css
- [ ] Remove quickSwitcherActive prop plumbing if no longer needed
- [ ] Update StatusBar.test.tsx to remove hotkey-hint assertions
- [ ] Keep title attributes on buttons so shortcut is still visible on hover

## Files

- src/components/TabBar/TabBar.tsx
- src/components/Toolbar/Toolbar.tsx
- src/components/StatusBar/StatusBar.tsx
- src/styles.css
- src/__tests__/components/StatusBar.test.tsx

## QA

None — covered by tests

## Work Log

### 2026-02-27T11:10:44.122Z - Removed all .hotkey-hint spans from StatusBar (7), TabBar (1), and Toolbar (1). Removed .hotkey-hint CSS rules from styles.css. Removed quickSwitcherActive prop from StatusBar and Toolbar interfaces and App.tsx usage. Removed hotkey-hint test from StatusBar.test.tsx. Title attributes with shortcut info preserved for hover discovery.

