---
id: "395-4448"
title: "Fix afterMerge ask mode - show dialog instead of silently completing"
status: pending
priority: P2
created: 2026-02-26T12:47:28.108Z
updated: 2026-02-26T12:47:28.108Z
dependencies: []
---

# Fix afterMerge ask mode - show dialog instead of silently completing

## Problem Statement

When afterMerge setting is ask, merge_and_archive_worktree returns action=pending but the frontend (useGitOperations.ts:644) ignores it and just logs success. No dialog is shown to the user.

## Acceptance Criteria

- [ ] When result.action === pending, show a dialog asking archive or delete
- [ ] Dialog choice triggers the appropriate follow-up Tauri call
- [ ] User can cancel (worktree stays as-is after merge)

## Files

- src/hooks/useGitOperations.ts
- src/components/

## Work Log

