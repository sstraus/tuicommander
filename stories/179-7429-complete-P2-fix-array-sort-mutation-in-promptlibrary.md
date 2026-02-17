---
id: 179-7429
title: Fix Array.sort() mutation in promptLibrary
status: complete
priority: P2
created: "2026-02-16T07:12:19.793Z"
updated: "2026-02-16T07:32:50.496Z"
dependencies: []
---

# Fix Array.sort() mutation in promptLibrary

## Problem Statement

prompts.sort() mutates SolidJS store array in-place. Must copy before sorting to avoid corrupting reactive state.

## Acceptance Criteria

- [ ] Change to [...prompts].sort()
- [ ] Add test verifying original array is not mutated

## Files

- src/stores/promptLibrary.ts

## Related

- TS-01

## Work Log

### 2026-02-16T07:32:50.431Z - Changed prompts.sort() to [...prompts].sort() to avoid mutating SolidJS store. Added test verifying store array order is preserved after getFilteredPrompts().

