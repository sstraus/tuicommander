---
id: "088-45fc"
title: "Add PTY session metrics for observability"
status: pending
priority: P3
created: 2026-02-08T10:18:04.006Z
updated: 2026-02-08T10:18:04.006Z
dependencies: []
---

# Add PTY session metrics for observability

## Problem Statement

No visibility into PTY subsystem health. When terminals misbehave at scale (50+ sessions), there is no way to diagnose whether the issue is spawn failures, high throughput, paused readers, or leaked sessions without adding console logs and rebuilding.

## Acceptance Criteria

- [ ] Create SessionMetrics struct with AtomicUsize counters: total_spawned, failed_spawns, active_sessions, bytes_emitted, pauses_triggered
- [ ] Increment counters at appropriate points in PTY lifecycle
- [ ] Add get_session_metrics Tauri command returning JSON metrics
- [ ] Add getMetrics() to usePty hook
- [ ] Zero runtime overhead when not queried (AtomicUsize load is free)

## Files

- src-tauri/src/lib.rs
- src/hooks/usePty.ts

## Work Log

