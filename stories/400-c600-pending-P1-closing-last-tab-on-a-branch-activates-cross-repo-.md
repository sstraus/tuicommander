---
id: "400-c600"
title: "Closing last tab on a branch activates cross-repo terminal"
status: pending
priority: P1
created: 2026-02-26T14:16:21.868Z
updated: 2026-02-26T14:16:21.868Z
dependencies: []
---

# Closing last tab on a branch activates cross-repo terminal

## Problem Statement

When closing the last terminal tab on repo1 branch, terminalsStore.remove() picks any remaining terminal globally (terminals.ts:109) instead of scoping to the active branch. The fallback in closeTerminal (useTerminalLifecycle.ts:152) also accepts cross-repo terminals. Result: user sees repo2 shell without tab bar after closing repo1 last tab.

## Acceptance Criteria

- [ ] terminalsStore.remove() sets activeId to null when no same-branch terminals remain (not a random global terminal)
- [ ] closeTerminal fallback chain in useTerminalLifecycle.ts:150-152 never selects a terminal outside the active branch
- [ ] When last tab on a branch is closed, activeId becomes null and UI shows empty state (not another repo shell)
- [ ] handleTerminalFocus cross-repo guard (line 307) is not bypassed by the store-level fallback

## Files

- src/stores/terminals.ts
- src/hooks/useTerminalLifecycle.ts

## Work Log

