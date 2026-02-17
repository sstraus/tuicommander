---
id: 095-fd6a
title: Show merged PRs in purple across sidebar and popover
status: complete
priority: P2
created: "2026-02-11T06:22:48.173Z"
updated: "2026-02-15T11:03:38.561Z"
dependencies: []
---

# Show merged PRs in purple across sidebar and popover

## Problem Statement

The backend get_repo_pr_statuses command only fetches --state open PRs, so merged and closed PRs never appear in the sidebar or popover. Additionally, the PrBadge component maps merged state to the info variant instead of a dedicated merged class, so even if merged data arrived, the purple CSS (.status-badge.pr.merged) would not be applied. The sidebar PrBadgeSidebar component has no state-aware styling at all — it renders the same regardless of PR state.

## Acceptance Criteria

- [ ] Change Rust get_repo_pr_statuses to fetch --state all (or add a second call for merged PRs) so merged/closed PRs appear in githubStore
- [ ] Sidebar PrBadgeSidebar applies purple styling when PR state is MERGED and red when CLOSED
- [ ] Fix PrBadge variant mapping in StatusBadge.tsx: merged should produce .status-badge.pr.merged class (not info variant) so existing purple CSS applies
- [ ] PrDetailPopover correctly shows purple MERGED state badge (already has CSS, verify data flows through)
- [ ] Unit tests for merged/closed PR rendering in sidebar badge, PrBadge variant, and popover state badge

## Files

- src-tauri/src/lib.rs:1038-1073
- src/components/Sidebar/Sidebar.tsx:44-48
- src/components/ui/StatusBadge.tsx:69-91
- src/components/PrDetailPopover/PrDetailPopover.tsx
- src/stores/github.ts
- src/styles.css

## Related

- 094-503e
- 093-0d92
- 091-aa97
- 090-f90e

## Work Log

### 2026-02-15T11:03:34.999Z - Implemented merged PR purple styling: Rust fetches all PR states, PrBadge uses merged/closed variants, sidebar PrBadgeSidebar shows state-aware colors, CSS updated. 883 TS + 40 Rust tests pass.

