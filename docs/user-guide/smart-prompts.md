# Smart Prompts

One-click AI automation for common git and code tasks. Smart Prompts inject context-aware commands into your active agent terminal, or run headless one-shot operations.

## What are Smart Prompts?

Smart Prompts are pre-built prompt templates that automatically resolve git context (branch, diff, changed files, PR data) and deliver them to your AI agent. Instead of manually typing "review the changes on this branch," click a button and the right prompt — with the right context — is sent instantly.

TUICommander ships with 24 built-in prompts covering commit workflows, code review, PR management, CI fixes, and code investigation.

## How to Use

### Toolbar Dropdown

Press **Cmd+Shift+K** (Ctrl+Shift+K on Windows/Linux) or click the lightning bolt icon in the toolbar. The dropdown shows all enabled prompts grouped by category with a search bar at the top.

### Git Panel — Changes Tab

The SmartButtonStrip appears above the changed files list. Quick access to prompts like Smart Commit, Review Changes, and Write Tests.

### PR Detail Popover

Click a PR badge in the sidebar to open the popover. The SmartButtonStrip shows PR-specific prompts: Review PR, Address Review Comments, Fix CI Failures, Update PR Description.

### Command Palette

Open with **Cmd+Shift+P** and type "Smart" to see all smart prompts prefixed with "Smart:".

### Branch Context Menu

Right-click a branch in the Branches tab (Cmd+G) for branch-specific prompts like Create PR, Merge Main Into Branch, and Summarize Branch.

## Built-in Prompts

| Category | Prompts |
|----------|---------|
| **Git & Commit** | Smart Commit, Commit & Push, Amend Commit, Generate Commit Message |
| **Code Review** | Review Changes, Review Staged, Review PR, Address Review Comments |
| **Pull Requests** | Create PR, Update PR Description, Generate PR Description |
| **Merge & Conflicts** | Resolve Conflicts, Merge Main Into Branch, Rebase on Main |
| **CI & Quality** | Fix CI Failures, Fix Lint Issues, Write Tests, Run & Fix Tests |
| **Investigation** | Investigate Issue, What Changed?, Summarize Branch, Explain Changes |
| **Code Operations** | Suggest Refactoring, Security Audit |

## Customizing Smart Prompts

Open **Settings > Smart Prompts** to manage prompts:

- **Enable/disable** individual prompts (disabled prompts are hidden from all UI surfaces)
- **Edit** prompt content — built-in prompts show a "Reset to default" button to revert your changes
- **Create** your own smart prompts with the same placement options and variable system
- **View** each prompt's placement (toolbar, git-changes, pr-popover, git-branches) and execution mode

## Context Variables

Prompts use `{variable_name}` syntax. Most variables are auto-resolved at execution time:

| Variable | Description |
|----------|-------------|
| `{branch}` | Current branch name |
| `{base_branch}` | Default branch (main/master/develop) |
| `{repo_name}` | Repository directory name |
| `{diff}` | Working tree diff |
| `{staged_diff}` | Staged changes diff |
| `{changed_files}` | Short status output |
| `{commit_log}` | Last 20 commits |
| `{last_commit}` | Last commit hash + message |
| `{conflict_files}` | Files with merge conflicts |
| `{stash_list}` | Stash entries |
| `{pr_number}` | PR number for current branch |
| `{pr_title}` | PR title |
| `{pr_checks}` | CI check summary |
| `{agent_type}` | Active agent type |
| `{cwd}` | Active terminal working directory |

Variables not found in auto-resolution (e.g. `{issue_number}`) prompt the user for input before execution.

## Execution Modes

### Inject Mode (Default)

The resolved prompt is written directly into the active terminal's PTY — as if you typed it. The agent processes it like any other input. Before sending, TUICommander checks that the agent is idle (configurable per prompt).

### Headless Mode

Runs a one-shot subprocess without using the terminal. Useful for quick operations like generating a commit message. Output is routed to the clipboard or shown as a toast notification.

**Setup:** Go to **Settings > Agents** and configure the "Headless Command Template" for each agent type. The template uses `{prompt}` as a placeholder:

- Claude: `claude -p "{prompt}"`
- Gemini: `gemini -p "{prompt}"`

Without a template, headless prompts automatically fall back to inject mode.

**Note:** Headless mode is not available in the Mobile Companion (PWA) — prompts fall back to inject mode automatically.
