---
id: "033f"
title: "SolidJS container components (Sidebar, TabBar, StatusBar)"
status: pending
priority: P1
created: 2026-02-04T13:00:00.000Z
updated: 2026-02-04T13:00:00.000Z
dependencies: ["033a", "033b", "033c", "033d", "033e"]
blocks: ["033g"]
---

# SolidJS container components

## Problem Statement

Migrate the main layout container components that compose the UI: Sidebar, TabBar, and StatusBar. These connect stores to leaf components.

## Components to Migrate

### 1. Sidebar
Contains:
- Logo/header
- Repository list (with status icons)
- Terminal list
- Add terminal button

Connects to:
- repositories store
- terminals store
- Actions: addRepository, removeRepository, createTerminalInRepo, setActiveTerminal

### 2. TabBar
Contains:
- Tab list (For each terminal)
- Markdown toggle button
- IDE dropdown
- Zoom indicator

Connects to:
- terminals store
- ui store (markdownViewVisible)
- settings store (selectedIDE)
- Actions: setActiveTerminal, closeTerminal, openInIDE, toggleMarkdownView

### 3. StatusBar
Contains:
- Session count
- GitHub status badges
- Agent selector dropdown
- Font selector dropdown
- Status info text

Connects to:
- terminals store (count)
- repositories store (for GitHub status)
- settings store (selectedAgent, selectedFont)
- Agent stats display

### 4. PromptOverlay
Contains:
- Question text
- Options list (using PromptOption)
- Keyboard hint footer

Connects to:
- prompt store
- Actions: selectOption, confirmSelection, hidePrompt

### 5. DiffPanel
Contains:
- Header with close button
- DiffViewer component

Connects to:
- ui store (diffPanelVisible, currentDiffRepo)
- useRepository hook for diff data

### 6. MarkdownPanel
Contains:
- Header with close button
- MarkdownRenderer component

Connects to:
- ui store (markdownViewVisible)
- terminals store (active terminal content)

## Acceptance Criteria

- [ ] Create src/components/Sidebar/Sidebar.tsx
- [ ] Create src/components/TabBar/TabBar.tsx
- [ ] Create src/components/StatusBar/StatusBar.tsx
- [ ] Create src/components/PromptOverlay/PromptOverlay.tsx
- [ ] Create src/components/DiffPanel/DiffPanel.tsx
- [ ] Create src/components/MarkdownPanel/MarkdownPanel.tsx
- [ ] All components read from stores reactively
- [ ] Actions dispatch to store mutations
- [ ] Keyboard shortcuts handled at App level, not in components

## Files

- src/components/Sidebar/Sidebar.tsx
- src/components/Sidebar/RepositoryList.tsx
- src/components/Sidebar/TerminalList.tsx
- src/components/TabBar/TabBar.tsx
- src/components/StatusBar/StatusBar.tsx
- src/components/PromptOverlay/PromptOverlay.tsx
- src/components/DiffPanel/DiffPanel.tsx
- src/components/MarkdownPanel/MarkdownPanel.tsx
