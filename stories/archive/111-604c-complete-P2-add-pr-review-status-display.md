---
id: 111-604c
title: Add PR review status display
status: complete
priority: P2
created: "2026-02-15T13:52:19.404Z"
updated: "2026-02-15T17:35:11.118Z"
dependencies: []
---

# Add PR review status display

## Problem Statement

The PR visualization shows state, CI, merge readiness, and diff stats but no review information. Users cannot tell if a PR has been approved, has changes requested, or is awaiting review without opening GitHub. This is critical information for PR workflow.

## Acceptance Criteria

- [ ] Fetch reviewDecision field from gh pr list
- [ ] Display review status in PrDetailPopover (APPROVED, CHANGES_REQUESTED, REVIEW_REQUIRED)
- [ ] Show review status icon/badge in sidebar PrBadgeSidebar
- [ ] Color coding: green=approved, red=changes requested, yellow=pending review

## Files

- src-tauri/src/lib.rs
- src/stores/github.ts
- src/components/PrDetailPopover/PrDetailPopover.tsx
- src/components/Sidebar/Sidebar.tsx

## Work Log

### 2026-02-15T17:35:11.055Z - Added reviewDecision to Rust BranchPrStatus struct and gh pr list query. Frontend type updated. PrDetailPopover shows color-coded review badge (APPROVED/CHANGES_REQUESTED/REVIEW_REQUIRED). Fixed parking_lot unwrap and partial move bugs.

