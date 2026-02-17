---
id: 172-897b
title: Memoize terminalIds() computation
status: complete
priority: P2
created: "2026-02-16T07:12:19.790Z"
updated: "2026-02-16T07:37:08.417Z"
dependencies: []
---

# Memoize terminalIds() computation

## Problem Statement

O(n-squared) filter called on every TabBar render without createMemo wrapping.

## Acceptance Criteria

- [ ] Wrap terminalIds() in createMemo()
- [ ] Verify TabBar re-renders only when terminal list changes

## Files

- src/App.tsx

## Related

- PERF-04

## Work Log

### 2026-02-16T07:37:08.351Z - Wrapped terminalIds() in createMemo() in App.tsx. The filter+includes computation now only re-runs when getActiveTerminals() or getIds() change.

