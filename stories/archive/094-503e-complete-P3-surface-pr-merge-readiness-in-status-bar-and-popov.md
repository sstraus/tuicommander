---
id: 094-503e
title: Surface PR merge readiness in status bar and popover
status: complete
priority: P3
created: "2026-02-09T21:22:25.859Z"
updated: "2026-02-15T10:54:16.129Z"
dependencies: []
---

# Surface PR merge readiness in status bar and popover

## Problem Statement

We do not check or display PR mergeable state. Users cannot tell from the TUI whether a PR is mergeable, behind the base branch, has conflicts, or is blocked by pending checks. a competitor v0.5.8 fixed a similar gap where PRs marked BEHIND or with pending checks were incorrectly shown as unmergeable.

## Acceptance Criteria

- [ ] Fetch mergeable state via gh pr view --json mergeable,mergeStateStatus for active branch PR
- [ ] Display merge readiness indicator in PrDetailPopover (mergeable, blocked, behind, conflicting, unknown)
- [ ] Show visual cue on PrBadge in StatusBar when PR is not mergeable (e.g. warning icon or color)
- [ ] Handle BEHIND state distinctly from CONFLICTING — behind is auto-resolvable
- [ ] Unit tests for merge state parsing and display logic

## Files

- src-tauri/src/lib.rs
- src/components/PrDetailPopover/PrDetailPopover.tsx
- src/components/ui/StatusBadge.tsx
- src/stores/github.ts

## Related

- 093-0d92
- 091-aa97

## Work Log

### 2026-02-15T10:54:16.059Z - Added mergeable+mergeStateStatus to Rust BranchPrStatus, TS types, PrDetailPopover badge, PrBadge variant, and CSS. All 872 TS + 39 Rust tests pass.

