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

When an agent creates a worktree without spawning a separate session, its tab
stays associated with that worktree during repository refreshes. This changes
sidebar placement while preserving the shell's working directory. An explicit
worktree declaration also survives restart when a saved agent tab is restored.
Removing the worktree also removes its saved placement declaration, so a recycled
checkout does not leave an agent pointed at the old directory.

Suspended tabs still own their checkout. Close or resume and move these tabs before
removing, archiving, or recycling that checkout. Cleanup names the suspended tab
and refuses even a forced removal; recycling skips it. A removed checkout also
loses its old Worktrees bell offer, so that offer cannot open the obsolete path.

## Worktree Storage Strategies

Configure where worktrees are stored (Settings → Git & GitHub → Worktree Defaults → Storage):

| Strategy | Location | Use case |
|----------|----------|----------|
| **Sibling** (default) | `{repo_parent}/{repo_name}__wt/` | Keeps worktrees near the repo |
| **App directory** | `~/Library/Application Support/tuicommander/worktrees/{repo_name}/` | Centralised storage |
| **Inside repo** | `{repo_path}/.worktrees/` | Self-contained, add to `.gitignore` |
| **Claude Code default** | `{repo_path}/.claude/worktrees/` | Compatible with Claude Code's native `EnterWorktree` |

Override per-repo in Settings → Repository → Worktree.

## Warm Linked Worktrees

`git worktree add` checks out the tracked files and nothing else, so every
ignored build directory is missing and the first build is a full one.
TUICommander asks git which directories the parent ignores and copies them into
the new worktree copy-on-write, at the same near-zero disk cost. It is
language-agnostic — `node_modules`, `target`, `.venv`, `dist`, `vendor` and
anything else your `.gitignore` covers, except `.tmp` and `.mdkb` directories —
and it copies **directories only**: an
ignored *file* such as `.env` stays where it is, because materialising a
credential into a new workspace is not a convenience.

A moved cargo target directory keeps its cache. A CMake one does not:
`CMakeCache.txt` records the directory it was configured in, so anything built
through CMake (whisper.cpp here) reconfigures and rebuilds in the new worktree.

Warming is best-effort. Git has already produced a complete worktree by the time
it runs, so a copy that fails costs build time and nothing else — creation still
succeeds and the response carries a warning naming the directory that stayed
cold. Measured on this repository: **~38 s for 30 GB across 80k files**
(`node_modules` 19.5 s, `src-tauri/target` 17.4 s, everything else under 0.3 s).
For desktop, HTTP, and MCP creation, wait until the workspace's warm status is
`done` or `failed` before installing dependencies or building. When creation
runs a configured setup script through HTTP or MCP, it finishes before the copy
starts. Desktop setup and warming can overlap.

Copy-on-write warming needs filesystem support (APFS, Btrfs, XFS with reflink…) and both
directories on the same volume. TUICommander never trusts the filesystem *name*
for this — it makes a real copy-on-write copy of one file and looks at whether it
worked, trying macOS `clonefile` and then a reflink copy. Where neither works,
creation still succeeds with one warning and a cold worktree.
Copied files and directories gain owner write permission in the new worktree,
even when ignored build evidence is read-only in the source. Symlinks are not
followed, and the source permissions and contents stay unchanged.

## Creating Worktrees

### From the `+` Button (with prompt)

Click `+` next to a repository name. A dialog opens where you can:
- Type a new branch name (creates branch + worktree)
- Select an existing branch from the list
- Choose a "Start from" base ref (default branch, or any local branch)
- Generate a random sci-fi name

### From the `+` Button (instant mode)

When "Prompt on create" is off (Settings → Git & GitHub → Worktree Defaults), clicking `+` instantly creates a linked worktree with an auto-generated name based on the default branch.

The default branch is the main checkout's explicitly configured base when it
resolves to a commit. Otherwise, TUICommander uses local `main`, then local
`master`, then the remote HEAD branch, then the main checkout's attached branch. This also supports repositories with no remote and
a non-standard base branch. Linked worktrees do not supply the fallback branch.

### From Branch Right-Click (quick-clone)

Right-click any non-main branch without a worktree → **Create Worktree**. This creates a new branch named `{source}--{random-name}` based on the selected branch, with a worktree directory.

Every creation path uses the same linked-worktree implementation. The parent’s
tracked and untracked changes stay in the parent; only Git-ignored directories
are considered for warming.

## Worktree Settings

Global defaults apply to all repos. Per-repo overrides take precedence when set.

### Global Defaults (Settings → Git & GitHub → Worktree Defaults)

| Setting | Options | Default |
|---------|---------|---------|
| **Storage** | Sibling / App directory / Inside repo / Claude Code default | Sibling |
| **Prompt on create** | On / Off | On |
| **Delete branch on remove** | On / Off | On |
| **Auto-archive merged** | On / Off | Off |
| **Orphan cleanup** | Ask before removing / Auto-remove / Keep | Ask |
| **Safe orphan countdown** (Expert Mode) | 5 / 10 / 20 / 30 seconds | 10 seconds |
| **PR merge strategy** | Merge / Squash / Rebase | Merge |
| **After merge** | Archive / Delete / Ask | Archive |

A detached HEAD does not mean that a branch was deleted. Cleanup excludes a
checkout when a non-default local branch still points at its HEAD. A missing
checkout directory remains eligible for registration cleanup. Detached checkouts
without that branch protection retain the dirty-file, operation and live-session
checks. Clean, reachable checkouts without live sessions are removed silently;
other candidates require review in Ask mode. Choosing **Keep** holds until the
checkout fingerprint changes. An agent can answer an open review through MCP
`repo action=orphan_cleanup_answer path=<repo> decision=remove|keep`; the backend
checks removal again. Failed removals remain available for a later attempt.

### Per-Repository Overrides (Settings → Repository → Worktree)

Each setting can use the global default or be overridden for a specific repository.

## Merge & Archive

Right-click a worktree branch → **Merge & Archive** to:

1. Merge the branch into the main branch
2. Handle the worktree based on the "After merge" setting:
   - **Archive**: Moves the worktree directory to `__archived/` — the whole directory, uncommitted changes included. Its Git checkout, HEAD, reflog, and initialized submodules remain usable; it is hidden from the sidebar. A locked worktree is left untouched.
   - **Delete**: Removes the worktree and branch entirely. Anything not committed is gone
   - **Ask**: Merge succeeds, then you choose what to do

The merge uses `--no-edit` for a clean fast-forward or merge commit. If conflicts are detected, TUICommander attempts `git merge --abort` and leaves the worktree intact. If that abort fails, the error message tells you the repository may still be conflicted and includes the manual abort command. If your Git config sets `merge.ff=only` (or the target branch's `branch.<name>.mergeOptions` ends in `--ff-only`), TUICommander passes `--ff` for this merge so a diverged target gets a merge commit instead of failing with "Not possible to fast-forward"; `merge.ff=false` and default configs are unchanged.

### Uncommitted work in the worktree

Before manual removal, the confirmation distinguishes a branch with no commits of its own from one whose commits were merged. It names live terminal sessions and counts uncommitted and untracked files. Confirming still allows removal; review the warnings before proceeding. Automatic PR-close deletion keeps a worktree with live sessions or local changes and logs why it was skipped.

Both **Archive** and **Delete** remove the worktree, so one-click cleanup asks before merging when the worktree is not known to be clean or has a live terminal session. The confirmation names the sessions and what happens to the files: archived files travel to `__archived/`, deleted files do not come back. If the check itself cannot run, cleanup stops. Post-merge cleanup uses the same review before moving or deleting the checkout.

A branch can be integrated even when its original commits are absent from
main: squash merges and rebased copies change commit IDs. TUICommander checks
patches, the result of a virtual merge, and revised copies with matching
subjects. MCP exposes the reason with `repo branch_integration` or
`repo branch_integrations`. A branch that only follows its base is `in_sync`,
not integrated: its query returns `integrated: false` and no merge proof. A content-based result still needs a recovery ref
at `refs/archive/<branch>` pointing to the current tip before safe branch
deletion; incomplete or uncertain changes remain protected.

The automatic sweep never asks. It archives only a branch whose own commits are merged, when the worktree is clean and has no live sessions. It keeps other worktrees and names the reason in the status line.

Removing a worktree without force also refuses uncommitted changes when the
branch will be kept, including changes inside submodules. It stops if a Git operation is in progress. If an archive
script adds a commit after the removal check, the worktree can be removed, but
the branch stays and the result warns that the branch changed.
The confirmation lists submodule commits absent from remote-tracking branches.
A changed checkout status, HEAD, or submodule ref after force confirmation stops removal
for a fresh review. The status check tracks which paths are dirty, not further
edits to a path that was already dirty. Initialized submodule refs, stash entries,
and reflog-only commits are preserved in the main checkout's module repository
before removal. If the main checkout's copy of a
submodule is uninitialized or points at the wrong Git repository, removal stops
and leaves the worktree available for recovery. If the checkout directory is already missing, the confirmation identifies the stale Git registration and needs no fabricated dirty-file fingerprint. Cleanup skips the archive script because its working directory is gone. A lock requires a separate override. TUICommander preserves registered submodule refs before pruning, including modules whose configured names differ from their checkout paths. Each destination must have one valid path in its owning main module's `.gitmodules`; missing or malformed mappings stop cleanup and retain the registration.

If Git has already removed the registration but left a directory behind, cleanup
still cancels any pending build-input warming before reporting a directory error.

Deinitialized submodules with retained Git history also block removal, even when
their Git module name differs from the checkout path or they are nested.

When using **Ask** mode, the cleanup dialog detects uncommitted changes and auto-stashes them during the branch switch. An "Unstash after switch" checkbox lets you restore changes on the target branch. That stash covers the **base repository**; the warning under the worktree step is about the branch's own directory, which is a different place.

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

Cancelling or failing the initial confirmation keeps the workspace and its terminals. You can retry removal; duplicate requests for the same workspace remain serialized.

Removing a worktree:
1. Closes all terminals associated with that branch
2. Checks checkout and submodule state, restores owner write permission inside the worktree and owner search permission on directories before traversing them, then runs `git worktree remove` to clean up; a clean populated submodule needs one `--force` after a fresh safety check. If permission repair fails, the checkout stays registered and the error names its path. If Git unregisters the checkout but leaves files behind, TUICommander finishes removing that known checkout before it can delete the branch. If that cleanup fails, the error names the remaining directory.
3. Removes the branch entry from the sidebar
4. If branch deletion was requested, checks for unmerged commits before removing the worktree. An unmerged branch blocks a normal removal. A confirmed dirty-file removal can remove the checkout, but keeps an unmerged branch and reports a warning. Merge it first, or turn off **Delete branch on remove** to keep the branch.
5. If Git rejects branch deletion after removal despite the preflight check, shows a status message that the worktree was removed and the branch was kept.

Immediately before removal, TUICommander refreshes the backend lifecycle verdict
for the exact workspace id. Dirty files require an explicit destructive
confirmation; `Unknown` blocks removal. Commits live in the parent object store,
and a clean worktree with unmerged commits is marked `Unmerged` in the sidebar.
The proof also accepts a tip contained in the main checkout's current branch,
or a merged GitHub PR whose verified head contains the local tip after a squash.
An open or closed unmerged PR, or a PR head behind the local tip, is insufficient.
Turning off **Delete branch on remove** keeps the branch without needing merge
proof, while the clean-checkout and lock checks still apply.

For a local branch that has no worktree, agents can use MCP
`repo action=branch_delete` with the repository path and branch name. It
deletes only the local ref after proving its commits are in the checked-out
integration branch, including equivalent patches from a squash merge. A
checked-out, current, default, unmerged, or changed branch is kept; no remote
branch is deleted.

## Worktree Manager Panel

Open the Worktree Manager with `Cmd+Shift+W` (or via the Command Palette → "Worktree Manager"). It shows a unified view of all worktrees across your repositories.

### What It Shows

Each worktree row displays:
- **Branch name** and **repository badge**
- **Lifecycle badges** — dirty working-tree state and whether the worktree is `Merged` or `Unknown`, from the same progressive refresh used by the sidebar
- **Dirty status** — file additions/deletions, or "clean"
- **PR state** — open (with PR number), merged, or closed
- **Last commit timestamp** — relative time since last activity
- **Main badge** — marks the main branch (actions disabled)

Orphan worktrees (detached HEAD or deleted branch) appear at the bottom with a warning badge and a **Prune** button. Prune refuses a Git operation in progress and keeps detached commits that have no durable branch, tag, or other ref. Create a ref for that commit before retrying if you want to keep the commit and remove the checkout.

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

AI agents connected via MCP can create worktrees using `repo action=worktree_create`.

Because nothing enforces how an agent treats a workspace, the response also
carries instructions stating that tracked changes were not carried over,
identifying warmed build directories, and explaining that refs and objects are
shared with the parent.

When TUICommander receives a worktree creation event while the active terminal is running an agent, the confirmation offers **Open Worktree**. Accepting selects an existing terminal in the new worktree or creates one when needed. The running agent stays in its original terminal, branch, and working directory; TUICommander does not relabel or interrupt it.

### Claude Code — Agent Bridge

Claude Code cannot change its working directory mid-session. When CC creates a worktree via MCP, the response includes a `cc_agent_hint` field with:

- `worktree_path` — Absolute path to the worktree directory
- `suggested_prompt` — Instructions for spawning a subagent that works in the worktree using absolute paths

CC should spawn a subagent (Agent tool) with the suggested prompt. The subagent uses Read, Edit, Glob, Grep with absolute file paths and `cd <path> && ...` for shell commands.

### Other MCP Clients

Non-Claude Code MCP clients receive the standard `{worktree_path, branch}` response without the `cc_agent_hint` field. These clients can change into the worktree directory directly.

## External Worktree Detection

TUICommander monitors `.git/worktrees/` for changes. Worktrees created outside the app (via CLI or other tools) are detected and appear in the sidebar after the next refresh.
If a session starts in a new sibling worktree before that refresh, TUICommander refreshes its registered repository and places the tab under the new worktree.

## Branch Switching

Switching branches in TUICommander does not change the working directory of existing terminals. Each branch's terminals stay in their worktree path.

If a branch changes inside an existing linked worktree, its terminal sessions remain open and the sidebar updates their workspace to the new branch. Removing or recycling a worktree closes only terminals whose current working directory is inside that checkout. A repo-root terminal is never closed merely because a stale sidebar row lists it. Creating or declaring a worktree does not move the caller's tab unless its working directory is in that checkout. Reload and saved-tab restore also use the terminal's working directory, not an old worktree declaration. If a saved tab belongs to another workspace, restoring it does not activate it in the selected workspace or retain it in that workspace's pane layout. The selected workspace shows its own tabs or its normal empty state.

When you switch branches:
- Previous branch's terminals are hidden (but remain alive)
- New branch's terminals are shown
- If the new branch has no terminals, a fresh one is created

Force-deleting a branch preserves its tip at `refs/archive/<branch>`. It refuses a branch checked out in any worktree and never replaces a different existing archive: a reused branch name is archived as `refs/archive/<branch>-<sha7>`.

Worktree names that sanitize to an existing checkout name produce an error when they target a different branch. TUICommander keeps that checkout and its uncommitted files.

Automatic sidebar refreshes are spaced at least five seconds apart for each repository. Changes during that window are collected into one refresh at its end, so the final update is retained. Badges can lag by five seconds plus the time needed to read Git state.

Explicit repository additions, branch checkouts and merge cleanup refresh the affected repository immediately, while still waiting for any active refresh to finish.

### Reuse of finished worktrees

A safe removal keeps a clean, integrated checkout in the archive area for reuse.
A later worktree creation can reuse it under the new branch name without copying
ignored build artifacts. Freshly created worktrees stay reserved even before a session
starts. Dirty, unmerged, locked, and live worktrees are not reused.
