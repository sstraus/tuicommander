---
id: 191-4a6a
title: Add test coverage for App.tsx critical paths
status: complete
priority: P3
created: "2026-02-16T07:17:10.816Z"
updated: "2026-02-16T11:35:04.403Z"
dependencies: ["185-721f"]
---

# Add test coverage for App.tsx critical paths

## Problem Statement

App.tsx is 1771 lines with zero test coverage. Contains 28 keyboard shortcuts, terminal cleanup logic, and PTY routing with no verification.

## Acceptance Criteria

- [ ] Add tests for keyboard shortcut handling
- [ ] Add tests for store hydration flow
- [ ] Add tests for terminal cleanup on quit

## Files

- src/App.tsx

## Related

- TEST-03

## Work Log

### 2026-02-16T11:35:00.461Z - All 3 acceptance criteria met by hook extraction (Story 185): 28 keyboard shortcut tests, 10 initApp tests covering hydration/cleanup, plus 81 more tests across other hooks. Total: 119 new tests added.

