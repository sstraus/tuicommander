---
id: 420-e0ea
title: Auto-delete local branch when its remote PR is merged or closed on GitHub
status: in_progress
priority: P2
created: "2026-02-27T08:28:54.915Z"
updated: "2026-02-27T11:54:38.132Z"
dependencies: []
---

# Auto-delete local branch when its remote PR is merged or closed on GitHub

## Problem Statement

When a PR is merged or closed on GitHub, the remote branch is usually deleted automatically. But the local branch and any associated worktree remain, cluttering the sidebar. Users have to manually delete local branches that no longer have a remote counterpart. A per-repo setting should detect when a tracked PR transitions to merged/closed and offer to (or automatically) delete the corresponding local branch and clean up worktrees.

## Acceptance Criteria

- [x] Add autoDeleteOnPrClose setting to per-repo config (options: off, ask, auto; default: off)
- [x] Detect PR merged/closed state transitions in the existing GitHub polling loop
- [x] When triggered and mode is ask: show in-app confirm dialog listing branch name and PR number
- [x] When triggered and mode is auto: delete local branch silently, log to appLogger
- [x] If branch has a linked worktree, remove worktree first then delete branch
- [x] Never auto-delete the base/default branch
- [x] Setting visible in Settings > repo settings section
- [x] Setting visible in Settings > global defaults section
- [x] Handle edge case: branch has uncommitted changes — always ask, never auto-delete

## Work Log

### 2026-02-27 — Full implementation

**Config layer** (pre-existing uncommitted changes):
- Rust `AutoDeleteOnPrClose` enum (Off/Ask/Auto) in config.rs
- `RepoSettingsEntry.auto_delete_on_pr_close: Option<AutoDeleteOnPrClose>`
- `RepoDefaultsConfig.auto_delete_on_pr_close: AutoDeleteOnPrClose` (default Off)
- TypeScript types, stores (repoDefaults.ts, repoSettings.ts), per-repo settings UI

**Settings UI**:
- Added dropdown to GeneralTab (global defaults) for autoFetchInterval + autoDeleteOnPrClose
- Per-repo dropdown in RepoWorktreeTab already present

**Rust commands** (worktree.rs):
- `check_worktree_dirty(repo_path, branch_name)` — checks `git status --porcelain` in worktree dir
- `delete_local_branch(repo_path, branch_name)` — removes worktree if linked, then `git branch -d`
- Safety: refuses to delete default branch via `get_remote_default_branch()` check
- 6 new tests (all passing)

**Frontend wiring** (github.ts → useAutoDeleteBranch.ts → App.tsx):
- `setOnPrTerminal` callback on github store, fired on merged/closed transitions
- `useAutoDeleteBranch` hook: reads effective setting, checks dirty state, handles off/ask/auto modes
- Deduplication via processed Set to prevent double-firing
- 3 new transition callback tests, 10 new hook tests (all passing)

**Tests**: 2216 total tests pass, 0 type errors

