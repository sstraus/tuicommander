---
id: 303-6489
title: Fix non-null assertions inside Show blocks (15 sites across 3 files)
status: complete
priority: P2
created: "2026-02-20T19:25:07.614Z"
updated: "2026-02-20T19:48:51.641Z"
dependencies: []
---

# Fix non-null assertions inside Show blocks (15 sites across 3 files)

## Problem Statement

StatusBar.tsx (8 sites), PrDetailPopover.tsx (5 sites), Toolbar.tsx (2 sites) use signal()! inside Show when={signal()} children instead of the SolidJS keyed accessor pattern. TypeScript cannot narrow the type, the ! suppresses the error, and the signal is re-evaluated multiple times in JSX.

## Acceptance Criteria

- [ ] Verify before: grep for '()!' inside Show blocks in those 3 files
- [ ] StatusBar.tsx: convert github.status()! and activePrData()! to keyed Show accessors
- [ ] PrDetailPopover.tsx: convert checkSummary()! to keyed Show accessor
- [ ] Toolbar.tsx: convert prDetailTarget()! to keyed Show accessor
- [ ] No ! assertions remain inside Show children in these files
- [ ] tsc passes with no new errors
- [ ] Run make check and tests pass

## Files

- src/components/StatusBar/StatusBar.tsx
- src/components/PrDetailPopover/PrDetailPopover.tsx
- src/components/Toolbar/Toolbar.tsx

## Work Log

### 2026-02-20T19:48:51.575Z - Converted 15 ! assertions to keyed Show accessors across 3 files. tsc clean, 1663 tests pass.

