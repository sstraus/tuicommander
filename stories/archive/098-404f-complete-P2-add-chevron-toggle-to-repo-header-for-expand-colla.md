---
id: 098-404f
title: Add chevron toggle to repo header for expand/collapse
status: complete
priority: P2
created: "2026-02-15T10:21:14.984Z"
updated: "2026-02-15T13:40:55.628Z"
dependencies: []
---

# Add chevron toggle to repo header for expand/collapse

## Problem Statement

Repo sections expand/collapse by clicking the header, but there is no visual affordance (arrow/chevron) indicating the toggle or current state. Users cannot tell at a glance whether a repo section is collapsible or what state it is in.

## Acceptance Criteria

- [ ] Add a chevron arrow (>) to the right end of repo-header, after the + button
- [ ] Chevron rotates: > (right) when collapsed, v (down) when expanded
- [ ] Chevron is always visible (not hover-only like the other repo-actions)
- [ ] Clicking the chevron toggles the expanded state (same as clicking header)
- [ ] Smooth CSS rotation transition on the chevron (transform: rotate, ~150ms)
- [ ] Chevron uses var(--fg-muted) color, slightly larger than action buttons (~14px)
- [ ] Keep existing header click-to-toggle behavior unchanged

## Files

- src/components/Sidebar/Sidebar.tsx
- src/styles.css

## Work Log

### 2026-02-15T11:14:21.222Z - Added chevron toggle with rotation animation, always visible. Also added border to stats pills. 891 tests pass.

### 2026-02-15T13:38:28.135Z - BUG: repo-actions (⋯ and + buttons) use position: absolute; right: 12px which overlaps the chevron on hover. The chevron sits at the flex end but the absolutely positioned actions cover it. Fix: either (1) make repo-actions part of the flex flow instead of absolute, with chevron always at the rightmost position, or (2) add right padding/margin to repo-actions to account for chevron width.

