# Smart Prompts

One-click AI automation for common git and code tasks. Smart Prompts inject context-aware commands into your active agent terminal, run headless one-shot operations, execute shell scripts directly, or call LLM APIs.

## What are Smart Prompts?

Smart Prompts are context-aware automation shortcuts that turn repetitive developer workflows into single-click actions. They automatically resolve git context (branch, diff, changed files, PR data) and deliver a well-crafted prompt to your AI agent — no manual typing, no copy-pasting, no context switching.

TUICommander ships with **24 built-in prompts** covering commit workflows, code review, PR management, CI fixes, and code investigation. Each prompt includes a description explaining what it does, so you always know what will happen before clicking.

## How to Use

### Toolbar Dropdown

Press **Cmd+Shift+K** (Ctrl+Shift+K on Windows/Linux) or click the lightning bolt icon in the toolbar. The dropdown shows all enabled prompts grouped by category with a search bar at the top.

### Git Panel — Changes Tab

The SmartButtonStrip appears above the changed files list. Quick access to prompts like Smart Commit, Review Changes, and Write Tests.

### PR Detail Popover

Click a PR badge in the sidebar to open the popover. The SmartButtonStrip shows PR-specific prompts: Review PR, Address Review Comments, Fix CI Failures, Update PR Description.

### Command Palette

Open with **Cmd+P** and type "Smart" to see all smart prompts prefixed with "Smart:".

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

## Status Feedback

When prompts cannot execute, the dropdown shows a **status banner** at the top explaining why:

- **"No active terminal"** — open a terminal first
- **"No AI agent detected"** — the active terminal has no agent running
- **"Agent is busy"** — wait for the current operation to finish

Items are visually dimmed but visible, so you can still browse what's available.

## Context Variables

Prompts use `{variable_name}` syntax. Most variables are auto-resolved at execution time — no manual input needed.

### Git Context (from Rust backend)

| Variable | Description |
|----------|-------------|
| `{branch}` | Current branch name |
| `{base_branch}` | Default branch (main/master/develop) |
| `{repo_name}` | Repository directory name |
| `{repo_path}` | Full filesystem path to the repository root |
| `{diff}` | Working tree diff (truncated to 50KB) |
| `{staged_diff}` | Staged changes diff (truncated to 50KB) |
| `{changed_files}` | Short status output |
| `{commit_log}` | Last 20 commits |
| `{last_commit}` | Last commit hash + message |
| `{conflict_files}` | Files with merge conflicts |
| `{stash_list}` | Stash entries |

### GitHub/PR Context (from frontend stores)

| Variable | Description |
|----------|-------------|
| `{pr_number}` | PR number for current branch |
| `{pr_title}` | PR title |
| `{pr_url}` | GitHub pull request URL |
| `{pr_state}` | PR state: OPEN, MERGED, or CLOSED |
| `{pr_checks}` | CI check summary (e.g. "3 passed, 1 failed") |
| `{merge_status}` | Merge status: MERGEABLE, CONFLICTING, or BEHIND |
| `{review_decision}` | Review status: APPROVED, CHANGES_REQUESTED, or REVIEW_REQUIRED |

### Agent/Terminal Context

| Variable | Description |
|----------|-------------|
| `{agent_type}` | Active agent type (claude, aider, codex, etc.) |
| `{cwd}` | Active terminal working directory |

### Manual Input Variables

| Variable | Description |
|----------|-------------|
| `{issue_number}` | GitHub issue number to investigate |

### Variable Input Dialog

When a prompt contains variables that cannot be auto-resolved, a **Variable Input Dialog** appears before execution. Each field shows the variable name and a human-readable description, so you know exactly what to fill in. Pre-populated suggestions are shown where available.

## Execution Modes

### Inject Mode (Default)

The resolved prompt is written directly into the active terminal's PTY — as if you typed it. The agent processes it like any other input. Before sending, TUICommander checks that the agent is idle (configurable per prompt).

### Shell Script Mode

Executes the prompt content as a shell script directly — no AI agent involved. The content runs via the system shell (`sh -c` on macOS/Linux, `cmd /C` on Windows) in the active repository's directory.

Useful for automating repetitive CLI tasks: pruning orphan branches, running linters, collecting metrics, or any command pipeline you'd otherwise type manually. Context variables like `{branch}` and `{repo_path}` are resolved before execution.

Output is routed based on the prompt's output target (clipboard, toast, panel, or returned in result). Timeout: 60 seconds.

### Headless Mode

Runs a one-shot subprocess without using the terminal. Useful for quick operations like generating a commit message. Output is routed to the clipboard or shown as a toast notification.

**Setup:** Go to **Settings > Agents** and configure the "Headless Command Template" for each agent type. The template uses `{prompt}` as a placeholder:

- Claude: `claude -p "{prompt}"`
- Gemini: `gemini -p "{prompt}"`

Without a template, headless prompts automatically fall back to inject mode.

**Note:** Headless mode is not available in the Mobile Companion (PWA) — prompts fall back to inject mode automatically.

### API Mode

Calls LLM providers directly via HTTP API without terminal or agent CLI. Supports an optional system prompt per prompt. Output routed via the same output target options. Requires LLM API configuration in Settings > Agents.
