---
id: "062-1530"
title: "Optimize GitHub status polling"
status: complete
priority: P2
created: 2026-02-04T22:00:24.967Z
updated: 2026-02-04T22:00:24.967Z
dependencies: []
---

# Optimize GitHub status polling

## Problem Statement

GitHub status is polled at fixed intervals even for inactive repositories. This wastes API calls and can hit rate limits.

## Acceptance Criteria

- [ ] Reduce polling frequency for repos without recent activity
- [ ] Stop polling for minimized/unfocused app
- [ ] Add debounce to manual refresh triggers
- [ ] Batch multiple repo status requests where possible

## Files

- src/App.tsx
- src/stores/repositories.ts

## Work Log

