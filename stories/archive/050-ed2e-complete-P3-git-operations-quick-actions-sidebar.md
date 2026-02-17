---
id: "050-ed2e"
title: "Git operations quick actions sidebar"
status: complete
priority: P3
created: 2026-02-04T17:06:31.284Z
updated: 2026-02-04T17:06:31.284Z
dependencies: []
---

# Git operations quick actions sidebar

## Problem Statement

GitOperationsPanel (Cmd+Shift+G) requires opening a modal for common git operations like pull/push/stash. Users want faster access without modal interruption. Need quick action buttons in sidebar.

## Acceptance Criteria

- [ ] Add collapsible Git Quick Actions section in sidebar below repositories
- [ ] Show buttons: Pull, Push, Fetch, Stash, Pop
- [ ] Buttons execute on currently selected repo/branch
- [ ] Show loading spinner during operation
- [ ] Display success/error toast notifications
- [ ] Operations execute in background without blocking UI

## Files

- src/components/Sidebar/Sidebar.tsx
- src/components/GitOperationsPanel/GitOperationsPanel.tsx:23-61

## Work Log

