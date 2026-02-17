---
id: "033g"
title: "SolidJS App component and split layout"
status: pending
priority: P1
created: 2026-02-04T13:00:00.000Z
updated: 2026-02-04T13:00:00.000Z
dependencies: ["033a", "033b", "033c", "033d", "033e", "033f"]
blocks: ["033h"]
---

# SolidJS App component and split layout

## Problem Statement

The App component orchestrates the entire application: layout, keyboard shortcuts, event listeners, and the complex split pane system for terminals.

## Split Pane System Analysis

Current implementation creates nested DOM structure:
```html
<div class="split-container split-horizontal">
  <div class="split-pane" style="flex-basis: 50%">
    <div class="terminal-pane">...</div>
  </div>
  <div class="split-handle split-handle-horizontal"></div>
  <div class="split-pane" style="flex-basis: 50%">
    <div class="terminal-pane">...</div>
  </div>
</div>
```

This requires a recursive component or a flat list with layout metadata.

## Component Design

```typescript
// Option A: Recursive split tree
interface SplitNode {
  type: 'terminal' | 'split';
  direction?: 'horizontal' | 'vertical';
  children?: [SplitNode, SplitNode];
  terminalId?: string;
  size?: number; // percentage
}

// Option B: Flat list with CSS Grid
// Simpler but less flexible for deep nesting
```

## App Component Responsibilities

1. **Layout**: Sidebar | Main (TabBar + TerminalArea + StatusBar)
2. **Keyboard shortcuts**: Global handler with event delegation
3. **Tauri events**: pty-output, pty-exit listeners
4. **Initialization**: Load settings, create first terminal, restore session
5. **Split management**: Create/remove splits, resize handles

## Acceptance Criteria

- [ ] Create src/components/SplitPane/SplitPane.tsx (recursive)
- [ ] Create src/components/SplitPane/SplitHandle.tsx
- [ ] Create src/components/TerminalArea/TerminalArea.tsx
- [ ] Create src/App.tsx with full layout
- [ ] Global keyboard shortcuts via onKeyDown
- [ ] Tauri event listeners via onMount + onCleanup
- [ ] Session restore on mount
- [ ] Session save on beforeunload

## Technical Challenges

1. **Split state**: Need to track tree structure in store
2. **Resize handles**: Mouse drag + state update + refit terminals
3. **Terminal focus**: Clicking terminal sets active in store
4. **Event bubbling**: Prevent shortcuts when typing in terminal

## Files

- src/components/SplitPane/SplitPane.tsx
- src/components/SplitPane/SplitHandle.tsx
- src/components/TerminalArea/TerminalArea.tsx
- src/App.tsx
- src/index.tsx (entry point)
