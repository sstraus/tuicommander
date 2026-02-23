---
id: 188-a2f8
title: Simplify wrapper stores that duplicate class APIs
status: wontfix
priority: P3
created: "2026-02-16T07:17:10.815Z"
updated: "2026-02-16T07:58:13.043Z"
dependencies: []
---

# Simplify wrapper stores that duplicate class APIs

## Problem Statement

errorHandling.ts and notifications.ts wrap class instances (ErrorHandler, notificationManager) but expose identical APIs. Either use classes directly or remove them.

## Acceptance Criteria

- [ ] Remove one layer of abstraction (store wrapper or underlying class)
- [ ] Maintain same public API for consumers

## Files

- src/stores/errorHandling.ts
- src/stores/notifications.ts

## Related

- SIMP-05

## Work Log

### 2026-02-16T07:58:12.975Z - WONTFIX: The two-layer pattern (class + store) is intentional. Classes (ErrorHandler, notificationManager) handle imperative logic (audio, retry decisions). Stores provide SolidJS reactive state that components observe. Removing either layer loses functionality. Not pure duplication.

