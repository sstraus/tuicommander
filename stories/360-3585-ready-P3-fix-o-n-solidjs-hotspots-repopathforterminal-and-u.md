---
id: 360-3585
title: "Fix O(n) SolidJS hotspots: repoPathForTerminal and updateSavedTerminals"
status: ready
priority: P3
created: "2026-02-22T16:16:43.686Z"
updated: "2026-02-23T07:52:04.048Z"
dependencies: []
---

# Fix O(n) SolidJS hotspots: repoPathForTerminal and updateSavedTerminals

## Problem Statement

repoPathForTerminal() and updateSavedTerminals() in terminalsStore perform O(n) linear scans on every reactive update. With many terminals these create visible jank as SolidJS re-runs them frequently.

## Acceptance Criteria

- [ ] repoPathForTerminal lookup uses a derived Map keyed by terminal ID (O(1))
- [ ] updateSavedTerminals batch-writes in a single pass
- [ ] Performance: no measurable difference in reactive timing with 50 terminals

## Files

- src/stores/terminalsStore.ts

## Work Log

