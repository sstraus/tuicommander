---
id: 112-d053
title: Add PR labels and draft indicator
status: complete
priority: P3
created: "2026-02-15T13:52:19.405Z"
updated: "2026-02-15T18:13:51.528Z"
dependencies: []
---

# Add PR labels and draft indicator

## Problem Statement

PR labels and draft status are not fetched or displayed. Labels provide context (bug, enhancement, priority) and draft PRs should be visually distinct to avoid premature reviews.

## Acceptance Criteria

- [ ] Fetch labels and isDraft fields from gh pr list
- [ ] Display labels as colored pills in PrDetailPopover
- [ ] Show draft indicator (dashed border or muted style) on sidebar PR badge
- [ ] Draft PRs show Draft text in popover state badge

## Files

- src-tauri/src/lib.rs
- src/stores/github.ts
- src/components/PrDetailPopover/PrDetailPopover.tsx
- src/components/ui/StatusBadge.tsx

## Work Log

### 2026-02-15T18:13:51.448Z - Added labels and isDraft to Rust BranchPrStatus, TypeScript types, gh pr list query. PrDetailPopover shows colored label pills. Sidebar PR badge shows dashed border for drafts. State badge shows 'Draft' text for draft PRs. 4 new tests.

