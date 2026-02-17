---
id: 131-cd09
title: Stop persisting session-ephemeral tab state to localStorage
status: complete
priority: P3
created: "2026-02-15T21:17:30.375Z"
updated: "2026-02-15T22:19:35.697Z"
dependencies: []
---

# Stop persisting session-ephemeral tab state to localStorage

## Problem Statement

diffTabs and markdownTabs persist open tabs to localStorage. These reference transient file state that may be stale after restart.

## Acceptance Criteria

- [ ] Remove loadState/saveState from diffTabs store
- [ ] Remove loadState/saveState from markdownTabs store
- [ ] Tabs start empty on app launch

## Files

- src/stores/diffTabs.ts
- src/stores/markdownTabs.ts

## Work Log

### 2026-02-15T22:19:35.633Z - Removed loadState/saveState and localStorage persistence from diffTabs.ts and mdTabs.ts. Both stores now start with empty state. Simplified App.tsx onMount cleanup. All 923 tests pass.

