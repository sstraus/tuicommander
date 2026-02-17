---
id: 110-d428
title: Sort merged branches to bottom in sidebar
status: complete
priority: P2
created: "2026-02-15T13:52:19.404Z"
updated: "2026-02-15T17:30:53.949Z"
dependencies: []
---

# Sort merged branches to bottom in sidebar

## Problem Statement

The sidebar branch list shows all branches in the same order regardless of state. Merged branches clutter the view, pushing active branches down. a competitor sorts merged branches to the bottom, keeping actionable branches at the top.

## Acceptance Criteria

- [ ] Branches with merged PRs are sorted to the bottom of the branch list within each repo
- [ ] Active/open PR branches appear at the top
- [ ] Branches without PRs appear in the middle
- [ ] Sort order is stable (no flickering on re-render)

## Files

- src/components/Sidebar/Sidebar.tsx
- src/stores/github.ts

## Work Log

### 2026-02-15T17:30:53.886Z - Updated sortedBranches memo in RepoSection to sort MERGED/CLOSED PR branches to bottom. Sort order: main > open PRs/no PR > merged/closed, alphabetical within each group.

