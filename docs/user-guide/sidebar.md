# Sidebar

The sidebar is your primary navigation for repositories, branches, and git operations.

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
- **`⋯` button** — Opens a menu with: Repo Settings, Remove, Move to Group

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
| **PR badge** | Colored by state — green (open), purple (merged), red (closed), gray (draft). Click for detail popover. |
| **Diff stats** | `+N / -N` additions and deletions |
| **Question icon** | An agent in this branch's terminal is asking a question |

### Branch Actions

- **Double-click** the branch name to rename the branch
- **Right-click** for context menu: Copy Path, Add Terminal, Delete Worktree, Open in IDE, Rename Branch

## Park Repos

Temporarily hide repos you're not actively using.

### Parking

Right-click any repo in the sidebar → **Park**. The repo disappears from the main list.

### Viewing Parked Repos

A button in the sidebar footer shows all parked repos with a count badge. Click it to open a popover listing them.

### Unparking

Click **Unpark** on any repo in the parked repos popover. It returns to the main sidebar list.

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

For more git operations (merge, checkout, conflict resolution), use the Git Operations Panel (`Cmd+Shift+G`).
