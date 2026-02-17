---
id: 091-aa97
title: Centralized GitHub PR/CI store with batch polling
status: complete
priority: P2
created: "2026-02-08T17:14:12.271Z"
updated: "2026-02-09T21:25:46.993Z"
dependencies: ["["090-f90e"]"]
---

# Centralized GitHub PR/CI store with batch polling

## Problem Statement

The current useGitHub hook holds PR/CI state locally in the StatusBar component and only tracks the active branch. To show CI status for all worktrees in the sidebar, we need a centralized store that batch-polls all repos and distributes PR/CI data per-branch.

## Acceptance Criteria

- [ ] New githubStore (SolidJS store) holding per-branch PR+CI data keyed by repoPath -> branchName
- [ ] Background poller calls get_repo_pr_statuses for each repo with GitHub remotes
- [ ] Smart polling: 30s base interval, 2min when hidden, exponential backoff on errors
- [ ] StatusBar reads from githubStore instead of local useGitHub hook for PR/CI data
- [ ] useGitHub hook retained for branch-level ahead/behind data (not available from gh pr list)
- [ ] Store exposes per-branch accessor: getCheckSummary(repoPath, branch) -> {passed, failed, pending}
- [ ] Store exposes per-branch accessor: getPrStatus(repoPath, branch) -> PrStatus | null

## Files

- src/stores/github.ts
- src/hooks/useGitHub.ts
- src/components/StatusBar/StatusBar.tsx
- src/types/index.ts

## Related

- 062-1530

## Work Log

