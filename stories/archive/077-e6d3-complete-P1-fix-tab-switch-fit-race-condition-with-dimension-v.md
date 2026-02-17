---
id: 077-e6d3
title: Fix tab-switch fit() race condition with dimension validation
status: complete
priority: P1
created: "2026-02-08T09:44:21.529Z"
updated: "2026-02-08T09:50:49.182Z"
dependencies: []
---

# Fix tab-switch fit() race condition with dimension validation

## Problem Statement

When switching tabs, the CSS transitions from display:none to display:block. The createEffect calls fit() in requestAnimationFrame. If the CSS transition has not completed by the time rAF fires, fit() measures dimensions=0, corrupting xterm internal state. The ResizeObserver is disconnected while inactive and reconnected on activation, creating a window where resize events are missed. This manifests as garbled text when switching terminal tabs.

## Acceptance Criteria

- [ ] After activating a terminal pane, validate container dimensions (offsetWidth > 0 and offsetHeight > 0) before calling fit()
- [ ] If dimensions are zero, retry with rAF loop and max retries until container is visible
- [ ] Add dimension validation guard in ResizeObserver callback to skip fit() if container is zero-sized
- [ ] Verify with manual test: rapidly switch between 3+ terminal tabs, confirm no garbled rendering

## Files

- src/components/Terminal/Terminal.tsx (createEffect watching activeId, ResizeObserver)

## Related

- 066-3a86

## Work Log

