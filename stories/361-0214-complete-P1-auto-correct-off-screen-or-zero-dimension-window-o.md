---
id: 361-0214
title: Auto-correct off-screen or zero-dimension window on startup
status: complete
priority: P1
created: "2026-02-22T19:19:59.076Z"
updated: "2026-02-22T19:22:33.156Z"
dependencies: []
---

# Auto-correct off-screen or zero-dimension window on startup

## Problem Statement

Persisted window state can have position way off-screen (e.g. x:4452 y:-386) or zero dimensions (width:0 height:0), causing the app window to appear invisible or in an unusable state on next launch. The app must detect and auto-correct this on startup.

## Acceptance Criteria

- [ ] On startup, validate persisted window position against all available monitors
- [ ] If window is off-screen (no intersection with any monitor), reset position to center of primary monitor
- [ ] If width or height is 0 or below minimum (800x600), reset to default dimensions
- [ ] Fix must apply before the window is shown to avoid flicker

## Files

- src-tauri/src/lib.rs

## Work Log

### 2026-02-22T19:22:33.042Z - Moved ensure_window_visible from setup() to RunEvent::Ready handler. The window-state plugin restores persisted position after setup(), so the guard was running too early. Now fires after the plugin applies its state.

