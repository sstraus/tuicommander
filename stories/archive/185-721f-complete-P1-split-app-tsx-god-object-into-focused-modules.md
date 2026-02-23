---
id: 185-721f
title: Split App.tsx God Object into focused modules
status: complete
priority: P1
created: "2026-02-16T07:17:10.812Z"
updated: "2026-02-16T11:24:13.485Z"
dependencies: []
---

# Split App.tsx God Object into focused modules

## Problem Statement

App.tsx is 1771 lines managing terminals, git ops, keyboard shortcuts, split panes, settings, dialogs, and 13 stores. Violates SRP and is untestable.

## Acceptance Criteria

- [ ] Extract terminal lifecycle management into TerminalOrchestrator
- [ ] Extract git operations coordination into GitOrchestrator
- [ ] Extract keyboard shortcut handling into KeyboardManager hook
- [ ] App.tsx should be thin shell delegating to extracted modules
- [ ] All existing tests still pass

## Files

- src/App.tsx

## Related

- ARCH-01

## Work Log

### 2026-02-16T11:24:13.404Z - Extracted 7 hooks from App.tsx: useTerminalLifecycle, useGitOperations, useAppLazygit, useDictation, useQuickSwitcher, useSplitPanes, useKeyboardShortcuts, plus initApp. App.tsx reduced from 1783 to 746 lines. 119 new tests added (1036→1155). All tests pass, TypeScript clean.

