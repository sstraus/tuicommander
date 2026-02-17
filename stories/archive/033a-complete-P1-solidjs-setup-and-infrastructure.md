---
id: "033a"
title: "SolidJS setup and infrastructure"
status: pending
priority: P1
created: 2026-02-04T13:00:00.000Z
updated: 2026-02-04T13:00:00.000Z
dependencies: []
blocks: ["033b", "033c", "033d", "033e", "033f", "033g", "033h"]
---

# SolidJS setup and infrastructure

## Problem Statement

Before migrating any components, need to set up SolidJS with Vite, create the folder structure, and establish patterns for Tauri integration.

## Acceptance Criteria

- [ ] Install solid-js and vite-plugin-solid
- [ ] Update vite.config.ts for SolidJS
- [ ] Create src/components/ folder structure
- [ ] Create src/stores/ folder for global state
- [ ] Create src/hooks/ folder for Tauri integration hooks
- [ ] Create src/types/ folder for shared interfaces
- [ ] Setup App.tsx as root component (empty shell initially)
- [ ] Verify HMR works with SolidJS
- [ ] Old main.ts still works during migration

## Technical Notes

SolidJS uses JSX but compiles to fine-grained reactivity (no VDOM). Key patterns:
- `createSignal()` for local state
- `createStore()` for complex state
- `createEffect()` for side effects
- `onMount()` / `onCleanup()` for lifecycle
- Refs via `ref={el => ...}` for xterm.js integration

## Files

- package.json
- vite.config.ts
- tsconfig.json
- src/App.tsx (NEW)
- src/components/.gitkeep
- src/stores/.gitkeep
- src/hooks/.gitkeep
- src/types/index.ts (NEW)
