---
id: 291-1634
title: Hydration failure logged at console.debug only
status: complete
priority: P2
created: "2026-02-20T13:57:16.829Z"
updated: "2026-02-20T14:13:14.242Z"
dependencies: []
---

# Hydration failure logged at console.debug only

## Problem Statement

App starts with empty sidebar no user feedback at repositories.ts:199-202.

## Acceptance Criteria

- [ ] Escalate to console.error

## Files

- src/stores/repositories.ts

## Work Log

### 2026-02-20T14:13:14.172Z - Escalated hydration error from console.debug to console.error

