---
id: 309-17c3
title: Write tests for Toolbar PR bell, plan file button, and uiStore planFilePath
status: complete
priority: P2
created: "2026-02-20T19:25:07.618Z"
updated: "2026-02-20T20:06:13.815Z"
dependencies: []
---

# Write tests for Toolbar PR bell, plan file button, and uiStore planFilePath

## Problem Statement

Toolbar PR notification bell (toggle, dismiss, click-to-open PrDetailPopover) and plan file button (render, click, display name) are completely untested. uiStore.setPlanFilePath() and clearPlanFile() have no tests.

## Acceptance Criteria

- [ ] Verify before: grep Toolbar.test.tsx for notification bell and plan file -- no matches
- [ ] Add Toolbar tests: bell renders when active notifications exist, hides when none; popover toggles on click; dismiss all and individual dismiss work
- [ ] Add Toolbar tests: plan button renders when planFilePath set, hidden when null; click calls onOpenPlan; X calls clearPlanFile()
- [ ] Add Toolbar tests: planDisplayName strips path prefix and .md/.mdx extension
- [ ] Add ui.test.ts tests: planFilePath defaults null; setPlanFilePath sets it; clearPlanFile resets to null
- [ ] Run tests and confirm all pass

## Files

- src/components/Toolbar/Toolbar.tsx
- src/__tests__/components/Toolbar.test.tsx
- src/stores/ui.ts
- src/__tests__/stores/ui.test.ts

## Work Log

### 2026-02-20T20:06:13.745Z - Added 18 bell/plan tests to Toolbar.test.tsx (PR notif bell show/hide, count, popover toggle, dismiss all/individual, PrDetailPopover trigger; plan button show/hide, display name, .md/.mdx extension strip, onOpenPlan, clearPlanFile). Added 4 planFilePath tests to ui.test.ts. All 1626 tests pass.

