---
id: 327-1682
title: "CSS: generic activity-* classes and two-line item layout"
status: ready
priority: P2
created: "2026-02-21T09:35:32.591Z"
updated: "2026-02-21T10:02:49.067Z"
dependencies: ["325-90ef"]
---

# CSS: generic activity-* classes and two-line item layout

## Problem Statement

CSS classes are hardcoded for PR notifications (pr-notif-*) and plans (plan-button-*). These must be replaced with generic activity-* classes, sections must have divider headers, and items need a two-line layout (larger title + smaller subtitle). Take screenshot to verify.

## Acceptance Criteria

- [ ] pr-notif-* CSS classes renamed to activity-* equivalents (pr-notif-wrapper -> activity-wrapper, etc.)
- [ ] plan-button-* CSS classes removed
- [ ] activity-section-header style: section label text + optional dismiss-all button
- [ ] activity-item-title style: larger font weight for main title
- [ ] activity-item-subtitle style: smaller muted text for secondary info
- [ ] activity-last-item button: same visual weight as former plan-button, shows icon + truncated title
- [ ] Type-specific icon colors preserved for PR items (.notif-merged, .notif-ready, etc.)
- [ ] Screenshot taken after changes to verify rendering

## Files

- src/styles.css

## Related

- 325-90ef

## Work Log

