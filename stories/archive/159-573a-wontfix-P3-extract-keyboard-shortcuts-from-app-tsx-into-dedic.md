---
id: 159-573a
title: Extract keyboard shortcuts from App.tsx into dedicated hook
status: wontfix
priority: P3
created: "2026-02-16T07:05:09.779Z"
updated: "2026-02-16T08:55:07.726Z"
dependencies: []
---

# Extract keyboard shortcuts from App.tsx into dedicated hook

## Problem Statement

App.tsx is 1770+ lines with 19 createEffect calls and keyboard handling spanning 200+ lines. This makes reactivity chains hard to trace and increases risk of memory leaks from uncleaned effects. Extracting keyboard logic improves maintainability.

## Acceptance Criteria

- [ ] Extract keyboard shortcut handling into src/hooks/useKeyboardShortcuts.ts
- [ ] Extract quick switcher logic into src/hooks/useQuickSwitcher.ts or integrate into keyboard hook
- [ ] App.tsx passes callbacks, hook manages listeners and cleanup
- [ ] All existing shortcuts work identically after extraction
- [ ] No new dependencies added

## Files

- src/App.tsx
- src/hooks/useKeyboardShortcuts.ts

## Work Log

### 2026-02-16T08:55:07.661Z - Subsumed by story 185 (Split App.tsx) — keyboard shortcuts will be extracted as part of the god object split.

