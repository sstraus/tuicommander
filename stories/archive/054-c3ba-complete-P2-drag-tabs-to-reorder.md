---
id: "054-c3ba"
title: "Drag tabs to reorder"
status: complete
priority: P2
created: 2026-02-04T17:37:45.256Z
updated: 2026-02-04T17:37:45.256Z
dependencies: []
---

# Drag tabs to reorder

## Problem Statement

Users cannot reorder terminal tabs by dragging. Tabs remain in creation order which may not match user workflow preferences.

## Acceptance Criteria

- [ ] Add drag-and-drop handlers to tab components
- [ ] Visual feedback during drag (placeholder, drop indicator)
- [ ] Update terminal order in store on drop
- [ ] Persist tab order per branch

## Files

- src/components/TabBar/TabBar.tsx
- src/stores/terminals.ts

## Work Log

