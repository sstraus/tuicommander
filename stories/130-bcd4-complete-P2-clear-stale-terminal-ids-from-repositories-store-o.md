---
id: 130-bcd4
title: Clear stale terminal IDs from repositories store on startup
status: complete
priority: P2
created: "2026-02-15T21:17:30.374Z"
updated: "2026-02-15T22:18:13.449Z"
dependencies: []
---

# Clear stale terminal IDs from repositories store on startup

## Problem Statement

repositories.ts persists branch.terminals to localStorage. Terminal IDs are session-ephemeral and invalid after restart.

## Acceptance Criteria

- [ ] Clear all branch.terminals arrays on app startup
- [ ] OR exclude terminals from localStorage serialization

## Files

- src/stores/repositories.ts
- src/App.tsx

## Work Log

### 2026-02-15T22:18:13.377Z - Excluded terminals from localStorage serialization in persist(). Clear stale terminal IDs on store initialization. Removed redundant clearing loop from App.tsx onMount. All 923 tests pass.

