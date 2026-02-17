---
id: 055-9432
title: Windows platform compatibility
status: complete
priority: P1
created: "2026-02-04T17:38:02.466Z"
updated: "2026-02-04T17:39:48.174Z"
dependencies: []
---

# Windows platform compatibility

## Problem Statement

Current titlebar implementation uses macOS-specific features (titleBarStyle overlay, traffic light padding). Windows users will have broken UI with incorrect padding and potentially non-functional window controls.

## Acceptance Criteria

- [ ] Detect platform at runtime (macOS/Windows/Linux)
- [ ] Conditional CSS for titlebar height and padding per platform
- [ ] Windows: use standard decorations or custom titlebar with min/max/close buttons
- [ ] Linux: handle various window managers gracefully
- [ ] Test window dragging on all platforms

## Files

- src-tauri/tauri.conf.json
- src/styles.css
- src/App.tsx

## Work Log

