---
id: 273-a217
title: Shell injection in Sidebar git quick-action buttons
status: complete
priority: P1
created: "2026-02-20T13:56:35.896Z"
updated: "2026-02-20T13:58:24.536Z"
dependencies: []
---

# Shell injection in Sidebar git quick-action buttons

## Problem Statement

repo.path interpolated into shell commands without escapeShellArg() in Sidebar.tsx:795-826. Adjacent GitOperationsPanel correctly escapes.

## Acceptance Criteria

- [ ] All git quick-action buttons use escapeShellArg(repo.path)
- [ ] Consistent with GitOperationsPanel pattern

## Files

- src/components/Sidebar/Sidebar.tsx

## Work Log

### 2026-02-20T13:58:24.459Z - Added escapeShellArg import and wrapped all 4 git quick-action button commands

