---
id: 298-d0ed
title: Fix memory leak and silent listener failures in App.tsx and useAppInit.ts
status: complete
priority: P1
created: "2026-02-20T19:23:06.082Z"
updated: "2026-02-20T19:36:15.506Z"
dependencies: []
---

# Fix memory leak and silent listener failures in App.tsx and useAppInit.ts

## Problem Statement

1) window.addEventListener("focus") at App.tsx:252 is not wrapped in onMount+onCleanup, leaking listeners on re-mount. 2) listen("head-changed") and listen("repo-changed") in useAppInit.ts:155-165 use .catch(()=>{}) swallowing errors silently — if registration fails, git-change awareness is lost with no diagnostic.

## Acceptance Criteria

- [ ] Verify before: confirm bare addEventListener at App.tsx:252 and .catch(()=>{}) at useAppInit.ts:155,165
- [ ] Wrap focus listener in onMount + onCleanup with named handler
- [ ] Replace .catch(()=>{}) on listen() with console.error logging
- [ ] Verify after: no accumulating listeners on re-mount, errors produce console output
- [ ] Run make check and existing tests pass

## Files

- src/App.tsx
- src/hooks/useAppInit.ts

## Work Log

### 2026-02-20T19:36:15.430Z - Wrapped focus listener in onMount+onCleanup, replaced empty .catch(()=>{}) on listen() with console.error, added logging to PTY cleanup and cache clearing. tsc clean, 1628 tests pass.

