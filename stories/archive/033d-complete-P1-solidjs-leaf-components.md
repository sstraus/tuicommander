---
id: "033d"
title: "SolidJS leaf components (no children)"
status: pending
priority: P1
created: 2026-02-04T13:00:00.000Z
updated: 2026-02-04T13:00:00.000Z
dependencies: ["033a", "033b", "033c"]
blocks: ["033e", "033f"]
---

# SolidJS leaf components (no children)

## Problem Statement

Migrate simple, self-contained UI components that don't contain other custom components. These are the building blocks.

## Components to Migrate

### 1. Dropdown (generic, reusable)
- Used by: IDE selector, Font selector, Agent selector
- Props: items[], selected, onSelect, position
- Features: toggle visibility, click outside to close

### 2. ZoomIndicator
- Displays current zoom percentage
- Reads from: activeTerminal.fontSize

### 3. StatusBadge
- Used for: branch, PR, CI status
- Props: type, label, variant (success/warning/error)

### 4. DiffViewer
- Renders git diff with syntax highlighting
- Props: diff string
- Pure rendering component

### 5. MarkdownRenderer
- Converts terminal output to HTML
- Props: content string
- Uses: stripAnsi(), simpleMarkdownToHtml()

### 6. PromptOption
- Single option in prompt overlay
- Props: index, label, selected, onClick

## Acceptance Criteria

- [ ] Create src/components/ui/Dropdown.tsx
- [ ] Create src/components/ui/ZoomIndicator.tsx
- [ ] Create src/components/ui/StatusBadge.tsx
- [ ] Create src/components/ui/DiffViewer.tsx
- [ ] Create src/components/ui/MarkdownRenderer.tsx
- [ ] Create src/components/ui/PromptOption.tsx
- [ ] All components are typed with proper props interfaces
- [ ] Components use Solid's Show, For, Switch for conditionals
- [ ] No direct DOM manipulation - use reactive bindings

## Files

- src/components/ui/Dropdown.tsx
- src/components/ui/ZoomIndicator.tsx
- src/components/ui/StatusBadge.tsx
- src/components/ui/DiffViewer.tsx
- src/components/ui/MarkdownRenderer.tsx
- src/components/ui/PromptOption.tsx
- src/components/ui/index.ts
