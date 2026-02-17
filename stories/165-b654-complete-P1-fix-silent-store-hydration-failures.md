---
id: 165-b654
title: Fix silent store hydration failures
status: complete
priority: P1
created: "2026-02-16T07:11:38.774Z"
updated: "2026-02-16T07:25:33.281Z"
dependencies: []
---

# Fix silent store hydration failures

## Problem Statement

Promise.all in App.tsx swallows individual store hydration failures. If one store fails, app proceeds with defaults silently. User settings and repos disappear without warning.

## Acceptance Criteria

- [ ] Use Promise.allSettled instead of Promise.all
- [ ] Warn user on partial hydration failures via status bar or toast
- [ ] Log which stores failed and why

## Files

- src/App.tsx

## Related

- SF-01

## Work Log

### 2026-02-16T07:25:33.217Z - Changed Promise.all to Promise.allSettled, added user warning via setStatusInfo on partial failures

