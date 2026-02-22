# GitHub Integration

TUICommander monitors your GitHub PRs and CI status automatically.

## Requirements

- `gh` CLI installed and authenticated (`gh auth login`)
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

## Troubleshooting

**No PR data showing:**
1. Check `gh auth status` — must be authenticated
2. Check repository has a GitHub remote (`git remote -v`)
3. Check that `gh pr list` works in the repo directory

**Stale data:**
- Click the refresh button or switch away and back to the branch
- Polling updates every 30 seconds automatically
