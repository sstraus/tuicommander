---
id: 253-f4bc
title: "Fix status bar: wrong branch shown, truncation, and poor responsiveness"
status: complete
priority: P1
created: "2026-02-18T16:53:19.826Z"
updated: "2026-02-18T16:54:21.685Z"
dependencies: []
---

# Fix status bar: wrong branch shown, truncation, and poor responsiveness

## Problem Statement

Three status bar bugs: (1) When switching between repos, the branch shown belongs to the previous repo — useGitHub only polls on a timer and does not refresh when currentRepoPath changes. (2) The branch name is truncated at 200px even when plenty of space is available. (3) The status bar does not adapt correctly when the window is resized — github-status section has no flex shrink/grow and overflows.

## Acceptance Criteria

- [ ] Branch badge immediately reflects the active repo branch when switching repos — no stale branch shown from previous repo
- [ ] Branch name is never truncated when there is horizontal space available; max-width removed or made large enough
- [ ] Status bar sections use flex layout properly so the branch section can grow/shrink with window width
- [ ] Branch section does not overflow or clip when window is narrow

## Files

- src/hooks/useGitHub.ts
- src/components/ui/StatusBadge.tsx
- src/styles.css
- src/components/StatusBar/StatusBar.tsx

## Work Log

### 2026-02-18T16:54:21.566Z - Fix 1 (wrong branch): added createEffect in useGitHub that clears status and re-fetches immediately when getRepoPath() changes. Fix 2 (truncation): removed max-width: 200px from .status-badge; branch badge now uncapped. Fix 3 (responsiveness): added flex: 1 to #github-status so it grows/shrinks with window; added flex-shrink: 1 + min-width: 0 to .status-badge.branch so long names truncate gracefully when narrow.

