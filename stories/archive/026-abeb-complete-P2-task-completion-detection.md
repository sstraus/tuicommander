---
id: 026-abeb
title: Task completion detection
status: pending
priority: P2
created: "2026-02-04T11:31:14.252Z"
updated: "2026-02-04T11:53:03.895Z"
dependencies: ["033-9a09"]
---

# Task completion detection

## Problem Statement

Need to detect when an agent task is complete to trigger next task or update UI. Agents may signal completion via patterns or exit codes.

## Acceptance Criteria

- [ ] Detect COMPLETE signal pattern in output
- [ ] Handle exit code 0 as implicit completion
- [ ] Emit task:completed event
- [ ] Update task status in tracker
- [ ] Trigger next task selection

## Files

- src/task-detector.ts
- src/main.ts

## Work Log

