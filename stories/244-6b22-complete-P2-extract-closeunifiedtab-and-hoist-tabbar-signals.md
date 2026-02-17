---
id: 244-6b22
title: Extract closeUnifiedTab and hoist TabBar signals
status: complete
priority: P2
created: "2026-02-17T11:05:06.225Z"
updated: "2026-02-17T11:09:58.766Z"
dependencies: []
---

# Extract closeUnifiedTab and hoist TabBar signals

## Problem Statement

Close-both-panes logic duplicated in TabBar onClick and onAuxClick. layout() and isUnified() computed per-tab inside For loop instead of once outside.

## Acceptance Criteria

- [ ] Extract closeUnifiedTab() function used by both click and auxclick handlers
- [ ] Hoist layout() and isUnified() signals outside the For loop
- [ ] Existing TabBar tests still pass
- [ ] npx vitest run passes and npx tsc --noEmit passes

## Files

- src/components/TabBar/TabBar.tsx

## Work Log

### 2026-02-17T11:09:54.030Z - Hoisted layout() and isUnifiedMode() outside For loop. Extracted handleCloseTab() for dedup. vitest passes.

