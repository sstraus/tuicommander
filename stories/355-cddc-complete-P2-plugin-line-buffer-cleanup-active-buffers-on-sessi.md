---
id: 355-cddc
title: "Plugin line buffer: cleanup active buffers on session close to prevent memory leak"
status: complete
priority: P2
created: "2026-02-22T16:16:43.683Z"
updated: "2026-02-23T07:38:47.898Z"
dependencies: []
---

# Plugin line buffer: cleanup active buffers on session close to prevent memory leak

## Problem Statement

pluginRegistry.ts accumulates line buffers for active plugins in a Map keyed by terminal session ID, but never removes entries when a session is closed. Long-running apps accumulate stale buffers indefinitely.

## Acceptance Criteria

- [ ] Line buffer entries are removed when the corresponding terminal session is closed or the plugin deactivated
- [ ] No memory accumulation after 100 open/close cycles in tests
- [ ] Existing plugin line-buffer tests still pass

## Files

- src/plugins/pluginRegistry.ts

## Work Log

### 2026-02-23T07:38:47.827Z - Verified complete in prior session - code in HEAD

