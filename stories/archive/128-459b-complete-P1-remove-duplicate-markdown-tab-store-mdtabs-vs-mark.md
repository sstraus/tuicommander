---
id: 128-459b
title: Remove duplicate markdown tab store (mdTabs vs markdownTabs)
status: complete
priority: P1
created: "2026-02-15T21:17:30.371Z"
updated: "2026-02-15T21:59:22.573Z"
dependencies: []
---

# Remove duplicate markdown tab store (mdTabs vs markdownTabs)

## Problem Statement

mdTabs.ts and markdownTabs.ts are duplicate stores with different localStorage keys, causing confusion and potential state desync.

## Acceptance Criteria

- [ ] Consolidate to single markdownTabs store
- [ ] Migrate data from tui-commander-md-tabs key on startup
- [ ] Delete src/stores/mdTabs.ts
- [ ] Update all imports from mdTabsStore to markdownTabsStore

## Files

- src/stores/mdTabs.ts
- src/stores/markdownTabs.ts
- src/App.tsx

## Work Log

### 2026-02-15T21:59:22.412Z - Deleted unused src/stores/markdownTabs.ts. Only mdTabs.ts was imported across the codebase (App.tsx, TabBar.tsx, MarkdownPanel.tsx). markdownTabsStore had zero imports — pure dead code.

