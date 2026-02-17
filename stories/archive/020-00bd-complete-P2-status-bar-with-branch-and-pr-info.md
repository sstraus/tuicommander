---
id: 020-00bd
title: Status bar with branch and PR info
status: complete
priority: P2
created: "2026-02-04T11:09:48.389Z"
updated: "2026-02-04T11:20:49.954Z"
dependencies: []
---

# Status bar with branch and PR info

## Problem Statement

Need status bar showing current branch, PR status, and ready-to-create-PR indicator like in the reference UI.

## Acceptance Criteria

- [ ] Display current git branch in status bar
- [ ] Show PR status if branch has open PR
- [ ] Ready to create PR indicator when branch is ahead
- [ ] Refresh on git operations

## Files

- src/main.ts
- index.html
- src-tauri/src/lib.rs

## Work Log

