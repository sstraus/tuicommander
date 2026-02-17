---
id: "033h"
title: "SolidJS cleanup and remove old vanilla code"
status: pending
priority: P1
created: 2026-02-04T13:00:00.000Z
updated: 2026-02-04T13:00:00.000Z
dependencies: ["033a", "033b", "033c", "033d", "033e", "033f", "033g"]
---

# SolidJS cleanup and remove old vanilla code

## Problem Statement

After all components are migrated and tested, remove the old main.ts and update entry points. This is the final step that commits to the new architecture.

## Acceptance Criteria

- [ ] All features work in SolidJS version (manual testing)
- [ ] Remove src/main.ts (old 1700+ line file)
- [ ] Update index.html to use new entry point
- [ ] Run TypeScript strict mode checks
- [ ] Fix any remaining type errors
- [ ] Test all keyboard shortcuts
- [ ] Test all Tauri integrations
- [ ] Test session save/restore
- [ ] Test split pane resize
- [ ] Verify bundle size is acceptable
- [ ] Update story 033 as complete

## Migration Checklist

### Terminal Features
- [ ] Create terminal
- [ ] Close terminal
- [ ] Split horizontal/vertical
- [ ] Resize splits
- [ ] Per-pane zoom (Cmd+/-/0)
- [ ] Tab switching (Cmd+1-9)
- [ ] Focus navigation (Cmd+[/])

### Repository Features
- [ ] Add repository
- [ ] Remove repository
- [ ] Create terminal in repo
- [ ] Git status indicators

### UI Features
- [ ] IDE launcher dropdown
- [ ] Font selector dropdown
- [ ] Agent selector dropdown
- [ ] Diff panel toggle
- [ ] Markdown view toggle
- [ ] Agent prompt overlay
- [ ] GitHub status badges
- [ ] Agent stats display

### Persistence
- [ ] Save session on close
- [ ] Restore session on load
- [ ] Remember IDE selection
- [ ] Remember font selection
- [ ] Remember agent selection

## Files to Remove

- src/main.ts

## Files to Update

- index.html (script src)
- package.json (if needed)
