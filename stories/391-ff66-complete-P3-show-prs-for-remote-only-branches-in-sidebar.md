---
id: 391-ff66
title: Show PRs for remote-only branches in sidebar
status: complete
priority: P3
created: "2026-02-26T08:19:59.442Z"
updated: "2026-02-27T07:08:40.300Z"
dependencies: []
---

# Show PRs for remote-only branches in sidebar

## Problem Statement

TUICommander only shows PR indicators for local branches/worktrees. If a PR headRefName has no matching local branch, the PR is fetched from GitHub but never displayed anywhere. The user has no visibility into open PRs on remote-only branches.

## Design Decision

**Badge on repo header + popover flyout.**

### Badge

- A small counter badge on the repo header row showing the number of open PRs on remote-only branches (no local branch or worktree)
- Hidden when count is 0
- Only counts PRs whose headRefName does NOT match any key in `repo.branches`

### Popover

Click the badge opens a popover/flyout listing the remote-only PRs:

```
┌─────────────────────────────────┐
│ #42  fix-auth        Draft      │
│ #38  add-logging     Review Req.│
│ #35  refactor-db     CI Running │
└─────────────────────────────────┘
```

Each row shows: PR number, branch name, PR state badge (reuse existing PrStateBadge component).

### Actions per row

- **Click row** → open PR detail panel (existing)
- **Checkout button** → `git checkout -b <branch> origin/<branch>` — creates local branch tracking remote, switches main worktree to it. Branch then appears in sidebar normally with PR badge. PR disappears from popover (count decrements).
- **Context menu → Create Worktree** → creates a linked worktree from the remote branch for parallel work without switching main worktree.

## Acceptance Criteria

- [ ] Compute remote-only PR count: open PRs from githubStore whose headRefName is not in repo.branches
- [ ] Show badge with count on repo header row (hidden when 0)
- [ ] Click badge opens popover with PR list (number, branch, PrStateBadge)
- [ ] Click PR row opens PR detail panel
- [ ] "Checkout" action runs `git checkout -b <branch> origin/<branch>`, refreshes sidebar
- [ ] Context menu "Create Worktree" creates linked worktree from the remote branch
- [ ] PR disappears from popover once it has a local branch/worktree
- [ ] Popover follows STYLE_GUIDE.md patterns

## Work Log

### 2026-02-27T07:08:37.241Z - Completed: Added remote-only PR badge in repo header, RemoteOnlyPrPopover component with checkout/worktree actions, checkout_remote_branch Tauri command + HTTP route, getRemoteOnlyPrs github store method, handleCheckoutRemoteBranch in useGitOperations. All wired through Sidebar → App.

