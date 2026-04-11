# GitHub Integration

TUICommander monitors your GitHub PRs and CI status automatically.

## Authentication

TUICommander needs a GitHub token to access PRs and CI status. You have two options:

### Option 1: OAuth Login (Recommended)

1. Open **Settings > GitHub**
2. Click **"Login with GitHub"**
3. A code appears — it's auto-copied to your clipboard
4. Your browser opens GitHub's authorization page
5. Paste the code and authorize
6. Done — the token is stored securely in your OS keyring

This method automatically requests the correct scopes (`repo`, `read:org`) and works with private repositories and organization repos.

### Option 2: gh CLI

If you prefer, install the `gh` CLI and run `gh auth login`. TUICommander will use the `gh` token automatically.

### Token Priority

When multiple sources are available, TUICommander uses this priority:

1. `GH_TOKEN` environment variable
2. `GITHUB_TOKEN` environment variable
3. OAuth token (from Settings login)
4. `gh` CLI token

## Requirements

- GitHub authentication (see above)
- Repository with a GitHub remote

## PR Monitoring

When you have a branch with an open pull request, the sidebar shows:

### PR Badge

A colored badge next to the branch name showing the PR number:

| Color | State |
|-------|-------|
| Green | Open PR |
| Purple | Merged |
| Red | Closed |
| Gray/dim | Draft |

Click the PR badge to open the PR detail popover.

### CI Ring

A circular indicator showing CI check status:

| Segment | Color | Meaning |
|---------|-------|---------|
| Green arc | — | Passed checks |
| Red arc | — | Failed checks |
| Yellow arc | — | Pending checks |

The ring segments are proportional to the number of checks in each state. Click to see detailed CI check information.

### Diff Stats

Per-branch addition/deletion counts shown as `+N / -N` next to the branch name.

## PR Detail Popover

Click a PR badge or CI ring to open the detail popover. Shows:

- **PR title and number** with link to GitHub
- **Author and timestamps** (created, last updated)
- **State indicators:**
  - Draft/Open/Merged/Closed
  - Merge readiness (Ready to merge, Has conflicts, Behind base, Blocked)
  - Review decision (Approved, Changes requested, Review required)
- **CI check details** — Individual check names and status
- **Labels** — With GitHub-matching colors
- **Line changes** — Total additions and deletions
- **Commit count**

## PR Notifications (Toolbar Bell)

When any branch has a PR event that needs attention, a bell icon with a count badge appears in the toolbar.

**Click the bell** to see all active notifications in a popover list. Each notification shows the repo, branch, and event type.

### Notification Types

| Type | Meaning |
|------|---------|
| Merged | PR was merged |
| Closed | PR was closed without merge |
| Conflicts | Merge conflicts detected |
| CI Failed | One or more CI checks failed |
| Changes Req. | Reviewer requested changes |
| Ready | PR is ready to merge (all checks pass, approved) |

### Interacting with Notifications

- **Click a notification item** — Opens the full PR detail popover for that branch
- **Click the dismiss (x) button** on an item — Dismiss that single notification
- **Click "Dismiss All"** — Clear all notifications at once

### PR Badge on Sidebar Branches

Click the colored PR status badge on any branch in the sidebar to open the PR detail popover directly.

## Polling

GitHub data is polled automatically:

- **Active window:** Every 30 seconds
- **Hidden window:** Every 2 minutes (reduced to save API budget)
- **API budget:** ~2 calls/min/repo = 1,200/hr for 10 repos (GitHub limit: 5,000/hr)

Polling starts automatically when a repository with a GitHub remote is active.

## Merge State Classification

| State | Label | Meaning |
|-------|-------|---------|
| MERGEABLE + CLEAN | Ready to merge | All checks pass, no conflicts |
| MERGEABLE + UNSTABLE | Checks failing | Mergeable but some checks fail |
| CONFLICTING | Has conflicts | Merge conflicts with base branch |
| BEHIND | Behind base | Base branch has newer commits |
| BLOCKED | Blocked | Branch protection prevents merge |
| DRAFT | Draft | PR is in draft state |

## Review State Classification

| Decision | Label |
|----------|-------|
| APPROVED | Approved |
| CHANGES_REQUESTED | Changes requested |
| REVIEW_REQUIRED | Review required |

## Auto-Delete Branch on PR Close

When a PR is merged or closed on GitHub, TUICommander can automatically clean up the corresponding local branch. Configure per-repo in **Settings > Repository Settings** or set a global default in **Settings > General > Repository Defaults**.

| Mode | Behavior |
|------|----------|
| **Off** (default) | No action taken |
| **Ask** | Shows a confirmation dialog before deleting |
| **Auto** | Deletes silently; falls back to Ask if worktree has uncommitted changes |

**Safety guarantees:**
- The default/main branch is never deleted
- If a branch has a linked worktree, the worktree is removed first
- Uses safe `git branch -d` — refuses to delete branches with unmerged commits
- Dirty worktrees (uncommitted changes) always escalate to Ask mode, even when set to Auto

## Remote-Only Pull Requests

When a branch exists only on the remote (not checked out locally) but has an open PR, it still appears in the sidebar with a PR badge. These "remote-only" PRs support inline accordion actions:

- **Checkout** — Creates a local tracking branch from the remote
- **Create Worktree** — Creates a worktree for the branch

### PR Detail Popover Actions

Clicking the PR badge on any branch (local or remote-only) opens the detail popover. Available actions:

| Button | When Shown | What It Does |
|--------|------------|--------------|
| **View Diff** | Always | Opens PR diff in a dedicated panel tab |
| **Merge** | PR is open, approved, CI green | Merges via GitHub API (auto-detects allowed merge method) |
| **Approve** | Remote-only PRs | Submits an approving review via GitHub API |

### Post-Merge Cleanup

After merging a PR from the popover, a **cleanup dialog** appears with checkable steps:

1. **Switch to base branch** — if the working directory has uncommitted changes, they are automatically stashed. An inline warning shows with an optional "Unstash after switch" checkbox
2. **Pull base branch** — fast-forward only
3. **Delete local branch** — closes terminals first, refuses to delete default branch
4. **Delete remote branch** — gracefully handles "already deleted"

Steps execute sequentially via the Rust backend (not PTY — your terminal may be occupied by an AI agent). Each step shows live status: pending → running → success/error.

### Dismiss & Show Dismissed

Remote-only PRs can be dismissed from the sidebar to reduce clutter. A "Show Dismissed" toggle in the sidebar reveals them again.

## GitHub Issues

The GitHub panel shows issues alongside PRs in a unified view.

### Issue Filter

Control which issues appear using the filter dropdown in **Settings > GitHub** or directly in the panel:

| Filter | Shows |
|--------|-------|
| **Assigned** (default) | Issues assigned to you |
| **Created** | Issues you opened |
| **Mentioned** | Issues that mention you |
| **All** | All open issues in the repo |
| **Disabled** | Hides the issues section |

The filter setting persists across sessions.

### Issue Details

Expand an issue to see:
- **Labels** with GitHub-matching colors
- **Assignees** and **milestone**
- **Comment count** and **timestamps** (created/updated)

### Issue Actions

| Action | Description |
|--------|-------------|
| Open in GitHub | Opens the issue in your browser |
| Close / Reopen | Changes issue state via GitHub API |
| Copy number | Copies `#123` to clipboard |

## Troubleshooting

**No PR data showing:**
1. Check `gh auth status` — must be authenticated
2. Check repository has a GitHub remote (`git remote -v`)
3. Check that `gh pr list` works in the repo directory

**Stale data:**
- Click the refresh button or switch away and back to the branch
- Polling updates every 30 seconds automatically
