# GitHub Integration

TUICommander monitors your GitHub PRs and CI status automatically.

PR creation, issue creation, and review submission use the repository's selected account over HTTP, including GitHub Enterprise. These writes do not require an installed `gh` executable.

## Authentication

TUICommander needs a GitHub token to access PRs and CI status. You have two options:

### Option 1: OAuth Login (Recommended)

1. Open **Settings > Git & GitHub**
2. Click **"Login with GitHub"**
3. A code appears — it's auto-copied to your clipboard
4. Your browser opens GitHub's authorization page
5. Paste the code and authorize
6. Done — the token is stored securely in your OS keyring

Settings shows the requested scopes before login, using the same scope list as the login request. This method automatically requests the correct scopes (`repo` and `read:project`) and works with private repositories and organization repos.

### Option 2: Environment variable

Set `GH_TOKEN` or `GITHUB_TOKEN` before you start TUICommander. A macOS app started from the Dock or Finder does not see variables from your shell profile, so OAuth login is the practical choice there.

TUICommander does not read the `gh` CLI login. If you only signed in with `gh auth login`, the GitHub tab shows "Not connected" until you use OAuth login or set a token variable.

OAuth credentials show **Logout**. It removes the stored OAuth token and falls back to an environment token, if present. Environment credentials show **Switch to OAuth** and **Disconnect**. Disconnect stops using the default account until you log in again or restart the app; it does not remove your environment variables.

### Token Priority

When multiple sources are available, TUICommander uses this priority:

1. `GH_TOKEN` environment variable
2. `GITHUB_TOKEN` environment variable
3. OAuth token (from Settings login)

## Multiple Accounts (github.com + GitHub Enterprise)

TUICommander is account-centric: you can monitor repos across **several GitHub accounts at once** — additional github.com logins and GitHub Enterprise Server (GHE) instances — and bind each repo to the account that should monitor it. If you only use one github.com account, nothing changes; skip this section.

### Adding accounts

Open **Settings > Git & GitHub > Additional GitHub Accounts**. The account you logged in with above is your default.

- **Another github.com account** — click **"Add another github.com account"** and complete the device flow (same as the primary login).
- **GitHub Enterprise Server** — under **"Add Enterprise account"**, enter the host (e.g. `github.mycompany.com`) and a **Personal Access Token** with `repo` scope, then click **Add account**. The PAT is validated against your GHE server and stored in the OS keyring. (GHE uses a PAT because there is no per-host OAuth App.)

Each account shows its login, avatar, and a source badge. Remove an account to delete its token, its repo bindings, and its cached state.

### Binding repos to accounts

Open **Settings > Git & GitHub > Repository Bindings**. Each workspace repo shows how it resolves:

- **Bound** — the repo is monitored by the shown account. Click **Unbind** to detach it.
- **Needs binding** — the repo matches more than one account or GitHub remote. TUICommander does **not** guess `origin`; pick the right account/remote from the chooser to bind it.
- **Needs account** — a github.com repo with no account configured yet; add one first.

A repo with exactly one matching account/remote resolves automatically without a saved binding. Repository Bindings stays hidden in this single-account setup. A fork and an upstream repository on the same account still require a remote choice because they have different PRs and issues. Worktrees share their main checkout's binding.

### How multiple accounts behave

- Each account polls independently with its own rate-limit budget and circuit breaker, so a rate limit or auth failure on one account never stalls the others.
- **Limitation:** CI Auto-Heal (which shells out to `gh`) is disabled for GHE-bound repos and shows a clear message. PRs, CI status, issues, merge, and approve all work against bound GHE repos.

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

### OS Notifications

While TUICommander is in the background, **ready**, **CI failed**, **changes requested** and **merged** transitions also raise an operating-system notification showing `repo #number: title`. On macOS, clicking it opens the PR on GitHub. The same event delivered twice within two minutes notifies once. Turn it off in **Settings > Notifications > Pull Requests**.

### PR Badge on Sidebar Branches

Click the colored PR status badge on any branch in the sidebar to open the PR detail popover directly.

## Polling

GitHub data is polled automatically:

- **Active window:** Every 60 seconds (backs off to 120 s when the rate-limit budget runs low, up to 300 s)
- **Hidden window:** Every 2 minutes (reduced to save API budget)
- **API budget:** one batched GraphQL query per repository per poll. Measured with `rateLimit(dryRun: true)`: 2 points for 1 repository and 6 for 6 (5 before the unresolved-thread count was added), i.e. about 360 points/hr for 6 repositories at 60 s (GitHub limit: 5,000/hr). Unresolved threads are read as a plain `isResolved` flag (first 50 per PR); reading the thread comments in the batch would have cost 17 points for 1 repository and 51 for 6, so the bot/human split is fetched per PR, on demand, when the PR panel opens.

Polling starts automatically when a repository with a GitHub remote is active.

## Readiness Verdict

The sidebar badge, the PR panel chip and the Ops dashboard all show one verdict, computed in a single function (`src/utils/prReadiness.ts`). First match wins: Draft, Merged, Closed, Conflicts, Checking (GitHub still computing mergeability), CI Failed, Changes Req., Comments (unresolved review threads), Review, CI Running, Ready (approved, mergeable, nothing outstanding), otherwise Open.

The PR panel also shows the number of unresolved review threads, split into human and bot threads (a bot is a GitHub App/bot author; the author of a thread is the author of its first comment). Counts cover the first 50 threads of a PR.

## Merge State Classification

| State | Label | Meaning |
|-------|-------|---------|
| MERGEABLE + CLEAN | Ready to merge | All checks pass, no conflicts |
| MERGEABLE + UNSTABLE | Checks failing | Mergeable but some checks fail |
| CONFLICTING | Has conflicts | Merge conflicts with base branch |
| BEHIND | Behind base | Base branch has newer commits |
| BLOCKED | Blocked | Branch protection prevents merge |
| DRAFT | Draft | PR is in draft state |

## CI Auto-Heal

Actions log fetching uses the repository account on github.com or GitHub Enterprise and does not require the GitHub CLI. It keeps failure tails and stops the log sweep after 60 seconds.

Enable **Auto-heal** in a blocked PR's detail popover to send failed GitHub Actions or configured CircleCI job logs to the agent terminal assigned to that branch. A completed failed job can trigger healing while other jobs in the same workflow are still running. The displayed three-attempt budget counts only prompts successfully delivered to the agent; log-fetch and terminal-delivery errors do not consume it.

Auto-heal reads failing checks directly from the selected or local head commit, including external CI links. It uses the repository's GitHub account and reads every check page before discarding older reruns; the number of open PRs does not limit this lookup.

Set a read-only CircleCI token in **Settings > Git & GitHub > CircleCI**. The token is never shown again. The Log button on a failed CircleCI check also works for remote-only PRs.

## Review State Classification

| Decision | Label |
|----------|-------|
| APPROVED | Approved |
| CHANGES_REQUESTED | Changes requested |
| REVIEW_REQUIRED | Review required |

## Auto-Delete Branch on PR Close

When a PR is merged or closed on GitHub, TUICommander can automatically clean up the corresponding local branch. Configure per-repo in **Settings > Repository Settings** or set a global default in **Settings > Git & GitHub > Pull Requests > Auto-Delete on PR Close**.

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
| **Merge** | PR is open, approved, CI green | Merges via GitHub API (auto-detects allowed merge method). The merge is pinned to the head commit the panel showed: if the agent pushed since, GitHub refuses and the panel shows "PR head changed" — refresh and review before merging again |
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

Control which issues appear using the filter dropdown in **Settings > Git & GitHub** or directly in the panel:

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

### PR Row Actions (panel)

Expand a PR in the GitHub panel for these actions, next to Checkout, Merge and Diff:

| Action | When Shown | What It Does |
|--------|------------|--------------|
| **Update branch** | PR is open and GitHub reports it behind its base | Merges the base branch into the PR branch, pinned to the head the panel showed (refuses with "PR head changed" if the agent pushed meanwhile). The merge commit lands on the remote: pull it in any local clone |
| **Close** | PR is open | Closes the PR without merging, after a confirmation |
| **Copy ref** | PR has a GitHub URL | Copies `owner/repo#number` for pasting into prompts |

Open PRs older than two weeks carry an age marker on the row: `2w`, `1m`, `3m` or `6m`, measured from the creation date.

### PR Diff: collapsed files

In the PR diff tab, lockfiles (`pnpm-lock.yaml`, `Cargo.lock`, ...), generated files (`dist/`, `*.min.js`, `*.snap`, `*.pb.go`, files marked `linguist-generated` in the repo's `.gitattributes`) and test files (`__tests__/`, `*.test.*`, `*_test.go`, ...) start collapsed. The header shows how many are collapsed; click a file header to expand it. `.gitattributes` is read from the local checkout, which stands in for the PR's base when another branch is checked out.

## Panel Keyboard Navigation

The GitHub panel is keyboard-navigable without ever moving focus off the panel itself:

| Key | Action |
|-----|--------|
| ↓ / ↑ | Move between rows, walking across the My Pull Requests, Pull Requests and Issues sections in order. Rows of a collapsed section are skipped. |
| Enter | Expand or collapse the highlighted row |
| Escape | Collapse an expanded issue, or close the panel when nothing is expanded |

Section collapse state is remembered per section and survives closing the panel and restarting the
app (stored in the UI prefs alongside panel widths). A section that has never been toggled keeps its
own default: PR sections start collapsed when empty, Issues starts open.

## Troubleshooting

**No PR data showing:**
1. Check Settings > Git & GitHub — the account must show as signed in
2. Check repository has a GitHub remote (`git remote -v`)
3. Check that the repository is bound to the intended account

**Stale data:**
- Click the refresh button or switch away and back to the branch
- Polling updates every 60 seconds automatically

Merged-PR evidence for worktree and local-branch cleanup uses the GitHub account
and host selected for that repository. It no longer depends on the GitHub CLI
login. If the complete PR history cannot be read, cleanup does not treat that
lookup as proof that a branch was merged.

### Tasks notifications

Settings → Notifications → Tasks has **OS notifications for Tasks**, off by default and independent of the existing PR notification preference. Failed checks on a linked GitHub PR use the existing PR notice once and name the linked Tasks item. Click the notice to open the PR.

Use **Follow task** in an item detail to follow comments locally; use **Unfollow task** to stop. Follow state and the policy are saved in the existing Tasks UI preferences. A first observation establishes a silent baseline. Existing visible detail refreshes (including its 60-second refresh) and provider-bridge detail reads compare authoritative comments; there is no extra background poller. Your own comment action advances the baseline silently. Native notices recheck the Tasks policy after permission is granted and retain the existing background-window policy; browser clients do not send native OS notices.

GitHub provides a full normal-comment count, so an increase is detected even beyond the first page. Jira and GitLab provide paged comment entries: newly observed comments notify when their creation time is newer than the prior baseline, or when a previously unseen comment ID shares its latest timestamp. Unloaded pages, unseen review sources and items that are not refreshed cannot produce a notice. Editing a comment or loading older history does not notify. No Linear provider is shipped, so it is not covered.

GitHub logins created before Projects v2 support must sign in again to consent to `read:project`. Existing credentials are not upgraded automatically; a Projects read can return `INSUFFICIENT_SCOPES` until reauthorization.

## Setup through Settings MCP

After you enable the Settings MCP tool in trusted UI, an assistant can discover the published GitHub owner actions. Cloud setup uses device login; you authenticate on GitHub, and the assistant follows the returned flow state and poll interval. Enterprise PAT setup verifies the host/account before storage. A pending or expired flow is not a connected account. [Settings startup workflows](../backend/settings-startup.md#github) documents `list_accounts` for stable account IDs and credential presence, plus `/auth/source` for current credential provenance. Enumeration never returns token values or proves a live account login.

## Persistent PR visibility filters

Global `pr_hide_drafts`, `pr_hide_conflicting` and `pr_hide_ci_failing` values
persist in `config.json` and default to false (show all). Each repository stores
nullable overrides in `repo-settings.json`: null inherits the current global
value, true hides the matching PRs, and false explicitly shows them. Settings
owner transactions preserve unrelated fields, and settings refresh adopts owner
readback without saving it again. The Rust settings catalogue describes both scopes.

### Troubleshoot repository access

Settings > Git & GitHub shows repository access failures by owner and account,
with GitHub's exact message. Reasons distinguish NOT_FOUND, FORBIDDEN and unknown
responses without inferring organization policy from message wording.

For any github.com OAuth failure, select **Grant or request organization access**
to open the authorized TUICommander OAuth app page. This action is also available
for NOT_FOUND because pending organization approval can hide private repositories. After granting access, select **Refresh status** to clear NOT_FOUND retry
delays for all accounts, then wait for the next monitoring poll. Access denials do not require a new
login or a 24-hour wait. Only explicit `NOT_FOUND` responses keep the 24-hour retry
interval. GitHub can use this response for an inaccessible private repository too.
