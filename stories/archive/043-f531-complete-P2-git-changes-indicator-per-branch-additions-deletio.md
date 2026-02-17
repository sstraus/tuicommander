---
id: 043-f531
title: Git changes indicator per branch (+additions -deletions)
status: complete
priority: P2
created: "2026-02-04T14:45:57.906Z"
updated: "2026-02-07T22:25:07.893Z"
dependencies: []
---

# Git changes indicator per branch (+additions -deletions)

## Problem Statement

Need to show uncommitted changes count per branch in green/red format like +172 -42.

## Acceptance Criteria

- [ ] Fetch git diff stats (additions/deletions) per branch
- [ ] Display +N in green, -N in red below branch name
- [ ] Update periodically or on focus

## Files

- src/components/Sidebar/Sidebar.tsx
- src-tauri/src/lib.rs

## Work Log

