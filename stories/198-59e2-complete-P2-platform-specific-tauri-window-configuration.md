---
id: 198-59e2
title: Platform-specific Tauri window configuration
status: complete
priority: P2
created: "2026-02-16T13:20:02.392Z"
updated: "2026-02-16T13:47:33.957Z"
dependencies: []
---

# Platform-specific Tauri window configuration

## Problem Statement

tauri.conf.json has macOS-only settings: hiddenTitle, titleBarStyle Overlay, trafficLightPosition. Windows/Linux may have toolbar content overlapping with native window controls.

## Acceptance Criteria

- [ ] Verify toolbar layout renders correctly on Windows and Linux
- [ ] Add platform-specific CSS for Windows/Linux window control spacing if needed
- [ ] Document which tauri.conf.json settings are macOS-only

## Files

- src-tauri/tauri.conf.json
- src/styles.css

## Work Log

### 2026-02-16T13:47:33.886Z - Verified: macOS-only tauri.conf.json fields (hiddenTitle, titleBarStyle, trafficLightPosition) are silently ignored on Windows/Linux. CSS already handles platform differences with .platform-macos/.platform-windows/.platform-linux classes. No code changes needed.

