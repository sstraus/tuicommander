---
id: 004-711b
title: Multi-repository sidebar
status: complete
priority: P2
created: "2026-02-04T10:50:24.105Z"
updated: "2026-02-04T11:08:37.126Z"
dependencies: []
---

# Multi-repository sidebar

## Problem Statement

Sidebar needs to show multiple repos with branch info and status icons (★ starred, ⚠ warnings). Currently generic terminal list.

## Acceptance Criteria

- [ ] Add repository to sidebar (path + display name)
- [ ] Show current branch per repo
- [ ] Status icons (clean, dirty, conflicts)
- [ ] Click to create terminal in that repo

## Files

- src/main.ts:256-270
- index.html:14-27
- src/styles.css:46-146

## Work Log

