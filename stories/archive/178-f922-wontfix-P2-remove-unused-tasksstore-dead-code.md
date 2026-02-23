---
id: 178-f922
title: Remove unused tasksStore (dead code)
status: wontfix
priority: P2
created: "2026-02-16T07:12:19.792Z"
updated: "2026-02-16T07:31:46.891Z"
dependencies: []
---

# Remove unused tasksStore (dead code)

## Problem Statement

244 lines with no references outside tests and barrel export. Never used in the app. Pure dead code.

## Acceptance Criteria

- [ ] Verify no runtime references to tasksStore
- [ ] Remove src/stores/tasks.ts
- [ ] Remove src/__tests__/stores/tasks.test.ts
- [ ] Remove export from src/stores/index.ts

## Files

- src/stores/tasks.ts
- src/__tests__/stores/tasks.test.ts
- src/stores/index.ts

## Related

- SIMP-03

## Work Log

### 2026-02-16T07:31:46.828Z - FALSE POSITIVE: tasksStore is actively used by TaskQueuePanel component (imported in TaskQueuePanel.tsx and App.tsx). Not dead code.

