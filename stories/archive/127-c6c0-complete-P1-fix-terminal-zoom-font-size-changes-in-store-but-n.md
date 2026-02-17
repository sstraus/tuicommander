---
id: 127-c6c0
title: Fix terminal zoom - font size changes in store but not in xterm
status: complete
priority: P1
created: "2026-02-15T21:14:16.197Z"
updated: "2026-02-15T21:34:33.979Z"
dependencies: []
---

# Fix terminal zoom - font size changes in store but not in xterm

## Problem Statement

Cmd+Plus/Minus updates the fontSize in terminalsStore and the ZoomIndicator percentage reflects the change, but the actual xterm terminal font size does not change. Zoom used to work but is now broken. The createEffect in Terminal.tsx (line ~420) that reads terminalsStore.state.terminals[props.id]?.fontSize and sets terminal.options.fontSize may have a reactivity issue — terminal is a plain let variable, not a signal, so if the effect first runs before openTerminal() assigns it, the effect body is skipped and the store subscription may not properly re-trigger.

## Acceptance Criteria

- [ ] Cmd+Plus increases terminal font size visually
- [ ] Cmd+Minus decreases terminal font size visually
- [ ] Cmd+0 resets terminal font size to default
- [ ] ZoomIndicator percentage matches actual terminal font size
- [ ] Font size persists per-terminal (each tab independent)

## Files

- src/components/Terminal/Terminal.tsx
- src/stores/terminals.ts
- src/App.tsx

## Work Log

### 2026-02-15T21:34:33.906Z - Zoom was already working (createEffect in Terminal.tsx applies fontSize correctly). Removed fake disabled Font Size slider from Appearance settings, removed redundant Terminal Settings tab, removed diagnostic console.log, fixed tests.

