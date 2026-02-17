---
id: 158-a469
title: Add CanvasAddon as WebGL fallback renderer
status: blocked
priority: P3
created: "2026-02-16T07:05:09.778Z"
updated: "2026-02-16T08:08:00.001Z"
dependencies: []
---

# Add CanvasAddon as WebGL fallback renderer

## Problem Statement

If WebGL silently fails (GPU blocklist, driver issues, context limit), xterm falls back to DOM renderer which is the slowest option. Adding @xterm/addon-canvas as middle-ground fallback gives 2-3x improvement over DOM while being more compatible than WebGL.

## Acceptance Criteria

- [ ] Add @xterm/addon-canvas dependency
- [ ] Fallback chain: WebGL → Canvas → DOM (current silent catch becomes Canvas attempt)
- [ ] Log which renderer is active to console for debugging
- [ ] Surface active renderer in Settings > General or status bar for user visibility

## Files

- src/components/Terminal/Terminal.tsx
- package.json

## Work Log

### 2026-02-16T08:07:55.881Z - BLOCKED: @xterm/addon-canvas@0.7.0 requires peer @xterm/xterm@^5.0.0 but project uses @xterm/xterm@6.0.0. No compatible version available yet. Cannot add without --legacy-peer-deps which risks runtime incompatibility.

