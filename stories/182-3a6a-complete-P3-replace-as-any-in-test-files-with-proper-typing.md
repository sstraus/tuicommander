---
id: 182-3a6a
title: Replace as-any in test files with proper typing
status: complete
priority: P3
created: "2026-02-16T07:12:39.084Z"
updated: "2026-02-16T08:01:19.942Z"
dependencies: []
---

# Replace as-any in test files with proper typing

## Problem Statement

Test files use as any to bypass type checking instead of creating properly-typed fixtures.

## Acceptance Criteria

- [ ] Remove all as any from test files
- [ ] Create typed test fixtures or use Partial<T>

## Files

- src/__tests__/hooks/usePty.test.ts
- src/__tests__/components/RepoSettingsPanel.test.tsx
- src/__tests__/components/Toolbar.test.tsx

## Related

- TS-03

## Work Log

### 2026-02-16T08:01:16.187Z - Replaced all 3 as-any casts: 2 in usePty.test.ts (PtyConfig objects), 1 in RepoSettingsPanel.test.tsx (RepoSettings import). Zero as-any remaining in test files.

