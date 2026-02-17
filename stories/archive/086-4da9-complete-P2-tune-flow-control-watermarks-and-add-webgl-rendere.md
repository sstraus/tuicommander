---
id: "086-4da9"
title: "Tune flow control watermarks and add WebGL renderer"
status: pending
priority: P2
created: 2026-02-08T10:18:04.005Z
updated: 2026-02-08T10:18:04.005Z
dependencies: []
---

# Tune flow control watermarks and add WebGL renderer

## Problem Statement

Current HIGH_WATERMARK (1MB) is 2x the recommended xterm.js limit of 500KB. This can cause input lag under heavy output. Additionally, xterm.js default canvas renderer is 3-5x slower than the WebGL addon.

## Acceptance Criteria

- [ ] Lower HIGH_WATERMARK to 512KB and LOW_WATERMARK to 128KB
- [ ] Add @xterm/addon-webgl to package.json
- [ ] Load WebGL addon in Terminal.tsx with onContextLoss fallback to canvas
- [ ] User input remains responsive during cat of a 10MB file
- [ ] WebGL context loss gracefully falls back without crash

## Files

- src/components/Terminal/Terminal.tsx
- package.json

## Work Log

