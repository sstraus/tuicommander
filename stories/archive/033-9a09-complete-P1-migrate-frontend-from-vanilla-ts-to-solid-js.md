---
id: 033-9a09
title: Migrate frontend from vanilla TS to Solid.js
status: complete
priority: P1
created: "2026-02-04T11:52:53.066Z"
updated: "2026-02-04T12:13:26.055Z"
dependencies: []
---

# Migrate frontend from vanilla TS to Solid.js

## Problem Statement

Current vanilla TypeScript implementation (1700+ lines in main.ts) has scaling issues: global state management, duplicated code (createTerminal variants), manual DOM manipulation, and no systematic component lifecycle. As a production application, maintainability and team collaboration require a modern reactive framework.

## Sub-Stories (in order)

| Story | Title | Description |
|-------|-------|-------------|
| 033a | Setup and infrastructure | Install SolidJS, folder structure, config |
| 033b | Shared stores and types | Migrate global state to Solid stores |
| 033c | Tauri integration hooks | Reusable hooks for invoke() calls |
| 033d | Leaf components | Dropdown, StatusBadge, DiffViewer, etc. |
| 033e | Terminal component | xterm.js integration with refs |
| 033f | Container components | Sidebar, TabBar, StatusBar, Panels |
| 033g | App and split layout | Main layout, keyboard shortcuts, splits |
| 033h | Cleanup | Remove old code, final testing |

## Architecture Overview

```
src/
├── App.tsx                 # Root component
├── index.tsx              # Entry point
├── components/
│   ├── ui/                # Leaf components (Dropdown, Badge, etc.)
│   ├── Terminal/          # xterm.js wrapper
│   ├── Sidebar/           # Left panel
│   ├── TabBar/            # Top tabs
│   ├── StatusBar/         # Bottom bar
│   ├── SplitPane/         # Split container
│   ├── DiffPanel/         # Git diff viewer
│   ├── MarkdownPanel/     # MD renderer
│   └── PromptOverlay/     # Agent prompts
├── stores/
│   ├── terminals.ts       # Terminal state
│   ├── repositories.ts    # Repo state
│   ├── ui.ts              # UI toggles
│   ├── prompt.ts          # Prompt state
│   └── settings.ts        # Persisted settings
├── hooks/
│   ├── usePty.ts          # PTY commands
│   ├── useRepository.ts   # Git commands
│   ├── useGitHub.ts       # GitHub status
│   └── useAgentDetection.ts
├── types/
│   └── index.ts           # Shared interfaces
└── agents.ts              # (existing) Agent config
```

## Acceptance Criteria

- [ ] All sub-stories completed (033a-033h)
- [ ] All existing functionality preserved
- [ ] No regressions in keyboard shortcuts
- [ ] Bundle size within 10% of original
- [ ] HMR works during development

## Files

See sub-stories for detailed file lists.
