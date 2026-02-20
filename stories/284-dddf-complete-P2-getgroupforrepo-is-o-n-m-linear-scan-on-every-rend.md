---
id: 284-dddf
title: getGroupForRepo is O(n*m) linear scan on every render
status: complete
priority: P2
created: "2026-02-20T13:57:16.824Z"
updated: "2026-02-20T14:13:14.388Z"
dependencies: []
---

# getGroupForRepo is O(n*m) linear scan on every render

## Problem Statement

Object.values(groups).find(g => g.repoOrder.includes(repoPath)) at repositories.ts:605-607. Called 3-4 times per repo per render.

## Acceptance Criteria

- [ ] Inverted index repoPath->groupId maintained
- [ ] O(1) lookups for group membership

## Files

- src/stores/repositories.ts

## Work Log

### 2026-02-20T14:13:14.316Z - Changed getGroupForRepo to use early-return loop with indexOf instead of find+includes

