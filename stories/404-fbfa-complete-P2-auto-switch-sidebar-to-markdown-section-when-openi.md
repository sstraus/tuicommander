---
id: 404-fbfa
title: Auto-switch sidebar to Markdown section when opening a markdown tab
status: complete
priority: P2
created: "2026-02-26T20:43:17.943Z"
updated: "2026-02-27T10:57:45.999Z"
dependencies: []
---

# Auto-switch sidebar to Markdown section when opening a markdown tab

## Problem Statement

When the user opens a markdown file (e.g. from file browser, deep link, or command palette), only the markdown tab appears in the content area. The sidebar stays on whichever section it was showing (e.g. Branches). The user then has to manually switch the sidebar to the Markdown section to see the file list / TOC.

The sidebar should automatically switch to the Markdown section when a markdown tab is opened, so the user gets full context immediately.

## Acceptance Criteria

- [ ] Opening a markdown file auto-switches the sidebar to the Markdown section
- [ ] If the sidebar is already on the Markdown section, no-op (no flicker)
- [ ] If the sidebar is collapsed/hidden, it should NOT auto-expand (only switch the active section)
- [ ] Closing the last markdown tab does NOT auto-switch the sidebar back (user may have navigated away already)

## QA

None — covered by tests

## Work Log

### 2026-02-27T10:57:42.026Z - Completed: Centralized setMarkdownPanelVisible(true) into mdTabsStore add/addVirtual/addPluginPanel/addClaudeUsage methods. Removed 4 redundant calls from App.tsx, PluginsTab.tsx, Toolbar.tsx. Added 6 tests. All 211 related tests pass.

