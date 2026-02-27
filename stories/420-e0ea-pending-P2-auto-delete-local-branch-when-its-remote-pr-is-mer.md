---
id: "420-e0ea"
title: "Auto-delete local branch when its remote PR is merged or closed on GitHub"
status: pending
priority: P2
created: 2026-02-27T08:28:54.915Z
updated: 2026-02-27T08:28:54.915Z
dependencies: []
---

# Auto-delete local branch when its remote PR is merged or closed on GitHub

## Problem Statement

When a PR is merged or closed on GitHub, the remote branch is usually deleted automatically. But the local branch and any associated worktree remain, cluttering the sidebar. Users have to manually delete local branches that no longer have a remote counterpart. A per-repo setting should detect when a tracked PR transitions to merged/closed and offer to (or automatically) delete the corresponding local branch and clean up worktrees.

## Acceptance Criteria

- [ ] Add autoDeleteOnPrClose setting to per-repo config (options: off, ask, auto; default: off)
- [ ] Detect PR merged/closed state transitions in the existing GitHub polling loop
- [ ] When triggered and mode is ask: show in-app confirm dialog listing branch name and PR number
- [ ] When triggered and mode is auto: delete local branch silently, log to appLogger
- [ ] If branch has a linked worktree, remove worktree first then delete branch
- [ ] Never auto-delete the base/default branch
- [ ] Setting visible in Settings > repo settings section
- [ ] Handle edge case: branch has uncommitted changes — always ask, never auto-delete

## Work Log

