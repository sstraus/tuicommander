---
id: 160-1ad2
title: Intercept .md file paths as clickable links to MD viewer
status: complete
priority: P2
created: "2026-02-16T07:11:16.843Z"
updated: "2026-02-16T08:07:01.042Z"
dependencies: []
---

# Intercept .md file paths as clickable links to MD viewer

## Problem Statement

When .md file paths appear in the UI (terminal output, rendered markdown content), they are plain text or dead links. Users cannot click them to open in the built-in MD viewer. For example, paths like reviews/review-2026-02-16-whole-project.md in terminal output or [link](file.md) in rendered markdown are not actionable.

## Acceptance Criteria

- [ ] Terminal output: detect .md file paths and make them clickable (open in MD viewer tab)
- [ ] Rendered markdown: intercept relative .md links so clicking opens in MD viewer instead of dead link
- [ ] Opening a .md link auto-shows the markdown panel if hidden
- [ ] Works with both absolute and relative paths within the repo

## Files

- src/components/Terminal/Terminal.tsx
- src/components/ui/MarkdownRenderer.tsx
- src/stores/mdTabs.ts
- src/components/MarkdownTab/MarkdownTab.tsx
- src/App.tsx

## Work Log

### 2026-02-16T08:06:55.861Z - Implemented .md link interception in two places: (1) MarkdownRenderer gets onLinkClick prop, intercepts clicks on relative .md anchors, MarkdownTab resolves paths and opens new tabs via mdTabsStore. (2) Terminal gets onOpenMdFile prop with custom xterm ILinkProvider that detects .md paths in terminal output. App.tsx wires both to mdTabsStore.add() + panel visibility. 3 new tests added.

