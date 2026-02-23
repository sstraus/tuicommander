---
id: 360-3585
title: "Fix O(n) SolidJS hotspots: repoPathForTerminal and updateSavedTerminals"
status: complete
priority: P3
created: "2026-02-22T16:16:43.686Z"
updated: "2026-02-23T08:23:43.859Z"
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

### 2026-02-23T08:23:43.799Z - Added getRepoPathForTerminal() to repositoriesStore (O(1) from store, eliminates duplicate inline scan in TerminalArea.tsx). Removed dead updateSavedTerminals() (superseded by snapshotTerminals since story 307). 4 new tests, all 1955 pass.

