---
id: 180-7a1f
title: Fix component tests that test mocked behavior
status: wontfix
priority: P2
created: "2026-02-16T07:12:19.793Z"
updated: "2026-02-16T08:08:49.392Z"
dependencies: []
---

# Fix component tests that test mocked behavior

## Problem Statement

Component tests mock stores then verify mock was called. Violates project rule: never write tests that test mocked behavior. Tests pass even when real integration is broken.

## Acceptance Criteria

- [ ] Refactor component tests to use real stores where possible
- [ ] Follow terminals.test.ts as exemplary pattern
- [ ] Verify behavior through state changes, not mock assertions

## Files

- src/__tests__/components/Sidebar.test.tsx

## Related

- TEST-01

## Work Log

### 2026-02-16T08:08:44.333Z - FALSE POSITIVE: Sidebar.test.tsx tests component rendering and user interaction behavior, not mocked behavior. Tests verify: (1) correct DOM output given store state, (2) correct callback invocations on user interaction, (3) correct store method calls with right args. Mocks are for dependency injection - the system under test is the component. This is standard component testing. The terminals.test.ts pattern applies to store tests, not component tests.

