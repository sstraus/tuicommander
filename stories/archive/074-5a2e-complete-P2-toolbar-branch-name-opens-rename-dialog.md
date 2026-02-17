---
id: 074-5a2e
title: Toolbar branch name opens rename dialog
status: complete
priority: P2
created: "2026-02-07T21:53:42.828Z"
updated: "2026-02-07T22:25:08.208Z"
dependencies: []
---

# Toolbar branch name opens rename dialog

## Problem Statement

The branch name displayed in the toolbar center (e.g. "main" with the git branch icon) is purely decorative — it cannot be clicked. Users expect clicking it to open the rename branch dialog, similar to how a competitor handles it. The RenameBranchDialog component already exists and works from the sidebar context menu. This story wires it up as a click target on the toolbar branch name.

## Acceptance Criteria

- [ ] Clicking the branch name in the toolbar opens the RenameBranchDialog
- [ ] The dialog is pre-filled with the current active branch name
- [ ] Cursor changes to pointer on hover over the branch name
- [ ] Visual hover feedback (subtle background change) to indicate clickability
- [ ] Renaming via the toolbar dialog updates the branch everywhere (store, sidebar, toolbar)

## Files

- src/components/Toolbar/Toolbar.tsx
- src/App.tsx

## Work Log

