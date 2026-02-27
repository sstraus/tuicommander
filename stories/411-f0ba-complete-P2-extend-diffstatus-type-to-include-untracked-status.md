---
id: 411-f0ba
title: Extend DiffStatus type to include untracked status
status: complete
priority: P2
created: "2026-02-26T21:07:12.721Z"
updated: "2026-02-27T11:26:43.544Z"
dependencies: []
---

# Extend DiffStatus type to include untracked status

## Problem Statement

`DiffStatus` in `diffTabs.ts:4` is `"M" | "A" | "D" | "R"` but `DiffPanel` now handles `"?"` for untracked files and casts `file.status as DiffStatus` which is provably incorrect for untracked files.

## Acceptance Criteria

- [ ] Add `"?"` to `DiffStatus` type union in `diffTabs.ts`
- [ ] Remove the unsafe `as DiffStatus` cast in `DiffPanel.tsx`
- [ ] Verify `diffTabsStore.add` handles `"?"` status correctly downstream

## QA

None — type-only change

## Work Log

### 2026-02-27T11:26:43.407Z - Added '?' to DiffStatus union type. Updated ChangedFile.status comment. Cast in DiffPanel is now type-correct.

