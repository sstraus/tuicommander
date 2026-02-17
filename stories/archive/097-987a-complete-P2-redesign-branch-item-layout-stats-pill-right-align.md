---
id: 097-987a
title: "Redesign branch item layout: stats pill right-aligned"
status: complete
priority: P2
created: "2026-02-15T10:19:58.679Z"
updated: "2026-02-15T11:12:02.603Z"
dependencies: []
---

# Redesign branch item layout: stats pill right-aligned

## Problem Statement

Current branch items show git stats (+/-) as plain text on a second line below the branch name, left-aligned. This wastes horizontal space and looks cluttered. The desired layout moves stats into a right-aligned pill/badge and uses smaller fonts for a more compact, professional look.

## Acceptance Criteria

- [ ] Move StatsBadge from inside branch-content to right-aligned position (same row as branch name, pushed to right edge)
- [ ] Style StatsBadge as a pill: rounded rect background (var(--bg-tertiary) or subtle border), border-radius ~6px, padding 2px 6px, containing green +N and red -N
- [ ] Reduce branch-name font-size from 14px to 12px
- [ ] Reduce branch-stats font-size from 11px to 10px
- [ ] Restructure branch-item as a CSS grid or nested flex: left column (icon + branch-name/secondary-info) and right column (stats-pill on row 1, shortcut on row 2)
- [ ] Stats pill and branch-shortcut must both be right-aligned but stats vertically centered with branch name, shortcut below
- [ ] Ensure PR badge and CI ring still fit in the layout without overlap
- [ ] StatsBadge condition: show when additions OR deletions > 0 (currently requires BOTH > 0)

## Work Log

### 2026-02-15T11:12:02.539Z - Redesigned branch layout: right-aligned stats pill, reduced font sizes, fixed OR condition. 888 tests pass.

