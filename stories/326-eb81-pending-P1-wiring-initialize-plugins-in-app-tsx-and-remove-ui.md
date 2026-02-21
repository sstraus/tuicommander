---
id: 326-eb81
title: "Wiring: initialize plugins in App.tsx and remove uiStore plan state"
status: pending
priority: P1
created: "2026-02-21T09:35:19.156Z"
updated: "2026-02-21T09:35:42.554Z"
dependencies: ["322-f60f", "323-46a4", "324-9b46", "325-90ef"]
---

# Wiring: initialize plugins in App.tsx and remove uiStore plan state

## Problem Statement

The plugin system needs to be initialized at app startup. planFilePath state in uiStore and the direct uiStore.setPlanFilePath call in Terminal.tsx must be removed now that the plan plugin owns that responsibility.

## Acceptance Criteria

- [ ] src/plugins/index.ts exports BUILTIN_PLUGINS array and an initPlugins() function
- [ ] App.tsx calls initPlugins() at startup, registering plan and stories plugins
- [ ] App.tsx onOpenPlan prop wiring removed (plan plugin handles opening via virtual tab)
- [ ] App.tsx adds handler for activity item clicks that opens virtual markdown tabs
- [ ] uiStore.ts: planFilePath state, setPlanFilePath, clearPlanFile removed
- [ ] Terminal.tsx: direct uiStore.setPlanFilePath(parsed.path) in plan-file case removed (plugin now handles via dispatchStructuredEvent)
- [ ] simulator.ts: plan simulation updated to go through pluginRegistry instead of uiStore
- [ ] No TypeScript errors remain referencing planFilePath
- [ ] Full test suite passes

## Files

- src/plugins/index.ts
- src/App.tsx
- src/stores/ui.ts
- src/components/Terminal/Terminal.tsx
- src/dev/simulator.ts

## Related

- 322-f60f
- 323-46a4
- 324-9b46
- 325-90ef

## Work Log

