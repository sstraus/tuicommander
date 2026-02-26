---
id: 387-3d35
title: Replace console.* with appLogger in UI components (wave 3 of 4)
status: complete
priority: P2
created: "2026-02-25T17:53:56.234Z"
updated: "2026-02-25T20:00:23.441Z"
dependencies: ["384-75e3"]
---

# Replace console.* with appLogger in UI components (wave 3 of 4)

## Problem Statement

Implement: Replace console.* with appLogger in UI components (wave 3 of 4)

## Acceptance Criteria

- [ ] Implement as described

## Work Log

### 2026-02-25T17:54:22.294Z - Files: App.tsx (5), PluginsTab.tsx (3), AgentsTab.tsx (3), ServicesTab.tsx (4), CodeEditorTab.tsx (3), FileBrowserPanel.tsx (5), IdeLauncher.tsx (2), DictationSettings.tsx (1), GeneralTab.tsx (1), StatusBar.tsx (1), MarkdownPanel.tsx (1), MarkdownTab.tsx (1), ActivityDashboard.tsx (1), PromptOverlay.tsx (1), PromptDrawer.tsx (1), GitOperationsPanel.tsx (1), MarkdownRenderer.tsx (1). Source: plugin for plugin components, app for misc, git for git operations. Depends on 384-75e3.

### 2026-02-25T20:00:23.508Z - Replaced ~38 console.* calls across 17 component files and App.tsx. Commit be8147f.

