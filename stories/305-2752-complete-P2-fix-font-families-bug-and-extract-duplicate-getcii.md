---
id: 305-2752
title: Fix FONT_FAMILIES bug and extract duplicate getCiIcon/getCiClass helpers
status: complete
priority: P2
created: "2026-02-20T19:25:07.615Z"
updated: "2026-02-20T19:53:53.358Z"
dependencies: []
---

# Fix FONT_FAMILIES bug and extract duplicate getCiIcon/getCiClass helpers

## Problem Statement

Terminal.tsx has its own 3-font FONT_FAMILIES while settings.ts has 11 fonts — when user picks any of the 8 extra fonts, terminal silently falls back to JetBrains Mono. getCiIcon and getCiClass are duplicated in StatusBar.tsx and PrDetailPopover.tsx; StatusBar version handles fewer cases.

## Acceptance Criteria

- [ ] Verify before: confirm two FONT_FAMILIES exports and two getCiIcon definitions
- [ ] Remove FONT_FAMILIES from Terminal.tsx; import from settings.ts in getFontFamily()
- [ ] Create src/utils/ciDisplay.ts with canonical getCiIcon and getCiClass using PrDetailPopover version as base
- [ ] Remove getCiIcon/getCiClass from StatusBar.tsx and PrDetailPopover.tsx; import from ciDisplay.ts
- [ ] Run make check and all tests pass
- [ ] tsc passes with no new errors

## Files

- src/components/Terminal/Terminal.tsx
- src/stores/settings.ts
- src/components/StatusBar/StatusBar.tsx
- src/components/PrDetailPopover/PrDetailPopover.tsx
- src/utils/ciDisplay.ts

## Work Log

### 2026-02-20T19:53:53.281Z - Removed FONT_FAMILIES from Terminal.tsx (now imports from settings.ts which has all 11 fonts). Created src/utils/ciDisplay.ts with canonical getCiIcon/getCiClass. Removed local definitions from StatusBar.tsx and PrDetailPopover.tsx. All 1663 tests pass.

