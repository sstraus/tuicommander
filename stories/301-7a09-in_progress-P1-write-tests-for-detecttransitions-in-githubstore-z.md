---
id: 301-7a09
title: Write tests for detectTransitions() in githubStore (zero coverage)
status: in_progress
priority: P1
created: "2026-02-20T19:23:06.084Z"
updated: "2026-02-20T19:44:56.659Z"
dependencies: []
---

# Write tests for detectTransitions() in githubStore (zero coverage)

## Problem Statement

detectTransitions() is the core engine mapping PR state changes to user notifications (6 branches: merged, closed, blocked, ci_failed, changes_requested, ready). github.test.ts has zero tests asserting any notification is emitted.

## Acceptance Criteria

- [ ] Verify before: grep github.test.ts for detectTransitions — no matches
- [ ] Add tests via updateRepoData with pre-existing branch entry
- [ ] Test: OPEN->MERGED emits merged notification
- [ ] Test: OPEN->CLOSED emits closed notification
- [ ] Test: non-conflicting->CONFLICTING emits blocked
- [ ] Test: failed checks 0->N emits ci_failed
- [ ] Test: review_decision->CHANGES_REQUESTED emits changes_requested
- [ ] Test: all three ready conditions emit ready
- [ ] Test: no re-emit when state unchanged
- [ ] Test: no notification on first update (no prior data)
- [ ] Run tests and confirm all pass

## Files

- src/stores/github.ts
- src/__tests__/stores/github.test.ts

## Work Log

