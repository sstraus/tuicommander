---
id: 013-060b
title: Parallel agent orchestration
status: complete
priority: P2
created: "2026-02-04T10:50:24.118Z"
updated: "2026-02-04T11:17:03.091Z"
dependencies: []
---

# Parallel agent orchestration

## Problem Statement

Support 50+ agents running simultaneously with proper resource limits and monitoring.

## Acceptance Criteria

- [ ] Spawn up to 50 PTY sessions
- [ ] Monitor CPU/memory per agent
- [ ] Kill agent if exceeds limits
- [ ] Queue system when at capacity

## Files

- src-tauri/src/lib.rs:10-19

## Work Log

