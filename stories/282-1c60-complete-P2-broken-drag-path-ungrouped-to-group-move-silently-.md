---
id: 282-1c60
title: "Broken drag path: ungrouped-to-group move silently fails"
status: complete
priority: P2
created: "2026-02-20T13:57:16.823Z"
updated: "2026-02-20T14:09:59.479Z"
dependencies: []
---

# Broken drag path: ungrouped-to-group move silently fails

## Problem Statement

sourceGroupId ?? empty-string passes empty string to moveRepoBetweenGroups which guards against empty IDs and returns early at Sidebar.tsx:547.

## Acceptance Criteria

- [ ] Use addRepoToGroup when source is ungrouped
- [ ] moveRepoBetweenGroups only when both groups valid

## Files

- src/components/Sidebar/Sidebar.tsx

## Work Log

### 2026-02-20T14:09:59.398Z - Fixed ungrouped-to-group drag to use addRepoToGroup instead of moveRepoBetweenGroups with empty string

