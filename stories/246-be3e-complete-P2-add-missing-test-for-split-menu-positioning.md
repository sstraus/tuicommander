---
id: 246-be3e
title: Add missing test for split menu positioning
status: complete
priority: P2
created: "2026-02-17T11:35:07.639Z"
updated: "2026-02-17T11:37:30.472Z"
dependencies: []
---

# Add missing test for split menu positioning

## Problem Statement

TabBar new tab menu test verifies items exist but does not verify the menu opens at the correct position below the button via getBoundingClientRect.

## Acceptance Criteria

- [ ] Test verifies openAt receives correct coordinates from getBoundingClientRect
- [ ] npx vitest run passes

## Files

- src/__tests__/components/TabBar.test.tsx

## Work Log

### 2026-02-17T11:37:30.322Z - Added test verifying menu opens at getBoundingClientRect coordinates (left, bottom+4). vitest 1494 passed.

