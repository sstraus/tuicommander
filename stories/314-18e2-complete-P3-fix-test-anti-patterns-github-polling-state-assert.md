---
id: 314-18e2
title: "Fix test anti-patterns: github polling state assertions and missing edge cases"
status: complete
priority: P3
created: "2026-02-20T19:26:08.473Z"
updated: "2026-02-20T20:21:21.237Z"
dependencies: []
---

# Fix test anti-patterns: github polling state assertions and missing edge cases

## Problem Statement

github.test.ts polling tests verify invoke was called but never assert store state was updated from the response. PrDetailPopover CLOSED state CSS class not tested. Github store exponential backoff cap not tested. globToRegex utility has no tests.

## Acceptance Criteria

- [ ] Add github.test.ts test: after successful poll, verify store state actually updated with poll result data
- [ ] Add github.test.ts test: backoff interval increases after consecutive errors and caps at MAX_INTERVAL
- [ ] Add PrDetailPopover.test.tsx test: CLOSED state sets css class closed
- [ ] Create glob.test.ts: test *, **, ?, regex special char escaping, case-insensitive
- [ ] Run tests and confirm all pass

## Files

- src/__tests__/stores/github.test.ts
- src/__tests__/components/PrDetailPopover.test.tsx
- src/utils/glob.ts
- src/__tests__/utils/glob.test.ts

## Work Log

### 2026-02-20T20:21:18.908Z - Fixed backoff test (tests dead code path), added CLOSED state CSS test, created glob.test.ts with 15 tests

