---
id: 313-4642
title: "Minor: memoize contextMenuItems, fix CSS naming, openUrl logging, AgentType validation"
status: complete
priority: P3
created: "2026-02-20T19:26:08.472Z"
updated: "2026-02-20T20:21:21.225Z"
dependencies: []
---

# Minor: memoize contextMenuItems, fix CSS naming, openUrl logging, AgentType validation

## Problem Statement

getContextMenuItems() in App.tsx creates new array on every render; should be createMemo. --font-xxs in styles.css:4541 should be --font-2xs for consistency. ci-ring CSS classes on SVG elements serve no purpose. openUrl().catch with empty block in StatusBar.tsx and PrDetailPopover.tsx. agentFallback.ts:97-98 casts backend strings to AgentType without validation.

## Acceptance Criteria

- [ ] Wrap getContextMenuItems in createMemo in App.tsx
- [ ] Replace var(--font-xxs, 9px) with var(--font-2xs) at styles.css:4541
- [ ] Remove class ci-ring from CiRing.tsx and className from ciRingSegments.ts
- [ ] Add console.error logging to openUrl().catch in StatusBar.tsx and PrDetailPopover.tsx
- [ ] Add runtime validation for AgentType cast in agentFallback.ts:97-98
- [ ] Run make check and tests pass

## Files

- src/App.tsx
- src/styles.css
- src/components/ui/CiRing.tsx
- src/utils/ciRingSegments.ts
- src/components/StatusBar/StatusBar.tsx
- src/components/PrDetailPopover/PrDetailPopover.tsx
- src/stores/agentFallback.ts

## Work Log

### 2026-02-20T20:21:18.838Z - Fixed font var, removed ci-ring className, added openUrl error logging, guarded AgentType casts with isAgentType()

