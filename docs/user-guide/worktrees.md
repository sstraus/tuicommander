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

When using **Ask** mode, the cleanup dialog detects uncommitted changes and auto-stashes them during the branch switch. An "Unstash after switch" checkbox lets you restore changes on the target branch.

### Archive Script

A per-repo lifecycle hook that runs **before** a worktree is archived or deleted. Configure it in Settings → Repository → Scripts tab, or via `.tuic.json` (`archive_script` field).

- The script runs in the worktree directory that is about to be removed
- If the script exits with a non-zero code, the archive/delete operation is **blocked** and an error is shown
- Use cases: backing up local data, cleaning up resources, notifying external systems
- The script is invoked via the platform shell (`sh -c` on macOS/Linux, `cmd /C` on Windows)

## Moving Terminals Between Worktrees

Right-click a terminal tab → **Move to Worktree** to move it to a different worktree. The terminal will `cd` into the target worktree path, and the tab automatically reassigns to the new branch in the sidebar.

Also available via **Command Palette** — type "move to worktree" to see available targets for the active terminal.

## Removing Worktrees

- **Sidebar `×` button** on a non-main branch — Removes worktree and branch entry
- **Right-click → Delete Worktree** — Context menu option
- Both prompt for confirmation

Removing a worktree:
1. Closes all terminals associated with that branch
2. Runs `git worktree remove` to clean up
3. Removes the branch entry from the sidebar

## Worktree Manager Panel

Open the Worktree Manager with `Cmd+Shift+W` (or via the Command Palette → "Worktree Manager"). It shows a unified view of all worktrees across your repositories.

### What It Shows

Each worktree row displays:
- **Branch name** and **repository badge**
- **Dirty status** — file additions/deletions, or "clean"
- **PR state** — open (with PR number), merged, or closed
- **Last commit timestamp** — relative time since last activity
- **Main badge** — marks the main branch (actions disabled)

Orphan worktrees (detached HEAD or deleted branch) appear at the bottom with a warning badge and a **Prune** button to clean them up.

### Filtering

- **Repo pills** — Click a repository name to filter by repo (appears when you have multiple repos)
- **Text search** — Type in the search field to filter branches by name
- Filters compose: selecting a repo and typing text shows only matching branches in that repo

### Single-Row Actions

Each worktree row has action buttons (visible on the right):
- **`>_`** — Open a terminal in the worktree directory
- **`✔`** — Merge the branch into main and archive (disabled for main branches)
- **`✕`** — Delete the worktree and branch (disabled for main branches)

### Batch Operations

Select multiple worktrees using the checkboxes (shown when more than one selectable worktree exists). A batch bar appears with:
- **Merge & Archive (N)** — Merges and archives all selected branches
- **Delete (N)** — Deletes all selected worktrees

Use the **Select All** checkbox in the toolbar to toggle all non-main worktrees.

## MCP Worktree Creation (AI Agents)

AI agents connected via MCP can create worktrees using the `worktree action=create` tool.

### Claude Code — Agent Bridge

Claude Code cannot change its working directory mid-session. When CC creates a worktree via MCP, the response includes a `cc_agent_hint` field with:

- `worktree_path` — Absolute path to the worktree directory
- `suggested_prompt` — Instructions for spawning a subagent that works in the worktree using absolute paths

CC should spawn a subagent (Agent tool) with the suggested prompt. The subagent uses Read, Edit, Glob, Grep with absolute file paths and `cd <path> && ...` for shell commands.

### Other MCP Clients

Non-Claude Code MCP clients receive the standard `{worktree_path, branch}` response without the `cc_agent_hint` field. These clients can change into the worktree directory directly.

## External Worktree Detection

TUICommander monitors `.git/worktrees/` for changes. Worktrees created outside the app (via CLI or other tools) are detected and appear in the sidebar after the next refresh.

## Branch Switching

Switching branches in TUICommander does not change the working directory of existing terminals. Each branch's terminals stay in their worktree path.

When you switch branches:
- Previous branch's terminals are hidden (but remain alive)
- New branch's terminals are shown
- If the new branch has no terminals, a fresh one is created
