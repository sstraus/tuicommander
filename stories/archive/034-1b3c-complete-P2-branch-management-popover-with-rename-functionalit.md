---
id: 034-1b3c
title: Branch management popover with rename functionality
status: pending
priority: P2
created: "2026-02-04T11:54:54.150Z"
updated: "2026-02-04T11:54:59.967Z"
dependencies: ["033-9a09"]
---

# Branch management popover with rename functionality

## Problem Statement

Currently the branch name is displayed but not interactive. Users need a way to manage branches directly from the UI - at minimum rename the current branch via a popover dialog with input field, Cancel and Rename buttons,.

## Acceptance Criteria

- [ ] Add git branch icon (Y-shaped) next to branch name in status bar
- [ ] Clicking branch name opens popover dialog
- [ ] Popover shows Rename Branch title with editable input pre-filled with current branch
- [ ] Cancel button closes popover without changes
- [ ] Rename button executes git branch -m and updates UI
- [ ] Handle errors gracefully (branch name conflicts, protected branches)
- [ ] Popover has dark theme matching app style

## Files

- index.html
- src/main.ts
- src/styles.css
- src-tauri/src/lib.rs

## Work Log

