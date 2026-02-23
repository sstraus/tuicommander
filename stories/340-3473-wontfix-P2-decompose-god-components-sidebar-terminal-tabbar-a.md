---
id: 340-3473
title: Decompose god components (Sidebar, Terminal, TabBar, App.tsx)
status: wontfix
priority: P2
created: "2026-02-21T19:11:16.300Z"
updated: "2026-02-23T08:24:22.079Z"
dependencies: ["339-d200"]
---

# Decompose god components (Sidebar, Terminal, TabBar, App.tsx)

## Problem Statement

Sidebar (968 lines), Terminal (753), FileBrowser (466), TabBar (446), App.tsx (999 lines, 15 store imports) are too large. High coupling makes it hard for contributors to add features without understanding the entire component. New panel types require editing App.tsx directly.

## Acceptance Criteria

- [ ] Analysis of each god component to identify extractable sub-components
- [ ] Sidebar split into: RepoList, BranchList, GroupSection, DragDropHandler (<200 lines each)
- [ ] Terminal split into: xterm lifecycle vs UI chrome vs split-pane logic
- [ ] App.tsx panel orchestration extracted to PanelManager store/context
- [ ] App.tsx reduced below 500 lines
- [ ] No god component exceeds 300 lines
- [ ] All existing tests pass, no visual regressions

## Files

- src/App.tsx
- src/components/Sidebar/Sidebar.tsx
- src/components/Terminal/Terminal.tsx
- src/components/TabBar/TabBar.tsx
- src/components/FileBrowserPanel/FileBrowserPanel.tsx

## Related

- CSS modularization
- unified tab system

## Work Log

### 2026-02-23T08:24:22.013Z - Deferred: aggregated into ideas/codebase-decomposition.md — pure refactoring, no user-facing value now

