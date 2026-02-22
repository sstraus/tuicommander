---
id: 341-ad64
title: Unify tab systems into single TabManager abstraction
status: pending
priority: P2
created: "2026-02-21T19:11:16.302Z"
updated: "2026-02-21T19:11:22.387Z"
dependencies: ["340-3473"]
---

# Unify tab systems into single TabManager abstraction

## Problem Statement

Four separate tab systems (terminals, diffTabs, mdTabs, editorTabs) each implement add/remove/activate/reorder independently. Adding a new tab type requires creating a new store from scratch. DRY violation and contributor friction.

## Acceptance Criteria

- [ ] Analysis of commonalities and differences across the 4 tab stores
- [ ] Generic TabManager<T> store or factory that handles: add, remove, activate, reorder, dedup, clearForRepo
- [ ] Existing 4 tab stores refactored to use TabManager internally
- [ ] Adding a new tab type requires only content definition, not a new store
- [ ] All existing tab tests pass with no behavior changes
- [ ] Tab bar renders all tab types uniformly

## Files

- src/stores/terminals.ts
- src/stores/diffTabs.ts
- src/stores/mdTabs.ts
- src/stores/editorTabs.ts
- src/components/TabBar/TabBar.tsx

## Related

- god component decomposition

## Work Log

