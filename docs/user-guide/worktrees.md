# Git Worktrees

TUICommander uses git worktrees to give each branch an isolated working directory.

## What Are Worktrees?

Git worktrees let you check out multiple branches simultaneously, each in its own directory. Instead of stashing or committing before switching branches, each branch has its own complete copy of the files.

## How TUICommander Uses Them

When you click a non-main branch in the sidebar:

1. TUICommander creates a git worktree for that branch
2. A terminal opens in the worktree directory
3. You work independently without affecting other branches

Main branches (main, master, develop) use the original repository directory — no worktree is created.

## Worktree Storage Strategies

Configure where worktrees are stored (Settings → General → Worktree Defaults → Storage):

| Strategy | Location | Use case |
|----------|----------|----------|
| **Sibling** (default) | `{repo_parent}/{repo_name}__wt/` | Keeps worktrees near the repo |
| **App directory** | `~/Library/Application Support/tuicommander/worktrees/{repo_name}/` | Centralised storage |
| **Inside repo** | `{repo_path}/.worktrees/` | Self-contained, add to `.gitignore` |

Override per-repo in Settings → Repository → Worktree.

## Creating Worktrees

### From the `+` Button (with prompt)

Click `+` next to a repository name. A dialog opens where you can:
- Type a new branch name (creates branch + worktree)
- Select an existing branch from the list
- Choose a "Start from" base ref (default branch, or any local branch)
- Generate a random sci-fi name

### From the `+` Button (instant mode)

When "Prompt on create" is off (Settings → Worktree Defaults), clicking `+` instantly creates a worktree with an auto-generated name based on the default branch.

### From Branch Right-Click (quick-clone)

Right-click any non-main branch without a worktree → **Create Worktree**. This creates a new branch named `{source}--{random-name}` based on the selected branch, with a worktree directory.

## Worktree Settings

Global defaults apply to all repos. Per-repo overrides take precedence when set.

### Global Defaults (Settings → General → Worktree Defaults)

| Setting | Options | Default |
|---------|---------|---------|
| **Storage** | Sibling / App directory / Inside repo | Sibling |
| **Prompt on create** | On / Off | On |
| **Delete branch on remove** | On / Off | On |
| **Auto-archive merged** | On / Off | Off |
| **Orphan cleanup** | Manual / Prompt / Auto | Manual |
| **PR merge strategy** | Merge / Squash / Rebase | Merge |
| **After merge** | Archive / Delete / Ask | Archive |

### Per-Repository Overrides (Settings → Repository → Worktree)

Each setting can use the global default or be overridden for a specific repository.

## Merge & Archive

Right-click a worktree branch → **Merge & Archive** to:

1. Merge the branch into the main branch
2. Handle the worktree based on the "After merge" setting:
   - **Archive**: Moves the worktree directory to `__archived/` (accessible but removed from sidebar)
   - **Delete**: Removes the worktree and branch entirely
   - **Ask**: Merge succeeds, then you choose what to do

The merge uses `--no-edit` for a clean fast-forward or merge commit. If conflicts are detected, the merge is aborted and the worktree is left intact.

## Removing Worktrees

- **Sidebar `×` button** on a non-main branch — Removes worktree and branch entry
- **Right-click → Delete Worktree** — Context menu option
- Both prompt for confirmation

Removing a worktree:
1. Closes all terminals associated with that branch
2. Runs `git worktree remove` to clean up
3. Removes the branch entry from the sidebar

## External Worktree Detection

TUICommander monitors `.git/worktrees/` for changes. Worktrees created outside the app (via CLI or other tools) are detected and appear in the sidebar after the next refresh.

## Branch Switching

Switching branches in TUICommander does not change the working directory of existing terminals. Each branch's terminals stay in their worktree path.

When you switch branches:
- Previous branch's terminals are hidden (but remain alive)
- New branch's terminals are shown
- If the new branch has no terminals, a fresh one is created
