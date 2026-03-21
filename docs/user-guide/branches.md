# Branch Management

TUICommander has a built-in Branches tab inside the Git Panel. It lets you view, create, delete, rename, merge, rebase, push, pull, and compare branches — all without leaving the app.

## Opening the Branch Panel

Three ways to open it:

- **`Cmd+G`** — Opens the Git Panel directly on the Branches tab
- **Click the "GIT" vertical label** in the sidebar — Opens the Git Panel on the Branches tab
- **`Cmd+Shift+D`**, then click the "Branches" tab header — Opens the Git Panel on the last active tab; click Branches to switch

## Branch List Overview

The panel shows two collapsible sections:

- **Local** — all branches in your local repo
- **Remote** — tracking branches from all remotes

Each branch row displays:
- Branch name
- Ahead/behind counts relative to its upstream (e.g. `↑2 ↓1`)
- Relative date of the last commit (e.g. "3h ago", "2d ago")
- **Merged** badge — shown on branches already merged into the default branch
- Stale dimming — branches with no commit in the last 30 days appear dimmed

A **Recent Branches** section at the top shows recently checked-out branches from the git reflog, for quick re-access.

## Prefix Folding

When you have many branches following a naming convention (`feature/`, `bugfix/`, `chore/`), prefix folding groups them automatically:

- Branches sharing a common `/`-delimited prefix collapse into a folder row (e.g. `feature/ (5)`)
- Click the folder row (or press `→` / `←`) to expand or collapse it
- The toggle button in the panel header enables or disables prefix folding globally

## Search / Filter

Type in the search bar at the top of the panel to filter branches by name. The filter applies to all sections simultaneously. Press `Escape` to clear the search.

## Keyboard Operations

With the Branches panel focused:

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate branches |
| `Enter` | Checkout selected branch |
| `n` | Create new branch |
| `d` | Delete selected branch |
| `R` | Rename selected branch (inline edit) |
| `M` | Merge selected branch into current |
| `r` | Rebase current branch onto selected |
| `P` | Push selected branch |
| `p` | Pull current branch |
| `f` | Fetch all remotes |
| `Escape` | Close panel |

Switching between Git Panel tabs:

| Key | Tab |
|-----|-----|
| `Ctrl/Cmd+1` | Changes |
| `Ctrl/Cmd+2` | Log |
| `Ctrl/Cmd+3` | Stashes |
| `Ctrl/Cmd+4` | Branches |

## Checkout

Press `Enter` (or double-click) on any branch to check it out.

If your working tree has uncommitted changes, a dialog appears with three options:
- **Stash** — automatically stashes changes, then checks out
- **Force** — discards changes and checks out
- **Cancel** — aborts the checkout

## Create Branch

Press **`n`** to open the inline branch creation form:

1. Type the new branch name
2. Optionally change the start point (defaults to HEAD)
3. Toggle "Checkout after create" (on by default)
4. Press `Enter` to confirm or `Escape` to cancel

## Delete Branch

Press **`d`** to delete the selected branch.

- Uses safe delete (`git branch -d`) by default — refuses to delete unmerged branches
- A confirmation prompt lets you switch to force-delete (`git branch -D`) if needed
- Deleting the current branch or the default branch (`main`, `master`, `develop`) is blocked

## Rename Branch

Press **`R`** to edit the branch name inline. The current name is pre-filled. Press `Enter` to confirm or `Escape` to cancel.

## Merge

Press **`M`** to merge the selected branch into the current branch. The merge runs in the background. Conflicts are reported in the Git Panel header.

## Rebase

Press **`r`** to rebase the current branch onto the selected branch. Runs in the background; conflicts are reported.

## Push

Press **`P`** to push the current branch. If no upstream is set, TUICommander automatically configures the tracking relationship (`--set-upstream origin <branch>`).

## Pull

Press **`p`** to pull the current branch from its upstream.

## Fetch

Press **`f`** to fetch all remotes (`git fetch --all`).

## Context Menu

Right-click any branch for the full context menu:

| Action | Description |
|--------|-------------|
| Checkout | Switch to this branch |
| Create Branch from Here | Create a new branch starting from this commit |
| Delete | Delete branch (safe by default) |
| Rename | Rename inline |
| Merge into Current | Merge this branch into the current one |
| Rebase Current onto This | Rebase current branch onto this one |
| Push | Push this branch |
| Pull | Pull this branch |
| Fetch | Fetch all remotes |
| Compare | Show `git diff --name-status` between this branch and current |

## Stale and Merged Indicators

- **Stale** (dimmed): the branch has no commits in the last 30 days — a visual cue that it may be abandoned
- **Merged** badge: the branch has been merged into the default branch and is safe to delete
