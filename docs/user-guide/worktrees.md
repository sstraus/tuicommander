# Git Worktrees

TUI Commander uses git worktrees to give each branch an isolated working directory.

## What Are Worktrees?

Git worktrees let you check out multiple branches simultaneously, each in its own directory. Instead of stashing or committing before switching branches, each branch has its own complete copy of the files.

## How TUI Commander Uses Them

When you click a non-main branch in the sidebar:

1. TUI Commander creates a git worktree for that branch
2. A terminal opens in the worktree directory
3. You work independently without affecting other branches

Main branches (main, master, develop) use the original repository directory — no worktree is created.

## Worktree Storage

Worktrees are stored in:
- macOS: `~/Library/Application Support/tui-commander/worktrees/`
- Linux: `~/.config/tui-commander/worktrees/`
- Windows: `%APPDATA%/tui-commander/worktrees/`

Named as: `{repo-name}--{branch-name}`

## Creating Worktrees

### From Existing Branch

Click any branch in the sidebar. If no worktree exists, one is created automatically.

### New Branch + Worktree

Click the `+` button next to a repository name. This creates a new branch (with a generated name) and its worktree.

## Worktree Configuration

Per-repository worktree settings (Settings → Repository → Worktree):

| Setting | Description |
|---------|-------------|
| **Base branch** | Branch to create worktrees from (auto, main, master, develop) |
| **Copy ignored files** | Copy .gitignored files to new worktrees |
| **Copy untracked files** | Copy untracked files to new worktrees |

### Setup Script

Configure a script to run after worktree creation (Settings → Repository → Scripts):

```bash
npm install
cp .env.example .env
```

This runs once, immediately after the worktree is created.

## Removing Worktrees

- **Sidebar `×` button** on a non-main branch — Removes worktree and branch entry
- **Right-click → Delete Worktree** — Context menu option
- Both prompt for confirmation

Removing a worktree:
1. Closes all terminals associated with that branch
2. Runs `git worktree remove` to clean up
3. Removes the branch entry from the sidebar

## Branch Switching

Switching branches in TUI Commander does not change the working directory of existing terminals. Each branch's terminals stay in their worktree path.

When you switch branches:
- Previous branch's terminals are hidden (but remain alive)
- New branch's terminals are shown
- If the new branch has no terminals, a fresh one is created
