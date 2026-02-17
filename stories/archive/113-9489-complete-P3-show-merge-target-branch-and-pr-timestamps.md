---
id: 113-9489
title: Show merge target branch and PR timestamps
status: complete
priority: P3
created: "2026-02-15T13:52:19.407Z"
updated: "2026-02-15T18:17:11.366Z"
dependencies: []
---

# Show merge target branch and PR timestamps

## Problem Statement

The PR popover does not show which branch a PR targets (base branch) or when it was created/updated. This context helps users quickly assess PR relevance and freshness.

## Acceptance Criteria

- [ ] Fetch baseRefName, createdAt, updatedAt from gh pr list
- [ ] Display merge direction (head -> base) in PrDetailPopover
- [ ] Show relative timestamps (2h ago, 3 days ago) for creation and last update
- [ ] Timestamps update on each poll cycle without full re-render

## Files

- src-tauri/src/lib.rs
- src/stores/github.ts
- src/components/PrDetailPopover/PrDetailPopover.tsx

## Work Log

### 2026-02-15T18:17:11.300Z - Added baseRefName, createdAt, updatedAt to Rust BranchPrStatus and TypeScript types. PrDetailPopover shows merge direction (head -> base) and relative timestamps. Created relativeTime utility. 10 new tests (7 time, 3 popover).

