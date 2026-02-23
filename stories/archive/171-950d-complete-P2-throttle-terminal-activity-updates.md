---
id: 171-950d
title: Throttle terminal activity updates
status: complete
priority: P2
created: "2026-02-16T07:12:19.789Z"
updated: "2026-02-16T07:41:23.341Z"
dependencies: []
---

# Throttle terminal activity updates

## Problem Statement

Every PTY data chunk updates store triggering SolidJS reactivity. High-throughput commands emit 100+ chunks/sec causing unnecessary re-renders.

## Acceptance Criteria

- [ ] Throttle activity flag updates to max 10Hz
- [ ] Use local state with debounced store sync

## Files

- src/components/Terminal/Terminal.tsx

## Related

- PERF-03

## Work Log

### 2026-02-16T07:41:23.278Z - Added local activityFlagged/busyFlagged flags to avoid redundant terminalsStore.update() calls per PTY data chunk. Activity flagged once per inactive period, reset via createEffect when terminal becomes active. Busy flagged once until idle timer fires.

