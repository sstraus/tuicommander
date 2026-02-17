---
id: 248-4757
title: Replace manual CSS_VAR_MAP with algorithmic camelToKebab
status: complete
priority: P3
created: "2026-02-17T11:43:00.220Z"
updated: "2026-02-17T11:44:20.565Z"
dependencies: []
---

# Replace manual CSS_VAR_MAP with algorithmic camelToKebab

## Problem Statement

CSS_VAR_MAP manually maps 13 camelCase property names to kebab-case CSS variable names. This transformation is algorithmic and the manual map requires maintenance when IAppTheme changes.

## Acceptance Criteria

- [ ] Replace CSS_VAR_MAP with a camelToKebab function that derives CSS var names from IAppTheme keys
- [ ] Remove the 15-line CSS_VAR_MAP constant
- [ ] Existing theme tests still pass
- [ ] npx vitest run passes and npx tsc --noEmit passes

## Files

- src/themes.ts
- src/__tests__/themes.test.ts

## Work Log

### 2026-02-17T11:44:20.418Z - Replaced CSS_VAR_MAP with camelToKebab helper. Removed 15 lines. vitest 1495 passed.

