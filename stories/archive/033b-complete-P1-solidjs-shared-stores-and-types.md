---
id: "033b"
title: "SolidJS shared stores and types"
status: pending
priority: P1
created: 2026-02-04T13:00:00.000Z
updated: 2026-02-04T13:00:00.000Z
dependencies: ["033a"]
blocks: ["033c", "033d", "033e", "033f", "033g"]
---

# SolidJS shared stores and types

## Problem Statement

The current code has 12+ global state variables scattered throughout main.ts. Need to consolidate into typed Solid stores for reactive state management.

## Current Global State to Migrate

```typescript
// Terminal state
terminals: Map<string, TerminalPane>
activeTerminalId: string | null
terminalCounter: number

// Repository state
repositories: Map<string, Repository>

// UI state
selectedIDE: string
selectedFont: string
selectedAgent: AgentType
diffPanelVisible: boolean
currentDiffRepo: string | null
markdownViewVisible: boolean

// Prompt state
activePrompt: DetectedPrompt | null
selectedOptionIndex: number
outputBuffer: string

// Agent stats
statsBuffer: string
sessionStats: Map<string, AgentStats>

// Polling
githubRefreshInterval: number | null
```

## Acceptance Criteria

- [ ] Create src/types/index.ts with all interfaces (TerminalPane, Repository, etc.)
- [ ] Create src/stores/terminals.ts - terminal management store
- [ ] Create src/stores/repositories.ts - repository management store
- [ ] Create src/stores/ui.ts - UI toggles and selections
- [ ] Create src/stores/prompt.ts - agent prompt state
- [ ] Create src/stores/settings.ts - persisted settings (localStorage sync)
- [ ] All stores use createStore() with proper typing
- [ ] Stores export both state and actions (mutations)

## Files

- src/types/index.ts
- src/stores/terminals.ts
- src/stores/repositories.ts
- src/stores/ui.ts
- src/stores/prompt.ts
- src/stores/settings.ts
- src/stores/index.ts (re-exports)
