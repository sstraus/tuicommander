---
id: 242-e2f7
title: Add useSplitPanes unified mode tests
status: complete
priority: P1
created: "2026-02-17T11:05:06.224Z"
updated: "2026-02-17T11:09:58.747Z"
dependencies: []
---

# Add useSplitPanes unified mode tests

## Problem Statement

useSplitPanes unified mode branch-skip logic has ZERO test coverage despite being core feature logic.

## Acceptance Criteria

- [ ] New test file src/__tests__/hooks/useSplitPanes.test.ts
- [ ] Test: separate mode adds split terminal to branch
- [ ] Test: unified mode does NOT add split terminal to branch
- [ ] Test: split with no active repo works without error
- [ ] npx vitest run passes

## Files

- src/__tests__/hooks/useSplitPanes.test.ts
- src/hooks/useSplitPanes.ts

## Work Log

### 2026-02-17T11:09:53.893Z - Created useSplitPanes.test.ts with tests for separate mode branch tracking, unified mode branch skip, and no-repo split. vitest passes.

