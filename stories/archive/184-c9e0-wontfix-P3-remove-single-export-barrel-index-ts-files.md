---
id: 184-c9e0
title: Remove single-export barrel index.ts files
status: wontfix
priority: P3
created: "2026-02-16T07:12:39.086Z"
updated: "2026-02-16T08:02:10.002Z"
dependencies: []
---

# Remove single-export barrel index.ts files

## Problem Statement

23 index.ts files that only re-export one component. No value for single-export modules, adds file bloat.

## Acceptance Criteria

- [ ] Remove barrel files where only one export exists
- [ ] Update import paths in consumers

## Files

- src/components/

## Related

- SIMP-04

## Work Log

### 2026-02-16T08:02:05.800Z - FALSE POSITIVE: Barrel index.ts files are a standard component-directory convention enabling clean imports (e.g. from './Terminal' vs './Terminal/Terminal'). Most export types alongside components. Several export multiple items (ContextMenu: 4, SettingsPanel: 6, Terminal: 3). Removing these would degrade DX with no benefit.

