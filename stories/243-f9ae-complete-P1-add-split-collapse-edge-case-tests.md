---
id: 243-f9ae
title: Add split collapse edge case tests
status: complete
priority: P1
created: "2026-02-17T11:05:06.224Z"
updated: "2026-02-17T11:09:58.758Z"
dependencies: []
---

# Add split collapse edge case tests

## Problem Statement

Split collapse tests only cover happy path. Missing boundary conditions: closing terminal not in panes, empty panes, layout already none.

## Acceptance Criteria

- [ ] Test: closing terminal NOT in panes array does not collapse split
- [ ] Test: closing terminal when panes array is empty works safely
- [ ] Test: closing terminal when layout direction is already none is safe
- [ ] npx vitest run passes

## Files

- src/__tests__/hooks/useTerminalLifecycle.test.ts

## Work Log

### 2026-02-17T11:09:53.958Z - Added 3 edge case tests: terminal not in panes, empty panes, layout already none. vitest passes.

