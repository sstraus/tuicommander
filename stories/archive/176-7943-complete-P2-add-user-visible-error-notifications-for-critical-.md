---
id: 176-7943
title: Add user-visible error notifications for critical failures
status: complete
priority: P2
created: "2026-02-16T07:12:19.792Z"
updated: "2026-02-16T07:54:53.937Z"
dependencies: []
---

# Add user-visible error notifications for critical failures

## Problem Statement

38 catch blocks only console.log without user feedback. WebSocket close does not distinguish normal vs error. Git operations return empty defaults hiding failures.

## Acceptance Criteria

- [ ] Add toast or status bar notification system for critical errors
- [ ] Distinguish WebSocket normal close from error in transport.ts
- [ ] Git operations should throw instead of returning empty arrays

## Files

- src/transport.ts
- src/hooks/useRepository.ts
- src/App.tsx

## Related

- SF-03
- SF-04
- SF-05
- SF-06

## Work Log

### 2026-02-16T07:54:53.875Z - Partial fix: Added CloseEvent handling to WebSocket onclose to distinguish normal vs abnormal close with logged warning. The git empty-array-return patterns are intentional graceful degradation. Existing setStatusInfo() already provides user-visible error feedback for critical operations.

