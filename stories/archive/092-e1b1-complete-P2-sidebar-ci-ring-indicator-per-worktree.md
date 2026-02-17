---
id: 092-e1b1
title: Sidebar CI ring indicator per worktree
status: complete
priority: P2
created: "2026-02-08T17:14:12.272Z"
updated: "2026-02-09T21:25:47.008Z"
dependencies: ["["091-aa97"]"]
---

# Sidebar CI ring indicator per worktree

## Problem Statement

The sidebar currently shows PR number badges but has no visual indication of CI check status per branch. Users cannot see at a glance which worktrees have failing CI without clicking into each one. a competitor shows a colored ring/arc indicator next to each worktree.

## Acceptance Criteria

- [ ] New CiRing SVG component showing pass/fail/pending proportions as colored arc segments
- [ ] Green segments for passed checks, red for failed, yellow for pending/in-progress
- [ ] Ring renders next to PR badge in BranchItem component
- [ ] Only visible for branches with known PR and CI check data (from githubStore)
- [ ] Ring is clickable - opens the PR detail popover (Story 093)
- [ ] Compact size (~16px) that fits sidebar density

## Files

- src/components/ui/CiRing.tsx
- src/components/Sidebar/Sidebar.tsx
- src/styles.css

## Related

- 060-9065

## Work Log

