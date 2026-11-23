# Sidebar

The sidebar is your primary navigation for repositories, branches, and git operations.

## Session identifiers

Right-click a session row under any main or linked branch to open its menu. **Copy session ID** copies the PTY session ID. **Copy process ID** copies the live PTY child PID, not an agent tool subprocess. A toast confirms each copy. **Copy alias** copies the exact alias accepted by MCP (for example, `orc-1`). The process and alias entries are hidden when their values are unknown. Hover an entry to see its value; process and alias values are read again before copying. Left-click still selects the terminal.

## Toggle & Resize

- **Toggle visibility:** `Cmd+[`
- **Resize:** Drag the right edge (200–500px range)
- Width persists across sessions

## Repository Management

### Adding Repositories

Click the `+` button at the top of the sidebar and select a git repository folder.

### Repository Entry

Each repo shows a header with the repo name and action buttons:

- **Click** the header to expand/collapse the branch list
- **Click again** to toggle icon-only mode (shows repo initials — saves space)
- **`⋯` button** — Opens a menu with: Repo Settings, Create Worktree, Move to Group (with submenus for existing groups, Ungrouped, and New Group), Park Repository, Remove Repository
- **Right-click main worktree row** → **Switch Branch** submenu: shows all local branches with a checkmark on the current one. If the working tree is dirty, prompts to stash changes first. Blocks switching when a terminal has a running process.

### Removing Repositories

Repo `⋯` → Remove. This only removes the repo from the sidebar — it does not delete any files.

## Repository Groups

Organize repos into named, colored groups.

### Creating a Group

- Repo `⋯` → **Move to Group** → **New Group...**
- Enter a name in the dialog

### Moving Repos Between Groups

- **Drag** a repo onto a group header
- Or: Repo `⋯` → **Move to Group** → select a group
- To ungroup: Repo `⋯` → **Move to Group** → **Ungrouped**

### Managing Groups

Right-click a group header for:

- **Rename** — Change the group name
- **Change Color** — Pick a new accent color
- **Delete** — Remove the group (repos become ungrouped)

Groups can be collapsed/expanded by clicking the header, and reordered by drag-and-drop.

## Branches

### Selecting a Branch

Click a branch name to switch to it. This:

1. Creates a git worktree (for non-main branches) if one doesn't exist
2. Shows the branch's terminals (or creates a new one)
3. Hides terminals from the previous branch

### Branch Indicators

Each branch row can show:

| Indicator | Meaning |
|-----------|---------|
| **CI ring** | Proportional arc segments — green (passed), red (failed), yellow (pending) |
| **PR marker** | A colored square and the PR number. The color shows the highest-priority state; a conflict turns the square into a diamond. Hover or focus it for the state name, such as Draft, Conflicts, or CI Failed. Click for detail popover. |
| **Diff stats** | `+N / -N` additions and deletions, always shown beside the PR marker. Hover for exact counts and uncommitted files. |
| **Dirty badge** | The workspace contains staged, unstaged, or untracked changes. Hover or focus the badge for the removal-safety explanation. |
| **Merged badge** | Branches merged into main show a "Merged" badge |
| **Unmerged mark** | A small `↑` means the worktree has commits outside the default branch. A zero diff or dirty count does not mean those commits are merged. |
| **Unknown badge** | TUICommander could not verify the workspace state, so removal is blocked. Hover or focus the badge for the inspection error. A missing default branch shows **No base branch**. Branch-name tooltips show each diagnostic once; main checkouts do not show removal warnings. |
| **Question icon** | An agent in this branch's terminal is asking a question |
| **Grey icon** | No active terminals in the repo — branch icons dim to grey |

### Branch Actions

- **Double-click** the branch name to rename the branch
- **Right-click** for context menu: Copy Path, Add Terminal, Create Worktree (for branches without a worktree), Delete Worktree, Open in IDE, Rename Branch/Worktree, Merge & Archive

A clean worktree can be removed with its local branch when the branch tip is contained in any remote-tracking branch, even with a different name and no upstream. The remote branch stays intact.

When removal would delete an unmerged branch, the error card offers **Remove worktree, keep branch**. The action reruns lifecycle checks and confirmation with branch deletion disabled for that removal only; the repository setting stays unchanged.

### Agent and session activity

Nested agent and terminal rows are an opt-in feature and are disabled by default. Enable **Settings → Appearance → Tabs → Nested Terminal Tabs** to show them; the change applies immediately.

When enabled, the branch icon of each branch with open terminal sessions shows or hides its agents; the rest of the row selects the branch. Enter or Space activates the focused icon. The collapsed session count stays beside it. Agents are shown by default, and collapsed state is remembered. Expanded, the list shows a single activity card containing the sessions assigned to that branch:

- Detected agents show their icon, terminal name, current intent or task, last-update age, and status.
- A sub-agent row has a muted robot tag; hover or focus it to see the parent agent. The GitHub badge in the repo header keeps its accent color.
- Plain shells appear as terminal rows.
- Clicking a row switches to that session.
- A branch with one session can still expand.

When the setting is off, the sidebar intentionally shows none of the activity card or nested rows. Enabling it does not create a session or detect an agent outside an open terminal assigned to the branch.

## Remote-Only PRs

When a repository has open PRs on branches that only exist on the remote (not checked out locally), a badge appears in the branch section. Click it to open a popover listing these PRs. Each row shows the PR number, title, and state badge. Click a row to expand an inline accordion showing PR details in the UI v3 layout:

- **Next action** — one or two buttons chosen by the PR state: **Approve** when you can review it, **Merge** when it is ready, **Resolve conflicts**, **Open in Tasks** for open threads. A line above them says why.
- **Toolbar** — always the same: **Diff** (open the PR diff in a panel tab) and **Checkout** (create a local tracking branch). The `⋯` menu holds **Create worktree**.
- **Sections** — Description, Checks, Threads and AI Review. Only one opens by itself: Checks when CI is red, otherwise Threads when some are open. Each open review thread has **Reply** and **Resolve**.
- **Split button** — `✦ Review ▾` (or the state's agent action) runs the last pick. `▾` lists Start actions, the project's published **Workflows** (started with the PR as subject) and agent prompts, then **Manage workflows…**.

### Dismiss & Show Dismissed

Remote-only PRs can be dismissed to reduce sidebar clutter. Right-click the remote PRs badge or choose **Hide from list** in the row `⋯` menu. A "Show Dismissed" toggle at the bottom reveals dismissed PRs again.

## Park Repos

Temporarily hide repos you're not actively using.

### Parking

Right-click any repo in the sidebar → **Park**. The repo disappears from the main list.

### Viewing Parked Repos

A button in the sidebar footer shows all parked repos with a count badge. Click it to open a popover listing them.

### Unparking

Click **Unpark** on any repo in the parked repos popover. It returns to the main sidebar list.

## Compact and rich layout

The **layout button** in the toolbar (left of the filter icon) cycles the sidebar through three modes and prints the current one on itself: **R** (rich), **C** (compact), **H** (harness-only, with rich rows). R and C restore at startup. A saved H opens as R, because harness sessions are not live yet at startup.

**Compact** is the one-line rows. **Rich** keeps the same navigation density and adds facts:

- **Branch:** the full branch name, with the path prefix muted; the name wraps instead of truncating. The unmerged `↑` mark and the last-commit age follow the name. The facts sit at the right of the same line, or on a second right-aligned line when they do not fit: ahead/behind of the upstream (`↑2 ↓1`), diff stats, a yellow `● N` dirty badge (click opens Changes), `Stale` (no commit for 30 days, not merged), `Unknown` (removal blocked, status could not be read) and the PR badge. The PR title follows on its own line. Each fact explains itself in a tooltip, with the same removal-safety text compact shows. A main checkout never shows stale, merged, dirty or unknown.
- **Agent:** a Working, Needs input or Error pill beside the title, the intent line below while it works, and the robot icon on subagent rows; its in-session subagents, one line each with state, title, tool calls and age (more than three fold into "N subagents", click to expand); a vertical guide connects nested TUIC child sessions to their parent agent on the same branch.
- **Repository:** current branch, positive open-PR and worktree counts, and the age of the last remote poll.

Compact carries the same facts in tooltips: the branch name's tooltip has the commit age, ahead/behind and the stale rule; an agent row's tooltip has its state and line. Subagents and nesting exist only in rich.

**H** shows only branches with a live harness session, together with their repositories and groups. R and C clear this harness filter.

In R and C, branches with no session, no diff, no dirty files, no PR, no lifecycle chip and no Keep mark fold into a **Quiet** section at the end of the repository. A fold line, **› Quiet · N branches · oldest X**, shows the count and the oldest last-commit age; click it to show the rows. The main checkout and the selected branch never fold. Quick-switch numbers stay the same while a row is folded. H has no Quiet section. The separate active-only filter stays available in every mode. Older Auto preferences open in rich mode.

If you choose a repository in the Command Palette (`Cmd+P`) and a sidebar filter hides it, the sidebar switches to the unfiltered view and shows the repository. If you choose a repository that is already visible, the sidebar keeps its filters.

Subagent lines come from the existing `progress_flow` command, read at most every 5 seconds per repository, only in rich.

## Active-Only Filter

When you have many repos open, hide the ones you aren't using right now.

Click the **filter icon** in the toolbar (next to the sidebar collapse button) to show only repositories that have at least one open terminal. The icon turns accent-colored while the filter is on, and a banner at the top of the sidebar shows how many repos are shown out of the total — click it (or "Show all") to clear the filter. The filter is session-only and resets when you restart.

## Quick Branch Switcher

Switch branches by number without the mouse:

1. **Hold** `Cmd+Ctrl` (macOS) or `Ctrl+Alt` (Windows/Linux)
2. All branches show **numbered badges** (1, 2, 3...)
3. **Press a number** (`1–9`) to switch to that branch instantly
4. **Release** the modifier to dismiss the overlay

## Git Quick Actions

When a repo is active, the bottom of the sidebar shows quick action buttons:

- **Pull** — `git pull` in the active terminal
- **Push** — `git push`
- **Fetch** — `git fetch`
- **Stash** — `git stash`

For more git operations (staging, commit, push, pull, stash, blame, history), use the Git Panel (`Cmd+Shift+D`).


### Touch Branch Actions

Swipe a branch row left to reveal **More**, **+**, and, for removable linked
worktrees, a red remove button. **More** opens the branch menu. Tap **+** to
add a terminal, or hold it to choose an agent. Removal uses the existing
confirmation dialog and is disabled while removal is in progress.

Swipe right, tap the row, or tap elsewhere to close the actions. Opening another
row closes the previous one. Vertical swipes continue to scroll the sidebar.
Mouse and trackpad actions appear over the right edge of the row on hover or keyboard focus. Crowded facts do not push the actions outside the sidebar or scroll row names left.

In rich mode, sessions idle for more than two hours fold into an expandable count. Backend activity timestamps determine age; busy sessions, awaiting input, unread output, selected rows and parents of visible children stay visible. Expanding preserves row order. Compact mode does not fold idle sessions.

Rich working agent rows show their intent on at most two lines, with no reserved blank space when there is none. Idle, awaiting-input and error rows stay on one line with a state pill; their full intent, task or prompt remains in the tooltip. Compact rows keep their one-line layout.

Rich navigation keeps compact typography and padding. Branch names show in full and wrap; facts share the name line when they fit and move to a right-aligned second line when they do not. Repository metadata disappears when PR and worktree counts are both zero. Header controls use compact sizing to protect repo names. Finished subagents fold into an expandable **N done** count. Interrupted subagents are labeled **Interrupted** and do not count as done.

In rich mode, ungrouped repositories with open terminals appear first, followed by an Idle section whose header counts its repositories. Manual order is preserved inside each section. Idle headers start on one line; disclosure reveals their branches without changing activity or saved order. Headers count agents in the backend working state, independently of open-terminal membership.

Rich active/idle sections also apply independently inside each existing repository group. Group membership and group ordering remain intact; active-only filtering hides idle-only groups and empty Idle headings.

Rich merged linked worktrees with no sessions and verified lifecycle facts use a one-line summary: the name, the dirty badge and a **Merged** pill. When the worktree can be removed, the pill turns into **Clean** on hover or keyboard focus; there is no separate Clean up button. Live, unmerged, or unknown worktrees retain full rows. Clean enters the existing lifecycle preflight and confirmation workflow; cancellation preserves the worktree. Branches show **⎇**; merged branches show a purple **⌥**; the main checkout shows **✳**. A lock in the Merged pill marks a worktree that cleanup must keep (dirty files, unsafe removal or a sweep refusal); the tooltip gives the reason. Done subagents fold into **› N done** and long-idle sessions into **› N idle**. Repository header counts (working, needs you) are small chips.

Rich parent terminal rows show N agents in each other repository containing live children. Clicking reveals the child group, repository and workspace, then selects a child through existing terminal navigation. Session placement and same-branch nesting stay intact; closed children disappear from summaries.

### Detached worktrees

Detached checkouts have non-selectable rows with their directory name, short SHA and **detached**. Hover shows the full path and HEAD SHA. The repository header retains a detached count while folded. These rows do not change branch selection, terminal placement or rich-mode idle-session folding.
