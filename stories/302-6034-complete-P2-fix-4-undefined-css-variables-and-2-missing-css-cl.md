---
id: 302-6034
title: Fix 4 undefined CSS variables and 2 missing CSS class definitions
status: complete
priority: P2
created: "2026-02-20T19:25:07.612Z"
updated: "2026-02-20T19:38:23.561Z"
dependencies: []
---

# Fix 4 undefined CSS variables and 2 missing CSS class definitions

## Problem Statement

4 CSS variables used but never defined cause silent rendering bugs: --border-primary (7 uses, markdown borders invisible), --bg-hover (2 uses, plan button hover broken), --danger (2 uses, group delete hover broken), --text-muted (1 use, help note text invisible). Also: dropdown-top/dropdown-item-icon/dropdown-item-label classes applied in Dropdown.tsx but have no CSS rules; prompt-dialog/prompt-hint applied in PromptOverlay.tsx but have no CSS rules.

## Acceptance Criteria

- [ ] Verify before: grep styles.css for --border-primary, --bg-hover, --danger, --text-muted and confirm none defined in :root
- [ ] Replace all --border-primary with --border (7 occurrences)
- [ ] Replace all --bg-hover with --bg-highlight (2 occurrences)
- [ ] Replace all --danger with --error (2 occurrences)
- [ ] Replace --text-muted with --fg-muted (1 occurrence)
- [ ] Add CSS rules for .dropdown-top, .dropdown-item-icon, .dropdown-item-label
- [ ] Add CSS rules for .prompt-dialog and .prompt-hint
- [ ] Take screenshot to verify markdown borders, plan button hover, group delete hover, help text visible
- [ ] Run make check and tests pass

## Files

- src/styles.css
- src/components/ui/Dropdown.tsx
- src/components/PromptOverlay/PromptOverlay.tsx

## Work Log

### 2026-02-20T19:38:23.490Z - Fixed 4 undefined CSS vars (--border-primary, --bg-hover, --danger, --text-muted) and added 5 missing CSS class definitions (.dropdown-top, .dropdown-item-icon, .dropdown-item-label, .prompt-dialog, .prompt-hint). Updated .dropdown-item to flex layout for icon+label. tsc clean, 1628 tests pass.

