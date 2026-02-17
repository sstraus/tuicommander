---
id: 093-0d92
title: Rich PR detail popover with CI checks breakdown
status: complete
priority: P2
created: "2026-02-08T17:14:12.273Z"
updated: "2026-02-09T21:25:47.018Z"
dependencies: ["["091-aa97"]"]
---

# Rich PR detail popover with CI checks breakdown

## Problem Statement

The current CI popover only shows a flat list of check names and statuses. a competitor shows a rich PR detail view with PR metadata (state, title, author, merge target), diff stats (+/- lines), CI summary (N failed, N in progress, N successful), and individual check list with status icons. This popover should be accessible from both sidebar and status bar.

## Acceptance Criteria

- [ ] New PrDetailPopover component with full PR information display
- [ ] Header shows: PR state badge (OPEN/MERGED/CLOSED), title, PR number
- [ ] Subheader shows: author wants to merge N commits into base from head
- [ ] Diff stats row: +additions -deletions with green/red coloring
- [ ] CI summary row with ring icon: N failed, N in progress, N successful
- [ ] Scrollable list of individual CI checks with status icons (green checkmark, red X, yellow dot)
- [ ] Each check is clickable - opens check URL on GitHub
- [ ] Popover triggers from: sidebar CI ring click, sidebar PR badge click, status bar CI badge click
- [ ] Click-outside or Escape closes the popover
- [ ] Replaces or extends existing CI popover from Story 060

## Files

- src/components/PrDetailPopover/PrDetailPopover.tsx
- src/components/Sidebar/Sidebar.tsx
- src/components/StatusBar/StatusBar.tsx
- src/styles.css

## Related

- 060-9065
- 071-cc1f

## Work Log

