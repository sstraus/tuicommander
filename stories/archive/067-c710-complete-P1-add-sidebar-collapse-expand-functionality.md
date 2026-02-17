---
id: 067-c710
title: Add sidebar collapse/expand functionality
status: complete
priority: P1
created: "2026-02-05T09:39:39.436Z"
updated: "2026-02-05T09:49:53.811Z"
dependencies: []
---

# Add sidebar collapse/expand functionality

## Problem Statement

TUI Commander sidebar is always expanded, taking fixed width and not allowing users to maximize terminal space. Users cannot collapse repositories to show only icons, reducing available screen real estate for the main terminal area.

## Acceptance Criteria

- [ ] Add collapsed boolean field to repository state in stores/repositories.ts
- [ ] Add toggleCollapsed(repoPath) action to repositories store
- [ ] Persist collapsed state in localStorage
- [ ] Add chevron button next to each repository name in Sidebar
- [ ] Show only repo icon/initials when repository is collapsed
- [ ] Show full repo name and branches when repository is expanded
- [ ] Add CSS transitions for smooth collapse/expand animation
- [ ] Adjust sidebar width dynamically based on collapsed state

## Files

- src/stores/repositories.ts
- src/components/Sidebar/Sidebar.tsx
- src/styles.css

## Work Log

