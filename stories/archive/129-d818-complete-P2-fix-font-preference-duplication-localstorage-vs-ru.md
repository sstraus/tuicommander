---
id: 129-d818
title: Fix font preference duplication (localStorage vs Rust config)
status: complete
priority: P2
created: "2026-02-15T21:17:30.374Z"
updated: "2026-02-15T22:16:20.040Z"
dependencies: []
---

# Fix font preference duplication (localStorage vs Rust config)

## Problem Statement

Font is stored in both localStorage (tui-commander-font) and Rust config. Two sources of truth can diverge.

## Acceptance Criteria

- [ ] Use Rust config.json as single source of truth for font
- [ ] Remove font from localStorage persistence in settings.ts
- [ ] Load font from Rust on startup via invoke

## Files

- src/stores/settings.ts
- src-tauri/src/lib.rs

## Work Log

### 2026-02-15T22:16:16.670Z - Removed FONT from STORAGE_KEYS in settings.ts. Font no longer loaded from or written to localStorage. setFont() now persists to Rust config.json via invoke('load_config') + invoke('save_config'). Added loadFontFromConfig() async method. App.tsx onMount calls loadFontFromConfig(). All 921 tests pass.

