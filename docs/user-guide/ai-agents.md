# AI Agents

TUICommander detects, monitors, and manages AI coding agents running in your terminals.

## Supported Agents

| Agent | Binary | Resume Command |
|-------|--------|----------------|
| Claude Code | `claude` | `claude --continue` |
| Codex CLI | `codex` | `codex resume --last` |
| Aider | `aider` | `aider --restore-chat-history` |
| Gemini CLI | `gemini` | `gemini --resume` |
| OpenCode | `opencode` | `opencode -c` |
| Amp | `amp` | `amp threads continue` |
| Cursor Agent | `cursor-agent` | `cursor-agent resume` |
| Warp Oz | `oz` | — |
| Droid (Factory) | `droid` | — |

## Agent Detection

TUICommander auto-detects which agent is running in each terminal by matching output patterns. When detected:

- The **status bar** shows the agent's brand logo and name
- The **tab indicator** updates to reflect agent state
- Rate limit and question detection activate for that provider's patterns

Binary detection uses `resolve_cli()` — Rust probes well-known directories so agents are found even in release builds where the user's shell PATH isn't available.

## Rate Limit Detection

When an agent hits a rate limit, TUICommander detects it from terminal output:

- **Status bar warning** — Shows a badge with the number of rate-limited sessions and a countdown timer
- **Per-session tracking** — Each session's rate limit is tracked independently with automatic cleanup when expired
- **Provider-specific patterns** — Custom regex for Claude ("overloaded", "rate limit"), Gemini ("429", "quota exceeded"), OpenAI ("too many requests"), and generic patterns

## Question Detection

When an agent asks an interactive question (Y/N, multiple choice, numbered options), TUICommander:

1. Changes the **tab indicator** to a `?` icon
2. Shows a **prompt overlay** with keyboard navigation:
   - `↑/↓` to navigate options
   - `Enter` to select
   - Number keys `1-9` for numbered options
   - `Escape` to dismiss
3. Plays a **notification sound** (if enabled in Settings → Notifications)

For unrecognized agents, silence-based detection kicks in — if the terminal stops producing output for a configured duration, it's treated as a potential prompt.

## Usage Limit Tracking

For Claude Code, TUICommander detects weekly and session usage limit messages from terminal output:

- **Unified agent badge** — When Claude is the active agent, the status bar shows a single badge combining the agent icon with usage data. The badge displays rate limit countdowns (when rate-limited), Claude Usage API data (5h/7d utilization percentages), or terminal-detected usage limits, in that priority order.
  - Blue: < 70% utilization
  - Yellow: 70–89%
  - Red (pulsing): >= 90%
- Clicking the badge opens the Claude Usage Dashboard.

This helps you pace your usage across the week.

## Claude Usage Dashboard

A native feature (not a plugin) that provides detailed analytics for your Claude Code usage. Enable it in **Settings** > **Agents** > expand **Claude Code** > **Features** > **Usage Dashboard**.

When enabled, TUICommander polls the Claude API every 5 minutes and shows:

- **Rate limits** — 5-hour and 7-day utilization bars with reset countdowns. Color-coded: green (OK), yellow (70%+), red (90%+).
- **Usage Over Time** — 7-day token usage chart (input vs. output tokens) with hover tooltips.
- **Insights** — Session count, message counts, input/output/cache token totals.
- **Activity heatmap** — 52-week GitHub-style heatmap of daily message counts with per-project drill-down on hover.
- **Model usage** — Breakdown by model (messages, input, output, cache created, cache read).
- **Per-project breakdown** — All projects ranked by token usage. Click a project to filter the dashboard to that project.

The dashboard opens as a tab in the Activity Center. You can also reach it by clicking the Claude usage badge in the status bar.

## Sleep Prevention

When agents are actively working, TUICommander can keep your machine awake:

- Enable in **Settings** → **General** → **Prevent sleep when busy**
- Uses the `keepawake` system integration
- Automatically releases when all agents are idle

## Tips

- **Multiple agents on the same repo** — Use split panes (`Cmd+\`) to run two agents side by side on the same branch
- **Different agents per branch** — Each worktree is independent, so you can run Claude on one branch and Aider on another
- **Monitor all at once** — Use the Activity Dashboard (`Cmd+Shift+A`) to see every terminal's agent status in one view
