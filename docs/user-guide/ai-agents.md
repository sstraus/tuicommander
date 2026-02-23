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
| Amp | `amp` | — |
| Jules | `jules` | — |
| Cursor Agent | `cursor-agent` | — |
| Warp Oz | `oz` | — |
| ONA (Gitpod) | `gitpod` | — |

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

## Fallback Agent Chain

Configure a primary agent and ordered fallback list:

1. Open **Settings** → **Agents**
2. Set your **Primary Agent** (e.g., Claude Code)
3. Configure the **Fallback Chain** (e.g., Gemini CLI → Aider)

When the primary agent hits a rate limit:
- TUICommander automatically switches to the next agent in the chain
- **Auto-recovery** checks every 5 minutes if the primary becomes available again
- Use **Reset to Primary** to force-switch back immediately

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

For Claude Code, TUICommander detects weekly and session usage limit messages:

- **Status bar badge** with percentage:
  - Blue: < 70%
  - Yellow: 70–89%
  - Red (pulsing): >= 90%

This helps you pace your usage across the week.

## Sleep Prevention

When agents are actively working, TUICommander can keep your machine awake:

- Enable in **Settings** → **General** → **Prevent sleep when busy**
- Uses the `keepawake` system integration
- Automatically releases when all agents are idle

## Tips

- **Multiple agents on the same repo** — Use split panes (`Cmd+\`) to run two agents side by side on the same branch
- **Different agents per branch** — Each worktree is independent, so you can run Claude on one branch and Aider on another
- **Monitor all at once** — Use the Activity Dashboard (`Cmd+Shift+A`) to see every terminal's agent status in one view
