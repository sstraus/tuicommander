---
id: "064-d91d"
title: "Visible hotkey hints on buttons"
status: complete
priority: P3
created: 2026-02-04T22:00:24.968Z
updated: 2026-02-04T22:00:24.968Z
dependencies: []
---

# Visible hotkey hints on buttons

## Problem Statement

Users must hover or guess keyboard shortcuts. Showing shortcut hints directly on buttons (like ⌘K) improves discoverability.

## Acceptance Criteria

- [ ] Add shortcut label styling for buttons
- [ ] Show hints after brief delay (avoid visual noise)
- [ ] Hints appear on hover or focus
- [ ] Consistent styling across all toolbar buttons

## Files

- src/styles.css
- src/components/TabBar/TabBar.tsx
- src/components/StatusBar/StatusBar.tsx

## Work Log

