---
id: 247-555d
title: Add missing test for unified mode close cleanup
status: complete
priority: P2
created: "2026-02-17T11:35:07.641Z"
updated: "2026-02-17T11:37:30.727Z"
dependencies: []
---

# Add missing test for unified mode close cleanup

## Problem Statement

Test verifies handleClose is called for both terminals in unified mode but does not verify layout state is cleaned up after close.

## Acceptance Criteria

- [ ] Test verifies layout direction is none and panes is empty after unified close of both terminals
- [ ] npx vitest run passes

## Files

- src/__tests__/components/TabBar.test.tsx

## Work Log

### 2026-02-17T11:37:30.395Z - Added test with real close handler verifying layout collapses to none after unified close. vitest 1494 passed.

