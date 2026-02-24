---
id: 241-fcde
title: Add warning for unknown theme fallback
status: complete
priority: P1
created: "2026-02-17T11:05:06.223Z"
updated: "2026-02-17T11:09:58.734Z"
dependencies: []
---

# Add warning for unknown theme fallback

## Problem Statement

applyAppTheme() silently falls back to vscode-dark for unknown theme names with no warning logged, making debugging impossible.

## Acceptance Criteria

- [ ] applyAppTheme() logs console.warn when theme key not found in APP_THEMES
- [ ] Add test verifying warning is logged for unknown theme
- [ ] npx vitest run passes

## Files

- src/themes.ts
- src/__tests__/themes.test.ts

## Work Log

### 2026-02-17T11:09:53.823Z - Added console.warn in applyAppTheme for unknown theme keys. Added test verifying warning. vitest passes.

