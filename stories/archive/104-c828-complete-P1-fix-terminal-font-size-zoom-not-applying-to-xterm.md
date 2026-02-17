---
id: 104-c828
title: Fix terminal font size zoom not applying to xterm
status: complete
priority: P1
created: "2026-02-15T11:17:51.246Z"
updated: "2026-02-15T17:16:25.620Z"
dependencies: []
---

# Fix terminal font size zoom not applying to xterm

## Problem Statement

Cmd+/Cmd- updates fontSize in terminalsStore and the StatusBar shows the correct percentage, but the xterm terminal does not visually resize. The createEffect at Terminal.tsx:420 that should apply fontSize to terminal.options is not reactive because terminalsStore.get(props.id) returns a snapshot, not a tracked proxy. SolidJS cannot track the deep .fontSize property access through the .get() method indirection.

## Acceptance Criteria

- [ ] Fix the createEffect at Terminal.tsx:420 to properly track fontSize changes reactively. Access the store property directly (e.g. terminalsStore.state.terminals[props.id]?.fontSize) instead of going through .get() which breaks SolidJS reactivity tracking
- [ ] After setting terminal.options.fontSize, call fitAddon.fit() to reflow the terminal grid to the new font size
- [ ] Verify Cmd+ zooms in, Cmd- zooms out, Cmd+0 resets, all visually reflected in the terminal
- [ ] Verify the StatusBar percentage display stays in sync with the actual terminal font size
- [ ] Check if the same reactivity bug affects other createEffect blocks that use terminalsStore.get() and fix them too

## Files

- src/components/Terminal/Terminal.tsx
- src/stores/terminals.ts

## Work Log

### 2026-02-15T17:16:25.553Z - Fixed createEffect to use direct store path access for fontSize reactivity, added test for store path accessibility

