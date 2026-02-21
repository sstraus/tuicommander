---
id: 325-90ef
title: "Generic bell UI: sections from multiple sources plus last-item shortcut"
status: complete
priority: P1
created: "2026-02-21T09:35:09.522Z"
updated: "2026-02-21T10:23:47.779Z"
dependencies: ["318-d0e3", "321-a3f0", "323-46a4"]
---

# Generic bell UI: sections from multiple sources plus last-item shortcut

## Problem Statement

The bell dropdown is hardcoded for PR notifications and the plan button is hardcoded for plan state. Both must be replaced with a generic bell that renders sections from activityStore (plugins) and prNotificationsStore (native), plus a last-item shortcut button showing the most recently added item from any source.

## Acceptance Criteria

- [ ] Plan button (plan-button-group) removed from Toolbar.tsx
- [ ] onOpenPlan prop removed from ToolbarProps
- [ ] Bell badge count = prNotificationsStore.getActive().length + activityStore.getActive().length
- [ ] Bell visible when either store has active items
- [ ] Dropdown renders PR UPDATES section (from prNotificationsStore, unchanged behavior)
- [ ] Dropdown renders plugin sections from activityStore in priority order
- [ ] Each section has a header with label and optional Dismiss All button
- [ ] Each item shows icon, title (larger), subtitle (smaller), optional X dismiss button
- [ ] Clicking plugin item with contentUri: opens virtual markdown tab via mdTabsStore.addVirtual() and shows markdown panel
- [ ] Last-item shortcut button: visible when any items exist in either store
- [ ] Last-item button shows icon + truncated title of most recently created item across all sources
- [ ] Last-item button click performs the item action (open file, open virtual content, or PR detail)
- [ ] All updated Toolbar tests pass

## Files

- src/components/Toolbar/Toolbar.tsx
- src/__tests__/components/Toolbar.test.tsx

## Related

- 318-d0e3
- 321-a3f0
- 323-46a4

## Work Log

### 2026-02-21T10:23:47.714Z - Removed plan button from Toolbar. Merged bell to show PR + plugin items with combined badge count. Added plugin activity sections to dropdown. Added last-item shortcut button showing most recent item across both stores. 38/38 Toolbar tests + 1776 total tests green.

